/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0xA1
 * Schema Fingerprint: a15f1919c06e581b
 * Generated At: 2026-02-01T06:43:23.447495157+00:00
 */

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Motto.SDK
{
    /// <summary>Player connection status</summary>
    public enum PlayerStatus : byte
    {
        /// <summary>Player is offline</summary>
        Offline = 0,
        /// <summary>Player is online and idle</summary>
        Online = 1,
        /// <summary>Player is in a game</summary>
        InGame = 2,
        /// <summary>Player is away</summary>
        Away = 3,
    }

    /// <summary>Game event types</summary>
    public abstract class GameEvent
    {
        public abstract byte Tag { get; }
    }

    /// <summary>Player joined the game</summary>
    public sealed class GameEvent_PlayerJoined : GameEvent
    {
        public override byte Tag => 0;
        public PlayerId PlayerId { get; set; }
        public string Name { get; set; }
        public Position Position { get; set; }
    }

    /// <summary>Player left the game</summary>
    public sealed class GameEvent_PlayerLeft : GameEvent
    {
        public override byte Tag => 1;
        public PlayerId PlayerId { get; set; }
        public string Reason { get; set; }
    }

    /// <summary>Player moved</summary>
    public sealed class GameEvent_PlayerMoved : GameEvent
    {
        public override byte Tag => 2;
        public PlayerId Item1 { get; set; }
        public Position Item2 { get; set; }
    }

    /// <summary>Chat message</summary>
    public sealed class GameEvent_ChatMessage : GameEvent
    {
        public override byte Tag => 3;
        public PlayerId From { get; set; }
        public string Message { get; set; }
        public ulong Timestamp { get; set; }
    }

    /// <summary>Game state update</summary>
    public sealed class GameEvent_StateUpdate : GameEvent
    {
        public override byte Tag => 4;
        public GameState Item1 { get; set; }
    }

    /// <summary>Ping/keep-alive</summary>
    public sealed class GameEvent_Ping : GameEvent
    {
        public override byte Tag => 5;
    }

    /// <summary>Pong response</summary>
    public sealed class GameEvent_Pong : GameEvent
    {
        public override byte Tag => 6;
    }


    /// <summary>Client-to-server messages</summary>
    public abstract class ClientMessage
    {
        public abstract byte Tag { get; }
    }

    /// <summary>Join a room</summary>
    public sealed class ClientMessage_JoinRoom : ClientMessage
    {
        public override byte Tag => 0;
        public RoomId RoomId { get; set; }
        public string Password { get; set; }
    }

    /// <summary>Leave current room</summary>
    public sealed class ClientMessage_LeaveRoom : ClientMessage
    {
        public override byte Tag => 1;
    }

    /// <summary>Send movement input</summary>
    public sealed class ClientMessage_Move : ClientMessage
    {
        public override byte Tag => 2;
        public Velocity Direction { get; set; }
    }

    /// <summary>Send chat message</summary>
    public sealed class ClientMessage_Chat : ClientMessage
    {
        public override byte Tag => 3;
        public string Message { get; set; }
    }

    /// <summary>Request full state sync</summary>
    public sealed class ClientMessage_RequestSync : ClientMessage
    {
        public override byte Tag => 4;
    }

    /// <summary>Ping for latency measurement</summary>
    public sealed class ClientMessage_Ping : ClientMessage
    {
        public override byte Tag => 5;
        public ulong ClientTime { get; set; }
    }


    /// <summary>Server-to-client messages</summary>
    public abstract class ServerMessage
    {
        public abstract byte Tag { get; }
    }

    /// <summary>Welcome message after connection</summary>
    public sealed class ServerMessage_Welcome : ServerMessage
    {
        public override byte Tag => 0;
        public string ServerVersion { get; set; }
        public PlayerId PlayerId { get; set; }
    }

    /// <summary>Room joined successfully</summary>
    public sealed class ServerMessage_RoomJoined : ServerMessage
    {
        public override byte Tag => 1;
        public RoomConfig Room { get; set; }
    }

    /// <summary>Failed to join room</summary>
    public sealed class ServerMessage_JoinError : ServerMessage
    {
        public override byte Tag => 2;
        public string Reason { get; set; }
    }

    /// <summary>Game event occurred</summary>
    public sealed class ServerMessage_Event : ServerMessage
    {
        public override byte Tag => 3;
        public GameEvent Event { get; set; }
    }

    /// <summary>Full state synchronization</summary>
    public sealed class ServerMessage_Sync : ServerMessage
    {
        public override byte Tag => 4;
        public GameState State { get; set; }
    }

    /// <summary>Delta state update (only changed players)</summary>
    public sealed class ServerMessage_DeltaSync : ServerMessage
    {
        public override byte Tag => 5;
        public ulong Tick { get; set; }
        public PlayerUpdate[] Updates { get; set; }
    }

    /// <summary>Pong response</summary>
    public sealed class ServerMessage_Pong : ServerMessage
    {
        public override byte Tag => 6;
        public ulong ClientTime { get; set; }
        public ulong ServerTime { get; set; }
    }

    /// <summary>Error message</summary>
    public sealed class ServerMessage_Error : ServerMessage
    {
        public override byte Tag => 7;
        public ushort Code { get; set; }
        public string Message { get; set; }
    }


    /// <summary>2D position in the game world</summary>
    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct Position
    {
        /// <summary>X coordinate</summary>
        public float X { get; set; }
        /// <summary>Y coordinate</summary>
        public float Y { get; set; }
    }

    /// <summary>2D velocity vector</summary>
    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct Velocity
    {
        /// <summary>X component</summary>
        public float Dx { get; set; }
        /// <summary>Y component</summary>
        public float Dy { get; set; }
    }

    /// <summary>Player information</summary>
    public class Player
    {
        /// <summary>Unique player ID</summary>
        public PlayerId Id { get; set; }
        /// <summary>Display name</summary>
        public string Name { get; set; }
        /// <summary>Current position</summary>
        public Position Position { get; set; }
        /// <summary>Current velocity</summary>
        public Velocity Velocity { get; set; }
        /// <summary>Health points (0-100)</summary>
        public byte Health { get; set; }
        /// <summary>Current score</summary>
        public uint Score { get; set; }
        /// <summary>Connection status</summary>
        public PlayerStatus Status { get; set; }
        /// <summary>Avatar URL (optional)</summary>
        public string? AvatarUrl { get; set; }
    }

    /// <summary>Game room configuration</summary>
    public class RoomConfig
    {
        /// <summary>Room ID</summary>
        public RoomId Id { get; set; }
        /// <summary>Room name</summary>
        public string Name { get; set; }
        /// <summary>Maximum number of players</summary>
        public byte MaxPlayers { get; set; }
        /// <summary>Is the room public?</summary>
        public bool IsPublic { get; set; }
        /// <summary>Game mode</summary>
        public string GameMode { get; set; }
        /// <summary>Custom settings (JSON string)</summary>
        public string? Settings { get; set; }
    }

    /// <summary>Complete game state snapshot</summary>
    public class GameState
    {
        /// <summary>Tick number</summary>
        public ulong Tick { get; set; }
        /// <summary>Server timestamp (ms since epoch)</summary>
        public ulong Timestamp { get; set; }
        /// <summary>All players in the game</summary>
        public Player[] Players { get; set; }
        /// <summary>Current room configuration</summary>
        public RoomConfig Room { get; set; }
        /// <summary>Is the game paused?</summary>
        public bool Paused { get; set; }
    }

    /// <summary>Partial player update for delta sync</summary>
    public class PlayerUpdate
    {
        /// <summary>Player ID</summary>
        public PlayerId Id { get; set; }
        /// <summary>New position (if changed)</summary>
        public Position? Position { get; set; }
        /// <summary>New velocity (if changed)</summary>
        public Velocity? Velocity { get; set; }
        /// <summary>New health (if changed)</summary>
        public byte?? Health { get; set; }
        /// <summary>New score (if changed)</summary>
        public uint?? Score { get; set; }
    }

    /// <summary>Paginated response wrapper</summary>
    public class Paginated<T>
    {
        /// <summary>Items in this page</summary>
        public T[] Items { get; set; }
        /// <summary>Total number of items</summary>
        public uint Total { get; set; }
        /// <summary>Current page (0-indexed)</summary>
        public uint Page { get; set; }
        /// <summary>Items per page</summary>
        public uint PerPage { get; set; }
    }

    /// <summary>Result wrapper for API responses</summary>
    public class ApiResult<T>
    {
        /// <summary>Success flag</summary>
        public bool Success { get; set; }
        /// <summary>Data (if successful)</summary>
        public T? Data { get; set; }
        /// <summary>Error message (if failed)</summary>
        public string? Error { get; set; }
        /// <summary>Request ID for debugging</summary>
        public string RequestId { get; set; }
    }

}
