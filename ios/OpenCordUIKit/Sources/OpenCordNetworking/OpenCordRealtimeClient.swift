import Foundation
import OpenCordClientCore

public final class ServiceRealtimeClient: OpenCordRealtimeClient, @unchecked Sendable {
    public let events: AsyncStream<RealtimeEnvelope>

    private let config: OpenCordServiceConfig
    private let sessionStore: OpenCordAuthSessionStore
    private let urlSession: URLSession
    private let decoder: JSONDecoder
    private let continuation: AsyncStream<RealtimeEnvelope>.Continuation

    private var socketTask: URLSessionWebSocketTask?
    private var receiverTask: Task<Void, Never>?

    public init(
        config: OpenCordServiceConfig,
        sessionStore: OpenCordAuthSessionStore,
        urlSession: URLSession = .shared
    ) {
        self.config = config
        self.sessionStore = sessionStore
        self.urlSession = urlSession
        self.decoder = JSONDecoder()

        var continuationStorage: AsyncStream<RealtimeEnvelope>.Continuation?
        self.events = AsyncStream<RealtimeEnvelope> { continuation in
            continuationStorage = continuation
        }
        self.continuation = continuationStorage!
    }

    deinit {
        receiverTask?.cancel()
        socketTask?.cancel(with: .goingAway, reason: nil)
        continuation.finish()
    }

    public func connect() async throws {
        guard socketTask == nil else {
            return
        }

        guard let sessionID = sessionStore.currentSessionID else {
            throw OpenCordClientError.missingSession
        }

        let components = URLComponents(url: config.baseURL.appendingPathComponent("/v1/stream"), resolvingAgainstBaseURL: false)
        guard let url = components?.url else {
            throw OpenCordClientError.invalidResponse
        }

        guard var wsComponents = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            throw OpenCordClientError.invalidResponse
        }
        if wsComponents.scheme == "https" {
            wsComponents.scheme = "wss"
        } else if wsComponents.scheme == "http" {
            wsComponents.scheme = "ws"
        }

        guard let wsURL = wsComponents.url else {
            throw OpenCordClientError.invalidResponse
        }

        var request = URLRequest(url: wsURL)
        request.setValue("Bearer \(sessionID)", forHTTPHeaderField: "Authorization")

        let task = urlSession.webSocketTask(with: request)
        task.resume()
        socketTask = task

        receiverTask = Task { [weak self] in
            guard let self else { return }

            while !Task.isCancelled {
                do {
                    let message = try await task.receive()
                    switch message {
                    case let .string(text):
                        guard let data = text.data(using: .utf8) else { continue }
                        if let envelope = try? decoder.decode(RealtimeEnvelope.self, from: data) {
                            continuation.yield(envelope)
                        }
                    case let .data(data):
                        if let envelope = try? decoder.decode(RealtimeEnvelope.self, from: data) {
                            continuation.yield(envelope)
                        }
                    @unknown default:
                        continue
                    }
                } catch {
                    break
                }
            }

            socketTask = nil
            receiverTask = nil
        }
    }

    public func disconnect() async {
        receiverTask?.cancel()
        receiverTask = nil

        socketTask?.cancel(with: .normalClosure, reason: nil)
        socketTask = nil
    }
}
