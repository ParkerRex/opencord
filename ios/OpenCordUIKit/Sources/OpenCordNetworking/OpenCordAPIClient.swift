import Foundation
import OpenCordClientCore

public final class HTTPAPIClient: OpenCordAPIClient, @unchecked Sendable {
    private let config: OpenCordServiceConfig
    private let sessionStore: OpenCordAuthSessionStore
    private let urlSession: URLSession
    private let jsonDecoder: JSONDecoder
    private let jsonEncoder: JSONEncoder

    public init(
        config: OpenCordServiceConfig,
        sessionStore: OpenCordAuthSessionStore,
        urlSession: URLSession = .shared
    ) {
        self.config = config
        self.sessionStore = sessionStore
        self.urlSession = urlSession
        self.jsonDecoder = JSONDecoder()
        self.jsonEncoder = JSONEncoder()
    }

    public func startDiscordAuth(clientRedirectURI: URL?) async throws -> AuthStartResponseDTO {
        var queryItems: [URLQueryItem] = []
        if let clientRedirectURI {
            queryItems.append(
                URLQueryItem(name: "client_redirect_uri", value: clientRedirectURI.absoluteString)
            )
        }

        return try await send(
            path: "/v1/auth/discord/start",
            method: "POST",
            queryItems: queryItems,
            body: nil,
            authRequirement: .none
        )
    }

    public func logout() async throws {
        _ = try await sendEmpty(
            path: "/v1/auth/logout",
            method: "POST",
            authRequirement: .required
        )
    }

    public func currentUser() async throws -> UserDTO {
        try await send(path: "/v1/me", method: "GET", body: Optional<Data>.none, authRequirement: .required)
    }

    public func listGuilds() async throws -> [GuildDTO] {
        try await send(path: "/v1/guilds", method: "GET", body: Optional<Data>.none, authRequirement: .required)
    }

    public func listChannels(guildID: Snowflake) async throws -> [ChannelDTO] {
        try await send(
            path: "/v1/guilds/\(guildID)/channels",
            method: "GET",
            body: Optional<Data>.none,
            authRequirement: .required
        )
    }

    public func listMessages(channelID: Snowflake, limit: Int?) async throws -> [MessageDTO] {
        var queryItems: [URLQueryItem] = []
        if let limit {
            queryItems.append(URLQueryItem(name: "limit", value: String(limit)))
        }

        return try await send(
            path: "/v1/channels/\(channelID)/messages",
            method: "GET",
            queryItems: queryItems,
            body: Optional<Data>.none,
            authRequirement: .required
        )
    }

    public func createMessage(channelID: Snowflake, request: CreateMessageRequestDTO) async throws -> MessageDTO {
        let body = try jsonEncoder.encode(request)
        return try await send(
            path: "/v1/channels/\(channelID)/messages",
            method: "POST",
            body: body,
            authRequirement: .required
        )
    }

    public func editMessage(
        channelID: Snowflake,
        messageID: Snowflake,
        request: EditMessageRequestDTO
    ) async throws -> MessageDTO {
        let body = try jsonEncoder.encode(request)
        return try await send(
            path: "/v1/channels/\(channelID)/messages/\(messageID)",
            method: "PATCH",
            body: body,
            authRequirement: .required
        )
    }

    public func deleteMessage(channelID: Snowflake, messageID: Snowflake) async throws {
        _ = try await sendEmpty(
            path: "/v1/channels/\(channelID)/messages/\(messageID)",
            method: "DELETE",
            authRequirement: .required
        )
    }

    public func createVoiceSession(request: CreateVoiceSessionRequestDTO) async throws -> VoiceSessionDTO {
        let body = try jsonEncoder.encode(request)
        return try await send(
            path: "/v1/voice/sessions",
            method: "POST",
            body: body,
            authRequirement: .required
        )
    }

    public func updateVoiceSpeaking(
        voiceSessionID: String,
        request: UpdateVoiceSpeakingRequestDTO
    ) async throws -> VoiceSessionDTO {
        let body = try jsonEncoder.encode(request)
        return try await send(
            path: "/v1/voice/sessions/\(voiceSessionID)/speaking",
            method: "POST",
            body: body,
            authRequirement: .required
        )
    }

    public func deleteVoiceSession(voiceSessionID: String) async throws {
        _ = try await sendEmpty(
            path: "/v1/voice/sessions/\(voiceSessionID)",
            method: "DELETE",
            authRequirement: .required
        )
    }

    private enum AuthRequirement {
        case none
        case required
    }

    private func send<Response: Decodable>(
        path: String,
        method: String,
        queryItems: [URLQueryItem] = [],
        body: Data?,
        authRequirement: AuthRequirement
    ) async throws -> Response {
        let request = try makeRequest(
            path: path,
            method: method,
            queryItems: queryItems,
            body: body,
            authRequirement: authRequirement
        )
        let (data, response) = try await perform(request)

        guard let http = response as? HTTPURLResponse else {
            throw OpenCordClientError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            let bodyText = String(data: data, encoding: .utf8) ?? ""
            throw OpenCordClientError.httpStatus(http.statusCode, bodyText)
        }

        do {
            return try jsonDecoder.decode(Response.self, from: data)
        } catch {
            throw OpenCordClientError.decoding(error.localizedDescription)
        }
    }

    private func sendEmpty(
        path: String,
        method: String,
        queryItems: [URLQueryItem] = [],
        body: Data? = nil,
        authRequirement: AuthRequirement
    ) async throws -> Int {
        let request = try makeRequest(
            path: path,
            method: method,
            queryItems: queryItems,
            body: body,
            authRequirement: authRequirement
        )
        let (data, response) = try await perform(request)

        guard let http = response as? HTTPURLResponse else {
            throw OpenCordClientError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            let bodyText = String(data: data, encoding: .utf8) ?? ""
            throw OpenCordClientError.httpStatus(http.statusCode, bodyText)
        }

        return http.statusCode
    }

    private func makeRequest(
        path: String,
        method: String,
        queryItems: [URLQueryItem],
        body: Data?,
        authRequirement: AuthRequirement
    ) throws -> URLRequest {
        guard var components = URLComponents(url: config.baseURL.appendingPathComponent(path), resolvingAgainstBaseURL: false) else {
            throw OpenCordClientError.invalidResponse
        }

        components.queryItems = queryItems.isEmpty ? nil : queryItems

        guard let url = components.url else {
            throw OpenCordClientError.invalidResponse
        }

        var request = URLRequest(url: url)
        request.httpMethod = method

        switch authRequirement {
        case .none:
            break
        case .required:
            guard let sessionID = sessionStore.currentSessionID else {
                throw OpenCordClientError.missingSession
            }
            request.setValue("Bearer \(sessionID)", forHTTPHeaderField: "Authorization")
        }

        if let body {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }

        return request
    }

    private func perform(_ request: URLRequest) async throws -> (Data, URLResponse) {
        do {
            return try await urlSession.data(for: request)
        } catch {
            throw OpenCordClientError.transport(error.localizedDescription)
        }
    }
}
