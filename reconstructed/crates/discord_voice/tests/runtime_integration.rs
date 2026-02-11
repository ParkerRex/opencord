use discord_voice::{
    VoiceConnectionConfig, VoiceGatewayClient, VoiceGatewayEvent, VoiceRuntimeEvent,
    VoiceRuntimeOptions,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use url::Url;

#[tokio::test]
async fn runtime_sends_identify_and_heartbeats_then_shuts_down() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("listener address");

    let (server_signal_tx, mut server_signal_rx) = mpsc::unbounded_channel::<String>();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept connection");
        let mut socket = accept_async(stream).await.expect("upgrade websocket");

        let identify = socket
            .next()
            .await
            .expect("identify frame should exist")
            .expect("identify frame should decode");

        let Message::Text(identify_text) = identify else {
            panic!("expected identify text frame");
        };
        let identify_payload: Value =
            serde_json::from_str(identify_text.as_ref()).expect("identify JSON");
        assert_eq!(identify_payload["op"], 0);
        server_signal_tx
            .send("identify".to_owned())
            .expect("signal identify");

        socket
            .send(Message::Text(
                r#"{"op":8,"d":{"heartbeat_interval":20}}"#.into(),
            ))
            .await
            .expect("send hello frame");

        let heartbeat = socket
            .next()
            .await
            .expect("heartbeat frame should exist")
            .expect("heartbeat frame should decode");
        let Message::Text(heartbeat_text) = heartbeat else {
            panic!("expected heartbeat text frame");
        };
        let heartbeat_payload: Value =
            serde_json::from_str(heartbeat_text.as_ref()).expect("heartbeat JSON");
        assert_eq!(heartbeat_payload["op"], 3);
        server_signal_tx
            .send("heartbeat".to_owned())
            .expect("signal heartbeat");

        socket
            .send(Message::Text(r#"{"op":6,"d":null}"#.into()))
            .await
            .expect("send heartbeat ack");

        loop {
            match socket.next().await {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => continue,
                Some(Err(_)) => break,
            }
        }
    });

    let gateway_url = Url::parse(&format!("ws://{addr}")).expect("valid gateway URL");
    let client = VoiceGatewayClient::new(gateway_url);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<VoiceRuntimeEvent>();

    let runtime_task = tokio::spawn(async move {
        client
            .run_with_shutdown(
                VoiceConnectionConfig {
                    guild_id: "guild-1".to_owned(),
                    user_id: "user-1".to_owned(),
                    session_id: "session-1".to_owned(),
                    token: "token-1".to_owned(),
                },
                VoiceRuntimeOptions::default(),
                shutdown_rx,
                move |event| {
                    let _ = events_tx.send(event);
                },
            )
            .await
    });

    let identify_signal = timeout(Duration::from_secs(1), server_signal_rx.recv())
        .await
        .expect("identify signal timeout")
        .expect("identify signal missing");
    assert_eq!(identify_signal, "identify");

    let heartbeat_signal = timeout(Duration::from_secs(1), server_signal_rx.recv())
        .await
        .expect("heartbeat signal timeout")
        .expect("heartbeat signal missing");
    assert_eq!(heartbeat_signal, "heartbeat");

    let mut saw_hello = false;
    let mut saw_ack = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while tokio::time::Instant::now() < deadline {
        let Some(event) = timeout(Duration::from_millis(100), events_rx.recv())
            .await
            .ok()
            .flatten()
        else {
            continue;
        };

        match event {
            VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::Hello { .. }) => saw_hello = true,
            VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::HeartbeatAck) => {
                saw_ack = true;
                break;
            }
            _ => {}
        }
    }

    assert!(saw_hello, "expected hello event");
    assert!(saw_ack, "expected heartbeat ACK event");

    shutdown_tx.send(true).expect("shutdown send");

    runtime_task
        .await
        .expect("runtime join should succeed")
        .expect("runtime should exit cleanly");

    let mut saw_shutdown = false;

    while let Ok(event) = timeout(Duration::from_millis(250), events_rx.recv()).await {
        let Some(event) = event else {
            break;
        };

        match event {
            VoiceRuntimeEvent::Shutdown => {
                saw_shutdown = true;
                break;
            }
            _ => {}
        }
    }

    assert!(saw_shutdown, "expected shutdown event");

    server_task.await.expect("server task should finish");
}
