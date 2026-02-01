//
// MOTTO GENERATED CODE - DO NOT EDIT
//
// Protocol Version: 0xA1
// Schema Fingerprint: a15f1919c06e581b
// Generated At: 2026-02-01T06:43:23.447495157+00:00
//

import Foundation


// MARK: - Runtime

/// Connection state machine
public enum ConnectionState: Sendable {
    case disconnected
    case connecting
    case connected
    case reconnecting
    case error(Error)
}

/// Retry configuration
public struct RetryConfig: Sendable {
    public let maxRetries: Int
    public let initialDelayMs: Int
    public let maxDelayMs: Int
    public let backoffMultiplier: Double
    
    public static let `default` = RetryConfig(
        maxRetries: 5,
        initialDelayMs: 100,
        maxDelayMs: 30000,
        backoffMultiplier: 2.0
    )
    
    public init(maxRetries: Int, initialDelayMs: Int, maxDelayMs: Int, backoffMultiplier: Double) {
        self.maxRetries = maxRetries
        self.initialDelayMs = initialDelayMs
        self.maxDelayMs = maxDelayMs
        self.backoffMultiplier = backoffMultiplier
    }
}

/// Calculate retry delay with exponential backoff
public func calculateRetryDelay(attempt: Int, config: RetryConfig = .default) -> Int {
    let delay = Double(config.initialDelayMs) * pow(config.backoffMultiplier, Double(attempt))
    return min(Int(delay), config.maxDelayMs)
}

/// Motto transport protocol
public protocol MottoTransportProtocol: AnyObject, Sendable {
    var state: ConnectionState { get }
    func connect() async throws
    func disconnect() async
    func send(_ data: Data) async throws
    func receive() async throws -> Data
}

#if canImport(Network)
import Network

/// WebTransport-like connection using Network.framework
@available(iOS 15.0, macOS 12.0, *)
public actor MottoTransport: MottoTransportProtocol {
    private let url: URL
    private let retryConfig: RetryConfig
    private var connection: NWConnection?
    private var retryAttempt: Int = 0
    
    public private(set) var state: ConnectionState = .disconnected
    
    public init(url: URL, retryConfig: RetryConfig = .default) {
        self.url = url
        self.retryConfig = retryConfig
    }
    
    public func connect() async throws {
        state = .connecting
        
        // Note: This is a simplified TCP connection
        // Real WebTransport requires HTTP/3 + QUIC support
        let endpoint = NWEndpoint.url(url)!
        let parameters = NWParameters.tcp
        
        connection = NWConnection(to: endpoint, using: parameters)
        
        return try await withCheckedThrowingContinuation { continuation in
            connection?.stateUpdateHandler = { [weak self] newState in
                Task {
                    switch newState {
                    case .ready:
                        await self?.setState(.connected)
                        continuation.resume()
                    case .failed(let error):
                        await self?.setState(.error(error))
                        continuation.resume(throwing: error)
                    default:
                        break
                    }
                }
            }
            connection?.start(queue: .global())
        }
    }
    
    private func setState(_ newState: ConnectionState) {
        state = newState
    }
    
    public func disconnect() async {
        connection?.cancel()
        connection = nil
        state = .disconnected
    }
    
    public func send(_ data: Data) async throws {
        guard let connection = connection else {
            throw NSError(domain: "MottoTransport", code: -1, userInfo: [NSLocalizedDescriptionKey: "Not connected"])
        }
        
        return try await withCheckedThrowingContinuation { continuation in
            connection.send(content: data, completion: .contentProcessed { error in
                if let error = error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            })
        }
    }
    
    public func receive() async throws -> Data {
        guard let connection = connection else {
            throw NSError(domain: "MottoTransport", code: -1, userInfo: [NSLocalizedDescriptionKey: "Not connected"])
        }
        
        return try await withCheckedThrowingContinuation { continuation in
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65535) { data, _, _, error in
                if let error = error {
                    continuation.resume(throwing: error)
                } else if let data = data {
                    continuation.resume(returning: data)
                } else {
                    continuation.resume(throwing: NSError(domain: "MottoTransport", code: -2, userInfo: [NSLocalizedDescriptionKey: "No data received"]))
                }
            }
        }
    }
}
#endif
