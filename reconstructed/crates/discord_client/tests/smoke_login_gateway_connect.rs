use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_http::DiscordHttpClient;
use discord_storage::MemoryStore;
use futures_util::StreamExt;
use serde_json::json;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use url::Url;

#[derive(Debug)]
struct RequestRecord {
    method: String,
    path: String,
    authorization: Option<String>,
}

async fn read_http_request(stream: &mut TcpStream) -> io::Result<String> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }

        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(request)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "request was not valid utf-8"))
}

fn parse_request(raw: &str) -> RequestRecord {
    let mut lines = raw.lines();
    let request_line = lines.next().unwrap_or_default().trim_end_matches('\r');
    let mut request_parts = request_line.split_whitespace();

    let method = request_parts.next().unwrap_or_default().to_owned();
    let path = request_parts.next().unwrap_or_default().to_owned();

    let mut authorization = None;
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
        }
    }

    RequestRecord {
        method,
        path,
        authorization,
    }
}

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn spawn_rest_server(
    ws_url: String,
) -> io::Result<(String, mpsc::Receiver<RequestRecord>, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = mpsc::channel(4);

    let handle = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.expect("accept http connection");
            let raw_request = read_http_request(&mut stream)
                .await
                .expect("read http request");
            let request = parse_request(&raw_request);

            let (status, body) = match request.path.as_str() {
                "/users/@me" => (
                    "200 OK",
                    json!({
                        "id": "80351110224678912",
                        "username": "smoke-user",
                        "global_name": "Smoke User",
                        "avatar": null,
                        "bot": false
                    })
                    .to_string(),
                ),
                "/gateway/bot" => (
                    "200 OK",
                    json!({
                        "url": ws_url,
                        "shards": 1,
                        "session_start_limit": {
                            "total": 1000,
                            "remaining": 999,
                            "reset_after": 1,
                            "max_concurrency": 1
                        }
                    })
                    .to_string(),
                ),
                _ => (
                    "404 Not Found",
                    json!({ "code": 0, "message": "not found" }).to_string(),
                ),
            };

            let _ = tx.send(request).await;

            let response = json_response(status, &body);
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write http response");
            let _ = stream.shutdown().await;
        }
    });

    Ok((format!("http://{addr}"), rx, handle))
}

async fn spawn_gateway_server() -> io::Result<(String, oneshot::Receiver<()>, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (connected_tx, connected_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept websocket");
        let mut socket = accept_async(stream).await.expect("websocket handshake");
        let _ = connected_tx.send(());

        while let Some(message) = socket.next().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    Ok((format!("ws://{addr}"), connected_rx, handle))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_login_token_user_gateway_bot_connect() {
    let (ws_url, ws_connected_rx, ws_handle) = spawn_gateway_server()
        .await
        .expect("gateway server should start");
    let (http_base_url, mut request_rx, http_handle) = spawn_rest_server(ws_url.clone())
        .await
        .expect("rest server should start");

    let http = DiscordHttpClient::new(Url::parse(&http_base_url).expect("valid base URL"));
    let client = DiscordClient::new(http, MemoryTokenProvider::new(), MemoryStore::new());

    client
        .set_token("smoke-token".to_owned())
        .await
        .expect("token should be set");

    let user = client
        .current_user()
        .await
        .expect("/users/@me should succeed");
    assert_eq!(user.username, "smoke-user");

    let gateway_client = client
        .gateway_client()
        .await
        .expect("/gateway/bot should succeed");
    assert_eq!(
        gateway_client.gateway_url().query(),
        Some("v=10&encoding=json")
    );

    gateway_client
        .connect_once()
        .await
        .expect("gateway websocket should connect");

    timeout(Duration::from_secs(2), ws_connected_rx)
        .await
        .expect("websocket connect signal should arrive")
        .expect("websocket connect signal should be sent");

    let first_request = timeout(Duration::from_secs(2), request_rx.recv())
        .await
        .expect("first request should arrive")
        .expect("first request record should exist");
    let second_request = timeout(Duration::from_secs(2), request_rx.recv())
        .await
        .expect("second request should arrive")
        .expect("second request record should exist");

    assert_eq!(first_request.method, "GET");
    assert_eq!(first_request.path, "/users/@me");
    assert_eq!(
        first_request.authorization.as_deref(),
        Some("Bearer smoke-token")
    );

    assert_eq!(second_request.method, "GET");
    assert_eq!(second_request.path, "/gateway/bot");
    assert_eq!(
        second_request.authorization.as_deref(),
        Some("Bearer smoke-token")
    );

    http_handle.await.expect("http server task should finish");
    ws_handle.await.expect("gateway server task should finish");
}
