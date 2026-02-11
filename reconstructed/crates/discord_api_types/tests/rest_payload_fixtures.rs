use discord_api_types::{
    AllowedMentionType, AllowedMentions, Channel, CreateMessageRequest, CurrentUserGuild,
    EditMessageRequest, GatewayBotInfo, Message, Snowflake, User,
};
use serde_json::Value;

fn fixture(path: &str) -> &'static str {
    match path {
        "users_me" => include_str!("fixtures/rest/users_me.json"),
        "gateway_bot" => include_str!("fixtures/rest/gateway_bot.json"),
        "current_user_guilds" => include_str!("fixtures/rest/current_user_guilds.json"),
        "guild_channels" => include_str!("fixtures/rest/guild_channels.json"),
        "channel_messages" => include_str!("fixtures/rest/channel_messages.json"),
        "create_message_request" => include_str!("fixtures/rest/create_message_request.json"),
        "edit_message_request" => include_str!("fixtures/rest/edit_message_request.json"),
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn deserialize_users_me_fixture() {
    let user: User = serde_json::from_str(fixture("users_me")).expect("user fixture should parse");

    assert_eq!(user.id, Snowflake::from("80351110224678912"));
    assert_eq!(user.username, "Nelly");
    assert_eq!(user.global_name.as_deref(), Some("Nelly"));
    assert_eq!(
        user.avatar.as_deref(),
        Some("8342729096ea3675442027381ff50dfe")
    );
    assert_eq!(user.bot, Some(false));
}

#[test]
fn deserialize_gateway_bot_fixture() {
    let gateway: GatewayBotInfo =
        serde_json::from_str(fixture("gateway_bot")).expect("gateway fixture should parse");

    assert_eq!(gateway.url, "wss://gateway.discord.gg");
    assert_eq!(gateway.shards, 1);
    assert_eq!(gateway.session_start_limit.total, 1000);
    assert_eq!(gateway.session_start_limit.remaining, 999);
    assert_eq!(gateway.session_start_limit.max_concurrency, 1);
}

#[test]
fn deserialize_current_user_guilds_fixture() {
    let guilds: Vec<CurrentUserGuild> = serde_json::from_str(fixture("current_user_guilds"))
        .expect("current user guilds fixture should parse");

    assert_eq!(guilds.len(), 1);
    let guild = &guilds[0];
    assert_eq!(guild.id, Snowflake::from("290926798626357250"));
    assert_eq!(guild.name, "Mason's Test Server");
    assert!(guild.owner);
    assert_eq!(guild.permissions, "36953089");
    assert_eq!(guild.features, vec!["COMMUNITY"]);
    assert_eq!(guild.approximate_member_count, Some(42));
    assert_eq!(guild.approximate_presence_count, Some(7));
}

#[test]
fn deserialize_guild_channels_fixture() {
    let channels: Vec<Channel> = serde_json::from_str(fixture("guild_channels"))
        .expect("guild channels fixture should parse");

    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].id, Snowflake::from("123"));
    assert_eq!(channels[0].name.as_deref(), Some("general"));
    assert_eq!(channels[1].id, Snowflake::from("124"));
    assert_eq!(channels[1].name.as_deref(), Some("voice"));
}

#[test]
fn deserialize_channel_messages_fixture() {
    let messages: Vec<Message> = serde_json::from_str(fixture("channel_messages"))
        .expect("channel messages fixture should parse");

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.id, Snowflake::from("111"));
    assert_eq!(message.channel_id, Snowflake::from("123"));
    assert_eq!(message.author.id, Snowflake::from("80351110224678912"));
    assert_eq!(message.content, "hello from fixture");
    assert_eq!(message.timestamp, "2024-07-01T12:34:56.789000+00:00");
}

#[test]
fn serialize_create_message_request_matches_fixture() {
    let payload = CreateMessageRequest {
        content: Some("hello world".to_owned()),
        tts: Some(false),
        nonce: Some("fixture-123".to_owned()),
        allowed_mentions: Some(AllowedMentions {
            parse: vec![AllowedMentionType::Users],
            roles: None,
            users: Some(vec![Snowflake::from("80351110224678912")]),
            replied_user: Some(false),
        }),
    };

    let actual = serde_json::to_value(payload).expect("payload should serialize");
    let expected: Value = serde_json::from_str(fixture("create_message_request"))
        .expect("create message request fixture should parse");

    assert_eq!(actual, expected);
}

#[test]
fn serialize_edit_message_request_matches_fixture() {
    let payload = EditMessageRequest {
        content: Some("edited content".to_owned()),
        allowed_mentions: Some(AllowedMentions {
            parse: vec![AllowedMentionType::Users],
            roles: None,
            users: Some(vec![Snowflake::from("80351110224678912")]),
            replied_user: Some(false),
        }),
    };

    let actual = serde_json::to_value(payload).expect("payload should serialize");
    let expected: Value = serde_json::from_str(fixture("edit_message_request"))
        .expect("edit message request fixture should parse");

    assert_eq!(actual, expected);
}
