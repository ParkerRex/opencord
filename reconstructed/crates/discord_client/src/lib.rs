use discord_api_types::{GatewayBotInfo, User};
use discord_auth::{AuthError, TokenProvider};
use discord_gateway::GatewayClient;
use discord_http::{DiscordHttpClient, HttpError};
use discord_storage::{KeyValueStore, StorageError};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

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
        let mut url = Url::parse(&gateway_bot.url)?;

        if url.scheme() == "ws" || url.scheme() == "wss" {
            if url.query().is_none() {
                url.set_query(Some("v=10&encoding=json"));
            }
        }

        Ok(GatewayClient::new(url))
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
