use discord_api_types::routes::{
    CreateChannelMessage, GetChannelMessages, GetCurrentUser, GetCurrentUserGuilds, GetGatewayBot,
    GetGuildChannels, JsonBodyRoute, QueryRoute, Route,
};
use discord_api_types::{
    Channel, CreateMessageRequest, CurrentUserGuild, GatewayBotInfo, GetChannelMessagesQuery,
    GetCurrentUserGuildsQuery, Message, Snowflake, User,
};
use reqwest::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid URL path: {0}")]
    InvalidPath(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),
    #[error("request failed with status {status}: {body}")]
    Status { status: StatusCode, body: String },
}

#[derive(Clone, Debug)]
pub struct DiscordHttpClient {
    base_url: Url,
    client: reqwest::Client,
}

impl DiscordHttpClient {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(base_url: Url, client: reqwest::Client) -> Self {
        Self { base_url, client }
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn endpoint(&self, path: &str) -> Result<Url, HttpError> {
        let normalized = path.trim_start_matches('/');
        if normalized.is_empty() {
            return Err(HttpError::InvalidPath(path.to_owned()));
        }

        let base = self.base_url.as_str().trim_end_matches('/');
        Ok(Url::parse(&format!("{base}/{normalized}"))?)
    }

    pub async fn get_json<T>(&self, path: &str, bearer_token: Option<&str>) -> Result<T, HttpError>
    where
        T: DeserializeOwned,
    {
        let mut req = self.client.get(self.endpoint(path)?);
        if let Some(token) = bearer_token {
            req = req.bearer_auth(token);
        }
        decode_response(req.send().await?).await
    }

    pub async fn get_json_with_query<Q, T>(
        &self,
        path: &str,
        query: &Q,
        bearer_token: Option<&str>,
    ) -> Result<T, HttpError>
    where
        Q: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let mut req = self.client.get(self.endpoint(path)?).query(query);
        if let Some(token) = bearer_token {
            req = req.bearer_auth(token);
        }
        decode_response(req.send().await?).await
    }

    pub async fn post_json<B, T>(
        &self,
        path: &str,
        payload: &B,
        bearer_token: Option<&str>,
    ) -> Result<T, HttpError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let mut req = self.client.post(self.endpoint(path)?).json(payload);
        if let Some(token) = bearer_token {
            req = req.bearer_auth(token);
        }
        decode_response(req.send().await?).await
    }

    pub async fn get_route<R>(
        &self,
        route: &R,
        bearer_token: Option<&str>,
    ) -> Result<R::Response, HttpError>
    where
        R: Route,
        R::Response: DeserializeOwned,
    {
        self.get_json(&route.path(), bearer_token).await
    }

    pub async fn get_query_route<R>(
        &self,
        route: &R,
        bearer_token: Option<&str>,
    ) -> Result<R::Response, HttpError>
    where
        R: QueryRoute,
        R::Query: Serialize,
        R::Response: DeserializeOwned,
    {
        self.get_json_with_query(&route.path(), route.query(), bearer_token)
            .await
    }

    pub async fn post_route<R>(
        &self,
        route: &R,
        bearer_token: Option<&str>,
    ) -> Result<R::Response, HttpError>
    where
        R: JsonBodyRoute,
        R::Body: Serialize,
        R::Response: DeserializeOwned,
    {
        self.post_json(&route.path(), route.body(), bearer_token)
            .await
    }

    pub async fn get_current_user(&self, bearer_token: &str) -> Result<User, HttpError> {
        self.get_route(&GetCurrentUser, Some(bearer_token)).await
    }

    pub async fn get_gateway_bot(&self, bearer_token: &str) -> Result<GatewayBotInfo, HttpError> {
        self.get_route(&GetGatewayBot, Some(bearer_token)).await
    }

    pub async fn get_current_user_guilds(
        &self,
        bearer_token: &str,
        query: GetCurrentUserGuildsQuery,
    ) -> Result<Vec<CurrentUserGuild>, HttpError> {
        let route = GetCurrentUserGuilds { query };
        self.get_query_route(&route, Some(bearer_token)).await
    }

    pub async fn get_guild_channels(
        &self,
        guild_id: impl Into<Snowflake>,
        bearer_token: &str,
    ) -> Result<Vec<Channel>, HttpError> {
        let route = GetGuildChannels {
            guild_id: guild_id.into(),
        };
        self.get_route(&route, Some(bearer_token)).await
    }

    pub async fn get_channel_messages(
        &self,
        channel_id: impl Into<Snowflake>,
        query: GetChannelMessagesQuery,
        bearer_token: &str,
    ) -> Result<Vec<Message>, HttpError> {
        let route = GetChannelMessages {
            channel_id: channel_id.into(),
            query,
        };
        self.get_query_route(&route, Some(bearer_token)).await
    }

    pub async fn create_message(
        &self,
        channel_id: impl Into<Snowflake>,
        payload: CreateMessageRequest,
        bearer_token: &str,
    ) -> Result<Message, HttpError> {
        let route = CreateChannelMessage {
            channel_id: channel_id.into(),
            body: payload,
        };
        self.post_route(&route, Some(bearer_token)).await
    }
}

async fn decode_response<T>(response: reqwest::Response) -> Result<T, HttpError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if status.is_success() {
        return Ok(response.json::<T>().await?);
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|_| String::from("<no body>"));
    Err(HttpError::Status { status, body })
}
