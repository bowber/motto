//
// MOTTO GENERATED CODE - DO NOT EDIT
//
// Protocol Version: 0xA1
// Schema Fingerprint: a15f1919c06e581b
// Generated At: 2026-02-01T06:43:23.447495157+00:00
//

import Foundation

// MARK: - Codec

/// Protocol version byte embedded in all packets
public let PROTOCOL_VERSION_BYTE: UInt8 = 0xA1

/// Schema fingerprint for validation
public let SCHEMA_FINGERPRINT = "a15f1919c06e581b"

/// Zero-copy packet reader
public struct PacketReader {
    private var data: Data
    private var offset: Int = 0
    
    public init(data: Data) {
        self.data = data
    }
    
    public mutating func validateVersion() -> Bool {
        guard data.count > 0 else { return false }
        return data[0] == PROTOCOL_VERSION_BYTE
    }
    
    public mutating func skipVersionByte() {
        offset = 1
    }
    
    public mutating func readU8() -> UInt8 {
        let value = data[offset]
        offset += 1
        return value
    }
    
    public mutating func readU16() -> UInt16 {
        let value = data.withUnsafeBytes { ptr in
            ptr.load(fromByteOffset: offset, as: UInt16.self)
        }
        offset += 2
        return UInt16(littleEndian: value)
    }
    
    public mutating func readU32() -> UInt32 {
        let value = data.withUnsafeBytes { ptr in
            ptr.load(fromByteOffset: offset, as: UInt32.self)
        }
        offset += 4
        return UInt32(littleEndian: value)
    }
    
    public mutating func readU64() -> UInt64 {
        let value = data.withUnsafeBytes { ptr in
            ptr.load(fromByteOffset: offset, as: UInt64.self)
        }
        offset += 8
        return UInt64(littleEndian: value)
    }
    
    public mutating func readF32() -> Float {
        let bits = readU32()
        return Float(bitPattern: bits)
    }
    
    public mutating func readF64() -> Double {
        let bits = readU64()
        return Double(bitPattern: bits)
    }
    
    public mutating func readBool() -> Bool {
        return readU8() != 0
    }
    
    public mutating func readString() -> String {
        let length = Int(readU32())
        let stringData = data.subdata(in: offset..<(offset + length))
        offset += length
        return String(data: stringData, encoding: .utf8) ?? ""
    }
}

/// Packet builder for encoding
public struct PacketBuilder {
    private var data: Data
    
    public init(capacity: Int = 256) {
        data = Data(capacity: capacity)
        // Write version byte header
        data.append(PROTOCOL_VERSION_BYTE)
    }
    
    public mutating func writeU8(_ value: UInt8) {
        data.append(value)
    }
    
    public mutating func writeU16(_ value: UInt16) {
        var v = value.littleEndian
        withUnsafeBytes(of: &v) { data.append(contentsOf: $0) }
    }
    
    public mutating func writeU32(_ value: UInt32) {
        var v = value.littleEndian
        withUnsafeBytes(of: &v) { data.append(contentsOf: $0) }
    }
    
    public mutating func writeU64(_ value: UInt64) {
        var v = value.littleEndian
        withUnsafeBytes(of: &v) { data.append(contentsOf: $0) }
    }
    
    public mutating func writeF32(_ value: Float) {
        writeU32(value.bitPattern)
    }
    
    public mutating func writeF64(_ value: Double) {
        writeU64(value.bitPattern)
    }
    
    public mutating func writeBool(_ value: Bool) {
        writeU8(value ? 1 : 0)
    }
    
    public mutating func writeString(_ value: String) {
        let bytes = value.data(using: .utf8) ?? Data()
        writeU32(UInt32(bytes.count))
        data.append(bytes)
    }
    
    public func build() -> Data {
        return data
    }
}

// MARK: - Position Codec

extension Position {
    public func encode() -> Data {
        var builder = PacketBuilder()
        builder.writeF32(self.x)
        builder.writeF32(self.y)
        return builder.build()
    }

    public static func decode(from data: Data) -> Position? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return Position(
            x: reader.readF32(),
            y: reader.readF32()
        )
    }
}

// MARK: - Velocity Codec

extension Velocity {
    public func encode() -> Data {
        var builder = PacketBuilder()
        builder.writeF32(self.dx)
        builder.writeF32(self.dy)
        return builder.build()
    }

    public static func decode(from data: Data) -> Velocity? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return Velocity(
            dx: reader.readF32(),
            dy: reader.readF32()
        )
    }
}

// MARK: - Player Codec

extension Player {
    public func encode() -> Data {
        var builder = PacketBuilder()
        /* TODO: encode PlayerId */
        builder.writeString(self.name)
        /* TODO: encode Position */
        /* TODO: encode Velocity */
        builder.writeU8(self.health)
        builder.writeU32(self.score)
        /* TODO: encode PlayerStatus */
        if let avatarUrl = self.avatarUrl {
            builder.writeU8(1)
            /* TODO: encode Option<String> */
        } else {
            builder.writeU8(0)
        }
        return builder.build()
    }

    public static func decode(from data: Data) -> Player? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return Player(
            id: /* TODO: decode PlayerId */,
            name: reader.readString(),
            position: /* TODO: decode Position */,
            velocity: /* TODO: decode Velocity */,
            health: reader.readU8(),
            score: reader.readU32(),
            status: /* TODO: decode PlayerStatus */,
            avatarUrl: reader.readU8() != 0 ? /* TODO: decode Option<String> */ : nil
        )
    }
}

// MARK: - RoomConfig Codec

extension RoomConfig {
    public func encode() -> Data {
        var builder = PacketBuilder()
        /* TODO: encode RoomId */
        builder.writeString(self.name)
        builder.writeU8(self.maxPlayers)
        builder.writeBool(self.isPublic)
        builder.writeString(self.gameMode)
        if let settings = self.settings {
            builder.writeU8(1)
            /* TODO: encode Option<String> */
        } else {
            builder.writeU8(0)
        }
        return builder.build()
    }

    public static func decode(from data: Data) -> RoomConfig? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return RoomConfig(
            id: /* TODO: decode RoomId */,
            name: reader.readString(),
            maxPlayers: reader.readU8(),
            isPublic: reader.readBool(),
            gameMode: reader.readString(),
            settings: reader.readU8() != 0 ? /* TODO: decode Option<String> */ : nil
        )
    }
}

// MARK: - GameState Codec

extension GameState {
    public func encode() -> Data {
        var builder = PacketBuilder()
        builder.writeU64(self.tick)
        builder.writeU64(self.timestamp)
        /* TODO: encode Vec<Player> */
        /* TODO: encode RoomConfig */
        builder.writeBool(self.paused)
        return builder.build()
    }

    public static func decode(from data: Data) -> GameState? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return GameState(
            tick: reader.readU64(),
            timestamp: reader.readU64(),
            players: /* TODO: decode Vec<Player> */,
            room: /* TODO: decode RoomConfig */,
            paused: reader.readBool()
        )
    }
}

// MARK: - PlayerUpdate Codec

extension PlayerUpdate {
    public func encode() -> Data {
        var builder = PacketBuilder()
        /* TODO: encode PlayerId */
        if let position = self.position {
            builder.writeU8(1)
            /* TODO: encode Option<Position> */
        } else {
            builder.writeU8(0)
        }
        if let velocity = self.velocity {
            builder.writeU8(1)
            /* TODO: encode Option<Velocity> */
        } else {
            builder.writeU8(0)
        }
        if let health = self.health {
            builder.writeU8(1)
            /* TODO: encode Option<u8> */
        } else {
            builder.writeU8(0)
        }
        if let score = self.score {
            builder.writeU8(1)
            /* TODO: encode Option<u32> */
        } else {
            builder.writeU8(0)
        }
        return builder.build()
    }

    public static func decode(from data: Data) -> PlayerUpdate? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return PlayerUpdate(
            id: /* TODO: decode PlayerId */,
            position: reader.readU8() != 0 ? /* TODO: decode Option<Position> */ : nil,
            velocity: reader.readU8() != 0 ? /* TODO: decode Option<Velocity> */ : nil,
            health: reader.readU8() != 0 ? /* TODO: decode Option<u8> */ : nil,
            score: reader.readU8() != 0 ? /* TODO: decode Option<u32> */ : nil
        )
    }
}

// MARK: - Paginated Codec

extension Paginated {
    public func encode() -> Data {
        var builder = PacketBuilder()
        /* TODO: encode Vec<T> */
        builder.writeU32(self.total)
        builder.writeU32(self.page)
        builder.writeU32(self.perPage)
        return builder.build()
    }

    public static func decode(from data: Data) -> Paginated? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return Paginated(
            items: /* TODO: decode Vec<T> */,
            total: reader.readU32(),
            page: reader.readU32(),
            perPage: reader.readU32()
        )
    }
}

// MARK: - ApiResult Codec

extension ApiResult {
    public func encode() -> Data {
        var builder = PacketBuilder()
        builder.writeBool(self.success)
        if let data = self.data {
            builder.writeU8(1)
            /* TODO: encode Option<T> */
        } else {
            builder.writeU8(0)
        }
        if let error = self.error {
            builder.writeU8(1)
            /* TODO: encode Option<String> */
        } else {
            builder.writeU8(0)
        }
        builder.writeString(self.requestId)
        return builder.build()
    }

    public static func decode(from data: Data) -> ApiResult? {
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else { return nil }
        reader.skipVersionByte()
        
        return ApiResult(
            success: reader.readBool(),
            data: reader.readU8() != 0 ? /* TODO: decode Option<T> */ : nil,
            error: reader.readU8() != 0 ? /* TODO: decode Option<String> */ : nil,
            requestId: reader.readString()
        )
    }
}
