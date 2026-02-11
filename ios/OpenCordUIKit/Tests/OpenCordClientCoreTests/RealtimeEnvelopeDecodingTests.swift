import XCTest
@testable import OpenCordClientCore

final class RealtimeEnvelopeDecodingTests: XCTestCase {
    func testDecodesMessageCreatedEnvelope() throws {
        let payload = #"{"type":"message_created","session_id":"session-1","message":{"id":"m1","channel_id":"c1","author":{"id":"u1","username":"Nelly","global_name":"Nelly","avatar":null,"bot":false},"content":"hello","timestamp":"2026-02-11T00:00:00Z"}}"#
            .data(using: .utf8)!

        let envelope = try JSONDecoder().decode(RealtimeEnvelope.self, from: payload)

        switch envelope {
        case let .messageCreated(sessionID, message):
            XCTAssertEqual(sessionID, "session-1")
            XCTAssertEqual(message.id, "m1")
            XCTAssertEqual(message.content, "hello")
        default:
            XCTFail("Unexpected envelope")
        }
    }

    func testDecodesNotificationEnvelope() throws {
        let payload = #"{"type":"notification","session_id":null,"title":"gateway_shutdown","body":"gateway runtime stopped"}"#
            .data(using: .utf8)!

        let envelope = try JSONDecoder().decode(RealtimeEnvelope.self, from: payload)

        switch envelope {
        case let .notification(sessionID, title, body):
            XCTAssertNil(sessionID)
            XCTAssertEqual(title, "gateway_shutdown")
            XCTAssertEqual(body, "gateway runtime stopped")
        default:
            XCTFail("Unexpected envelope")
        }
    }
}
