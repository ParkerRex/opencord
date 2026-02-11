use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, patch, post},
};
use discord_api_types::{
    CreateMessageRequest, CurrentUserGuild, EditMessageRequest, GetChannelMessagesQuery,
    GetCurrentUserGuildsQuery, Message, Snowflake,
};
use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_gateway::{GatewayEvent, GatewayRuntimeEvent, GatewayRuntimeOptions};
use discord_http::{DiscordHttpClient, HttpError};
use discord_storage::{
    GATEWAY_SESSION_STORAGE_KEY, GatewaySessionRecord, KeyValueStore, StorageError,
};
use discord_voice::{
    VoiceConnectionConfig, VoiceGatewayClient, VoiceGatewayEvent, VoiceRuntimeEvent,
    VoiceRuntimeOptions,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, watch};
use tokio::task::JoinHandle;
use url::Url;
use uuid::Uuid;

const DEFAULT_GATEWAY_INTENTS: u64 = 641;
const TOKEN_REFRESH_SKEW: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("missing authorization header")]
    MissingAuthorization,
    #[error("invalid authorization header")]
    InvalidAuthorization,
    #[error("unauthorized session")]
    UnauthorizedSession,
    #[error("oauth configuration is missing")]
    MissingOAuthConfiguration,
    #[error("oauth state is invalid or expired")]
    InvalidOAuthState,
    #[error("invalid oauth client redirect URI")]
    InvalidOAuthClientRedirectUri,
    #[error("oauth exchange failed: {0}")]
    OAuthExchange(String),
    #[error("resource not found")]
    NotFound,
    #[error("gateway runtime failed: {0}")]
    GatewayRuntime(String),
    #[error("voice runtime failed: {0}")]
    VoiceRuntime(String),
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match self {
            ServiceError::MissingAuthorization
            | ServiceError::InvalidAuthorization
            | ServiceError::UnauthorizedSession => StatusCode::UNAUTHORIZED,
            ServiceError::MissingOAuthConfiguration => StatusCode::SERVICE_UNAVAILABLE,
            ServiceError::InvalidOAuthState | ServiceError::InvalidOAuthClientRedirectUri => {
                StatusCode::BAD_REQUEST
            }
            ServiceError::OAuthExchange(_)
            | ServiceError::GatewayRuntime(_)
            | ServiceError::VoiceRuntime(_) => StatusCode::BAD_GATEWAY,
            ServiceError::NotFound => StatusCode::NOT_FOUND,
            ServiceError::Http(HttpError::Status { status, .. }) => status,
            ServiceError::Http(_) | ServiceError::Storage(_) => StatusCode::BAD_GATEWAY,
        };

        let code = match &self {
            ServiceError::MissingAuthorization => "missing_authorization",
            ServiceError::InvalidAuthorization => "invalid_authorization",
            ServiceError::UnauthorizedSession => "unauthorized_session",
            ServiceError::MissingOAuthConfiguration => "missing_oauth_configuration",
            ServiceError::InvalidOAuthState => "invalid_oauth_state",
            ServiceError::InvalidOAuthClientRedirectUri => "invalid_oauth_client_redirect_uri",
            ServiceError::OAuthExchange(_) => "oauth_exchange_failed",
            ServiceError::NotFound => "not_found",
            ServiceError::GatewayRuntime(_) => "gateway_runtime_failed",
            ServiceError::VoiceRuntime(_) => "voice_runtime_failed",
            ServiceError::Http(HttpError::Status { .. }) => "discord_http_status",
            ServiceError::Http(_) => "discord_http_transport",
            ServiceError::Storage(_) => "storage_error",
        };

        let payload = Json(serde_json::json!({
            "code": code,
            "error": self.to_string(),
        }));
        (status, payload).into_response()
    }
}

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub authorize_url: Url,
    pub client_id: String,
    pub redirect_uri: Url,
    pub scope: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

#[async_trait]
pub trait OAuthCodeExchanger: Send + Sync {
    async fn exchange_code(&self, code: &str) -> Result<OAuthTokenSet, ServiceError>;

    async fn refresh_token(&self, _refresh_token: &str) -> Result<OAuthTokenSet, ServiceError> {
        Err(ServiceError::OAuthExchange(
            "oauth refresh is not configured".to_owned(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct UnsupportedOAuthCodeExchanger;

#[async_trait]
impl OAuthCodeExchanger for UnsupportedOAuthCodeExchanger {
    async fn exchange_code(&self, _code: &str) -> Result<OAuthTokenSet, ServiceError> {
        Err(ServiceError::OAuthExchange(
            "oauth code exchange is not configured".to_owned(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct DiscordOAuthCodeExchanger {
    client: reqwest::Client,
    token_url: Url,
    client_id: String,
    client_secret: String,
    redirect_uri: Url,
}

impl DiscordOAuthCodeExchanger {
    pub fn new(
        token_url: Url,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: Url,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            token_url,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri,
        }
    }
}

#[derive(Debug, Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[async_trait]
impl OAuthCodeExchanger for DiscordOAuthCodeExchanger {
    async fn exchange_code(&self, code: &str) -> Result<OAuthTokenSet, ServiceError> {
        let response = self
            .client
            .post(self.token_url.clone())
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
            ])
            .send()
            .await
            .map_err(|error| {
                ServiceError::OAuthExchange(format!("token exchange request failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<no body>"));
            return Err(ServiceError::OAuthExchange(format!(
                "token exchange failed with status {status}: {body}"
            )));
        }

        let payload: DiscordTokenResponse = response.json().await.map_err(|error| {
            ServiceError::OAuthExchange(format!("invalid token response: {error}"))
        })?;

        Ok(OAuthTokenSet {
            access_token: payload.access_token,
            token_type: payload.token_type,
            expires_in: payload.expires_in,
            refresh_token: payload.refresh_token,
            scope: payload.scope,
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokenSet, ServiceError> {
        let response = self
            .client
            .post(self.token_url.clone())
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
            ])
            .send()
            .await
            .map_err(|error| {
                ServiceError::OAuthExchange(format!("token refresh request failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<no body>"));
            return Err(ServiceError::OAuthExchange(format!(
                "token refresh failed with status {status}: {body}"
            )));
        }

        let payload: DiscordTokenResponse = response.json().await.map_err(|error| {
            ServiceError::OAuthExchange(format!("invalid refresh response: {error}"))
        })?;

        Ok(OAuthTokenSet {
            access_token: payload.access_token,
            token_type: payload.token_type,
            expires_in: payload.expires_in,
            refresh_token: payload.refresh_token,
            scope: payload.scope,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeEnvelope {
    MessageCreated {
        session_id: String,
        message: Message,
    },
    MessageUpdated {
        session_id: String,
        message: Message,
    },
    MessageDeleted {
        session_id: String,
        channel_id: String,
        message_id: String,
    },
    VoiceStateChanged {
        session_id: String,
        voice_session_id: String,
        guild_id: String,
        channel_id: String,
        speaking: bool,
    },
    GatewayDispatch {
        session_id: String,
        event_type: String,
        data: Value,
    },
    ReconnectScheduled {
        session_id: String,
        attempt: u32,
        resumable: bool,
        delay_ms: u64,
    },
    Notification {
        session_id: Option<String>,
        title: String,
        body: String,
    },
}

impl RealtimeEnvelope {
    fn is_for_session(&self, session_id: &str) -> bool {
        match self {
            RealtimeEnvelope::MessageCreated {
                session_id: owner, ..
            }
            | RealtimeEnvelope::MessageUpdated {
                session_id: owner, ..
            }
            | RealtimeEnvelope::MessageDeleted {
                session_id: owner, ..
            }
            | RealtimeEnvelope::VoiceStateChanged {
                session_id: owner, ..
            }
            | RealtimeEnvelope::GatewayDispatch {
                session_id: owner, ..
            }
            | RealtimeEnvelope::ReconnectScheduled {
                session_id: owner, ..
            } => owner == session_id,
            RealtimeEnvelope::Notification {
                session_id: Some(owner),
                ..
            } => owner == session_id,
            RealtimeEnvelope::Notification {
                session_id: None, ..
            } => true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceSession {
    pub id: String,
    pub guild_id: String,
    pub channel_id: String,
    pub speaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionTokenState {
    pub access_token: String,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub expires_at_unix_ms: Option<u64>,
}

impl SessionTokenState {
    fn from_access_token(access_token: String) -> Self {
        Self {
            access_token,
            token_type: None,
            refresh_token: None,
            scope: None,
            expires_at_unix_ms: None,
        }
    }

    fn from_token_set(token_set: OAuthTokenSet, fallback_refresh_token: Option<String>) -> Self {
        let expires_at_unix_ms = token_set.expires_in.map(|expires_in| {
            current_unix_epoch_ms().saturating_add(expires_in.saturating_mul(1000))
        });

        Self {
            access_token: token_set.access_token,
            token_type: Some(token_set.token_type),
            refresh_token: token_set.refresh_token.or(fallback_refresh_token),
            scope: token_set.scope,
            expires_at_unix_ms,
        }
    }

    fn should_refresh(&self) -> bool {
        let Some(expires_at_unix_ms) = self.expires_at_unix_ms else {
            return false;
        };

        let refresh_deadline =
            current_unix_epoch_ms().saturating_add(TOKEN_REFRESH_SKEW.as_millis() as u64);
        expires_at_unix_ms <= refresh_deadline
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRecord {
    discord_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    expires_at_unix_ms: Option<u64>,
    #[serde(default)]
    gateway_session: Option<GatewaySessionRecord>,
}

impl SessionRecord {
    fn from_token_state(token_state: SessionTokenState) -> Self {
        Self {
            discord_token: token_state.access_token,
            token_type: token_state.token_type,
            refresh_token: token_state.refresh_token,
            scope: token_state.scope,
            expires_at_unix_ms: token_state.expires_at_unix_ms,
            gateway_session: None,
        }
    }

    fn token_state(&self) -> SessionTokenState {
        SessionTokenState {
            access_token: self.discord_token.clone(),
            token_type: self.token_type.clone(),
            refresh_token: self.refresh_token.clone(),
            scope: self.scope.clone(),
            expires_at_unix_ms: self.expires_at_unix_ms,
        }
    }

    fn set_token_state(&mut self, token_state: SessionTokenState) {
        self.discord_token = token_state.access_token;
        self.token_type = token_state.token_type;
        self.refresh_token = token_state.refresh_token;
        self.scope = token_state.scope;
        self.expires_at_unix_ms = token_state.expires_at_unix_ms;
    }
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get_discord_token(&self, session_id: &str) -> Result<Option<String>, ServiceError>;
    async fn set_discord_token(&self, session_id: &str, token: String) -> Result<(), ServiceError>;
    async fn delete_session(&self, session_id: &str) -> Result<(), ServiceError>;

    async fn get_token_state(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionTokenState>, ServiceError> {
        Ok(self
            .get_discord_token(session_id)
            .await?
            .map(SessionTokenState::from_access_token))
    }

    async fn set_token_state(
        &self,
        session_id: &str,
        token_state: SessionTokenState,
    ) -> Result<(), ServiceError> {
        self.set_discord_token(session_id, token_state.access_token)
            .await
    }

    async fn load_gateway_session(
        &self,
        _session_id: &str,
    ) -> Result<Option<GatewaySessionRecord>, ServiceError> {
        Ok(None)
    }

    async fn save_gateway_session(
        &self,
        _session_id: &str,
        _record: GatewaySessionRecord,
    ) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn clear_gateway_session(&self, _session_id: &str) -> Result<(), ServiceError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct KvSessionStore<S>
where
    S: KeyValueStore,
{
    store: S,
    key_prefix: String,
}

impl<S> KvSessionStore<S>
where
    S: KeyValueStore,
{
    pub fn new(store: S, key_prefix: impl Into<String>) -> Self {
        Self {
            store,
            key_prefix: key_prefix.into(),
        }
    }

    fn key_for(&self, session_id: &str) -> String {
        format!("{}{}", self.key_prefix, session_id)
    }

    async fn load_record(&self, session_id: &str) -> Result<Option<SessionRecord>, ServiceError> {
        let key = self.key_for(session_id);
        let Some(bytes) = self.store.get(&key).await? else {
            return Ok(None);
        };

        let record: SessionRecord = serde_json::from_slice(&bytes).map_err(|error| {
            ServiceError::OAuthExchange(format!("invalid session payload: {error}"))
        })?;
        Ok(Some(record))
    }

    async fn save_record(
        &self,
        session_id: &str,
        record: &SessionRecord,
    ) -> Result<(), ServiceError> {
        let key = self.key_for(session_id);
        let payload = serde_json::to_vec(record).map_err(|error| {
            ServiceError::OAuthExchange(format!("failed to serialize session payload: {error}"))
        })?;

        self.store.set(&key, payload).await?;
        Ok(())
    }
}

#[async_trait]
impl<S> SessionStore for KvSessionStore<S>
where
    S: KeyValueStore,
{
    async fn get_discord_token(&self, session_id: &str) -> Result<Option<String>, ServiceError> {
        Ok(self
            .load_record(session_id)
            .await?
            .map(|record| record.discord_token))
    }

    async fn set_discord_token(&self, session_id: &str, token: String) -> Result<(), ServiceError> {
        let mut record = self.load_record(session_id).await?.unwrap_or_else(|| {
            SessionRecord::from_token_state(SessionTokenState::from_access_token(token.clone()))
        });
        record.set_token_state(SessionTokenState::from_access_token(token));
        self.save_record(session_id, &record).await
    }

    async fn get_token_state(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionTokenState>, ServiceError> {
        Ok(self
            .load_record(session_id)
            .await?
            .map(|record| record.token_state()))
    }

    async fn set_token_state(
        &self,
        session_id: &str,
        token_state: SessionTokenState,
    ) -> Result<(), ServiceError> {
        let mut record = self
            .load_record(session_id)
            .await?
            .unwrap_or_else(|| SessionRecord::from_token_state(token_state.clone()));
        record.set_token_state(token_state);
        self.save_record(session_id, &record).await
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), ServiceError> {
        self.store.delete(&self.key_for(session_id)).await?;
        Ok(())
    }

    async fn load_gateway_session(
        &self,
        session_id: &str,
    ) -> Result<Option<GatewaySessionRecord>, ServiceError> {
        Ok(self
            .load_record(session_id)
            .await?
            .and_then(|record| record.gateway_session))
    }

    async fn save_gateway_session(
        &self,
        session_id: &str,
        record: GatewaySessionRecord,
    ) -> Result<(), ServiceError> {
        let mut session_record = self
            .load_record(session_id)
            .await?
            .ok_or(ServiceError::UnauthorizedSession)?;
        session_record.gateway_session = Some(record);
        self.save_record(session_id, &session_record).await
    }

    async fn clear_gateway_session(&self, session_id: &str) -> Result<(), ServiceError> {
        let mut session_record = self
            .load_record(session_id)
            .await?
            .ok_or(ServiceError::UnauthorizedSession)?;
        session_record.gateway_session = None;
        self.save_record(session_id, &session_record).await
    }
}

#[derive(Debug, Default)]
pub struct MemorySessionStore {
    inner: RwLock<HashMap<String, SessionRecord>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert_for_tests(&self, session_id: &str, token: &str) {
        self.inner.write().await.insert(
            session_id.to_owned(),
            SessionRecord::from_token_state(SessionTokenState::from_access_token(token.to_owned())),
        );
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn get_discord_token(&self, session_id: &str) -> Result<Option<String>, ServiceError> {
        Ok(self
            .inner
            .read()
            .await
            .get(session_id)
            .map(|record| record.discord_token.clone()))
    }

    async fn set_discord_token(&self, session_id: &str, token: String) -> Result<(), ServiceError> {
        let mut guard = self.inner.write().await;
        let entry = guard.entry(session_id.to_owned()).or_insert_with(|| {
            SessionRecord::from_token_state(SessionTokenState::from_access_token(token.clone()))
        });
        entry.set_token_state(SessionTokenState::from_access_token(token));
        Ok(())
    }

    async fn get_token_state(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionTokenState>, ServiceError> {
        Ok(self
            .inner
            .read()
            .await
            .get(session_id)
            .map(|record| record.token_state()))
    }

    async fn set_token_state(
        &self,
        session_id: &str,
        token_state: SessionTokenState,
    ) -> Result<(), ServiceError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .entry(session_id.to_owned())
            .or_insert_with(|| SessionRecord::from_token_state(token_state.clone()));
        entry.set_token_state(token_state);
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), ServiceError> {
        self.inner.write().await.remove(session_id);
        Ok(())
    }

    async fn load_gateway_session(
        &self,
        session_id: &str,
    ) -> Result<Option<GatewaySessionRecord>, ServiceError> {
        Ok(self
            .inner
            .read()
            .await
            .get(session_id)
            .and_then(|record| record.gateway_session.clone()))
    }

    async fn save_gateway_session(
        &self,
        session_id: &str,
        record: GatewaySessionRecord,
    ) -> Result<(), ServiceError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .get_mut(session_id)
            .ok_or(ServiceError::UnauthorizedSession)?;
        entry.gateway_session = Some(record);
        Ok(())
    }

    async fn clear_gateway_session(&self, session_id: &str) -> Result<(), ServiceError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .get_mut(session_id)
            .ok_or(ServiceError::UnauthorizedSession)?;
        entry.gateway_session = None;
        Ok(())
    }
}

#[derive(Clone)]
struct ServiceGatewayStore {
    sessions: Arc<dyn SessionStore>,
    session_id: String,
}

impl ServiceGatewayStore {
    fn new(sessions: Arc<dyn SessionStore>, session_id: String) -> Self {
        Self {
            sessions,
            session_id,
        }
    }
}

#[async_trait]
impl KeyValueStore for ServiceGatewayStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        if key != GATEWAY_SESSION_STORAGE_KEY {
            return Ok(None);
        }

        let loaded = self
            .sessions
            .load_gateway_session(&self.session_id)
            .await
            .map_err(|error| StorageError::Backend(error.to_string()))?;

        let Some(record) = loaded else {
            return Ok(None);
        };

        serde_json::to_vec(&record).map(Some).map_err(|error| {
            StorageError::Backend(format!("serialize gateway session failed: {error}"))
        })
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        if key != GATEWAY_SESSION_STORAGE_KEY {
            return Ok(());
        }

        let record: GatewaySessionRecord = serde_json::from_slice(&value).map_err(|error| {
            StorageError::Backend(format!("deserialize gateway session failed: {error}"))
        })?;

        self.sessions
            .save_gateway_session(&self.session_id, record)
            .await
            .map_err(|error| StorageError::Backend(error.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        if key != GATEWAY_SESSION_STORAGE_KEY {
            return Ok(());
        }

        match self.sessions.clear_gateway_session(&self.session_id).await {
            Ok(()) => Ok(()),
            Err(ServiceError::UnauthorizedSession) => Ok(()),
            Err(error) => Err(StorageError::Backend(error.to_string())),
        }
    }
}

struct GatewayWorkerHandle {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

struct VoiceWorkerHandle {
    owner_session_id: String,
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

#[derive(Debug, Clone)]
struct PendingOAuthState {
    client_redirect_uri: Option<Url>,
}

#[derive(Debug, Clone)]
struct VoiceSessionState {
    owner_session_id: String,
    session: VoiceSession,
}

#[derive(Clone)]
pub struct ServiceState {
    http: DiscordHttpClient,
    sessions: Arc<dyn SessionStore>,
    oauth_config: Option<OAuthConfig>,
    oauth_exchanger: Arc<dyn OAuthCodeExchanger>,
    pending_oauth_states: Arc<RwLock<HashMap<String, PendingOAuthState>>>,
    voice_sessions: Arc<RwLock<HashMap<String, VoiceSessionState>>>,
    voice_workers: Arc<RwLock<HashMap<String, VoiceWorkerHandle>>>,
    gateway_workers: Arc<RwLock<HashMap<String, GatewayWorkerHandle>>>,
    gateway_intents: u64,
    events_tx: broadcast::Sender<RealtimeEnvelope>,
}

impl ServiceState {
    pub fn new(
        http: DiscordHttpClient,
        sessions: Arc<dyn SessionStore>,
        oauth_config: Option<OAuthConfig>,
        oauth_exchanger: Arc<dyn OAuthCodeExchanger>,
    ) -> Self {
        let (events_tx, _) = broadcast::channel(1024);

        Self {
            http,
            sessions,
            oauth_config,
            oauth_exchanger,
            pending_oauth_states: Arc::new(RwLock::new(HashMap::new())),
            voice_sessions: Arc::new(RwLock::new(HashMap::new())),
            voice_workers: Arc::new(RwLock::new(HashMap::new())),
            gateway_workers: Arc::new(RwLock::new(HashMap::new())),
            gateway_intents: DEFAULT_GATEWAY_INTENTS,
            events_tx,
        }
    }

    pub fn with_gateway_intents(mut self, gateway_intents: u64) -> Self {
        self.gateway_intents = gateway_intents;
        self
    }

    pub fn event_sender(&self) -> broadcast::Sender<RealtimeEnvelope> {
        self.events_tx.clone()
    }

    pub fn from_key_value_store<S>(
        http: DiscordHttpClient,
        store: S,
        key_prefix: impl Into<String>,
        oauth_config: Option<OAuthConfig>,
        oauth_exchanger: Arc<dyn OAuthCodeExchanger>,
    ) -> Self
    where
        S: KeyValueStore + 'static,
    {
        Self::new(
            http,
            Arc::new(KvSessionStore::new(store, key_prefix)),
            oauth_config,
            oauth_exchanger,
        )
    }

    async fn emit_event(&self, event: RealtimeEnvelope) {
        let _ = self.events_tx.send(event);
    }

    async fn resolve_discord_token(&self, session_id: &str) -> Result<String, ServiceError> {
        let mut token_state = self
            .sessions
            .get_token_state(session_id)
            .await?
            .ok_or(ServiceError::UnauthorizedSession)?;

        if token_state.should_refresh() {
            if let Some(refresh_token) = token_state.refresh_token.clone() {
                let refreshed = self.oauth_exchanger.refresh_token(&refresh_token).await?;
                token_state = SessionTokenState::from_token_set(refreshed, Some(refresh_token));
                self.sessions
                    .set_token_state(session_id, token_state.clone())
                    .await?;
            }
        }

        Ok(token_state.access_token)
    }

    async fn ensure_gateway_worker(
        &self,
        session_id: &str,
        discord_token: &str,
    ) -> Result<(), ServiceError> {
        if self.gateway_workers.read().await.contains_key(session_id) {
            return Ok(());
        }

        let mut workers = self.gateway_workers.write().await;
        if workers.contains_key(session_id) {
            return Ok(());
        }

        let session_id_owned = session_id.to_owned();
        let token_owned = discord_token.to_owned();

        let gateway_store =
            ServiceGatewayStore::new(self.sessions.clone(), session_id_owned.clone());
        let client = DiscordClient::new(
            self.http.clone(),
            MemoryTokenProvider::from_token(token_owned.clone()),
            gateway_store,
        );

        let startup = client
            .session_manager()
            .bootstrap_gateway(self.gateway_intents)
            .await
            .map_err(|error| {
                ServiceError::GatewayRuntime(format!("gateway bootstrap failed: {error}"))
            })?;

        let fallback_resume_url = startup.gateway_client.gateway_url().clone();
        let gateway_client = startup.gateway_client;
        let state_machine = startup.state_machine;
        let runtime_options = GatewayRuntimeOptions::default();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let workers_ref = self.gateway_workers.clone();
        let events_tx = self.events_tx.clone();
        let events_tx_for_runtime = events_tx.clone();
        let event_session = session_id_owned.clone();

        let task = tokio::spawn(async move {
            let result = gateway_client
                .run_with_state_machine_shutdown(
                    state_machine,
                    runtime_options,
                    shutdown_rx,
                    move |runtime_event| {
                        if let Some(event) =
                            runtime_event_to_envelope(&event_session, &runtime_event)
                        {
                            let _ = events_tx_for_runtime.send(event);
                        }
                    },
                )
                .await;

            match result {
                Ok(final_state_machine) => {
                    let _ = client
                        .session_manager()
                        .persist_from_state_machine(&final_state_machine, &fallback_resume_url)
                        .await;
                }
                Err(error) => {
                    let _ = events_tx.send(RealtimeEnvelope::Notification {
                        session_id: Some(session_id_owned.clone()),
                        title: "gateway_runtime_error".to_owned(),
                        body: error.to_string(),
                    });
                }
            }

            workers_ref.write().await.remove(&session_id_owned);
        });

        workers.insert(
            session_id.to_owned(),
            GatewayWorkerHandle { shutdown_tx, task },
        );
        Ok(())
    }

    async fn stop_gateway_worker(&self, session_id: &str) {
        let handle = self.gateway_workers.write().await.remove(session_id);
        if let Some(handle) = handle {
            let _ = handle.shutdown_tx.send(true);
            let _ = handle.task.await;
        }
    }

    async fn start_voice_worker(
        &self,
        owner_session_id: &str,
        voice_session_id: &str,
        config: VoiceConnectionConfig,
        gateway_url: Url,
    ) -> Result<(), ServiceError> {
        if self
            .voice_workers
            .read()
            .await
            .contains_key(voice_session_id)
        {
            return Ok(());
        }

        let mut workers = self.voice_workers.write().await;
        if workers.contains_key(voice_session_id) {
            return Ok(());
        }

        let voice_client = VoiceGatewayClient::new(gateway_url);
        let runtime_options = VoiceRuntimeOptions::default();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let owner_session_id_owned = owner_session_id.to_owned();
        let voice_session_id_owned = voice_session_id.to_owned();
        let owner_session_id_for_runtime = owner_session_id_owned.clone();
        let voice_session_id_for_runtime = voice_session_id_owned.clone();
        let workers_ref = self.voice_workers.clone();
        let events_tx = self.events_tx.clone();
        let events_tx_for_runtime = events_tx.clone();

        let task = tokio::spawn(async move {
            let result = voice_client
                .run_with_shutdown(config, runtime_options, shutdown_rx, move |event| {
                    if let Some(envelope) = voice_runtime_event_to_envelope(
                        &owner_session_id_for_runtime,
                        &voice_session_id_for_runtime,
                        &event,
                    ) {
                        let _ = events_tx_for_runtime.send(envelope);
                    }
                })
                .await;

            if let Err(error) = result {
                let _ = events_tx.send(RealtimeEnvelope::Notification {
                    session_id: Some(owner_session_id_owned.clone()),
                    title: "voice_runtime_error".to_owned(),
                    body: error.to_string(),
                });
            }

            workers_ref.write().await.remove(&voice_session_id_owned);
        });

        workers.insert(
            voice_session_id.to_owned(),
            VoiceWorkerHandle {
                owner_session_id: owner_session_id.to_owned(),
                shutdown_tx,
                task,
            },
        );
        Ok(())
    }

    async fn stop_voice_worker(&self, voice_session_id: &str) {
        let handle = self.voice_workers.write().await.remove(voice_session_id);
        if let Some(handle) = handle {
            let _ = handle.shutdown_tx.send(true);
            let _ = handle.task.await;
        }
    }

    async fn stop_voice_workers_for_session(&self, owner_session_id: &str) {
        let worker_ids: Vec<String> = {
            let workers = self.voice_workers.read().await;
            workers
                .iter()
                .filter_map(|(id, handle)| {
                    if handle.owner_session_id == owner_session_id {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for worker_id in worker_ids {
            self.stop_voice_worker(&worker_id).await;
        }
    }
}

pub fn router(state: ServiceState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/auth/discord/start", post(auth_start))
        .route("/v1/auth/discord/callback", get(auth_callback))
        .route("/v1/auth/logout", post(auth_logout))
        .route("/v1/me", get(get_current_user))
        .route("/v1/guilds", get(get_guilds))
        .route("/v1/guilds/{guild_id}/channels", get(get_guild_channels))
        .route(
            "/v1/channels/{channel_id}/messages",
            get(get_channel_messages).post(post_channel_message),
        )
        .route(
            "/v1/channels/{channel_id}/messages/{message_id}",
            patch(patch_channel_message).delete(delete_channel_message),
        )
        .route("/v1/voice/sessions", post(create_voice_session))
        .route(
            "/v1/voice/sessions/{voice_session_id}",
            delete(delete_voice_session),
        )
        .route(
            "/v1/voice/sessions/{voice_session_id}/speaking",
            post(update_voice_speaking_state),
        )
        .route("/v1/stream", get(stream_events))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

#[derive(Debug, Serialize)]
struct AuthStartResponse {
    authorize_url: String,
    state: String,
}

#[derive(Debug, Deserialize, Default)]
struct AuthStartQuery {
    client_redirect_uri: Option<String>,
}

async fn auth_start(
    State(state): State<ServiceState>,
    Query(query): Query<AuthStartQuery>,
) -> Result<Json<AuthStartResponse>, ServiceError> {
    let Some(config) = state.oauth_config.clone() else {
        return Err(ServiceError::MissingOAuthConfiguration);
    };

    let state_token = Uuid::new_v4().to_string();
    let client_redirect_uri = query
        .client_redirect_uri
        .as_deref()
        .map(Url::parse)
        .transpose();
    let client_redirect_uri =
        client_redirect_uri.map_err(|_| ServiceError::InvalidOAuthClientRedirectUri)?;

    state.pending_oauth_states.write().await.insert(
        state_token.clone(),
        PendingOAuthState {
            client_redirect_uri,
        },
    );

    let authorize_url = Url::parse_with_params(
        config.authorize_url.as_str(),
        &[
            ("response_type", "code"),
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("scope", config.scope.as_str()),
            ("state", state_token.as_str()),
        ],
    )
    .map_err(|error| {
        ServiceError::OAuthExchange(format!("failed to build authorize URL: {error}"))
    })?;

    Ok(Json(AuthStartResponse {
        authorize_url: authorize_url.to_string(),
        state: state_token,
    }))
}

#[derive(Debug, Deserialize)]
struct AuthCallbackQuery {
    state: String,
    code: String,
}

#[derive(Debug, Serialize)]
struct AuthCallbackResponse {
    session_id: String,
    token_type: String,
    expires_in: Option<u64>,
}

async fn auth_callback(
    State(state): State<ServiceState>,
    Query(query): Query<AuthCallbackQuery>,
) -> Result<Response, ServiceError> {
    let pending_state = state
        .pending_oauth_states
        .write()
        .await
        .remove(&query.state);
    let Some(pending_state) = pending_state else {
        return Err(ServiceError::InvalidOAuthState);
    };

    let token_set = state.oauth_exchanger.exchange_code(&query.code).await?;
    let session_id = Uuid::new_v4().to_string();
    let response_payload = AuthCallbackResponse {
        session_id: session_id.clone(),
        token_type: token_set.token_type.clone(),
        expires_in: token_set.expires_in,
    };

    state
        .sessions
        .set_token_state(
            &session_id,
            SessionTokenState::from_token_set(token_set, None),
        )
        .await?;

    if let Some(mut redirect_uri) = pending_state.client_redirect_uri {
        {
            let mut pairs = redirect_uri.query_pairs_mut();
            pairs.append_pair("session_id", &response_payload.session_id);
            pairs.append_pair("token_type", &response_payload.token_type);
            if let Some(expires_in) = response_payload.expires_in {
                pairs.append_pair("expires_in", &expires_in.to_string());
            }
        }
        return Ok(Redirect::to(redirect_uri.as_ref()).into_response());
    }

    Ok(Json(response_payload).into_response())
}

async fn auth_logout(
    State(state): State<ServiceState>,
    headers: HeaderMap,
) -> Result<StatusCode, ServiceError> {
    let (session_id, _) = authorize_request(&state, &headers).await?;
    state.stop_voice_workers_for_session(&session_id).await;
    state
        .voice_sessions
        .write()
        .await
        .retain(|_, voice_session| voice_session.owner_session_id != session_id);
    state.stop_gateway_worker(&session_id).await;
    state.sessions.delete_session(&session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_current_user(
    State(state): State<ServiceState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ServiceError> {
    let (_, discord_token) = authorize_request(&state, &headers).await?;
    let user = state.http.get_current_user(&discord_token).await?;
    Ok(Json(
        serde_json::to_value(user).expect("user payload should serialize"),
    ))
}

#[derive(Debug, Deserialize, Default)]
struct GuildsQuery {
    before: Option<String>,
    after: Option<String>,
    limit: Option<u8>,
    with_counts: Option<bool>,
}

async fn get_guilds(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Query(query): Query<GuildsQuery>,
) -> Result<Json<Vec<CurrentUserGuild>>, ServiceError> {
    let (_, discord_token) = authorize_request(&state, &headers).await?;
    let request_query = GetCurrentUserGuildsQuery {
        before: query.before.map(Snowflake),
        after: query.after.map(Snowflake),
        limit: query.limit,
        with_counts: query.with_counts,
    };

    let guilds = state
        .http
        .get_current_user_guilds(&discord_token, request_query)
        .await?;

    Ok(Json(guilds))
}

#[derive(Debug, Deserialize)]
struct GuildPath {
    guild_id: String,
}

async fn get_guild_channels(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<GuildPath>,
) -> Result<Json<Value>, ServiceError> {
    let (_, discord_token) = authorize_request(&state, &headers).await?;
    let channels = state
        .http
        .get_guild_channels(path.guild_id, &discord_token)
        .await?;

    Ok(Json(
        serde_json::to_value(channels).expect("channel payload should serialize"),
    ))
}

#[derive(Debug, Deserialize, Default)]
struct ChannelMessagesQuery {
    around: Option<String>,
    before: Option<String>,
    after: Option<String>,
    limit: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct ChannelPath {
    channel_id: String,
}

#[derive(Debug, Deserialize)]
struct ChannelMessagePath {
    channel_id: String,
    message_id: String,
}

async fn get_channel_messages(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<ChannelPath>,
    Query(query): Query<ChannelMessagesQuery>,
) -> Result<Json<Vec<Message>>, ServiceError> {
    let (_, discord_token) = authorize_request(&state, &headers).await?;
    let request_query = GetChannelMessagesQuery {
        around: query.around.map(Snowflake),
        before: query.before.map(Snowflake),
        after: query.after.map(Snowflake),
        limit: query.limit,
    };

    let messages = state
        .http
        .get_channel_messages(path.channel_id, request_query, &discord_token)
        .await?;

    Ok(Json(messages))
}

async fn post_channel_message(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<ChannelPath>,
    Json(payload): Json<CreateMessageRequest>,
) -> Result<Json<Message>, ServiceError> {
    let (session_id, discord_token) = authorize_request(&state, &headers).await?;
    let message = state
        .http
        .create_message(path.channel_id.clone(), payload, &discord_token)
        .await?;

    state
        .emit_event(RealtimeEnvelope::MessageCreated {
            session_id,
            message: message.clone(),
        })
        .await;

    Ok(Json(message))
}

async fn patch_channel_message(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<ChannelMessagePath>,
    Json(payload): Json<EditMessageRequest>,
) -> Result<Json<Message>, ServiceError> {
    let (session_id, discord_token) = authorize_request(&state, &headers).await?;
    let message = state
        .http
        .edit_message(
            path.channel_id.clone(),
            path.message_id.clone(),
            payload,
            &discord_token,
        )
        .await?;

    state
        .emit_event(RealtimeEnvelope::MessageUpdated {
            session_id,
            message: message.clone(),
        })
        .await;

    Ok(Json(message))
}

async fn delete_channel_message(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<ChannelMessagePath>,
) -> Result<StatusCode, ServiceError> {
    let (session_id, discord_token) = authorize_request(&state, &headers).await?;
    state
        .http
        .delete_message(
            path.channel_id.clone(),
            path.message_id.clone(),
            &discord_token,
        )
        .await?;

    state
        .emit_event(RealtimeEnvelope::MessageDeleted {
            session_id,
            channel_id: path.channel_id,
            message_id: path.message_id,
        })
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct CreateVoiceSessionRequest {
    guild_id: String,
    channel_id: String,
    #[serde(default)]
    connection: Option<CreateVoiceSessionConnection>,
}

#[derive(Debug, Deserialize)]
struct CreateVoiceSessionConnection {
    gateway_url: String,
    user_id: String,
    session_id: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct VoiceSessionPath {
    voice_session_id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateVoiceSpeakingRequest {
    speaking: bool,
}

async fn create_voice_session(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Json(payload): Json<CreateVoiceSessionRequest>,
) -> Result<Json<VoiceSession>, ServiceError> {
    let (session_id, _) = authorize_request(&state, &headers).await?;

    let voice_session = VoiceSession {
        id: Uuid::new_v4().to_string(),
        guild_id: payload.guild_id,
        channel_id: payload.channel_id,
        speaking: false,
    };

    if let Some(connection) = payload.connection {
        let gateway_url = Url::parse(&connection.gateway_url).map_err(|error| {
            ServiceError::VoiceRuntime(format!("invalid voice gateway URL: {error}"))
        })?;
        let runtime_config = VoiceConnectionConfig {
            guild_id: voice_session.guild_id.clone(),
            user_id: connection.user_id,
            session_id: connection.session_id,
            token: connection.token,
        };

        state
            .start_voice_worker(&session_id, &voice_session.id, runtime_config, gateway_url)
            .await?;
    }

    state.voice_sessions.write().await.insert(
        voice_session.id.clone(),
        VoiceSessionState {
            owner_session_id: session_id.clone(),
            session: voice_session.clone(),
        },
    );

    state
        .emit_event(RealtimeEnvelope::VoiceStateChanged {
            session_id: session_id.clone(),
            voice_session_id: voice_session.id.clone(),
            guild_id: voice_session.guild_id.clone(),
            channel_id: voice_session.channel_id.clone(),
            speaking: voice_session.speaking,
        })
        .await;

    Ok(Json(voice_session))
}

async fn delete_voice_session(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<VoiceSessionPath>,
) -> Result<StatusCode, ServiceError> {
    let (session_id, _) = authorize_request(&state, &headers).await?;

    let removed = {
        let mut sessions = state.voice_sessions.write().await;
        match sessions.get(&path.voice_session_id) {
            Some(voice_session) if voice_session.owner_session_id == session_id => {
                sessions.remove(&path.voice_session_id)
            }
            _ => None,
        }
    };

    if removed.is_none() {
        return Err(ServiceError::NotFound);
    }

    state.stop_voice_worker(&path.voice_session_id).await;

    Ok(StatusCode::NO_CONTENT)
}

async fn update_voice_speaking_state(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Path(path): Path<VoiceSessionPath>,
    Json(payload): Json<UpdateVoiceSpeakingRequest>,
) -> Result<Json<VoiceSession>, ServiceError> {
    let (session_id, _) = authorize_request(&state, &headers).await?;

    let mut sessions = state.voice_sessions.write().await;
    let Some(voice_session) = sessions.get_mut(&path.voice_session_id) else {
        return Err(ServiceError::NotFound);
    };
    if voice_session.owner_session_id != session_id {
        return Err(ServiceError::NotFound);
    }

    voice_session.session.speaking = payload.speaking;
    let updated = voice_session.session.clone();
    drop(sessions);

    state
        .emit_event(RealtimeEnvelope::VoiceStateChanged {
            session_id: session_id.clone(),
            voice_session_id: updated.id.clone(),
            guild_id: updated.guild_id.clone(),
            channel_id: updated.channel_id.clone(),
            speaking: updated.speaking,
        })
        .await;

    Ok(Json(updated))
}

async fn stream_events(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ServiceError> {
    let (session_id, token) = authorize_request(&state, &headers).await?;
    state.ensure_gateway_worker(&session_id, &token).await?;
    let events_rx = state.events_tx.subscribe();

    Ok(ws.on_upgrade(move |socket| websocket_loop(socket, events_rx, session_id)))
}

async fn websocket_loop(
    socket: WebSocket,
    mut events_rx: broadcast::Receiver<RealtimeEnvelope>,
    session_id: String,
) {
    let (mut sender, mut receiver) = socket.split();

    let receive_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, WsMessage::Close(_)) {
                break;
            }
        }
    });

    loop {
        match events_rx.recv().await {
            Ok(event) => {
                if !event.is_for_session(&session_id) {
                    continue;
                }

                let payload = match serde_json::to_string(&event) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                if sender.send(WsMessage::Text(payload.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let notification = RealtimeEnvelope::Notification {
                    session_id: Some(session_id.clone()),
                    title: "event_stream_lagged".to_owned(),
                    body: "realtime stream lag detected; some events may have been skipped"
                        .to_owned(),
                };

                if let Ok(payload) = serde_json::to_string(&notification) {
                    let _ = sender.send(WsMessage::Text(payload.into())).await;
                }
            }
        }
    }

    receive_task.abort();
}

#[derive(Debug, Deserialize)]
struct GatewayDeletePayload {
    id: String,
    channel_id: String,
}

#[derive(Debug, Deserialize)]
struct GatewayVoiceStatePayload {
    guild_id: Option<String>,
    channel_id: Option<String>,
    #[serde(default)]
    self_mute: Option<bool>,
    #[serde(default)]
    mute: Option<bool>,
}

fn runtime_event_to_envelope(
    session_id: &str,
    event: &GatewayRuntimeEvent,
) -> Option<RealtimeEnvelope> {
    match event {
        GatewayRuntimeEvent::GatewayEvent(event) => gateway_event_to_envelope(session_id, event),
        GatewayRuntimeEvent::ReconnectScheduled {
            attempt,
            resumable,
            delay,
        } => Some(RealtimeEnvelope::ReconnectScheduled {
            session_id: session_id.to_owned(),
            attempt: *attempt,
            resumable: *resumable,
            delay_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
        }),
        GatewayRuntimeEvent::Shutdown => Some(RealtimeEnvelope::Notification {
            session_id: Some(session_id.to_owned()),
            title: "gateway_shutdown".to_owned(),
            body: "gateway runtime stopped".to_owned(),
        }),
    }
}

fn gateway_event_to_envelope(session_id: &str, event: &GatewayEvent) -> Option<RealtimeEnvelope> {
    match event {
        GatewayEvent::Dispatch {
            event_type, data, ..
        } => match event_type.as_deref() {
            Some("MESSAGE_CREATE") => {
                let message = serde_json::from_value::<Message>(data.clone()).ok()?;
                Some(RealtimeEnvelope::MessageCreated {
                    session_id: session_id.to_owned(),
                    message,
                })
            }
            Some("MESSAGE_UPDATE") => {
                let message = serde_json::from_value::<Message>(data.clone()).ok()?;
                Some(RealtimeEnvelope::MessageUpdated {
                    session_id: session_id.to_owned(),
                    message,
                })
            }
            Some("MESSAGE_DELETE") => {
                let payload = serde_json::from_value::<GatewayDeletePayload>(data.clone()).ok()?;
                Some(RealtimeEnvelope::MessageDeleted {
                    session_id: session_id.to_owned(),
                    channel_id: payload.channel_id,
                    message_id: payload.id,
                })
            }
            Some("VOICE_STATE_UPDATE") => {
                let payload =
                    serde_json::from_value::<GatewayVoiceStatePayload>(data.clone()).ok()?;
                let guild_id = payload.guild_id?;
                let channel_id = payload.channel_id?;
                let speaking =
                    !payload.self_mute.unwrap_or(false) && !payload.mute.unwrap_or(false);

                Some(RealtimeEnvelope::VoiceStateChanged {
                    session_id: session_id.to_owned(),
                    voice_session_id: "gateway".to_owned(),
                    guild_id,
                    channel_id,
                    speaking,
                })
            }
            Some(event_type) => Some(RealtimeEnvelope::GatewayDispatch {
                session_id: session_id.to_owned(),
                event_type: event_type.to_owned(),
                data: data.clone(),
            }),
            None => Some(RealtimeEnvelope::GatewayDispatch {
                session_id: session_id.to_owned(),
                event_type: "dispatch".to_owned(),
                data: data.clone(),
            }),
        },
        _ => None,
    }
}

fn voice_runtime_event_to_envelope(
    session_id: &str,
    voice_session_id: &str,
    event: &VoiceRuntimeEvent,
) -> Option<RealtimeEnvelope> {
    match event {
        VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Hello {
            heartbeat_interval_ms,
        }) => Some(RealtimeEnvelope::GatewayDispatch {
            session_id: session_id.to_owned(),
            event_type: "VOICE_HELLO".to_owned(),
            data: json!({
                "voice_session_id": voice_session_id,
                "heartbeat_interval_ms": heartbeat_interval_ms,
            }),
        }),
        VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Ready {
            ip,
            port,
            ssrc,
            modes,
        }) => Some(RealtimeEnvelope::GatewayDispatch {
            session_id: session_id.to_owned(),
            event_type: "VOICE_READY".to_owned(),
            data: json!({
                "voice_session_id": voice_session_id,
                "ip": ip,
                "port": port,
                "ssrc": ssrc,
                "modes": modes,
            }),
        }),
        VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::SessionDescription {
            mode,
            secret_key,
        }) => Some(RealtimeEnvelope::GatewayDispatch {
            session_id: session_id.to_owned(),
            event_type: "VOICE_SESSION_DESCRIPTION".to_owned(),
            data: json!({
                "voice_session_id": voice_session_id,
                "mode": mode,
                "secret_key": secret_key,
            }),
        }),
        VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::HeartbeatAck) => {
            Some(RealtimeEnvelope::GatewayDispatch {
                session_id: session_id.to_owned(),
                event_type: "VOICE_HEARTBEAT_ACK".to_owned(),
                data: json!({
                    "voice_session_id": voice_session_id,
                }),
            })
        }
        VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Unknown(payload)) => {
            Some(RealtimeEnvelope::GatewayDispatch {
                session_id: session_id.to_owned(),
                event_type: "VOICE_UNKNOWN".to_owned(),
                data: json!({
                    "voice_session_id": voice_session_id,
                    "payload": payload,
                }),
            })
        }
        VoiceRuntimeEvent::ReconnectScheduled { attempt, delay } => {
            Some(RealtimeEnvelope::ReconnectScheduled {
                session_id: session_id.to_owned(),
                attempt: *attempt,
                resumable: false,
                delay_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
            })
        }
        VoiceRuntimeEvent::Shutdown => Some(RealtimeEnvelope::Notification {
            session_id: Some(session_id.to_owned()),
            title: "voice_runtime_shutdown".to_owned(),
            body: format!("voice runtime stopped for session {voice_session_id}"),
        }),
    }
}

async fn authorize_request(
    state: &ServiceState,
    headers: &HeaderMap,
) -> Result<(String, String), ServiceError> {
    let header = headers
        .get(AUTHORIZATION)
        .ok_or(ServiceError::MissingAuthorization)?;
    let header = header
        .to_str()
        .map_err(|_| ServiceError::InvalidAuthorization)?;
    let session_id = header
        .strip_prefix("Bearer ")
        .ok_or(ServiceError::InvalidAuthorization)?
        .trim();

    if session_id.is_empty() {
        return Err(ServiceError::InvalidAuthorization);
    }

    let token = state.resolve_discord_token(session_id).await?;

    Ok((session_id.to_owned(), token))
}

fn current_unix_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
