use discord_api_types::{GatewayBotInfo, User};
use discord_auth::{AuthError, TokenProvider};
use discord_gateway::{
    GatewayClient, GatewaySession, GatewayStateMachine, GatewayStateMachineConfig,
};
use discord_http::{DiscordHttpClient, HttpError};
use discord_storage::{
    GatewaySessionRecord, KeyValueStore, StorageError, clear_gateway_session, load_gateway_session,
    save_gateway_session,
};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tracing::{debug, warn};
use url::Url;

const DEFAULT_GATEWAY_QUERY: &str = "v=10&encoding=json";

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct GatewayStartup {
    pub gateway_bot: GatewayBotInfo,
    pub gateway_client: GatewayClient,
    pub state_machine: GatewayStateMachine,
    pub restored_session: Option<GatewaySessionRecord>,
}

#[derive(Debug)]
pub struct DiscordClient<TP, ST>
where
    TP: TokenProvider,
    ST: KeyValueStore,
{
    http: DiscordHttpClient,
    token_provider: TP,
    storage: ST,
}

pub struct SessionManager<'a, TP, ST>
where
    TP: TokenProvider,
    ST: KeyValueStore,
{
    client: &'a DiscordClient<TP, ST>,
}

impl<TP, ST> DiscordClient<TP, ST>
where
    TP: TokenProvider,
    ST: KeyValueStore,
{
    pub fn new(http: DiscordHttpClient, token_provider: TP, storage: ST) -> Self {
        Self {
            http,
            token_provider,
            storage,
        }
    }

    pub fn http(&self) -> &DiscordHttpClient {
        &self.http
    }

    pub fn session_manager(&self) -> SessionManager<'_, TP, ST> {
        SessionManager { client: self }
    }

    pub async fn gateway_startup(&self, intents: u64) -> Result<GatewayStartup, ClientError> {
        self.session_manager().bootstrap_gateway(intents).await
    }

    pub async fn set_token(&self, token: String) -> Result<(), ClientError> {
        self.token_provider.set_token(token).await?;
        Ok(())
    }

    pub async fn clear_token(&self) -> Result<(), ClientError> {
        self.token_provider.clear_token().await?;
        Ok(())
    }

    pub async fn token(&self) -> Result<String, ClientError> {
        self.token_provider
            .get_token()
            .await?
            .ok_or(AuthError::MissingToken)
            .map_err(ClientError::from)
    }

    pub async fn current_user(&self) -> Result<User, ClientError> {
        let token = self.token().await?;
        Ok(self.http.get_current_user(&token).await?)
    }

    pub async fn gateway_bot(&self) -> Result<GatewayBotInfo, ClientError> {
        let token = self.token().await?;
        Ok(self.http.get_gateway_bot(&token).await?)
    }

    pub async fn gateway_client(&self) -> Result<GatewayClient, ClientError> {
        let gateway_bot = self.gateway_bot().await?;
        let gateway_url = normalize_gateway_url(Url::parse(&gateway_bot.url)?);
        Ok(GatewayClient::new(gateway_url))
    }

    pub async fn cache_set_json<T: Serialize>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), ClientError> {
        let bytes = serde_json::to_vec(value)?;
        self.storage.set(key, bytes).await?;
        Ok(())
    }

    pub async fn cache_get_json<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, ClientError> {
        let Some(bytes) = self.storage.get(key).await? else {
            return Ok(None);
        };

        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub async fn cache_delete(&self, key: &str) -> Result<(), ClientError> {
        self.storage.delete(key).await?;
        Ok(())
    }
}

impl<'a, TP, ST> SessionManager<'a, TP, ST>
where
    TP: TokenProvider,
    ST: KeyValueStore,
{
    pub async fn bootstrap_gateway(&self, intents: u64) -> Result<GatewayStartup, ClientError> {
        let token = self.client.token().await?;
        let gateway_bot = self.client.http.get_gateway_bot(&token).await?;

        let mut restored_session = match load_gateway_session(&self.client.storage).await {
            Ok(session) => session,
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to load persisted gateway session; clearing stale value"
                );
                let _ = clear_gateway_session(&self.client.storage).await;
                None
            }
        };

        let default_gateway_url = Url::parse(&gateway_bot.url)?;
        let selected_gateway_url = match restored_session.as_ref() {
            Some(session) => match Url::parse(&session.resume_url) {
                Ok(url) => {
                    debug!(resume_url = %url, "using persisted gateway resume URL");
                    url
                }
                Err(error) => {
                    warn!(
                        error = %error,
                        "invalid persisted gateway resume URL; dropping persisted session"
                    );
                    let _ = clear_gateway_session(&self.client.storage).await;
                    restored_session = None;
                    default_gateway_url.clone()
                }
            },
            None => default_gateway_url.clone(),
        };

        let gateway_url = normalize_gateway_url(selected_gateway_url);
        let mut state_machine =
            GatewayStateMachine::new(GatewayStateMachineConfig::new(token, intents));

        if let Some(session) = restored_session.as_ref() {
            state_machine.restore_session_with_resume_url(
                GatewaySession {
                    session_id: session.session_id.clone(),
                    sequence: session.seq,
                },
                session.resume_url.clone(),
            );
        }

        Ok(GatewayStartup {
            gateway_bot,
            gateway_client: GatewayClient::new(gateway_url),
            state_machine,
            restored_session,
        })
    }

    pub async fn load_gateway_session(&self) -> Result<Option<GatewaySessionRecord>, ClientError> {
        Ok(load_gateway_session(&self.client.storage).await?)
    }

    pub async fn save_gateway_session(
        &self,
        session: &GatewaySessionRecord,
    ) -> Result<(), ClientError> {
        save_gateway_session(&self.client.storage, session).await?;
        Ok(())
    }

    pub async fn clear_gateway_session(&self) -> Result<(), ClientError> {
        clear_gateway_session(&self.client.storage).await?;
        Ok(())
    }

    pub async fn persist_from_state_machine(
        &self,
        state_machine: &GatewayStateMachine,
        fallback_resume_url: &Url,
    ) -> Result<(), ClientError> {
        let Some(session) = state_machine.session() else {
            debug!("state machine has no resumable session; clearing persisted gateway session");
            self.clear_gateway_session().await?;
            return Ok(());
        };

        let fallback_resume_url = normalize_gateway_url(fallback_resume_url.clone());
        let resume_url = if let Some(stored_resume_url) = state_machine.resume_url() {
            match Url::parse(stored_resume_url) {
                Ok(url) => normalize_gateway_url(url).to_string(),
                Err(error) => {
                    warn!(
                        error = %error,
                        fallback = %fallback_resume_url,
                        "invalid state-machine resume URL; using fallback"
                    );
                    fallback_resume_url.to_string()
                }
            }
        } else {
            fallback_resume_url.to_string()
        };

        let record = GatewaySessionRecord {
            session_id: session.session_id,
            seq: session.sequence,
            resume_url,
        };

        self.save_gateway_session(&record).await
    }
}

fn normalize_gateway_url(mut url: Url) -> Url {
    if (url.scheme() == "ws" || url.scheme() == "wss") && url.query().is_none() {
        url.set_query(Some(DEFAULT_GATEWAY_QUERY));
    }

    url
}

#[cfg(test)]
mod tests {
    use super::DiscordClient;
    use discord_auth::MemoryTokenProvider;
    use discord_gateway::{GatewaySession, GatewayStateMachine, GatewayStateMachineConfig};
    use discord_http::DiscordHttpClient;
    use discord_storage::{GatewaySessionRecord, MemoryStore};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use url::Url;

    fn spawn_gateway_bot_server(gateway_url: &str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("read listener address");

        let body = format!(
            "{{\"url\":\"{gateway_url}\",\"shards\":1,\"session_start_limit\":{{\"total\":1000,\"remaining\":999,\"reset_after\":1234,\"max_concurrency\":1}}}}"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request_buffer = [0_u8; 4096];
            let _ = stream.read(&mut request_buffer);
            stream
                .write_all(response.as_bytes())
                .expect("write HTTP response");
            stream.flush().expect("flush HTTP response");
        });

        (format!("http://{addr}"), handle)
    }

    fn build_client(
        base_url: &str,
        store: MemoryStore,
    ) -> DiscordClient<MemoryTokenProvider, MemoryStore> {
        DiscordClient::new(
            DiscordHttpClient::new(Url::parse(base_url).expect("valid base URL")),
            MemoryTokenProvider::from_token("token-123"),
            store,
        )
    }

    #[tokio::test]
    async fn bootstrap_gateway_restores_session_and_resume_url() {
        let store = MemoryStore::new();
        let (base_url, server) = spawn_gateway_bot_server("wss://gateway.discord.gg/");
        let client = build_client(&base_url, store);

        client
            .session_manager()
            .save_gateway_session(&GatewaySessionRecord {
                session_id: "session-abc".to_owned(),
                seq: 42,
                resume_url: "wss://resume.discord.gg".to_owned(),
            })
            .await
            .expect("persist session");

        let startup = client
            .session_manager()
            .bootstrap_gateway(513)
            .await
            .expect("bootstrap should succeed");

        assert_eq!(
            startup.gateway_client.gateway_url().as_str(),
            "wss://resume.discord.gg/?v=10&encoding=json"
        );
        assert!(startup.state_machine.can_resume());
        assert_eq!(
            startup.state_machine.resume_url(),
            Some("wss://resume.discord.gg")
        );
        assert_eq!(
            startup
                .restored_session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("session-abc")
        );

        server.join().expect("server thread should exit");
    }

    #[tokio::test]
    async fn persist_from_state_machine_writes_and_clears_session_data() {
        let store = MemoryStore::new();
        let client = build_client("https://discord.com/api/v10", store);
        let manager = client.session_manager();

        let mut state_machine =
            GatewayStateMachine::new(GatewayStateMachineConfig::new("token-123".to_owned(), 513));
        state_machine.restore_session_with_resume_url(
            GatewaySession {
                session_id: "session-xyz".to_owned(),
                sequence: 9,
            },
            "wss://resume.discord.gg".to_owned(),
        );

        manager
            .persist_from_state_machine(
                &state_machine,
                &Url::parse("wss://gateway.discord.gg").expect("valid fallback URL"),
            )
            .await
            .expect("persist from state machine");

        let saved = manager
            .load_gateway_session()
            .await
            .expect("load persisted session")
            .expect("session should be saved");

        assert_eq!(saved.session_id, "session-xyz");
        assert_eq!(saved.seq, 9);
        assert_eq!(
            saved.resume_url,
            "wss://resume.discord.gg/?v=10&encoding=json"
        );

        let empty_state_machine =
            GatewayStateMachine::new(GatewayStateMachineConfig::new("token-123".to_owned(), 513));
        manager
            .persist_from_state_machine(
                &empty_state_machine,
                &Url::parse("wss://gateway.discord.gg").expect("valid fallback URL"),
            )
            .await
            .expect("clear session when state is empty");

        assert!(
            manager
                .load_gateway_session()
                .await
                .expect("load after clear")
                .is_none()
        );
    }
}
