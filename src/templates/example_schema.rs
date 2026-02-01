//! Example Schema for Motto SDK Generation
//!
//! This file demonstrates the schema format expected by motto-cli.
//! Types annotated with `#[derive(Serialize, Deserialize)]` or `#[motto]`
//! will be included in the generated SDK.

use serde::{Deserialize, Serialize};

// ============================================================================
// Basic Types
// ============================================================================

/// Unique identifier for players
pub type PlayerId = u64;

/// Unique identifier for game rooms
pub type RoomId = u32;

// ============================================================================
// Enums
// ============================================================================

/// Player connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PlayerStatus {
    /// Player is offline
    Offline = 0,
    /// Player is online and idle
    Online = 1,
    /// Player is in a game
    InGame = 2,
    /// Player is away
    Away = 3,
}

/// Game event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    /// Player joined the game
    PlayerJoined {
        player_id: PlayerId,
        name: String,
        position: Position,
    },
    /// Player left the game
    PlayerLeft { player_id: PlayerId, reason: String },
    /// Player moved
    PlayerMoved(PlayerId, Position),
    /// Chat message
    ChatMessage {
        from: PlayerId,
        message: String,
        timestamp: u64,
    },
    /// Game state update
    StateUpdate(GameState),
    /// Ping/keep-alive
    Ping,
    /// Pong response
    Pong,
}

// ============================================================================
// Structs
// ============================================================================

/// 2D position in the game world
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate
    pub x: f32,
    /// Y coordinate  
    pub y: f32,
}

/// 2D velocity vector
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Velocity {
    /// X component
    pub dx: f32,
    /// Y component
    pub dy: f32,
}

/// Player information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    /// Unique player ID
    pub id: PlayerId,
    /// Display name
    pub name: String,
    /// Current position
    pub position: Position,
    /// Current velocity
    pub velocity: Velocity,
    /// Health points (0-100)
    pub health: u8,
    /// Current score
    pub score: u32,
    /// Connection status
    pub status: PlayerStatus,
    /// Avatar URL (optional)
    pub avatar_url: Option<String>,
}

/// Game room configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Room ID
    pub id: RoomId,
    /// Room name
    pub name: String,
    /// Maximum number of players
    pub max_players: u8,
    /// Is the room public?
    pub is_public: bool,
    /// Game mode
    pub game_mode: String,
    /// Custom settings (JSON string)
    pub settings: Option<String>,
}

/// Complete game state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    /// Tick number
    pub tick: u64,
    /// Server timestamp (ms since epoch)
    pub timestamp: u64,
    /// All players in the game
    pub players: Vec<Player>,
    /// Current room configuration
    pub room: RoomConfig,
    /// Is the game paused?
    pub paused: bool,
}

// ============================================================================
// Network Messages
// ============================================================================

/// Client-to-server messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Join a room
    JoinRoom {
        room_id: RoomId,
        password: Option<String>,
    },
    /// Leave current room
    LeaveRoom,
    /// Send movement input
    Move { direction: Velocity },
    /// Send chat message
    Chat { message: String },
    /// Request full state sync
    RequestSync,
    /// Ping for latency measurement
    Ping { client_time: u64 },
}

/// Server-to-client messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Welcome message after connection
    Welcome {
        server_version: String,
        player_id: PlayerId,
    },
    /// Room joined successfully
    RoomJoined { room: RoomConfig },
    /// Failed to join room
    JoinError { reason: String },
    /// Game event occurred
    Event { event: GameEvent },
    /// Full state synchronization
    Sync { state: GameState },
    /// Delta state update (only changed players)
    DeltaSync {
        tick: u64,
        updates: Vec<PlayerUpdate>,
    },
    /// Pong response
    Pong { client_time: u64, server_time: u64 },
    /// Error message
    Error { code: u16, message: String },
}

/// Partial player update for delta sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerUpdate {
    /// Player ID
    pub id: PlayerId,
    /// New position (if changed)
    pub position: Option<Position>,
    /// New velocity (if changed)
    pub velocity: Option<Velocity>,
    /// New health (if changed)
    pub health: Option<u8>,
    /// New score (if changed)
    pub score: Option<u32>,
}

// ============================================================================
// Generic Types
// ============================================================================

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// Items in this page
    pub items: Vec<T>,
    /// Total number of items
    pub total: u32,
    /// Current page (0-indexed)
    pub page: u32,
    /// Items per page
    pub per_page: u32,
}

/// Result wrapper for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResult<T> {
    /// Success flag
    pub success: bool,
    /// Data (if successful)
    pub data: Option<T>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Request ID for debugging
    pub request_id: String,
}
