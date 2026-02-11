use discord_voice::{
    VoiceConnectionConfig, VoiceGatewayClient, VoiceGatewayEvent, VoiceRuntimeEvent,
    VoiceRuntimeOptions, discover_udp_address,
};
use url::Url;

fn build_client() -> VoiceGatewayClient {
    VoiceGatewayClient::new(Url::parse("wss://gateway.discord.gg").expect("valid URL"))
}

async fn _voice_methods(client: &VoiceGatewayClient) {
    let _ = client.connect_once().await;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let _ = shutdown_tx;

    let _ = client
        .run_with_shutdown(
            VoiceConnectionConfig {
                guild_id: "guild".to_owned(),
                user_id: "user".to_owned(),
                session_id: "session".to_owned(),
                token: "token".to_owned(),
            },
            VoiceRuntimeOptions::default(),
            shutdown_rx,
            |_| {},
        )
        .await;

    let _ = discover_udp_address("127.0.0.1", 5000, 1).await;
}

#[test]
fn voice_public_api_lock() {
    let client = build_client();
    let _ = client.gateway_url();

    let _ = VoiceRuntimeEvent::Shutdown;
    let _ = VoiceRuntimeEvent::ReconnectScheduled {
        attempt: 1,
        delay: std::time::Duration::from_millis(10),
    };
    let _ = VoiceRuntimeEvent::GatewayEvent(VoiceGatewayEvent::HeartbeatAck);
}
