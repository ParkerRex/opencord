import Foundation

public typealias Snowflake = String

public struct UserDTO: Codable, Sendable, Equatable {
    public let id: Snowflake
    public let username: String
    public let globalName: String?
    public let avatar: String?
    public let bot: Bool?

    public init(id: Snowflake, username: String, globalName: String?, avatar: String?, bot: Bool?) {
        self.id = id
        self.username = username
        self.globalName = globalName
        self.avatar = avatar
        self.bot = bot
    }

    enum CodingKeys: String, CodingKey {
        case id
        case username
        case globalName = "global_name"
        case avatar
        case bot
    }
}

public struct GuildDTO: Codable, Sendable, Equatable {
    public let id: Snowflake
    public let name: String
    public let icon: String?
    public let banner: String?
    public let owner: Bool
    public let permissions: String

    public init(
        id: Snowflake,
        name: String,
        icon: String?,
        banner: String?,
        owner: Bool,
        permissions: String
    ) {
        self.id = id
        self.name = name
        self.icon = icon
        self.banner = banner
        self.owner = owner
        self.permissions = permissions
    }
}

public struct ChannelDTO: Codable, Sendable, Equatable {
    public let id: Snowflake
    public let guildID: Snowflake?
    public let name: String?
    public let topic: String?

    public init(id: Snowflake, guildID: Snowflake?, name: String?, topic: String?) {
        self.id = id
        self.guildID = guildID
        self.name = name
        self.topic = topic
    }

    enum CodingKeys: String, CodingKey {
        case id
        case guildID = "guild_id"
        case name
        case topic
    }
}

public struct MessageDTO: Codable, Sendable, Equatable {
    public let id: Snowflake
    public let channelID: Snowflake
    public let author: UserDTO
    public let content: String
    public let timestamp: String

    public init(id: Snowflake, channelID: Snowflake, author: UserDTO, content: String, timestamp: String) {
        self.id = id
        self.channelID = channelID
        self.author = author
        self.content = content
        self.timestamp = timestamp
    }

    enum CodingKeys: String, CodingKey {
        case id
        case channelID = "channel_id"
        case author
        case content
        case timestamp
    }
}

public struct VoiceSessionDTO: Codable, Sendable, Equatable {
    public let id: String
    public let guildID: Snowflake
    public let channelID: Snowflake
    public let speaking: Bool

    public init(id: String, guildID: Snowflake, channelID: Snowflake, speaking: Bool) {
        self.id = id
        self.guildID = guildID
        self.channelID = channelID
        self.speaking = speaking
    }

    enum CodingKeys: String, CodingKey {
        case id
        case guildID = "guild_id"
        case channelID = "channel_id"
        case speaking
    }
}

public struct CreateMessageRequestDTO: Codable, Sendable, Equatable {
    public let content: String

    public init(content: String) {
        self.content = content
    }
}

public struct EditMessageRequestDTO: Codable, Sendable, Equatable {
    public let content: String

    public init(content: String) {
        self.content = content
    }
}

public struct CreateVoiceSessionRequestDTO: Codable, Sendable, Equatable {
    public let guildID: Snowflake
    public let channelID: Snowflake
    public let connection: VoiceConnectionBootstrapDTO?

    public init(
        guildID: Snowflake,
        channelID: Snowflake,
        connection: VoiceConnectionBootstrapDTO? = nil
    ) {
        self.guildID = guildID
        self.channelID = channelID
        self.connection = connection
    }

    enum CodingKeys: String, CodingKey {
        case guildID = "guild_id"
        case channelID = "channel_id"
        case connection
    }
}

public struct VoiceConnectionBootstrapDTO: Codable, Sendable, Equatable {
    public let gatewayURL: String
    public let userID: String
    public let sessionID: String
    public let token: String

    public init(gatewayURL: String, userID: String, sessionID: String, token: String) {
        self.gatewayURL = gatewayURL
        self.userID = userID
        self.sessionID = sessionID
        self.token = token
    }

    enum CodingKeys: String, CodingKey {
        case gatewayURL = "gateway_url"
        case userID = "user_id"
        case sessionID = "session_id"
        case token
    }
}

public struct AuthStartResponseDTO: Codable, Sendable, Equatable {
    public let authorizeURL: String
    public let state: String

    public init(authorizeURL: String, state: String) {
        self.authorizeURL = authorizeURL
        self.state = state
    }

    enum CodingKeys: String, CodingKey {
        case authorizeURL = "authorize_url"
        case state
    }
}

public struct UpdateVoiceSpeakingRequestDTO: Codable, Sendable, Equatable {
    public let speaking: Bool

    public init(speaking: Bool) {
        self.speaking = speaking
    }
}

public enum RealtimeEnvelope: Sendable, Equatable {
    case messageCreated(sessionID: String, message: MessageDTO)
    case messageUpdated(sessionID: String, message: MessageDTO)
    case messageDeleted(sessionID: String, channelID: String, messageID: String)
    case voiceStateChanged(
        sessionID: String,
        voiceSessionID: String,
        guildID: String,
        channelID: String,
        speaking: Bool
    )
    case gatewayDispatch(sessionID: String, eventType: String, data: Data)
    case reconnectScheduled(sessionID: String, attempt: UInt32, resumable: Bool, delayMS: UInt64)
    case notification(sessionID: String?, title: String, body: String)
}

extension RealtimeEnvelope: Decodable {
    enum CodingKeys: String, CodingKey {
        case type
        case sessionID = "session_id"
        case message
        case channelID = "channel_id"
        case messageID = "message_id"
        case voiceSessionID = "voice_session_id"
        case guildID = "guild_id"
        case speaking
        case eventType = "event_type"
        case data
        case attempt
        case resumable
        case delayMS = "delay_ms"
        case title
        case body
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)

        switch type {
        case "message_created":
            self = .messageCreated(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                message: try container.decode(MessageDTO.self, forKey: .message)
            )
        case "message_updated":
            self = .messageUpdated(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                message: try container.decode(MessageDTO.self, forKey: .message)
            )
        case "message_deleted":
            self = .messageDeleted(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                channelID: try container.decode(String.self, forKey: .channelID),
                messageID: try container.decode(String.self, forKey: .messageID)
            )
        case "voice_state_changed":
            self = .voiceStateChanged(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                voiceSessionID: try container.decode(String.self, forKey: .voiceSessionID),
                guildID: try container.decode(String.self, forKey: .guildID),
                channelID: try container.decode(String.self, forKey: .channelID),
                speaking: try container.decode(Bool.self, forKey: .speaking)
            )
        case "gateway_dispatch":
            let rawData = try container.decode(ValueBox.self, forKey: .data)
            self = .gatewayDispatch(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                eventType: try container.decode(String.self, forKey: .eventType),
                data: rawData.data
            )
        case "reconnect_scheduled":
            self = .reconnectScheduled(
                sessionID: try container.decode(String.self, forKey: .sessionID),
                attempt: try container.decode(UInt32.self, forKey: .attempt),
                resumable: try container.decode(Bool.self, forKey: .resumable),
                delayMS: try container.decode(UInt64.self, forKey: .delayMS)
            )
        case "notification":
            self = .notification(
                sessionID: try container.decodeIfPresent(String.self, forKey: .sessionID),
                title: try container.decode(String.self, forKey: .title),
                body: try container.decode(String.self, forKey: .body)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type,
                in: container,
                debugDescription: "Unsupported realtime envelope type: \(type)"
            )
        }
    }
}

private struct ValueBox: Decodable {
    let data: Data

    init(from decoder: Decoder) throws {
        let rawValue = try ValueDecoder.decode(from: decoder)
        data = try JSONSerialization.data(withJSONObject: rawValue, options: [])
    }
}

private enum ValueDecoder {
    static func decode(from decoder: Decoder) throws -> Any {
        if let container = try? decoder.singleValueContainer() {
            if container.decodeNil() {
                return NSNull()
            }
            if let value = try? container.decode(Bool.self) {
                return value
            }
            if let value = try? container.decode(Double.self) {
                return value
            }
            if let value = try? container.decode(String.self) {
                return value
            }
            if let value = try? container.decode([String: DynamicValue].self) {
                return value.mapValues(\ .value)
            }
            if let value = try? container.decode([DynamicValue].self) {
                return value.map(\ .value)
            }
        }

        throw DecodingError.dataCorrupted(
            DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "Unsupported JSON value")
        )
    }
}

private struct DynamicValue: Decodable {
    let value: Any

    init(from decoder: Decoder) throws {
        value = try ValueDecoder.decode(from: decoder)
    }
}
