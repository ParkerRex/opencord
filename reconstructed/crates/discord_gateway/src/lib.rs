use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, trace, warn};
use url::Url;

mod runtime;
pub use runtime::{GatewayRuntimeEvent, GatewayRuntimeOptions};

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
        debug!(gateway_url = %self.gateway_url, "connecting to gateway");
        let (mut socket, _) = connect_async(self.gateway_url.as_str()).await?;
        socket.close(None).await?;
        debug!(gateway_url = %self.gateway_url, "gateway connection closed");
        Ok(())
    }

    pub async fn read_one_event(&self) -> Result<Option<GatewayEvent>, GatewayError> {
        debug!(gateway_url = %self.gateway_url, "reading single gateway event");
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayIdentifyPayload {
    pub token: String,
    pub intents: u64,
    pub properties: GatewayIdentifyProperties,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GatewayCommand<T> {
    pub op: u8,
    pub d: T,
}

pub fn identify_command(payload: GatewayIdentifyPayload) -> GatewayCommand<GatewayIdentifyPayload> {
    GatewayCommand { op: 2, d: payload }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayResumePayload {
    pub token: String,
    pub session_id: String,
    pub seq: u64,
}

pub fn resume_command(payload: GatewayResumePayload) -> GatewayCommand<GatewayResumePayload> {
    GatewayCommand { op: 6, d: payload }
}

pub fn heartbeat_command(last_sequence: Option<u64>) -> GatewayCommand<Option<u64>> {
    GatewayCommand {
        op: 1,
        d: last_sequence,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayHelloPayload {
    pub heartbeat_interval: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySession {
    pub session_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayStateMachineConfig {
    pub token: String,
    pub intents: u64,
    pub identify_properties: GatewayIdentifyProperties,
}

impl GatewayStateMachineConfig {
    pub fn new(token: String, intents: u64) -> Self {
        Self {
            token,
            intents,
            identify_properties: GatewayIdentifyProperties::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayConnectionState {
    Disconnected,
    AwaitingHello,
    Handshaking,
    Connected,
    ReconnectRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayStateAction {
    ConfigureHeartbeat { interval: Duration },
    SendIdentify(GatewayCommand<GatewayIdentifyPayload>),
    SendResume(GatewayCommand<GatewayResumePayload>),
    SendHeartbeat(GatewayCommand<Option<u64>>),
    Reconnect { resumable: bool },
}

#[derive(Debug, Clone)]
pub struct GatewayStateMachine {
    config: GatewayStateMachineConfig,
    state: GatewayConnectionState,
    heartbeat_interval: Option<Duration>,
    heartbeat_ack_pending: bool,
    session_id: Option<String>,
    last_sequence: Option<u64>,
    resume_url: Option<String>,
}

impl GatewayStateMachine {
    pub fn new(config: GatewayStateMachineConfig) -> Self {
        Self {
            config,
            state: GatewayConnectionState::Disconnected,
            heartbeat_interval: None,
            heartbeat_ack_pending: false,
            session_id: None,
            last_sequence: None,
            resume_url: None,
        }
    }

    pub fn state(&self) -> GatewayConnectionState {
        self.state
    }

    pub fn heartbeat_interval(&self) -> Option<Duration> {
        self.heartbeat_interval
    }

    pub fn session(&self) -> Option<GatewaySession> {
        match (self.session_id.as_ref(), self.last_sequence) {
            (Some(session_id), Some(sequence)) => Some(GatewaySession {
                session_id: session_id.clone(),
                sequence,
            }),
            _ => None,
        }
    }

    pub fn restore_session(&mut self, session: GatewaySession) {
        self.session_id = Some(session.session_id);
        self.last_sequence = Some(session.sequence);
        debug!(can_resume = self.can_resume(), "restored gateway session");
    }

    pub fn restore_session_with_resume_url(&mut self, session: GatewaySession, resume_url: String) {
        self.restore_session(session);
        self.set_resume_url(resume_url);
    }

    pub fn clear_session(&mut self) {
        self.session_id = None;
        self.last_sequence = None;
        self.resume_url = None;
        debug!("cleared gateway session");
    }

    pub fn resume_url(&self) -> Option<&str> {
        self.resume_url.as_deref()
    }

    pub fn set_resume_url(&mut self, resume_url: String) {
        if resume_url.trim().is_empty() {
            warn!("ignored empty gateway resume URL");
            return;
        }

        self.resume_url = Some(resume_url);
    }

    pub fn on_connect(&mut self) {
        self.state = GatewayConnectionState::AwaitingHello;
        self.heartbeat_interval = None;
        self.heartbeat_ack_pending = false;
        debug!(state = ?self.state, can_resume = self.can_resume(), "gateway connected");
    }

    pub fn on_disconnect(&mut self) {
        self.state = GatewayConnectionState::Disconnected;
        self.heartbeat_interval = None;
        self.heartbeat_ack_pending = false;
        debug!(state = ?self.state, can_resume = self.can_resume(), "gateway disconnected");
    }

    pub fn can_resume(&self) -> bool {
        self.session_id.is_some() && self.last_sequence.is_some()
    }

    pub fn on_heartbeat_tick(&mut self) -> GatewayStateAction {
        if self.heartbeat_ack_pending {
            self.state = GatewayConnectionState::ReconnectRequested;
            warn!(
                state = ?self.state,
                can_resume = self.can_resume(),
                "heartbeat ACK timeout; requesting reconnect"
            );
            return GatewayStateAction::Reconnect {
                resumable: self.can_resume(),
            };
        }

        self.heartbeat_ack_pending = true;
        trace!(last_sequence = self.last_sequence, "sending heartbeat");
        GatewayStateAction::SendHeartbeat(heartbeat_command(self.last_sequence))
    }

    pub fn on_event(&mut self, event: &GatewayEvent) -> Vec<GatewayStateAction> {
        if let GatewayEvent::Dispatch { sequence, .. } = event {
            if let Some(seq) = sequence {
                self.last_sequence = Some(*seq);
            }
        }

        trace!(event = ?event, state = ?self.state, "processing gateway event");

        match event {
            GatewayEvent::Hello(hello) => self.on_hello(hello),
            GatewayEvent::HeartbeatRequest => vec![self.send_heartbeat()],
            GatewayEvent::HeartbeatAck => {
                self.heartbeat_ack_pending = false;
                trace!("received heartbeat ACK");
                Vec::new()
            }
            GatewayEvent::Dispatch {
                event_type, data, ..
            } => {
                self.handle_dispatch(event_type.as_deref(), data);
                Vec::new()
            }
            GatewayEvent::Reconnect => {
                self.state = GatewayConnectionState::ReconnectRequested;
                warn!(
                    can_resume = self.can_resume(),
                    "gateway requested reconnect"
                );
                vec![GatewayStateAction::Reconnect {
                    resumable: self.can_resume(),
                }]
            }
            GatewayEvent::InvalidSession { resumable } => {
                self.state = GatewayConnectionState::ReconnectRequested;
                self.heartbeat_ack_pending = false;

                if !resumable {
                    self.clear_session();
                }

                warn!(
                    resumable = *resumable,
                    can_resume = self.can_resume(),
                    "gateway invalidated session"
                );

                vec![GatewayStateAction::Reconnect {
                    resumable: *resumable && self.can_resume(),
                }]
            }
            GatewayEvent::NonTextFrame | GatewayEvent::Unknown(_) => Vec::new(),
        }
    }

    fn on_hello(&mut self, hello: &GatewayHelloPayload) -> Vec<GatewayStateAction> {
        self.state = GatewayConnectionState::Handshaking;
        self.heartbeat_ack_pending = false;

        let interval = Duration::from_millis(hello.heartbeat_interval);
        self.heartbeat_interval = Some(interval);
        debug!(
            heartbeat_interval_ms = hello.heartbeat_interval,
            "received HELLO"
        );

        let mut actions = vec![GatewayStateAction::ConfigureHeartbeat { interval }];

        if let Some(command) = self.resume_gateway_command() {
            debug!("sending RESUME command");
            actions.push(GatewayStateAction::SendResume(command));
        } else {
            debug!("sending IDENTIFY command");
            actions.push(GatewayStateAction::SendIdentify(
                self.identify_gateway_command(),
            ));
        }

        actions
    }

    fn identify_gateway_command(&self) -> GatewayCommand<GatewayIdentifyPayload> {
        identify_command(GatewayIdentifyPayload {
            token: self.config.token.clone(),
            intents: self.config.intents,
            properties: self.config.identify_properties.clone(),
        })
    }

    fn resume_gateway_command(&self) -> Option<GatewayCommand<GatewayResumePayload>> {
        let session_id = self.session_id.clone()?;
        let seq = self.last_sequence?;

        Some(resume_command(GatewayResumePayload {
            token: self.config.token.clone(),
            session_id,
            seq,
        }))
    }

    fn send_heartbeat(&mut self) -> GatewayStateAction {
        self.heartbeat_ack_pending = true;
        GatewayStateAction::SendHeartbeat(heartbeat_command(self.last_sequence))
    }

    fn handle_dispatch(&mut self, event_type: Option<&str>, data: &Value) {
        match event_type {
            Some("READY") => {
                if let Some(session_id) = data.get("session_id").and_then(Value::as_str) {
                    self.session_id = Some(session_id.to_owned());
                }

                if let Some(resume_url) = data.get("resume_gateway_url").and_then(Value::as_str) {
                    self.set_resume_url(resume_url.to_owned());
                }

                self.state = GatewayConnectionState::Connected;
                debug!(
                    can_resume = self.can_resume(),
                    has_resume_url = self.resume_url.is_some(),
                    "gateway READY dispatch processed"
                );
            }
            Some("RESUMED") => {
                self.state = GatewayConnectionState::Connected;
                debug!("gateway RESUMED dispatch processed");
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum GatewayEvent {
    Dispatch {
        event_type: Option<String>,
        sequence: Option<u64>,
        data: Value,
    },
    HeartbeatRequest,
    Hello(GatewayHelloPayload),
    HeartbeatAck,
    Reconnect,
    InvalidSession {
        resumable: bool,
    },
    NonTextFrame,
    Unknown(Value),
}

pub fn parse_event(payload: &str) -> Result<GatewayEvent, GatewayError> {
    let body: Value = serde_json::from_str(payload)?;

    let op = body.get("op").and_then(|v| v.as_u64()).unwrap_or_default();
    let data = body.get("d").cloned().unwrap_or(Value::Null);
    trace!(op, "parsed gateway frame opcode");

    let event = match op {
        1 => GatewayEvent::HeartbeatRequest,
        0 => GatewayEvent::Dispatch {
            event_type: body
                .get("t")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            sequence: body.get("s").and_then(|v| v.as_u64()),
            data,
        },
        10 => GatewayEvent::Hello(serde_json::from_value(data)?),
        11 => GatewayEvent::HeartbeatAck,
        7 => GatewayEvent::Reconnect,
        9 => GatewayEvent::InvalidSession {
            resumable: data.as_bool().unwrap_or(false),
        },
        _ => GatewayEvent::Unknown(body),
    };

    Ok(event)
}
