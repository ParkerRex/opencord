use discord_gateway::{
    GatewayClient, GatewayRuntimeEvent, GatewayRuntimeOptions, GatewayStateMachineConfig,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use url::Url;

fn text_message(value: Value) -> Message {
    Message::Text(value.to_string().into())
}

async fn recv_client_json(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> Value {
    loop {
        match socket
            .next()
            .await
            .expect("client should send a websocket frame")
            .expect("frame should decode")
        {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref()).expect("client payload should be JSON");
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .expect("should pong client ping");
            }
            Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
            Message::Close(_) => panic!("client closed socket early"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_reconnects_and_uses_resume_after_reconnect_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("ws://{}", listener.local_addr().unwrap())).unwrap();

    let (server_done_tx, server_done_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        socket
            .send(text_message(
                json!({"op": 10, "d": {"heartbeat_interval": 50}}),
            ))
            .await
            .unwrap();

        let identify = recv_client_json(&mut socket).await;
        assert_eq!(identify["op"], 2);

        socket
            .send(text_message(json!({
                "op": 0,
                "t": "READY",
                "s": 1,
                "d": {"session_id": "session-1"}
            })))
            .await
            .unwrap();

        let heartbeat = recv_client_json(&mut socket).await;
        assert_eq!(heartbeat["op"], 1);
        socket
            .send(text_message(json!({"op": 11, "d": null})))
            .await
            .unwrap();

        socket
            .send(text_message(json!({"op": 7, "d": null})))
            .await
            .unwrap();
        let _ = socket.close(None).await;

        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        socket
            .send(text_message(
                json!({"op": 10, "d": {"heartbeat_interval": 50}}),
            ))
            .await
            .unwrap();

        let resume = recv_client_json(&mut socket).await;
        assert_eq!(resume["op"], 6);
        assert_eq!(resume["d"]["session_id"], "session-1");
        assert_eq!(resume["d"]["seq"], 1);

        socket
            .send(text_message(
                json!({"op": 0, "t": "RESUMED", "s": 2, "d": {}}),
            ))
            .await
            .unwrap();

        let _ = server_done_tx.send(());
        let _ = socket.next().await;
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let reconnect_events: Arc<Mutex<Vec<(u32, bool)>>> = Arc::new(Mutex::new(Vec::new()));
    let reconnect_events_for_runtime = Arc::clone(&reconnect_events);

    let client = GatewayClient::new(url);
    let runtime = tokio::spawn(async move {
        let mut options = GatewayRuntimeOptions::default();
        options.reconnect_base_delay = Duration::from_millis(10);
        options.reconnect_max_delay = Duration::from_millis(50);
        options.reconnect_jitter = Duration::ZERO;

        client
            .run_with_shutdown(
                GatewayStateMachineConfig::new("token-123".to_owned(), 513),
                options,
                shutdown_rx,
                move |event| {
                    if let GatewayRuntimeEvent::ReconnectScheduled {
                        attempt, resumable, ..
                    } = event
                    {
                        reconnect_events_for_runtime
                            .lock()
                            .unwrap()
                            .push((attempt, resumable));
                    }
                },
            )
            .await
    });

    server_done_rx.await.unwrap();
    shutdown_tx.send(true).unwrap();

    runtime.await.unwrap().unwrap();
    server.await.unwrap();

    let reconnect_events = reconnect_events.lock().unwrap();
    assert!(
        reconnect_events.iter().any(|(_, resumable)| *resumable),
        "runtime should schedule a resumable reconnect"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_falls_back_to_identify_after_non_resumable_invalid_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("ws://{}", listener.local_addr().unwrap())).unwrap();

    let (server_done_tx, server_done_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        socket
            .send(text_message(
                json!({"op": 10, "d": {"heartbeat_interval": 50}}),
            ))
            .await
            .unwrap();

        let identify = recv_client_json(&mut socket).await;
        assert_eq!(identify["op"], 2);

        socket
            .send(text_message(json!({
                "op": 0,
                "t": "READY",
                "s": 9,
                "d": {"session_id": "session-2"}
            })))
            .await
            .unwrap();

        socket
            .send(text_message(json!({"op": 9, "d": false})))
            .await
            .unwrap();
        let _ = socket.close(None).await;

        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        socket
            .send(text_message(
                json!({"op": 10, "d": {"heartbeat_interval": 50}}),
            ))
            .await
            .unwrap();

        let identify_again = recv_client_json(&mut socket).await;
        assert_eq!(identify_again["op"], 2);

        let _ = server_done_tx.send(());
        let _ = socket.next().await;
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let reconnect_events: Arc<Mutex<Vec<(u32, bool)>>> = Arc::new(Mutex::new(Vec::new()));
    let reconnect_events_for_runtime = Arc::clone(&reconnect_events);

    let client = GatewayClient::new(url);
    let runtime = tokio::spawn(async move {
        let mut options = GatewayRuntimeOptions::default();
        options.reconnect_base_delay = Duration::from_millis(10);
        options.reconnect_max_delay = Duration::from_millis(50);
        options.reconnect_jitter = Duration::ZERO;

        client
            .run_with_shutdown(
                GatewayStateMachineConfig::new("token-123".to_owned(), 513),
                options,
                shutdown_rx,
                move |event| {
                    if let GatewayRuntimeEvent::ReconnectScheduled {
                        attempt, resumable, ..
                    } = event
                    {
                        reconnect_events_for_runtime
                            .lock()
                            .unwrap()
                            .push((attempt, resumable));
                    }
                },
            )
            .await
    });

    server_done_rx.await.unwrap();
    shutdown_tx.send(true).unwrap();

    runtime.await.unwrap().unwrap();
    server.await.unwrap();

    let reconnect_events = reconnect_events.lock().unwrap();
    assert!(
        reconnect_events.iter().any(|(_, resumable)| !*resumable),
        "runtime should schedule a non-resumable reconnect after invalid session"
    );
}
