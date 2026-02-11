use discord_api_types::{
    CreateMessageRequest, EditMessageRequest, GetChannelMessagesQuery, GetCurrentUserGuildsQuery,
};
use discord_auth::{AuthError, MemoryTokenProvider};
use discord_client::{ClientError, DiscordClient};
use discord_gateway::{GatewaySession, GatewayStateMachine, GatewayStateMachineConfig};
use discord_http::DiscordHttpClient;
use discord_storage::{GatewaySessionRecord, MemoryStore};
use serde_json::json;
use url::Url;

fn build_client() -> DiscordClient<MemoryTokenProvider, MemoryStore> {
    DiscordClient::new(
        DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid URL")),
        MemoryTokenProvider::new(),
        MemoryStore::new(),
    )
}

async fn _client_methods(client: &DiscordClient<MemoryTokenProvider, MemoryStore>) {
    let _ = client.set_token("token".to_owned()).await;
    let _ = client.clear_token().await;
    let _ = client.token().await;
    let _ = client.current_user().await;
    let _ = client.gateway_bot().await;
    let _ = client.gateway_client().await;
    let _ = client.gateway_startup(513).await;
    let _ = client
        .list_guilds(GetCurrentUserGuildsQuery::default())
        .await;
    let _ = client.list_channels("123").await;
    let _ = client
        .list_messages("123", GetChannelMessagesQuery::default())
        .await;
    let _ = client
        .create_message("123", CreateMessageRequest::default())
        .await;
    let _ = client
        .edit_message("123", "456", EditMessageRequest::default())
        .await;
    let _ = client.delete_message("123", "456").await;

    let _ = client.cache_set_json("key", &json!({"ok": true})).await;
    let _: Result<Option<serde_json::Value>, _> = client.cache_get_json("key").await;
    let _ = client.cache_delete("key").await;

    let manager = client.session_manager();
    let _ = manager.bootstrap_gateway(513).await;
    let _ = manager
        .save_gateway_session(&GatewaySessionRecord {
            session_id: "session".to_owned(),
            seq: 1,
            resume_url: "wss://gateway.discord.gg".to_owned(),
        })
        .await;
    let _ = manager.load_gateway_session().await;
    let _ = manager.clear_gateway_session().await;

    let mut machine =
        GatewayStateMachine::new(GatewayStateMachineConfig::new("token".to_owned(), 513));
    machine.restore_session_with_resume_url(
        GatewaySession {
            session_id: "session".to_owned(),
            sequence: 1,
        },
        "wss://gateway.discord.gg".to_owned(),
    );

    let _ = manager
        .persist_from_state_machine(
            &machine,
            &Url::parse("wss://gateway.discord.gg").expect("valid URL"),
        )
        .await;
}

#[test]
fn client_public_api_lock() {
    let client = build_client();
    let _ = client.http();

    let _ = ClientError::Auth(AuthError::MissingToken);
}
