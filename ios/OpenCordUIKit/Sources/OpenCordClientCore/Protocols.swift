import Foundation

public protocol OpenCordAuthSessionStore: Sendable {
    var currentSessionID: String? { get }
    func setSessionID(_ sessionID: String?)
}

public protocol OpenCordAPIClient: Sendable {
    func startDiscordAuth(clientRedirectURI: URL?) async throws -> AuthStartResponseDTO
    func logout() async throws
    func currentUser() async throws -> UserDTO
    func listGuilds() async throws -> [GuildDTO]
    func listChannels(guildID: Snowflake) async throws -> [ChannelDTO]
    func listMessages(channelID: Snowflake, limit: Int?) async throws -> [MessageDTO]
    func createMessage(channelID: Snowflake, request: CreateMessageRequestDTO) async throws -> MessageDTO
    func editMessage(channelID: Snowflake, messageID: Snowflake, request: EditMessageRequestDTO) async throws -> MessageDTO
    func deleteMessage(channelID: Snowflake, messageID: Snowflake) async throws
    func createVoiceSession(request: CreateVoiceSessionRequestDTO) async throws -> VoiceSessionDTO
    func updateVoiceSpeaking(voiceSessionID: String, request: UpdateVoiceSpeakingRequestDTO) async throws -> VoiceSessionDTO
    func deleteVoiceSession(voiceSessionID: String) async throws
}

public protocol OpenCordRealtimeClient: Sendable {
    func connect() async throws
    func disconnect() async
    var events: AsyncStream<RealtimeEnvelope> { get }
}

public struct OpenCordServiceConfig: Sendable, Equatable {
    public let baseURL: URL

    public init(baseURL: URL) {
        self.baseURL = baseURL
    }
}

public final class InMemoryAuthSessionStore: OpenCordAuthSessionStore, @unchecked Sendable {
    private let lock = NSLock()
    private var sessionIDStorage: String?

    public init(sessionID: String? = nil) {
        sessionIDStorage = sessionID
    }

    public var currentSessionID: String? {
        lock.lock()
        defer { lock.unlock() }
        return sessionIDStorage
    }

    public func setSessionID(_ sessionID: String?) {
        lock.lock()
        sessionIDStorage = sessionID
        lock.unlock()
    }
}

public final class UserDefaultsAuthSessionStore: OpenCordAuthSessionStore, @unchecked Sendable {
    private let lock = NSLock()
    private let defaults: UserDefaults
    private let key: String

    public init(
        defaults: UserDefaults = .standard,
        key: String = "opencord.auth.session_id"
    ) {
        self.defaults = defaults
        self.key = key
    }

    public var currentSessionID: String? {
        lock.lock()
        defer { lock.unlock() }
        return defaults.string(forKey: key)
    }

    public func setSessionID(_ sessionID: String?) {
        lock.lock()
        if let sessionID {
            defaults.set(sessionID, forKey: key)
        } else {
            defaults.removeObject(forKey: key)
        }
        lock.unlock()
    }
}
