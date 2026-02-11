use discord_gateway::{
    GatewayClient, GatewayCommand, GatewayConnectionState, GatewayEvent, GatewayHelloPayload,
    GatewayIdentifyPayload, GatewayRuntimeEvent, GatewayRuntimeOptions, GatewaySession,
    GatewayStateAction, GatewayStateMachine, GatewayStateMachineConfig, heartbeat_command,
    identify_command, parse_event, resume_command,
};
use serde_json::Value;
use std::time::Duration;
use url::Url;

fn build_state_machine() -> GatewayStateMachine {
    GatewayStateMachine::new(GatewayStateMachineConfig::new("token-123".to_owned(), 513))
}

async fn _gateway_client_methods(client: &GatewayClient) {
    let _ = client.connect_once().await;
    let _ = client.read_one_event().await;
}

#[test]
fn gateway_public_api_lock() {
    let client = GatewayClient::new(Url::parse("wss://gateway.discord.gg").expect("valid URL"));
    let _ = client.gateway_url();

    let _ = parse_event(r#"{"op":11}"#);

    let identify: GatewayCommand<GatewayIdentifyPayload> =
        identify_command(GatewayIdentifyPayload {
            token: "token".to_owned(),
            intents: 513,
            properties: Default::default(),
        });
    assert_eq!(identify.op, 2);

    let resume = resume_command(discord_gateway::GatewayResumePayload {
        token: "token".to_owned(),
        session_id: "session".to_owned(),
        seq: 1,
    });
    assert_eq!(resume.op, 6);

    let heartbeat = heartbeat_command(Some(1));
    assert_eq!(heartbeat.op, 1);

    let mut machine = build_state_machine();
    machine.on_connect();
    let _ = machine.state();
    let _ = machine.heartbeat_interval();
    let _ = machine.can_resume();
    let _ = machine.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1000,
    }));
    let _ = machine.on_heartbeat_tick();
    machine.restore_session(GatewaySession {
        session_id: "session".to_owned(),
        sequence: 1,
    });
    machine.set_resume_url("wss://gateway.discord.gg".to_owned());
    let _ = machine.resume_url();
    machine.clear_session();
    machine.on_disconnect();

    let _ = GatewayConnectionState::Disconnected;
    let _ = GatewayStateAction::Reconnect { resumable: true };
    let _ = GatewayEvent::Unknown(Value::Null);

    let _ = GatewayRuntimeOptions::default();
    let _ = GatewayRuntimeEvent::GatewayEvent(GatewayEvent::HeartbeatAck);
    let _ = GatewayRuntimeEvent::ReconnectScheduled {
        attempt: 1,
        resumable: true,
        delay: Duration::from_secs(1),
    };
    let _ = GatewayRuntimeEvent::Shutdown;
}
