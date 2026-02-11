import Foundation

public enum OpenCordClientError: Error, Sendable, LocalizedError {
    case missingSession
    case invalidResponse
    case httpStatus(Int, String)
    case decoding(String)
    case transport(String)

    public var errorDescription: String? {
        switch self {
        case .missingSession:
            return "Missing authenticated session."
        case .invalidResponse:
            return "Invalid response from OpenCord service."
        case let .httpStatus(code, body):
            return "HTTP \(code): \(body)"
        case let .decoding(message):
            return "Failed to decode service payload: \(message)"
        case let .transport(message):
            return "Network transport failed: \(message)"
        }
    }
}
