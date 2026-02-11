use discord_api_types::EditMessageRequest;
use discord_http::DiscordHttpClient;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use url::Url;

fn client(base: &str) -> DiscordHttpClient {
    DiscordHttpClient::new(Url::parse(base).expect("valid base URL"))
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

fn spawn_single_response_server(
    response: String,
) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut request_buffer = [0_u8; 16384];
        let bytes_read = stream.read(&mut request_buffer).expect("read request");
        let request = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
        tx.send(request).expect("send captured request");

        stream
            .write_all(response.as_bytes())
            .expect("write scripted response");
        stream.flush().expect("flush scripted response");
    });

    (format!("http://{addr}"), rx, handle)
}

#[tokio::test]
async fn edit_message_uses_patch_route_and_returns_message() {
    let response_body = r#"{"id":"456","channel_id":"123","author":{"id":"80351110224678912","username":"Nelly","global_name":"Nelly","avatar":"8342729096ea3675442027381ff50dfe","bot":false},"content":"edited","timestamp":"2024-07-01T12:34:56.789000+00:00"}"#;
    let response = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        response_body,
    );

    let (base_url, rx, handle) = spawn_single_response_server(response);
    let client = client(&base_url);

    let message = client
        .edit_message(
            "123",
            "456",
            EditMessageRequest {
                content: Some("edited".to_owned()),
                ..Default::default()
            },
            "token-123",
        )
        .await
        .expect("edit message should succeed");

    assert_eq!(message.id.0, "456");

    let request = rx.recv().expect("captured request");
    let request_lower = request.to_lowercase();
    assert!(request.starts_with("PATCH /channels/123/messages/456 HTTP/1.1"));
    assert!(request_lower.contains("authorization: bearer token-123"));
    assert!(request.contains("\"content\":\"edited\""));

    handle.join().expect("server thread should exit cleanly");
}

#[tokio::test]
async fn delete_message_uses_delete_route_and_accepts_no_content() {
    let response = http_response("204 No Content", &[], "");
    let (base_url, rx, handle) = spawn_single_response_server(response);
    let client = client(&base_url);

    client
        .delete_message("123", "456", "token-123")
        .await
        .expect("delete message should succeed");

    let request = rx.recv().expect("captured request");
    let request_lower = request.to_lowercase();
    assert!(request.starts_with("DELETE /channels/123/messages/456 HTTP/1.1"));
    assert!(request_lower.contains("authorization: bearer token-123"));

    handle.join().expect("server thread should exit cleanly");
}
