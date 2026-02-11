use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_http::DiscordHttpClient;
use discord_storage::{GatewaySessionRecord, MemoryStore};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use url::Url;

fn spawn_gateway_bot_server(gateway_url: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("read listener address");

    let body = format!(
        "{{\"url\":\"{gateway_url}\",\"shards\":1,\"session_start_limit\":{{\"total\":1000,\"remaining\":999,\"reset_after\":1234,\"max_concurrency\":1}}}}"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut request_buffer = [0_u8; 4096];
        let _ = stream.read(&mut request_buffer);
        stream
            .write_all(response.as_bytes())
            .expect("write HTTP response");
        stream.flush().expect("flush HTTP response");
    });

    (format!("http://{addr}"), handle)
}

fn build_client(
    base_url: &str,
    store: MemoryStore,
) -> DiscordClient<MemoryTokenProvider, MemoryStore> {
    DiscordClient::new(
        DiscordHttpClient::new(Url::parse(base_url).expect("valid base URL")),
        MemoryTokenProvider::from_token("token-123"),
        store,
    )
}

#[tokio::test]
async fn bootstrap_gateway_drops_invalid_persisted_resume_url() {
    let store = MemoryStore::new();
    let (base_url, server) = spawn_gateway_bot_server("wss://gateway.discord.gg/");
    let client = build_client(&base_url, store);

    client
        .session_manager()
        .save_gateway_session(&GatewaySessionRecord {
            session_id: "session-abc".to_owned(),
            seq: 42,
            resume_url: "not-a-url".to_owned(),
        })
        .await
        .expect("persist session");

    let startup = client
        .gateway_startup(513)
        .await
        .expect("gateway startup should succeed");

    assert_eq!(
        startup.gateway_client.gateway_url().as_str(),
        "wss://gateway.discord.gg/?v=10&encoding=json"
    );
    assert!(!startup.state_machine.can_resume());

    let persisted = client
        .session_manager()
        .load_gateway_session()
        .await
        .expect("load persisted session");
    assert!(persisted.is_none());

    server.join().expect("server thread should exit");
}
