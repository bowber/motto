/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0xA1
 * Schema Fingerprint: a15f1919c06e581b
 * Generated At: 2026-02-01T06:43:23.447495157+00:00
 */

package io.motto.sdk

import kotlinx.serialization.*

/** Player connection status */
@Serializable
enum class PlayerStatus(val value: UByte) {
    /** Player is offline */
    OFFLINE(0),
    /** Player is online and idle */
    ONLINE(1),
    /** Player is in a game */
    INGAME(2),
    /** Player is away */
    AWAY(3);
}

/** Game event types */
@Serializable
sealed class GameEvent {
    /** Player joined the game */
    @Serializable
    data class PlayerJoined(val playerId: PlayerId, val name: String, val position: Position) : GameEvent()
    /** Player left the game */
    @Serializable
    data class PlayerLeft(val playerId: PlayerId, val reason: String) : GameEvent()
    /** Player moved */
    @Serializable
    data class PlayerMoved(val _0: PlayerId, val _1: Position) : GameEvent()
    /** Chat message */
    @Serializable
    data class ChatMessage(val from: PlayerId, val message: String, val timestamp: ULong) : GameEvent()
    /** Game state update */
    @Serializable
    data class StateUpdate(val _0: GameState) : GameEvent()
    /** Ping/keep-alive */
    @Serializable
    data object Ping : GameEvent()
    /** Pong response */
    @Serializable
    data object Pong : GameEvent()
}

/** Client-to-server messages */
@Serializable
sealed class ClientMessage {
    /** Join a room */
    @Serializable
    data class JoinRoom(val roomId: RoomId, val password: String?) : ClientMessage()
    /** Leave current room */
    @Serializable
    data object LeaveRoom : ClientMessage()
    /** Send movement input */
    @Serializable
    data class Move(val direction: Velocity) : ClientMessage()
    /** Send chat message */
    @Serializable
    data class Chat(val message: String) : ClientMessage()
    /** Request full state sync */
    @Serializable
    data object RequestSync : ClientMessage()
    /** Ping for latency measurement */
    @Serializable
    data class Ping(val clientTime: ULong) : ClientMessage()
}

/** Server-to-client messages */
@Serializable
sealed class ServerMessage {
    /** Welcome message after connection */
    @Serializable
    data class Welcome(val serverVersion: String, val playerId: PlayerId) : ServerMessage()
    /** Room joined successfully */
    @Serializable
    data class RoomJoined(val room: RoomConfig) : ServerMessage()
    /** Failed to join room */
    @Serializable
    data class JoinError(val reason: String) : ServerMessage()
    /** Game event occurred */
    @Serializable
    data class Event(val event: GameEvent) : ServerMessage()
    /** Full state synchronization */
    @Serializable
    data class Sync(val state: GameState) : ServerMessage()
    /** Delta state update (only changed players) */
    @Serializable
    data class DeltaSync(val tick: ULong, val updates: List<PlayerUpdate>) : ServerMessage()
    /** Pong response */
    @Serializable
    data class Pong(val clientTime: ULong, val serverTime: ULong) : ServerMessage()
    /** Error message */
    @Serializable
    data class Error(val code: UShort, val message: String) : ServerMessage()
}

/** 2D position in the game world */
@Serializable
data class Position (
    /** X coordinate */
    val x: Float,
    /** Y coordinate */
    val y: Float
)

/** 2D velocity vector */
@Serializable
data class Velocity (
    /** X component */
    val dx: Float,
    /** Y component */
    val dy: Float
)

/** Player information */
@Serializable
data class Player (
    /** Unique player ID */
    val id: PlayerId,
    /** Display name */
    val name: String,
    /** Current position */
    val position: Position,
    /** Current velocity */
    val velocity: Velocity,
    /** Health points (0-100) */
    val health: UByte,
    /** Current score */
    val score: UInt,
    /** Connection status */
    val status: PlayerStatus,
    /** Avatar URL (optional) */
    val avatarUrl: String?? = null
)

/** Game room configuration */
@Serializable
data class RoomConfig (
    /** Room ID */
    val id: RoomId,
    /** Room name */
    val name: String,
    /** Maximum number of players */
    val maxPlayers: UByte,
    /** Is the room public? */
    val isPublic: Boolean,
    /** Game mode */
    val gameMode: String,
    /** Custom settings (JSON string) */
    val settings: String?? = null
)

/** Complete game state snapshot */
@Serializable
data class GameState (
    /** Tick number */
    val tick: ULong,
    /** Server timestamp (ms since epoch) */
    val timestamp: ULong,
    /** All players in the game */
    val players: List<Player>,
    /** Current room configuration */
    val room: RoomConfig,
    /** Is the game paused? */
    val paused: Boolean
)

/** Partial player update for delta sync */
@Serializable
data class PlayerUpdate (
    /** Player ID */
    val id: PlayerId,
    /** New position (if changed) */
    val position: Position?? = null,
    /** New velocity (if changed) */
    val velocity: Velocity?? = null,
    /** New health (if changed) */
    val health: UByte?? = null,
    /** New score (if changed) */
    val score: UInt?? = null
)

/** Paginated response wrapper */
@Serializable
data class Paginated<T> (
    /** Items in this page */
    val items: List<T>,
    /** Total number of items */
    val total: UInt,
    /** Current page (0-indexed) */
    val page: UInt,
    /** Items per page */
    val perPage: UInt
)

/** Result wrapper for API responses */
@Serializable
data class ApiResult<T> (
    /** Success flag */
    val success: Boolean,
    /** Data (if successful) */
    val data: T?? = null,
    /** Error message (if failed) */
    val error: String?? = null,
    /** Request ID for debugging */
    val requestId: String
)

