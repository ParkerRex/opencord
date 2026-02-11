use crate::{
    GatewayClient, GatewayCommand, GatewayError, GatewayEvent, GatewayStateAction,
    GatewayStateMachine, GatewayStateMachineConfig,
};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::future::pending;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::time::{self, Instant, Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

#[derive(Debug, Clone)]
pub enum GatewayRuntimeEvent {
    GatewayEvent(GatewayEvent),
    ReconnectScheduled {
        attempt: u32,
        resumable: bool,
        delay: Duration,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct GatewayRuntimeOptions {
    pub reconnect_base_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub reconnect_jitter: Duration,
}

impl Default for GatewayRuntimeOptions {
    fn default() -> Self {
        Self {
            reconnect_base_delay: Duration::from_millis(500),
            reconnect_max_delay: Duration::from_secs(30),
            reconnect_jitter: Duration::from_millis(250),
        }
    }
}

impl GatewayRuntimeOptions {
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

type GatewaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

impl GatewayClient {
    pub async fn run_with_shutdown<H>(
        &self,
        state_machine_config: GatewayStateMachineConfig,
        options: GatewayRuntimeOptions,
        mut shutdown: watch::Receiver<bool>,
        mut on_runtime_event: H,
    ) -> Result<(), GatewayError>
    where
        H: FnMut(GatewayRuntimeEvent) + Send,
    {
        let mut state_machine = GatewayStateMachine::new(state_machine_config);
        let mut reconnect_attempt = 0u32;

        loop {
            if should_shutdown(&shutdown) {
                on_runtime_event(GatewayRuntimeEvent::Shutdown);
                return Ok(());
            }

            state_machine.on_connect();

            let connect_result = connect_async(self.gateway_url().as_str()).await;
            let (mut socket, _) = match connect_result {
                Ok(connection) => connection,
                Err(_) => {
                    state_machine.on_disconnect();
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    let delay = options.reconnect_delay(reconnect_attempt);
                    on_runtime_event(GatewayRuntimeEvent::ReconnectScheduled {
                        attempt: reconnect_attempt,
                        resumable: state_machine.can_resume(),
                        delay,
                    });

                    if wait_for_reconnect_delay(delay, &mut shutdown).await {
                        on_runtime_event(GatewayRuntimeEvent::Shutdown);
                        return Ok(());
                    }

                    continue;
                }
            };

            reconnect_attempt = 0;

            let reconnect_resumable = run_connection_loop(
                &mut socket,
                &mut state_machine,
                &mut shutdown,
                &mut on_runtime_event,
            )
            .await?;

            state_machine.on_disconnect();

            if let Some(resumable) = reconnect_resumable {
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                let delay = options.reconnect_delay(reconnect_attempt);
                on_runtime_event(GatewayRuntimeEvent::ReconnectScheduled {
                    attempt: reconnect_attempt,
                    resumable,
                    delay,
                });

                if wait_for_reconnect_delay(delay, &mut shutdown).await {
                    on_runtime_event(GatewayRuntimeEvent::Shutdown);
                    return Ok(());
                }

                continue;
            }

            on_runtime_event(GatewayRuntimeEvent::Shutdown);
            return Ok(());
        }
    }
}

async fn run_connection_loop<H>(
    socket: &mut GatewaySocket,
    state_machine: &mut GatewayStateMachine,
    shutdown: &mut watch::Receiver<bool>,
    on_runtime_event: &mut H,
) -> Result<Option<bool>, GatewayError>
where
    H: FnMut(GatewayRuntimeEvent) + Send,
{
    let mut heartbeat_interval: Option<Interval> = None;

    loop {
        if should_shutdown(shutdown) {
            let _ = socket.close(None).await;
            return Ok(None);
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || should_shutdown(shutdown) {
                    let _ = socket.close(None).await;
                    return Ok(None);
                }
            }
            _ = heartbeat_tick(&mut heartbeat_interval) => {
                let action = state_machine.on_heartbeat_tick();
                match apply_state_action(socket, &action, &mut heartbeat_interval).await {
                    Ok(Some(resumable)) => return Ok(Some(resumable)),
                    Ok(None) => {}
                    Err(GatewayError::WebSocket(_)) => return Ok(Some(state_machine.can_resume())),
                    Err(err) => return Err(err),
                }
            }
            next_frame = socket.next() => {
                match next_frame {
                    Some(Ok(message)) => {
                        match handle_socket_message(
                            message,
                            socket,
                            state_machine,
                            &mut heartbeat_interval,
                            on_runtime_event,
                        ).await {
                            Ok(Some(resumable)) => return Ok(Some(resumable)),
                            Ok(None) => {}
                            Err(GatewayError::WebSocket(_)) => return Ok(Some(state_machine.can_resume())),
                            Err(err) => return Err(err),
                        }
                    }
                    Some(Err(_)) | None => return Ok(Some(state_machine.can_resume())),
                }
            }
        }
    }
}

async fn handle_socket_message<H>(
    message: Message,
    socket: &mut GatewaySocket,
    state_machine: &mut GatewayStateMachine,
    heartbeat_interval: &mut Option<Interval>,
    on_runtime_event: &mut H,
) -> Result<Option<bool>, GatewayError>
where
    H: FnMut(GatewayRuntimeEvent) + Send,
{
    match message {
        Message::Text(text) => {
            let event = crate::parse_event(text.as_ref())?;
            on_runtime_event(GatewayRuntimeEvent::GatewayEvent(event.clone()));
            process_event_actions(event, socket, state_machine, heartbeat_interval).await
        }
        Message::Binary(_) => {
            let event = GatewayEvent::NonTextFrame;
            on_runtime_event(GatewayRuntimeEvent::GatewayEvent(event.clone()));
            process_event_actions(event, socket, state_machine, heartbeat_interval).await
        }
        Message::Close(_) => Ok(Some(state_machine.can_resume())),
        Message::Ping(payload) => {
            socket.send(Message::Pong(payload)).await?;
            Ok(None)
        }
        Message::Pong(_) => Ok(None),
        Message::Frame(_) => Ok(None),
    }
}

async fn process_event_actions(
    event: GatewayEvent,
    socket: &mut GatewaySocket,
    state_machine: &mut GatewayStateMachine,
    heartbeat_interval: &mut Option<Interval>,
) -> Result<Option<bool>, GatewayError> {
    for action in state_machine.on_event(&event) {
        if let Some(resumable) = apply_state_action(socket, &action, heartbeat_interval).await? {
            return Ok(Some(resumable));
        }
    }

    Ok(None)
}

async fn apply_state_action(
    socket: &mut GatewaySocket,
    action: &GatewayStateAction,
    heartbeat_interval: &mut Option<Interval>,
) -> Result<Option<bool>, GatewayError> {
    match action {
        GatewayStateAction::ConfigureHeartbeat { interval } => {
            let mut schedule = time::interval_at(Instant::now() + *interval, *interval);
            schedule.set_missed_tick_behavior(MissedTickBehavior::Skip);
            *heartbeat_interval = Some(schedule);
            Ok(None)
        }
        GatewayStateAction::SendIdentify(command) => {
            send_command(socket, command).await?;
            Ok(None)
        }
        GatewayStateAction::SendResume(command) => {
            send_command(socket, command).await?;
            Ok(None)
        }
        GatewayStateAction::SendHeartbeat(command) => {
            send_command(socket, command).await?;
            Ok(None)
        }
        GatewayStateAction::Reconnect { resumable } => Ok(Some(*resumable)),
    }
}

async fn send_command<T: Serialize>(
    socket: &mut GatewaySocket,
    command: &GatewayCommand<T>,
) -> Result<(), GatewayError> {
    let payload = serde_json::to_string(command)?;
    socket.send(Message::Text(payload.into())).await?;
    Ok(())
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
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);

    now_nanos ^ (attempt as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
}
