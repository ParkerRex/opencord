use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::future::pending;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::time::{self, Instant, Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, tungstenite};
use tracing::{debug, trace, warn};
use url::Url;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("invalid voice gateway URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("voice websocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),
    #[error("voice payload error: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("voice udp error: {0}")]
    Udp(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct VoiceGatewayClient {
    gateway_url: Url,
}

impl VoiceGatewayClient {
    pub fn new(gateway_url: Url) -> Self {
        Self { gateway_url }
    }

    pub fn gateway_url(&self) -> &Url {
        &self.gateway_url
    }

    pub async fn connect_once(&self) -> Result<(), VoiceError> {
        let (mut socket, _) = connect_async(self.gateway_url.as_str()).await?;
        socket.close(None).await?;
        Ok(())
    }

    pub async fn run_with_shutdown<H>(
        &self,
        config: VoiceConnectionConfig,
        options: VoiceRuntimeOptions,
        mut shutdown: watch::Receiver<bool>,
        mut on_runtime_event: H,
    ) -> Result<(), VoiceError>
    where
        H: FnMut(VoiceRuntimeEvent) + Send,
    {
        let mut reconnect_attempt = 0u32;

        loop {
            if should_shutdown(&shutdown) {
                on_runtime_event(VoiceRuntimeEvent::Shutdown);
                return Ok(());
            }

            let connect_result = connect_async(self.gateway_url.as_str()).await;
            let (mut socket, _) = match connect_result {
                Ok(connection) => connection,
                Err(error) => {
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    let delay = options.reconnect_delay(reconnect_attempt);
                    warn!(
                        attempt = reconnect_attempt,
                        delay_ms = delay.as_millis().min(u128::from(u64::MAX)) as u64,
                        error = %error,
                        "voice gateway connect failed; scheduling reconnect"
                    );

                    on_runtime_event(VoiceRuntimeEvent::ReconnectScheduled {
                        attempt: reconnect_attempt,
                        delay,
                    });

                    if wait_for_reconnect_delay(delay, &mut shutdown).await {
                        on_runtime_event(VoiceRuntimeEvent::Shutdown);
                        return Ok(());
                    }

                    continue;
                }
            };

            reconnect_attempt = 0;

            let reconnect =
                run_connection_loop(&mut socket, &config, &mut shutdown, &mut on_runtime_event)
                    .await?;

            if reconnect {
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                let delay = options.reconnect_delay(reconnect_attempt);
                on_runtime_event(VoiceRuntimeEvent::ReconnectScheduled {
                    attempt: reconnect_attempt,
                    delay,
                });

                if wait_for_reconnect_delay(delay, &mut shutdown).await {
                    on_runtime_event(VoiceRuntimeEvent::Shutdown);
                    return Ok(());
                }
                continue;
            }

            on_runtime_event(VoiceRuntimeEvent::Shutdown);
            return Ok(());
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoiceConnectionConfig {
    pub guild_id: String,
    pub user_id: String,
    pub session_id: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct VoiceRuntimeOptions {
    pub reconnect_base_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub reconnect_jitter: Duration,
}

impl Default for VoiceRuntimeOptions {
    fn default() -> Self {
        Self {
            reconnect_base_delay: Duration::from_millis(500),
            reconnect_max_delay: Duration::from_secs(30),
            reconnect_jitter: Duration::from_millis(250),
        }
    }
}

impl VoiceRuntimeOptions {
    fn reconnect_delay(&self, attempt: u32) -> Duration {
        let base_ms = clamp_duration_ms(self.reconnect_base_delay);
        let max_ms = clamp_duration_ms(self.reconnect_max_delay).max(base_ms);

        let exp_factor_shift = attempt.saturating_sub(1).min(10);
        let exp_factor = 1u64 << exp_factor_shift;
        let exp_ms = base_ms.saturating_mul(exp_factor).min(max_ms);

        let jitter_ms = if self.reconnect_jitter.is_zero() {
            0
        } else {
            let max_jitter_ms = clamp_duration_ms(self.reconnect_jitter);
            if max_jitter_ms == 0 {
                0
            } else {
                jitter_seed(attempt) % (max_jitter_ms + 1)
            }
        };

        Duration::from_millis(exp_ms.saturating_add(jitter_ms))
    }
}

#[derive(Debug, Clone)]
pub enum VoiceRuntimeEvent {
    GatewayEvent(VoiceGatewayEvent),
    ReconnectScheduled { attempt: u32, delay: Duration },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceGatewayEvent {
    Hello {
        heartbeat_interval_ms: u64,
    },
    Ready {
        ip: String,
        port: u16,
        ssrc: u32,
        modes: Vec<String>,
    },
    SessionDescription {
        mode: String,
        secret_key: Vec<u8>,
    },
    HeartbeatAck,
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceUdpDiscoveryResult {
    pub ip: String,
    pub port: u16,
}

pub async fn discover_udp_address(
    endpoint_host: &str,
    endpoint_port: u16,
    ssrc: u32,
) -> Result<VoiceUdpDiscoveryResult, VoiceError> {
    let bind_addr = "0.0.0.0:0";
    let remote_addr = format!("{endpoint_host}:{endpoint_port}");

    let socket = UdpSocket::bind(bind_addr).await?;
    socket.connect(&remote_addr).await?;

    let mut probe = [0_u8; 70];
    probe[0..4].copy_from_slice(&ssrc.to_be_bytes());
    socket.send(&probe).await?;

    let mut response = [0_u8; 70];
    let _ = socket.recv(&mut response).await?;

    let ip_end = response[4..68].iter().position(|b| *b == 0).unwrap_or(64);
    let ip = String::from_utf8_lossy(&response[4..(4 + ip_end)])
        .trim()
        .to_owned();
    let port = u16::from_be_bytes([response[68], response[69]]);

    Ok(VoiceUdpDiscoveryResult { ip, port })
}

async fn run_connection_loop<H>(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    config: &VoiceConnectionConfig,
    shutdown: &mut watch::Receiver<bool>,
    on_runtime_event: &mut H,
) -> Result<bool, VoiceError>
where
    H: FnMut(VoiceRuntimeEvent) + Send,
{
    send_identify(socket, config).await?;

    let mut heartbeat_interval: Option<Interval> = None;
    let mut awaiting_heartbeat_ack = false;
    let mut heartbeat_nonce = 0_u64;

    loop {
        if should_shutdown(shutdown) {
            let _ = socket.close(None).await;
            return Ok(false);
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || should_shutdown(shutdown) {
                    let _ = socket.close(None).await;
                    return Ok(false);
                }
            }
            _ = heartbeat_tick(&mut heartbeat_interval) => {
                if awaiting_heartbeat_ack {
                    warn!("voice heartbeat ACK timeout; reconnecting");
                    return Ok(true);
                }

                heartbeat_nonce = heartbeat_nonce.saturating_add(1);
                send_heartbeat(socket, heartbeat_nonce).await?;
                awaiting_heartbeat_ack = true;
            }
            next_frame = socket.next() => {
                match next_frame {
                    Some(Ok(message)) => {
                        match message {
                            Message::Text(text) => {
                                let event = parse_gateway_event(text.as_ref())?;
                                if let VoiceGatewayEvent::Hello { heartbeat_interval_ms } = event {
                                    configure_heartbeat(&mut heartbeat_interval, heartbeat_interval_ms);
                                    on_runtime_event(VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Hello {
                                        heartbeat_interval_ms,
                                    }));
                                } else if matches!(event, VoiceGatewayEvent::HeartbeatAck) {
                                    awaiting_heartbeat_ack = false;
                                    on_runtime_event(VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::HeartbeatAck));
                                } else {
                                    on_runtime_event(VoiceRuntimeEvent::GatewayEvent(event));
                                }
                            }
                            Message::Binary(_) => {
                                on_runtime_event(VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Unknown(json!({"kind": "binary"}))));
                            }
                            Message::Close(_) => return Ok(true),
                            Message::Ping(payload) => {
                                socket.send(Message::Pong(payload)).await?;
                            }
                            Message::Pong(_) | Message::Frame(_) => {}
                        }
                    }
                    Some(Err(_)) | None => return Ok(true),
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GatewayFrame {
    op: u64,
    #[serde(default)]
    d: Value,
}

#[derive(Debug, Deserialize)]
struct VoiceHelloPayload {
    heartbeat_interval: u64,
}

#[derive(Debug, Deserialize)]
struct VoiceReadyPayload {
    ip: String,
    port: u16,
    ssrc: u32,
    #[serde(default)]
    modes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VoiceSessionDescriptionPayload {
    mode: String,
    #[serde(default)]
    secret_key: Vec<u8>,
}

pub fn parse_gateway_event(payload: &str) -> Result<VoiceGatewayEvent, VoiceError> {
    let frame: GatewayFrame = serde_json::from_str(payload)?;

    match frame.op {
        8 => {
            let hello: VoiceHelloPayload = serde_json::from_value(frame.d)?;
            Ok(VoiceGatewayEvent::Hello {
                heartbeat_interval_ms: hello.heartbeat_interval,
            })
        }
        2 => {
            let ready: VoiceReadyPayload = serde_json::from_value(frame.d)?;
            Ok(VoiceGatewayEvent::Ready {
                ip: ready.ip,
                port: ready.port,
                ssrc: ready.ssrc,
                modes: ready.modes,
            })
        }
        4 => {
            let session_description: VoiceSessionDescriptionPayload =
                serde_json::from_value(frame.d)?;
            Ok(VoiceGatewayEvent::SessionDescription {
                mode: session_description.mode,
                secret_key: session_description.secret_key,
            })
        }
        6 => Ok(VoiceGatewayEvent::HeartbeatAck),
        _ => Ok(VoiceGatewayEvent::Unknown(json!({
            "op": frame.op,
            "d": frame.d,
        }))),
    }
}

async fn send_identify(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    config: &VoiceConnectionConfig,
) -> Result<(), VoiceError> {
    let payload = json!({
        "op": 0,
        "d": {
            "server_id": config.guild_id,
            "user_id": config.user_id,
            "session_id": config.session_id,
            "token": config.token,
        }
    });
    trace!("sending voice identify");
    socket
        .send(Message::Text(payload.to_string().into()))
        .await?;
    Ok(())
}

async fn send_heartbeat(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    nonce: u64,
) -> Result<(), VoiceError> {
    let payload = json!({
        "op": 3,
        "d": nonce,
    });
    trace!(nonce, "sending voice heartbeat");
    socket
        .send(Message::Text(payload.to_string().into()))
        .await?;
    Ok(())
}

fn configure_heartbeat(heartbeat_interval: &mut Option<Interval>, heartbeat_interval_ms: u64) {
    let interval = Duration::from_millis(heartbeat_interval_ms);
    let mut schedule = time::interval_at(Instant::now() + interval, interval);
    schedule.set_missed_tick_behavior(MissedTickBehavior::Skip);
    *heartbeat_interval = Some(schedule);

    debug!(heartbeat_interval_ms, "configured voice heartbeat schedule");
}

async fn heartbeat_tick(heartbeat_interval: &mut Option<Interval>) {
    match heartbeat_interval {
        Some(interval) => {
            interval.tick().await;
        }
        None => {
            pending::<()>().await;
        }
    }
}

async fn wait_for_reconnect_delay(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    if should_shutdown(shutdown) {
        return true;
    }

    tokio::select! {
        _ = time::sleep(delay) => false,
        changed = shutdown.changed() => {
            changed.is_err() || should_shutdown(shutdown)
        }
    }
}

fn should_shutdown(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow()
}

fn clamp_duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn jitter_seed(attempt: u32) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    (now.as_nanos() as u64)
        ^ ((attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ ((now.subsec_nanos() as u64) << 16)
}

#[cfg(test)]
mod tests {
    use super::{VoiceGatewayEvent, parse_gateway_event};

    #[test]
    fn parses_hello_payload() {
        let payload = r#"{"op":8,"d":{"heartbeat_interval":41250}}"#;
        let event = parse_gateway_event(payload).expect("hello payload should parse");

        assert_eq!(
            event,
            VoiceGatewayEvent::Hello {
                heartbeat_interval_ms: 41250,
            }
        );
    }

    #[test]
    fn parses_ready_payload() {
        let payload = r#"{"op":2,"d":{"ip":"203.0.113.1","port":50000,"ssrc":42,"modes":["xsalsa20_poly1305"]}}"#;
        let event = parse_gateway_event(payload).expect("ready payload should parse");

        assert_eq!(
            event,
            VoiceGatewayEvent::Ready {
                ip: "203.0.113.1".to_owned(),
                port: 50000,
                ssrc: 42,
                modes: vec!["xsalsa20_poly1305".to_owned()],
            }
        );
    }
}
