use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use discord_http::DiscordHttpClient;
use discord_service::{
    MemorySessionStore, OAuthCodeExchanger, OAuthConfig, OAuthTokenSet, RealtimeEnvelope,
    ServiceError, ServiceState, SessionStore, SessionTokenState, UnsupportedOAuthCodeExchanger,
    router,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use tokio::time::{Duration, timeout};
use tower::ServiceExt;
use url::Url;

fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut response = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    response
}

fn spawn_scripted_server(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server addr");

    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request_buffer = [0_u8; 16384];
            let _ = stream.read(&mut request_buffer);
            stream
                .write_all(response.as_bytes())
                .expect("write scripted response");
            stream.flush().expect("flush scripted response");
        }
    });

    (format!("http://{addr}"), handle)
}

fn spawn_recording_server(
    responses: Vec<String>,
) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server addr");
    let (request_tx, request_rx) = mpsc::channel::<String>();

    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request_buffer = [0_u8; 16384];
            let bytes_read = stream.read(&mut request_buffer).expect("read request");
            let request_text = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
            request_tx
                .send(request_text)
                .expect("request should be captured");

            stream
                .write_all(response.as_bytes())
                .expect("write scripted response");
            stream.flush().expect("flush scripted response");
        }
    });

    (format!("http://{addr}"), request_rx, handle)
}

#[derive(Debug)]
struct MockOAuthExchanger {
    exchanged: OAuthTokenSet,
    refreshed: OAuthTokenSet,
    refresh_calls: AtomicUsize,
}

#[async_trait]
impl OAuthCodeExchanger for MockOAuthExchanger {
    async fn exchange_code(&self, _code: &str) -> Result<OAuthTokenSet, ServiceError> {
        Ok(self.exchanged.clone())
    }

    async fn refresh_token(&self, _refresh_token: &str) -> Result<OAuthTokenSet, ServiceError> {
        self.refresh_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.refreshed.clone())
    }
}

#[tokio::test]
async fn me_route_proxies_to_discord_api_with_session_auth() {
    let user_json = r#"{"id":"80351110224678912","username":"Nelly","global_name":"Nelly","avatar":"8342729096ea3675442027381ff50dfe","bot":false}"#;
    let (base_url, handle) = spawn_scripted_server(vec![http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        user_json,
    )]);

    let sessions = Arc::new(MemorySessionStore::new());
    sessions
        .insert_for_tests("session-1", "discord-token")
        .await;

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse(&base_url).expect("valid URL")),
        sessions,
        None,
        Arc::new(UnsupportedOAuthCodeExchanger),
    );

    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/me")
                .header("authorization", "Bearer session-1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body should decode");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8 response body");
    assert!(body_str.contains("Nelly"));

    handle.join().expect("server thread should exit cleanly");
}

#[tokio::test]
async fn delete_message_route_emits_realtime_event() {
    let (base_url, handle) = spawn_scripted_server(vec![http_response("204 No Content", &[], "")]);

    let sessions = Arc::new(MemorySessionStore::new());
    sessions
        .insert_for_tests("session-1", "discord-token")
        .await;

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse(&base_url).expect("valid URL")),
        sessions,
        None,
        Arc::new(UnsupportedOAuthCodeExchanger),
    );

    let mut events_rx = state.event_sender().subscribe();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/channels/123/messages/456")
                .header("authorization", "Bearer session-1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let event = timeout(Duration::from_secs(1), events_rx.recv())
        .await
        .expect("event timeout")
        .expect("event should be present");

    match event {
        RealtimeEnvelope::MessageDeleted {
            session_id,
            channel_id,
            message_id,
        } => {
            assert_eq!(session_id, "session-1");
            assert_eq!(channel_id, "123");
            assert_eq!(message_id, "456");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    handle.join().expect("server thread should exit cleanly");
}

#[tokio::test]
async fn expired_session_uses_refresh_token_before_proxying_to_discord() {
    let user_json = r#"{"id":"80351110224678912","username":"Nelly","global_name":"Nelly","avatar":"8342729096ea3675442027381ff50dfe","bot":false}"#;
    let (base_url, requests_rx, handle) = spawn_recording_server(vec![http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        user_json,
    )]);

    let sessions = Arc::new(MemorySessionStore::new());
    sessions
        .set_token_state(
            "session-1",
            SessionTokenState {
                access_token: "expired-access-token".to_owned(),
                token_type: Some("Bearer".to_owned()),
                refresh_token: Some("refresh-token".to_owned()),
                scope: Some("identify guilds".to_owned()),
                expires_at_unix_ms: Some(0),
            },
        )
        .await
        .expect("seed token state");

    let oauth = Arc::new(MockOAuthExchanger {
        exchanged: OAuthTokenSet {
            access_token: "unused".to_owned(),
            token_type: "Bearer".to_owned(),
            expires_in: Some(3600),
            refresh_token: Some("unused-refresh".to_owned()),
            scope: Some("identify guilds".to_owned()),
        },
        refreshed: OAuthTokenSet {
            access_token: "fresh-access-token".to_owned(),
            token_type: "Bearer".to_owned(),
            expires_in: Some(3600),
            refresh_token: Some("rotated-refresh-token".to_owned()),
            scope: Some("identify guilds".to_owned()),
        },
        refresh_calls: AtomicUsize::new(0),
    });

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse(&base_url).expect("valid URL")),
        sessions.clone(),
        None,
        oauth.clone(),
    );

    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/me")
                .header("authorization", "Bearer session-1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let captured_request = requests_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("captured outbound request");
    let captured_request = captured_request.to_ascii_lowercase();
    assert!(
        captured_request.contains("authorization: bearer fresh-access-token"),
        "expected refreshed access token header in request: {captured_request}"
    );
    assert_eq!(oauth.refresh_calls.load(Ordering::SeqCst), 1);

    let persisted = sessions
        .get_token_state("session-1")
        .await
        .expect("token state lookup should succeed")
        .expect("token state should persist");
    assert_eq!(persisted.access_token, "fresh-access-token");
    assert_eq!(
        persisted.refresh_token.as_deref(),
        Some("rotated-refresh-token")
    );

    handle.join().expect("server thread should exit cleanly");
}

#[tokio::test]
async fn oauth_callback_redirects_to_client_uri_when_requested() {
    let sessions = Arc::new(MemorySessionStore::new());
    let oauth = Arc::new(MockOAuthExchanger {
        exchanged: OAuthTokenSet {
            access_token: "oauth-access-token".to_owned(),
            token_type: "Bearer".to_owned(),
            expires_in: Some(3600),
            refresh_token: Some("oauth-refresh-token".to_owned()),
            scope: Some("identify guilds".to_owned()),
        },
        refreshed: OAuthTokenSet {
            access_token: "unused".to_owned(),
            token_type: "Bearer".to_owned(),
            expires_in: Some(3600),
            refresh_token: Some("unused".to_owned()),
            scope: Some("identify guilds".to_owned()),
        },
        refresh_calls: AtomicUsize::new(0),
    });

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid base URL")),
        sessions,
        Some(OAuthConfig {
            authorize_url: Url::parse("https://discord.com/oauth2/authorize")
                .expect("authorize URL"),
            client_id: "client-id".to_owned(),
            redirect_uri: Url::parse("https://service.local/v1/auth/discord/callback")
                .expect("redirect URL"),
            scope: "identify guilds".to_owned(),
        }),
        oauth,
    );

    let app = router(state);
    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/discord/start?client_redirect_uri=opencord://auth/callback")
                .body(Body::empty())
                .expect("start request should build"),
        )
        .await
        .expect("start route should respond");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body = to_bytes(start_response.into_body(), 1024 * 1024)
        .await
        .expect("start body should decode");
    let payload: Value =
        serde_json::from_slice(&start_body).expect("start payload should be valid JSON");
    let state_token = payload
        .get("state")
        .and_then(Value::as_str)
        .expect("state token should exist")
        .to_owned();

    let callback_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/auth/discord/callback?state={state_token}&code=auth-code"
                ))
                .body(Body::empty())
                .expect("callback request should build"),
        )
        .await
        .expect("callback route should respond");

    assert_eq!(callback_response.status(), StatusCode::SEE_OTHER);
    let location = callback_response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("location header should exist");
    assert!(
        location.starts_with("opencord://auth/callback?session_id="),
        "unexpected redirect location: {location}"
    );
}

#[tokio::test]
async fn voice_session_routes_round_trip_and_emit_events() {
    let sessions = Arc::new(MemorySessionStore::new());
    sessions
        .insert_for_tests("session-1", "discord-token")
        .await;

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid URL")),
        sessions,
        None,
        Arc::new(UnsupportedOAuthCodeExchanger),
    );

    let mut events_rx = state.event_sender().subscribe();
    let app = router(state);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/voice/sessions")
                .header("authorization", "Bearer session-1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"guild_id":"g1","channel_id":"c1"}"#))
                .expect("create request should build"),
        )
        .await
        .expect("create route should respond");

    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .expect("create body should decode");
    let created: Value =
        serde_json::from_slice(&create_body).expect("create body should be valid JSON");
    let session_id = created
        .get("id")
        .and_then(Value::as_str)
        .expect("voice session id should exist")
        .to_owned();

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/voice/sessions/{session_id}/speaking"))
                .header("authorization", "Bearer session-1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"speaking":true}"#))
                .expect("update request should build"),
        )
        .await
        .expect("update route should respond");

    assert_eq!(update_response.status(), StatusCode::OK);

    let delete_response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/voice/sessions/{session_id}"))
                .header("authorization", "Bearer session-1")
                .body(Body::empty())
                .expect("delete request should build"),
        )
        .await
        .expect("delete route should respond");

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let create_event = timeout(Duration::from_secs(1), events_rx.recv())
        .await
        .expect("create event timeout")
        .expect("create event should exist");
    let update_event = timeout(Duration::from_secs(1), events_rx.recv())
        .await
        .expect("update event timeout")
        .expect("update event should exist");

    match create_event {
        RealtimeEnvelope::VoiceStateChanged { speaking, .. } => assert!(!speaking),
        other => panic!("unexpected create event: {other:?}"),
    }
    match update_event {
        RealtimeEnvelope::VoiceStateChanged { speaking, .. } => assert!(speaking),
        other => panic!("unexpected update event: {other:?}"),
    }
}

#[tokio::test]
async fn voice_session_connection_bootstrap_emits_runtime_reconnect_event() {
    let sessions = Arc::new(MemorySessionStore::new());
    sessions
        .insert_for_tests("session-1", "discord-token")
        .await;

    let state = ServiceState::new(
        DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid URL")),
        sessions,
        None,
        Arc::new(UnsupportedOAuthCodeExchanger),
    );

    let mut events_rx = state.event_sender().subscribe();
    let app = router(state);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/voice/sessions")
                .header("authorization", "Bearer session-1")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                        "guild_id":"g1",
                        "channel_id":"c1",
                        "connection":{
                            "gateway_url":"ws://127.0.0.1:1",
                            "user_id":"u1",
                            "session_id":"discord-voice-session",
                            "token":"voice-token"
                        }
                    }"#,
                ))
                .expect("create request should build"),
        )
        .await
        .expect("create route should respond");
    assert_eq!(create_response.status(), StatusCode::OK);

    let create_body = to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .expect("create body should decode");
    let created: Value =
        serde_json::from_slice(&create_body).expect("create body should be valid JSON");
    let voice_session_id = created
        .get("id")
        .and_then(Value::as_str)
        .expect("voice session id should exist")
        .to_owned();

    let mut saw_reconnect_event = false;
    for _ in 0..20 {
        let event = timeout(Duration::from_millis(250), events_rx.recv())
            .await
            .ok()
            .and_then(Result::ok);
        let Some(event) = event else {
            continue;
        };

        if let RealtimeEnvelope::ReconnectScheduled {
            session_id,
            resumable,
            ..
        } = event
        {
            if session_id == "session-1" {
                assert!(!resumable);
                saw_reconnect_event = true;
                break;
            }
        }
    }
    assert!(
        saw_reconnect_event,
        "voice runtime should emit reconnect event"
    );

    let delete_response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/voice/sessions/{voice_session_id}"))
                .header("authorization", "Bearer session-1")
                .body(Body::empty())
                .expect("delete request should build"),
        )
        .await
        .expect("delete route should respond");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
}
