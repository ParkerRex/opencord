use discord_api_types::routes::{
    CreateChannelMessage, GetChannelMessages, GetCurrentUser, GetCurrentUserGuilds, GetGatewayBot,
    GetGuildChannels, JsonBodyRoute, QueryRoute, Route,
};
use discord_api_types::{
    Channel, CreateMessageRequest, CurrentUserGuild, GatewayBotInfo, GetChannelMessagesQuery,
    GetCurrentUserGuildsQuery, Message, Snowflake, User,
};
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, warn};
use url::Url;

const MAX_RATE_LIMIT_RETRIES: usize = 5;
const DEFAULT_RATE_LIMIT_RETRY_AFTER: Duration = Duration::from_secs(1);
const MIN_RATE_LIMIT_RETRY_AFTER: Duration = Duration::from_millis(50);
const MAX_RATE_LIMIT_RETRY_AFTER: Duration = Duration::from_secs(30);

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
        let response = self
            .send_with_rate_limit_retry("GET", path, || {
                let mut req = self.client.get(self.endpoint(path)?);
                if let Some(token) = bearer_token {
                    req = req.bearer_auth(token);
                }
                Ok(req)
            })
            .await?;

        decode_response(response).await
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
        let response = self
            .send_with_rate_limit_retry("GET", path, || {
                let mut req = self.client.get(self.endpoint(path)?).query(query);
                if let Some(token) = bearer_token {
                    req = req.bearer_auth(token);
                }
                Ok(req)
            })
            .await?;

        decode_response(response).await
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
        let response = self
            .send_with_rate_limit_retry("POST", path, || {
                let mut req = self.client.post(self.endpoint(path)?).json(payload);
                if let Some(token) = bearer_token {
                    req = req.bearer_auth(token);
                }
                Ok(req)
            })
            .await?;

        decode_response(response).await
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

    async fn send_with_rate_limit_retry<F>(
        &self,
        method: &'static str,
        path: &str,
        build_request: F,
    ) -> Result<reqwest::Response, HttpError>
    where
        F: Fn() -> Result<reqwest::RequestBuilder, HttpError>,
    {
        for attempt in 1..=(MAX_RATE_LIMIT_RETRIES + 1) {
            let response = build_request()?.send().await?;
            let status = response.status();

            if status != StatusCode::TOO_MANY_REQUESTS {
                debug!(
                    http_method = method,
                    http_path = path,
                    attempt,
                    status = status.as_u16(),
                    "discord http request completed"
                );
                return Ok(response);
            }

            let headers = response.headers().clone();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<no body>"));

            if attempt > MAX_RATE_LIMIT_RETRIES {
                warn!(
                    http_method = method,
                    http_path = path,
                    attempt,
                    status = status.as_u16(),
                    "discord http rate-limit retries exhausted"
                );
                return Err(HttpError::Status { status, body });
            }

            let retry_after = parse_rate_limit_retry_after(&headers, &body)
                .unwrap_or(DEFAULT_RATE_LIMIT_RETRY_AFTER);
            let retry_after_ms = retry_after.as_millis().min(u128::from(u64::MAX)) as u64;

            warn!(
                http_method = method,
                http_path = path,
                attempt,
                retry_after_ms,
                "discord http request rate-limited, retrying"
            );
            sleep(retry_after).await;
        }

        unreachable!("retry loop returns before exhausting")
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

fn parse_rate_limit_retry_after(headers: &HeaderMap, body: &str) -> Option<Duration> {
    parse_header_retry_after(headers.get(RETRY_AFTER))
        .or_else(|| {
            parse_header_retry_after(
                headers.get(HeaderName::from_static("x-ratelimit-reset-after")),
            )
        })
        .or_else(|| parse_body_retry_after(body))
        .map(clamp_retry_after)
}

fn parse_header_retry_after(value: Option<&HeaderValue>) -> Option<Duration> {
    let value = value?;
    let value = value.to_str().ok()?;
    parse_retry_after_from_str(value)
}

fn parse_body_retry_after(body: &str) -> Option<Duration> {
    #[derive(Debug, Deserialize)]
    struct RateLimitBody {
        retry_after: Option<f64>,
    }

    let parsed: RateLimitBody = serde_json::from_str(body).ok()?;
    parse_retry_after_from_secs(parsed.retry_after?)
}

fn parse_retry_after_from_str(raw: &str) -> Option<Duration> {
    let seconds = raw.trim().parse::<f64>().ok()?;
    parse_retry_after_from_secs(seconds)
}

fn parse_retry_after_from_secs(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }

    Some(Duration::from_secs_f64(seconds))
}

fn clamp_retry_after(delay: Duration) -> Duration {
    delay.clamp(MIN_RATE_LIMIT_RETRY_AFTER, MAX_RATE_LIMIT_RETRY_AFTER)
}

#[cfg(test)]
mod tests {
    use super::{DiscordHttpClient, HttpError};
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use url::Url;

    fn client(base: &str) -> DiscordHttpClient {
        DiscordHttpClient::new(Url::parse(base).expect("valid base URL"))
    }

    fn spawn_scripted_server(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");

        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept connection");
                let mut request_buffer = [0_u8; 4096];
                let _ = stream.read(&mut request_buffer);
                stream
                    .write_all(response.as_bytes())
                    .expect("write scripted response");
                stream.flush().expect("flush scripted response");
            }
        });

        (format!("http://{addr}"), handle)
    }

    fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut response = format!("HTTP/1.1 {status}\r\n");
        for (name, value) in headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));
        response
    }

    #[test]
    fn endpoint_builds_expected_url() {
        let client = client("https://discord.com/api/v10/");
        let url = client
            .endpoint("/users/@me")
            .expect("endpoint should be valid");
        assert_eq!(url.as_str(), "https://discord.com/api/v10/users/@me");
    }

    #[test]
    fn endpoint_rejects_empty_path() {
        let client = client("https://discord.com/api/v10/");
        let error = client.endpoint("/").expect_err("expected invalid path");

        assert!(matches!(error, HttpError::InvalidPath(_)));
    }

    #[tokio::test]
    async fn retries_rate_limited_request_and_returns_success() {
        let rate_limited = http_response(
            "429 Too Many Requests",
            &[
                ("Content-Type", "application/json"),
                ("Retry-After", "0.01"),
            ],
            r#"{"message":"rate limited","retry_after":0.01}"#,
        );
        let success = http_response(
            "200 OK",
            &[("Content-Type", "application/json")],
            r#"{"ok":true}"#,
        );

        let (base_url, handle) = spawn_scripted_server(vec![rate_limited, success]);
        let client = client(&base_url);

        let response = client
            .get_json::<Value>("users/@me", None)
            .await
            .expect("request should eventually succeed");

        assert_eq!(response.get("ok").and_then(Value::as_bool), Some(true));
        handle.join().expect("server thread should exit cleanly");
    }

    #[tokio::test]
    async fn returns_status_error_when_rate_limit_retries_are_exhausted() {
        let response = http_response(
            "429 Too Many Requests",
            &[
                ("Content-Type", "application/json"),
                ("Retry-After", "0.001"),
            ],
            r#"{"message":"rate limited","retry_after":0.001}"#,
        );
        let (base_url, handle) = spawn_scripted_server(vec![response; 6]);
        let client = client(&base_url);

        let error = client
            .get_json::<Value>("users/@me", None)
            .await
            .expect_err("request should fail after retry budget");

        match error {
            HttpError::Status { status, .. } => {
                assert_eq!(status, reqwest::StatusCode::TOO_MANY_REQUESTS)
            }
            other => panic!("unexpected error: {other}"),
        }

        handle.join().expect("server thread should exit cleanly");
    }
}
