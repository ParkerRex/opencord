use discord_api_types::{
    CreateMessageRequest, EditMessageRequest, GetChannelMessagesQuery, GetCurrentUserGuildsQuery,
};
use discord_http::{DiscordHttpClient, HttpError};
use url::Url;

fn build_client() -> DiscordHttpClient {
    DiscordHttpClient::new(Url::parse("https://discord.com/api/v10").expect("valid URL"))
}

async fn _http_methods(client: &DiscordHttpClient) {
    let _ = client
        .get_json::<serde_json::Value>("users/@me", None)
        .await;
    let _ = client
        .get_json_with_query::<GetCurrentUserGuildsQuery, serde_json::Value>(
            "users/@me/guilds",
            &GetCurrentUserGuildsQuery::default(),
            None,
        )
        .await;
    let _ = client
        .post_json::<CreateMessageRequest, serde_json::Value>(
            "channels/123/messages",
            &CreateMessageRequest::default(),
            None,
        )
        .await;
    let _ = client
        .patch_json::<EditMessageRequest, serde_json::Value>(
            "channels/123/messages/456",
            &EditMessageRequest::default(),
            None,
        )
        .await;
    let _ = client.delete_empty("channels/123/messages/456", None).await;

    let _ = client.get_current_user("token").await;
    let _ = client.get_gateway_bot("token").await;
    let _ = client
        .get_current_user_guilds("token", GetCurrentUserGuildsQuery::default())
        .await;
    let _ = client.get_guild_channels("123", "token").await;
    let _ = client
        .get_channel_messages("123", GetChannelMessagesQuery::default(), "token")
        .await;
    let _ = client
        .create_message("123", CreateMessageRequest::default(), "token")
        .await;
    let _ = client
        .edit_message("123", "456", EditMessageRequest::default(), "token")
        .await;
    let _ = client.delete_message("123", "456", "token").await;
}

#[test]
fn http_public_api_lock() {
    let client = build_client();
    let _ = client.base_url();

    let _ = HttpError::InvalidPath("path".to_owned());
    let _ = HttpError::Status {
        status: reqwest::StatusCode::BAD_REQUEST,
        body: "body".to_owned(),
    };
}
