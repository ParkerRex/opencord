use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_http::DiscordHttpClient;
use discord_storage::MemoryStore;
use serde_json::json;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

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

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn spawn_gateway_bot_server(gateway_url: String) -> io::Result<(String, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept connection");
        let request = read_http_request(&mut stream)
            .await
            .expect("read request");
        assert!(request.starts_with("GET /gateway/bot HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("authorization: bearer smoke-token"));

        let body = json!({
            "url": gateway_url,
            "shards": 1,
            "session_start_limit": {
                "total": 1000,
                "remaining": 999,
                "reset_after": 1,
                "max_concurrency": 1
            }
        })
        .to_string();

        let response = json_response("200 OK", &body);
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response");
        let _ = stream.shutdown().await;
    });

    Ok((format!("http://{addr}"), handle))
}

fn build_client(base_url: &str) -> DiscordClient<MemoryTokenProvider, MemoryStore> {
    let http = DiscordHttpClient::new(Url::parse(base_url).expect("valid base URL"));
    DiscordClient::new(http, MemoryTokenProvider::new(), MemoryStore::new())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_client_preserves_existing_gateway_query() {
    let (base_url, server) = spawn_gateway_bot_server(
        "wss://gateway.discord.gg/?v=9&encoding=etf".to_owned(),
    )
    .await
    .expect("gateway bot server should start");

    let client = build_client(&base_url);
    client
        .set_token("smoke-token".to_owned())
        .await
        .expect("set token");

    let gateway_client = client.gateway_client().await.expect("gateway client");
    assert_eq!(gateway_client.gateway_url().query(), Some("v=9&encoding=etf"));

    server.await.expect("server task should finish");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_client_keeps_non_websocket_url_unchanged() {
    let (base_url, server) = spawn_gateway_bot_server("https://gateway.discord.gg/path".to_owned())
        .await
        .expect("gateway bot server should start");

    let client = build_client(&base_url);
    client
        .set_token("smoke-token".to_owned())
        .await
        .expect("set token");

    let gateway_client = client.gateway_client().await.expect("gateway client");
    assert_eq!(gateway_client.gateway_url().scheme(), "https");
    assert_eq!(gateway_client.gateway_url().query(), None);
    assert_eq!(gateway_client.gateway_url().path(), "/path");

    server.await.expect("server task should finish");
}
