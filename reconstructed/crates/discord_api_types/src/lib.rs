use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Snowflake(pub String);

impl From<String> for Snowflake {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Snowflake {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Display for Snowflake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Snowflake,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub bot: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guild {
    pub id: Snowflake,
    pub name: String,
    pub icon: Option<String>,
    pub owner_id: Option<Snowflake>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ChannelType {
    GuildText,
    Dm,
    GuildVoice,
    GroupDm,
    GuildCategory,
    GuildAnnouncement,
    AnnouncementThread,
    PublicThread,
    PrivateThread,
    GuildStageVoice,
    GuildDirectory,
    GuildForum,
    GuildMedia,
    Unknown(u16),
}

impl From<u16> for ChannelType {
    fn from(value: u16) -> Self {
        match value {
            0 => Self::GuildText,
            1 => Self::Dm,
            2 => Self::GuildVoice,
            3 => Self::GroupDm,
            4 => Self::GuildCategory,
            5 => Self::GuildAnnouncement,
            10 => Self::AnnouncementThread,
            11 => Self::PublicThread,
            12 => Self::PrivateThread,
            13 => Self::GuildStageVoice,
            14 => Self::GuildDirectory,
            15 => Self::GuildForum,
            16 => Self::GuildMedia,
            other => Self::Unknown(other),
        }
    }
}

impl From<ChannelType> for u16 {
    fn from(value: ChannelType) -> Self {
        match value {
            ChannelType::GuildText => 0,
            ChannelType::Dm => 1,
            ChannelType::GuildVoice => 2,
            ChannelType::GroupDm => 3,
            ChannelType::GuildCategory => 4,
            ChannelType::GuildAnnouncement => 5,
            ChannelType::AnnouncementThread => 10,
            ChannelType::PublicThread => 11,
            ChannelType::PrivateThread => 12,
            ChannelType::GuildStageVoice => 13,
            ChannelType::GuildDirectory => 14,
            ChannelType::GuildForum => 15,
            ChannelType::GuildMedia => 16,
            ChannelType::Unknown(other) => other,
        }
    }
}

impl Serialize for ChannelType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u16((*self).into())
    }
}

impl<'de> Deserialize<'de> for ChannelType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Ok(Self::from(value))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: Snowflake,
    pub guild_id: Option<Snowflake>,
    pub name: Option<String>,
    pub topic: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<ChannelType>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    pub author: User,
    pub content: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionStartLimit {
    pub total: u32,
    pub remaining: u32,
    pub reset_after: u64,
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayBotInfo {
    pub url: String,
    pub shards: u32,
    pub session_start_limit: SessionStartLimit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentUserGuild {
    pub id: Snowflake,
    pub name: String,
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub owner: bool,
    pub permissions: String,
    #[serde(default)]
    pub features: Vec<String>,
    pub approximate_member_count: Option<u64>,
    pub approximate_presence_count: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GetCurrentUserGuildsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_counts: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GetChannelMessagesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub around: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedMentionType {
    Roles,
    Users,
    Everyone,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AllowedMentions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse: Vec<AllowedMentionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_user: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CreateMessageRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: i64,
    pub message: String,
}

pub mod routes {
    use super::{
        Channel, CreateMessageRequest, CurrentUserGuild, GatewayBotInfo, GetChannelMessagesQuery,
        GetCurrentUserGuildsQuery, Message, Snowflake, User,
    };

    pub trait Route {
        type Response;
        fn path(&self) -> String;
    }

    pub trait QueryRoute: Route {
        type Query;
        fn query(&self) -> &Self::Query;
    }

    pub trait JsonBodyRoute: Route {
        type Body;
        fn body(&self) -> &Self::Body;
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct GetCurrentUser;

    impl Route for GetCurrentUser {
        type Response = User;

        fn path(&self) -> String {
            "users/@me".to_owned()
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct GetGatewayBot;

    impl Route for GetGatewayBot {
        type Response = GatewayBotInfo;

        fn path(&self) -> String {
            "gateway/bot".to_owned()
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct GetCurrentUserGuilds {
        pub query: GetCurrentUserGuildsQuery,
    }

    impl Route for GetCurrentUserGuilds {
        type Response = Vec<CurrentUserGuild>;

        fn path(&self) -> String {
            "users/@me/guilds".to_owned()
        }
    }

    impl QueryRoute for GetCurrentUserGuilds {
        type Query = GetCurrentUserGuildsQuery;

        fn query(&self) -> &Self::Query {
            &self.query
        }
    }

    #[derive(Clone, Debug)]
    pub struct GetGuildChannels {
        pub guild_id: Snowflake,
    }

    impl Route for GetGuildChannels {
        type Response = Vec<Channel>;

        fn path(&self) -> String {
            format!("guilds/{}/channels", self.guild_id)
        }
    }

    #[derive(Clone, Debug)]
    pub struct GetChannelMessages {
        pub channel_id: Snowflake,
        pub query: GetChannelMessagesQuery,
    }

    impl Route for GetChannelMessages {
        type Response = Vec<Message>;

        fn path(&self) -> String {
            format!("channels/{}/messages", self.channel_id)
        }
    }

    impl QueryRoute for GetChannelMessages {
        type Query = GetChannelMessagesQuery;

        fn query(&self) -> &Self::Query {
            &self.query
        }
    }

    #[derive(Clone, Debug)]
    pub struct CreateChannelMessage {
        pub channel_id: Snowflake,
        pub body: CreateMessageRequest,
    }

    impl Route for CreateChannelMessage {
        type Response = Message;

        fn path(&self) -> String {
            format!("channels/{}/messages", self.channel_id)
        }
    }

    impl JsonBodyRoute for CreateChannelMessage {
        type Body = CreateMessageRequest;

        fn body(&self) -> &Self::Body {
            &self.body
        }
    }
}

#[derive(Debug, Error)]
pub enum ApiTypeError {
    #[error("missing required token")]
    MissingToken,
}
