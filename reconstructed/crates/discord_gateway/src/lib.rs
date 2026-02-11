use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid gateway URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("invalid gateway payload: {0}")]
    Payload(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct GatewayClient {
    gateway_url: Url,
}

impl GatewayClient {
    pub fn new(gateway_url: Url) -> Self {
        Self { gateway_url }
    }

    pub fn gateway_url(&self) -> &Url {
        &self.gateway_url
    }

    pub async fn connect_once(&self) -> Result<(), GatewayError> {
        let (mut socket, _) = connect_async(self.gateway_url.as_str()).await?;
        socket.close(None).await?;
        Ok(())
    }

    pub async fn read_one_event(&self) -> Result<Option<GatewayEvent>, GatewayError> {
        let (mut socket, _) = connect_async(self.gateway_url.as_str()).await?;
        let next = socket.next().await.transpose()?;
        socket.close(None).await?;

        match next {
            Some(Message::Text(text)) => Ok(Some(parse_event(&text)?)),
            Some(Message::Binary(_)) => Ok(Some(GatewayEvent::NonTextFrame)),
            Some(_) => Ok(Some(GatewayEvent::Unknown(Value::Null))),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayIdentifyProperties {
    pub os: String,
    pub browser: String,
    pub device: String,
}

impl Default for GatewayIdentifyProperties {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_owned(),
            browser: "opencord-rust-client".to_owned(),
            device: "opencord-rust-client".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayIdentifyPayload {
    pub token: String,
    pub intents: u64,
    pub properties: GatewayIdentifyProperties,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayCommand<T> {
    pub op: u8,
    pub d: T,
}

pub fn identify_command(payload: GatewayIdentifyPayload) -> GatewayCommand<GatewayIdentifyPayload> {
    GatewayCommand { op: 2, d: payload }
}

#[derive(Debug, Clone)]
pub enum GatewayEvent {
    Dispatch {
        event_type: Option<String>,
        sequence: Option<u64>,
        data: Value,
    },
    Hello(Value),
    HeartbeatAck,
    Reconnect,
    InvalidSession,
    NonTextFrame,
    Unknown(Value),
}

pub fn parse_event(payload: &str) -> Result<GatewayEvent, GatewayError> {
    let body: Value = serde_json::from_str(payload)?;

    let op = body.get("op").and_then(|v| v.as_u64()).unwrap_or_default();
    let data = body.get("d").cloned().unwrap_or(Value::Null);

    let event = match op {
        0 => GatewayEvent::Dispatch {
            event_type: body
                .get("t")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            sequence: body.get("s").and_then(|v| v.as_u64()),
            data,
        },
        10 => GatewayEvent::Hello(data),
        11 => GatewayEvent::HeartbeatAck,
        7 => GatewayEvent::Reconnect,
        9 => GatewayEvent::InvalidSession,
        _ => GatewayEvent::Unknown(body),
    };

    Ok(event)
}
