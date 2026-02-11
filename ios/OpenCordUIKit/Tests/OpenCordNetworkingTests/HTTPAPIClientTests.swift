import Foundation
import XCTest
@testable import OpenCordClientCore
@testable import OpenCordNetworking

private final class URLProtocolStub: URLProtocol {
    static var requestHandler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool {
        true
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        guard let handler = URLProtocolStub.requestHandler else {
            fatalError("Request handler is not set")
        }

        do {
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

final class HTTPAPIClientTests: XCTestCase {
    override func tearDown() {
        URLProtocolStub.requestHandler = nil
        super.tearDown()
    }

    func testCurrentUserUsesAuthorizationHeaderAndDecodesPayload() async throws {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [URLProtocolStub.self]
        let session = URLSession(configuration: config)

        let sessionStore = InMemoryAuthSessionStore(sessionID: "session-abc")
        let apiClient = HTTPAPIClient(
            config: OpenCordServiceConfig(baseURL: URL(string: "https://example.com")!),
            sessionStore: sessionStore,
            urlSession: session
        )

        URLProtocolStub.requestHandler = { request in
            XCTAssertEqual(request.url?.path, "/v1/me")
            XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer session-abc")

            let payload = #"{"id":"u1","username":"Nelly","global_name":"Nelly","avatar":null,"bot":false}"#
                .data(using: .utf8)!
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!
            return (response, payload)
        }

        let user = try await apiClient.currentUser()
        XCTAssertEqual(user.id, "u1")
        XCTAssertEqual(user.username, "Nelly")
    }

    func testDeleteMessageAccepts204() async throws {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [URLProtocolStub.self]
        let session = URLSession(configuration: config)

        let sessionStore = InMemoryAuthSessionStore(sessionID: "session-abc")
        let apiClient = HTTPAPIClient(
            config: OpenCordServiceConfig(baseURL: URL(string: "https://example.com")!),
            sessionStore: sessionStore,
            urlSession: session
        )

        URLProtocolStub.requestHandler = { request in
            XCTAssertEqual(request.httpMethod, "DELETE")
            XCTAssertEqual(request.url?.path, "/v1/channels/c1/messages/m1")
            let response = HTTPURLResponse(url: request.url!, statusCode: 204, httpVersion: nil, headerFields: nil)!
            return (response, Data())
        }

        try await apiClient.deleteMessage(channelID: "c1", messageID: "m1")
    }

    func testStartDiscordAuthDoesNotRequireSessionHeader() async throws {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [URLProtocolStub.self]
        let session = URLSession(configuration: config)

        let sessionStore = InMemoryAuthSessionStore(sessionID: nil)
        let apiClient = HTTPAPIClient(
            config: OpenCordServiceConfig(baseURL: URL(string: "https://example.com")!),
            sessionStore: sessionStore,
            urlSession: session
        )

        URLProtocolStub.requestHandler = { request in
            XCTAssertEqual(request.httpMethod, "POST")
            XCTAssertEqual(request.url?.path, "/v1/auth/discord/start")
            XCTAssertEqual(
                request.url?.query,
                "client_redirect_uri=opencord://auth/callback"
            )
            XCTAssertNil(request.value(forHTTPHeaderField: "Authorization"))

            let payload = #"{"authorize_url":"https://discord.com/oauth2/authorize?state=s1","state":"s1"}"#
                .data(using: .utf8)!
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!
            return (response, payload)
        }

        let authStart = try await apiClient.startDiscordAuth(
            clientRedirectURI: URL(string: "opencord://auth/callback")
        )
        XCTAssertEqual(authStart.state, "s1")
    }
}
