//
// MOTTO GENERATED CODE - DO NOT EDIT
//
// Protocol Version: 0xA1
// Schema Fingerprint: a15f1919c06e581b
// Generated At: 2026-02-01T06:43:23.447495157+00:00
//

import Foundation

// MARK: - Type Definitions

/// Player connection status
public enum PlayerStatus: UInt8, Codable, Sendable {
    /// Player is offline
    case offline = 0
    /// Player is online and idle
    case online = 1
    /// Player is in a game
    case inGame = 2
    /// Player is away
    case away = 3
}

/// Game event types
public enum GameEvent: Codable, Sendable {
    /// Player joined the game
    case playerJoined(playerId: PlayerId, name: String, position: Position)
    /// Player left the game
    case playerLeft(playerId: PlayerId, reason: String)
    /// Player moved
    case playerMoved(PlayerId, Position)
    /// Chat message
    case chatMessage(from: PlayerId, message: String, timestamp: UInt64)
    /// Game state update
    case stateUpdate(GameState)
    /// Ping/keep-alive
    case ping
    /// Pong response
    case pong
}

/// Client-to-server messages
public enum ClientMessage: Codable, Sendable {
    /// Join a room
    case joinRoom(roomId: RoomId, password: String?)
    /// Leave current room
    case leaveRoom
    /// Send movement input
    case move(direction: Velocity)
    /// Send chat message
    case chat(message: String)
    /// Request full state sync
    case requestSync
    /// Ping for latency measurement
    case ping(clientTime: UInt64)
}

/// Server-to-client messages
public enum ServerMessage: Codable, Sendable {
    /// Welcome message after connection
    case welcome(serverVersion: String, playerId: PlayerId)
    /// Room joined successfully
    case roomJoined(room: RoomConfig)
    /// Failed to join room
    case joinError(reason: String)
    /// Game event occurred
    case event(event: GameEvent)
    /// Full state synchronization
    case sync(state: GameState)
    /// Delta state update (only changed players)
    case deltaSync(tick: UInt64, updates: [PlayerUpdate])
    /// Pong response
    case pong(clientTime: UInt64, serverTime: UInt64)
    /// Error message
    case error(code: UInt16, message: String)
}

/// 2D position in the game world
public struct Position: Codable, Sendable {
    /// X coordinate
    public var x: Float
    /// Y coordinate
    public var y: Float

    public init(
        x: Float,
        y: Float
    ) {
        self.x = x
        self.y = y
    }
}

/// 2D velocity vector
public struct Velocity: Codable, Sendable {
    /// X component
    public var dx: Float
    /// Y component
    public var dy: Float

    public init(
        dx: Float,
        dy: Float
    ) {
        self.dx = dx
        self.dy = dy
    }
}

/// Player information
public struct Player: Codable, Sendable {
    /// Unique player ID
    public var id: PlayerId
    /// Display name
    public var name: String
    /// Current position
    public var position: Position
    /// Current velocity
    public var velocity: Velocity
    /// Health points (0-100)
    public var health: UInt8
    /// Current score
    public var score: UInt32
    /// Connection status
    public var status: PlayerStatus
    /// Avatar URL (optional)
    public var avatarUrl: String??

    public init(
        id: PlayerId,
        name: String,
        position: Position,
        velocity: Velocity,
        health: UInt8,
        score: UInt32,
        status: PlayerStatus,
        avatarUrl: String?? = nil
    ) {
        self.id = id
        self.name = name
        self.position = position
        self.velocity = velocity
        self.health = health
        self.score = score
        self.status = status
        self.avatarUrl = avatarUrl
    }
}

/// Game room configuration
public struct RoomConfig: Codable, Sendable {
    /// Room ID
    public var id: RoomId
    /// Room name
    public var name: String
    /// Maximum number of players
    public var maxPlayers: UInt8
    /// Is the room public?
    public var isPublic: Bool
    /// Game mode
    public var gameMode: String
    /// Custom settings (JSON string)
    public var settings: String??

    public init(
        id: RoomId,
        name: String,
        maxPlayers: UInt8,
        isPublic: Bool,
        gameMode: String,
        settings: String?? = nil
    ) {
        self.id = id
        self.name = name
        self.maxPlayers = maxPlayers
        self.isPublic = isPublic
        self.gameMode = gameMode
        self.settings = settings
    }
}

/// Complete game state snapshot
public struct GameState: Codable, Sendable {
    /// Tick number
    public var tick: UInt64
    /// Server timestamp (ms since epoch)
    public var timestamp: UInt64
    /// All players in the game
    public var players: [Player]
    /// Current room configuration
    public var room: RoomConfig
    /// Is the game paused?
    public var paused: Bool

    public init(
        tick: UInt64,
        timestamp: UInt64,
        players: [Player],
        room: RoomConfig,
        paused: Bool
    ) {
        self.tick = tick
        self.timestamp = timestamp
        self.players = players
        self.room = room
        self.paused = paused
    }
}

/// Partial player update for delta sync
public struct PlayerUpdate: Codable, Sendable {
    /// Player ID
    public var id: PlayerId
    /// New position (if changed)
    public var position: Position??
    /// New velocity (if changed)
    public var velocity: Velocity??
    /// New health (if changed)
    public var health: UInt8??
    /// New score (if changed)
    public var score: UInt32??

    public init(
        id: PlayerId,
        position: Position?? = nil,
        velocity: Velocity?? = nil,
        health: UInt8?? = nil,
        score: UInt32?? = nil
    ) {
        self.id = id
        self.position = position
        self.velocity = velocity
        self.health = health
        self.score = score
    }
}

/// Paginated response wrapper
public struct Paginated<T>: Codable, Sendable {
    /// Items in this page
    public var items: [T]
    /// Total number of items
    public var total: UInt32
    /// Current page (0-indexed)
    public var page: UInt32
    /// Items per page
    public var perPage: UInt32

    public init(
        items: [T],
        total: UInt32,
        page: UInt32,
        perPage: UInt32
    ) {
        self.items = items
        self.total = total
        self.page = page
        self.perPage = perPage
    }
}

/// Result wrapper for API responses
public struct ApiResult<T>: Codable, Sendable {
    /// Success flag
    public var success: Bool
    /// Data (if successful)
    public var data: T??
    /// Error message (if failed)
    public var error: String??
    /// Request ID for debugging
    public var requestId: String

    public init(
        success: Bool,
        data: T?? = nil,
        error: String?? = nil,
        requestId: String
    ) {
        self.success = success
        self.data = data
        self.error = error
        self.requestId = requestId
    }
}

