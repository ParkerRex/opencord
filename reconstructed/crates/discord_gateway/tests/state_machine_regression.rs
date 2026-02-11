use discord_gateway::{
    GatewayCommand, GatewayConnectionState, GatewayEvent, GatewayHelloPayload,
    GatewayResumePayload, GatewaySession, GatewayStateAction, GatewayStateMachine,
    GatewayStateMachineConfig, parse_event,
};
use serde_json::json;
use std::time::Duration;

fn machine() -> GatewayStateMachine {
    GatewayStateMachine::new(GatewayStateMachineConfig::new("token-123".to_owned(), 513))
}

#[test]
fn parse_hello_event() {
    let event = parse_event(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#).unwrap();
    assert!(matches!(
        event,
        GatewayEvent::Hello(GatewayHelloPayload {
            heartbeat_interval: 41250
        })
    ));
}

#[test]
fn hello_without_session_sends_identify() {
    let mut sm = machine();
    sm.on_connect();

    let actions = sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1000,
    }));

    assert_eq!(sm.state(), GatewayConnectionState::Handshaking);
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions[0],
        GatewayStateAction::ConfigureHeartbeat {
            interval: Duration::from_millis(1000)
        }
    );
    assert!(matches!(
        &actions[1],
        GatewayStateAction::SendIdentify(GatewayCommand { op: 2, .. })
    ));
}

#[test]
fn hello_with_restored_session_sends_resume() {
    let mut sm = machine();
    sm.restore_session(GatewaySession {
        session_id: "session-abc".to_owned(),
        sequence: 42,
    });
    sm.on_connect();

    let actions = sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 45000,
    }));

    assert!(matches!(
        &actions[1],
        GatewayStateAction::SendResume(GatewayCommand {
            op: 6,
            d: GatewayResumePayload { seq: 42, .. }
        })
    ));
}

#[test]
fn ready_dispatch_persists_session_and_marks_connected() {
    let mut sm = machine();
    sm.on_connect();
    sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 30000,
    }));

    let dispatch = GatewayEvent::Dispatch {
        event_type: Some("READY".to_owned()),
        sequence: Some(7),
        data: json!({ "session_id": "session-xyz" }),
    };
    let actions = sm.on_event(&dispatch);

    assert!(actions.is_empty());
    assert_eq!(sm.state(), GatewayConnectionState::Connected);
    assert_eq!(
        sm.session(),
        Some(GatewaySession {
            session_id: "session-xyz".to_owned(),
            sequence: 7
        })
    );
    assert!(sm.resume_url().is_none());
}

#[test]
fn ready_dispatch_persists_resume_url() {
    let mut sm = machine();
    sm.on_connect();
    sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 30000,
    }));

    let dispatch = GatewayEvent::Dispatch {
        event_type: Some("READY".to_owned()),
        sequence: Some(7),
        data: json!({
            "session_id": "session-xyz",
            "resume_gateway_url": "wss://gateway.discord.gg/?v=10&encoding=json"
        }),
    };

    let actions = sm.on_event(&dispatch);
    assert!(actions.is_empty());
    assert_eq!(
        sm.resume_url(),
        Some("wss://gateway.discord.gg/?v=10&encoding=json")
    );
}

#[test]
fn heartbeat_tick_requires_ack_before_next_tick() {
    let mut sm = machine();
    sm.restore_session(GatewaySession {
        session_id: "session-xyz".to_owned(),
        sequence: 99,
    });

    let first = sm.on_heartbeat_tick();
    assert!(matches!(
        first,
        GatewayStateAction::SendHeartbeat(GatewayCommand { op: 1, d: Some(99) })
    ));

    let second = sm.on_heartbeat_tick();
    assert_eq!(second, GatewayStateAction::Reconnect { resumable: true });
    assert_eq!(sm.state(), GatewayConnectionState::ReconnectRequested);

    sm.on_event(&GatewayEvent::HeartbeatAck);
    assert_eq!(sm.state(), GatewayConnectionState::ReconnectRequested);
}

#[test]
fn invalid_session_non_resumable_forces_identify_next_time() {
    let mut sm = machine();
    sm.restore_session(GatewaySession {
        session_id: "session-xyz".to_owned(),
        sequence: 9,
    });

    let actions = sm.on_event(&GatewayEvent::InvalidSession { resumable: false });
    assert_eq!(
        actions,
        vec![GatewayStateAction::Reconnect { resumable: false }]
    );
    assert!(!sm.can_resume());

    sm.on_connect();
    let actions = sm.on_event(&GatewayEvent::Hello(GatewayHelloPayload {
        heartbeat_interval: 1000,
    }));
    assert!(matches!(
        actions[1],
        GatewayStateAction::SendIdentify(GatewayCommand { op: 2, .. })
    ));
}

#[test]
fn restore_session_with_resume_url_sets_both_fields() {
    let mut sm = machine();
    sm.restore_session_with_resume_url(
        GatewaySession {
            session_id: "session-abc".to_owned(),
            sequence: 42,
        },
        "wss://gateway.discord.gg/?v=10&encoding=json".to_owned(),
    );

    assert!(sm.can_resume());
    assert_eq!(
        sm.resume_url(),
        Some("wss://gateway.discord.gg/?v=10&encoding=json")
    );
}
