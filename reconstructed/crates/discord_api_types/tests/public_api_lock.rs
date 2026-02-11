use discord_api_types::routes::{
    CreateChannelMessage, GetChannelMessages, GetCurrentUser, GetCurrentUserGuilds, GetGatewayBot,
    GetGuildChannels, JsonBodyRoute, QueryRoute, Route,
};
use discord_api_types::{
    AllowedMentionType, AllowedMentions, ChannelType, CreateMessageRequest,
    GetChannelMessagesQuery, GetCurrentUserGuildsQuery, Snowflake,
};

#[test]
fn api_types_public_api_lock() {
    let _snowflake = Snowflake::from("123");

    let _ = ChannelType::from(0_u16);
    let _: u16 = ChannelType::GuildText.into();

    let _mentions = AllowedMentions {
        parse: vec![AllowedMentionType::Everyone],
        ..Default::default()
    };

    let get_current_user = GetCurrentUser;
    let _ = get_current_user.path();

    let get_gateway_bot = GetGatewayBot;
    let _ = get_gateway_bot.path();

    let get_current_user_guilds = GetCurrentUserGuilds {
        query: GetCurrentUserGuildsQuery::default(),
    };
    let _: &GetCurrentUserGuildsQuery = get_current_user_guilds.query();
    let _ = get_current_user_guilds.path();

    let get_guild_channels = GetGuildChannels {
        guild_id: Snowflake::from("1"),
    };
    let _ = get_guild_channels.path();

    let get_channel_messages = GetChannelMessages {
        channel_id: Snowflake::from("2"),
        query: GetChannelMessagesQuery::default(),
    };
    let _: &GetChannelMessagesQuery = get_channel_messages.query();
    let _ = get_channel_messages.path();

    let create_channel_message = CreateChannelMessage {
        channel_id: Snowflake::from("3"),
        body: CreateMessageRequest::default(),
    };
    let _: &CreateMessageRequest = create_channel_message.body();
    let _ = create_channel_message.path();
}
