use discord_http::DiscordHttpClient;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use url::Url;

fn client(base: &str) -> DiscordHttpClient {
    DiscordHttpClient::new(Url::parse(base).expect("valid base URL"))
}

fn spawn_scripted_server(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server addr");

    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request_buffer = [0_u8; 4096];
            let _ = stream.read(&mut request_buffer);
            stream
                .write_all(response.as_bytes())
                .expect("write scripted response");
            stream.flush().expect("flush scripted response");
        }
    });

    (format!("http://{addr}"), handle)
}

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

#[tokio::test]
async fn retries_from_json_body_retry_after_without_headers() {
    let rate_limited = http_response(
        "429 Too Many Requests",
        &[("Content-Type", "application/json")],
        r#"{"message":"rate limited","retry_after":0.01}"#,
    );
    let success = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"ok":true}"#,
    );

    let (base_url, handle) = spawn_scripted_server(vec![rate_limited, success]);
    let client = client(&base_url);

    let response = client
        .get_json::<Value>("users/@me", None)
        .await
        .expect("request should eventually succeed");

    assert_eq!(response.get("ok").and_then(Value::as_bool), Some(true));
    handle.join().expect("server thread should exit cleanly");
}
