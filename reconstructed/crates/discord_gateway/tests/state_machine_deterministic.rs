use discord_gateway::{
    GatewayCommand, GatewayConnectionState, GatewayEvent, GatewayHelloPayload,
    GatewayResumePayload, GatewaySession, GatewayStateAction, GatewayStateMachine,
    GatewayStateMachineConfig,
};
use serde_json::json;
use std::time::Duration;

fn machine() -> GatewayStateMachine {
    GatewayStateMachine::new(GatewayStateMachineConfig::new("token-123".to_owned(), 513))
}

fn establish_session(sm: &mut GatewayStateMachine, sequence: u64, session_id: &str) {
    sm.on_connect();
    let actions = sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 5_000,
    }));

    assert_eq!(
        actions[0],
        GatewayStateAction::ConfigureHeartbeat {
            interval: Duration::from_millis(5_000)
        }
    );

    sm.on_event(&GatewayEvent::Dispatch {
        event_type: Some("READY".to_owned()),
        sequence: Some(sequence),
        data: json!({ "session_id": session_id }),
    });
    assert_eq!(sm.state(), GatewayConnectionState::Connected);
}

#[test]
fn timer_tick_after_ack_sends_next_heartbeat_without_reconnect() {
    let mut sm = machine();
    establish_session(&mut sm, 12, "session-timer");

    let first = sm.on_heartbeat_tick();
    assert!(matches!(
        first,
        GatewayStateAction::SendHeartbeat(GatewayCommand { op: 1, d: Some(12) })
    ));

    sm.on_event(&GatewayEvent::HeartbeatAck);

    let second = sm.on_heartbeat_tick();
    assert!(matches!(
        second,
        GatewayStateAction::SendHeartbeat(GatewayCommand { op: 1, d: Some(12) })
    ));
    assert_eq!(sm.state(), GatewayConnectionState::Connected);
}

#[test]
fn timer_tick_without_ack_requests_resumable_reconnect() {
    let mut sm = machine();
    establish_session(&mut sm, 99, "session-reconnect");

    let _ = sm.on_heartbeat_tick();
    let reconnect = sm.on_heartbeat_tick();

    assert_eq!(reconnect, GatewayStateAction::Reconnect { resumable: true });
    assert_eq!(sm.state(), GatewayConnectionState::ReconnectRequested);
}

#[test]
fn reconnect_event_without_session_is_not_resumable() {
    let mut sm = machine();
    sm.on_connect();
    sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1_000,
    }));

    let actions = sm.on_event(&GatewayEvent::Reconnect);
    assert_eq!(
        actions,
        vec![GatewayStateAction::Reconnect { resumable: false }]
    );
    assert_eq!(sm.state(), GatewayConnectionState::ReconnectRequested);
}

#[test]
fn reconnect_event_with_session_is_resumable() {
    let mut sm = machine();
    establish_session(&mut sm, 77, "session-reconnect-event");

    let actions = sm.on_event(&GatewayEvent::Reconnect);
    assert_eq!(
        actions,
        vec![GatewayStateAction::Reconnect { resumable: true }]
    );
    assert_eq!(sm.state(), GatewayConnectionState::ReconnectRequested);
}

#[test]
fn resumable_invalid_session_keeps_resume_state_for_next_hello() {
    let mut sm = machine();
    sm.restore_session(GatewaySession {
        session_id: "session-resume".to_owned(),
        sequence: 42,
    });

    let actions = sm.on_event(&GatewayEvent::InvalidSession { resumable: true });
    assert_eq!(
        actions,
        vec![GatewayStateAction::Reconnect { resumable: true }]
    );

    sm.on_connect();
    let hello_actions = sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1_000,
    }));

    assert!(matches!(
        hello_actions[1],
        GatewayStateAction::SendResume(GatewayCommand {
            op: 6,
            d: GatewayResumePayload { seq: 42, .. }
        })
    ));
}
