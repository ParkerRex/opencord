use discord_gateway::{
    GatewayCommand, GatewayEvent, GatewayHelloPayload, GatewayStateAction, GatewayStateMachine,
    GatewayStateMachineConfig, parse_event,
};

fn machine() -> GatewayStateMachine {
    GatewayStateMachine::new(GatewayStateMachineConfig::new("token-edge".to_owned(), 513))
}

#[test]
fn parse_invalid_session_without_bool_defaults_to_non_resumable() {
    let event = parse_event(r#"{"op":9,"d":{}}"#).expect("parse should succeed");
    assert!(matches!(
        event,
        GatewayEvent::InvalidSession { resumable: false }
    ));
}

#[test]
fn empty_resume_url_is_ignored_to_prevent_poisoned_state() {
    let mut sm = machine();
    sm.set_resume_url("   ".to_owned());
    assert!(sm.resume_url().is_none());

    sm.set_resume_url("wss://gateway.discord.gg".to_owned());
    assert_eq!(sm.resume_url(), Some("wss://gateway.discord.gg"));
}

#[test]
fn heartbeat_request_without_ack_forces_reconnect_on_next_tick() {
    let mut sm = machine();
    sm.restore_session(discord_gateway::GatewaySession {
        session_id: "session-edge".to_owned(),
        sequence: 88,
    });
    sm.on_connect();
    sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1_000,
    }));

    let actions = sm.on_event(&GatewayEvent::HeartbeatRequest);
    assert!(matches!(
        actions.as_slice(),
        [GatewayStateAction::SendHeartbeat(GatewayCommand { op: 1, d: Some(88) })]
    ));

    let reconnect = sm.on_heartbeat_tick();
    assert_eq!(reconnect, GatewayStateAction::Reconnect { resumable: true });
}
