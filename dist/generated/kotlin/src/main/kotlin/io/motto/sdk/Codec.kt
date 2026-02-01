/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0xA1
 * Schema Fingerprint: a15f1919c06e581b
 * Generated At: 2026-02-01T06:43:23.447495157+00:00
 */

package io.motto.sdk

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Protocol version byte embedded in all packets */
const val PROTOCOL_VERSION_BYTE: Byte = 0xA1.toByte()

/** Schema fingerprint for validation */
const val SCHEMA_FINGERPRINT = "a15f1919c06e581b"

/** Zero-copy packet reader */
class PacketReader(private val buffer: ByteBuffer) {
    init {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
    }
    
    constructor(data: ByteArray) : this(ByteBuffer.wrap(data))
    
    fun validateVersion(): Boolean {
        if (buffer.remaining() < 1) return false
        return buffer.get(0) == PROTOCOL_VERSION_BYTE
    }
    
    fun skipVersionByte() {
        buffer.position(1)
    }
    
    fun readU8(): UByte = buffer.get().toUByte()
    fun readU16(): UShort = buffer.short.toUShort()
    fun readU32(): UInt = buffer.int.toUInt()
    fun readU64(): ULong = buffer.long.toULong()
    fun readI8(): Byte = buffer.get()
    fun readI16(): Short = buffer.short
    fun readI32(): Int = buffer.int
    fun readI64(): Long = buffer.long
    fun readF32(): Float = buffer.float
    fun readF64(): Double = buffer.double
    fun readBool(): Boolean = buffer.get() != 0.toByte()
    
    fun readString(): String {
        val length = buffer.int
        val bytes = ByteArray(length)
        buffer.get(bytes)
        return String(bytes, Charsets.UTF_8)
    }
}

/** Packet builder for encoding */
class PacketBuilder(initialCapacity: Int = 256) {
    private var buffer = ByteBuffer.allocate(initialCapacity).order(ByteOrder.LITTLE_ENDIAN)
    
    init {
        // Write version byte header
        buffer.put(PROTOCOL_VERSION_BYTE)
    }
    
    private fun ensureCapacity(need: Int) {
        if (buffer.remaining() < need) {
            val newCapacity = maxOf(buffer.capacity() * 2, buffer.position() + need)
            val newBuffer = ByteBuffer.allocate(newCapacity).order(ByteOrder.LITTLE_ENDIAN)
            buffer.flip()
            newBuffer.put(buffer)
            buffer = newBuffer
        }
    }
    
    fun writeU8(value: UByte) {
        ensureCapacity(1)
        buffer.put(value.toByte())
    }
    
    fun writeU16(value: UShort) {
        ensureCapacity(2)
        buffer.putShort(value.toShort())
    }
    
    fun writeU32(value: UInt) {
        ensureCapacity(4)
        buffer.putInt(value.toInt())
    }
    
    fun writeU64(value: ULong) {
        ensureCapacity(8)
        buffer.putLong(value.toLong())
    }
    
    fun writeI8(value: Byte) {
        ensureCapacity(1)
        buffer.put(value)
    }
    
    fun writeI16(value: Short) {
        ensureCapacity(2)
        buffer.putShort(value)
    }
    
    fun writeI32(value: Int) {
        ensureCapacity(4)
        buffer.putInt(value)
    }
    
    fun writeI64(value: Long) {
        ensureCapacity(8)
        buffer.putLong(value)
    }
    
    fun writeF32(value: Float) {
        ensureCapacity(4)
        buffer.putFloat(value)
    }
    
    fun writeF64(value: Double) {
        ensureCapacity(8)
        buffer.putDouble(value)
    }
    
    fun writeBool(value: Boolean) {
        ensureCapacity(1)
        buffer.put(if (value) 1.toByte() else 0.toByte())
    }
    
    fun writeString(value: String) {
        val bytes = value.toByteArray(Charsets.UTF_8)
        ensureCapacity(4 + bytes.size)
        buffer.putInt(bytes.size)
        buffer.put(bytes)
    }
    
    fun build(): ByteArray {
        val result = ByteArray(buffer.position())
        buffer.flip()
        buffer.get(result)
        return result
    }
}

/** Encode Position to binary */
fun Position.encode(): ByteArray {
    val builder = PacketBuilder()
    builder.writeF32(x)
    builder.writeF32(y)
    return builder.build()
}

/** Decode Position from binary */
fun Position.Companion.decode(data: ByteArray): Position? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return Position(
        x = reader.readF32(),
        y = reader.readF32()
    )
}

/** Encode Velocity to binary */
fun Velocity.encode(): ByteArray {
    val builder = PacketBuilder()
    builder.writeF32(dx)
    builder.writeF32(dy)
    return builder.build()
}

/** Decode Velocity from binary */
fun Velocity.Companion.decode(data: ByteArray): Velocity? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return Velocity(
        dx = reader.readF32(),
        dy = reader.readF32()
    )
}

/** Encode Player to binary */
fun Player.encode(): ByteArray {
    val builder = PacketBuilder()
    /* TODO: encode PlayerId */
    builder.writeString(name)
    /* TODO: encode Position */
    /* TODO: encode Velocity */
    builder.writeU8(health.toUByte())
    builder.writeU32(score.toUInt())
    /* TODO: encode PlayerStatus */
    avatarUrl ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<String> */
    } ?: builder.writeU8(0u)
    return builder.build()
}

/** Decode Player from binary */
fun Player.Companion.decode(data: ByteArray): Player? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return Player(
        id = null /* TODO: decode PlayerId */,
        name = reader.readString(),
        position = null /* TODO: decode Position */,
        velocity = null /* TODO: decode Velocity */,
        health = reader.readU8(),
        score = reader.readU32(),
        status = null /* TODO: decode PlayerStatus */,
        avatarUrl = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<String> */ else null
    )
}

/** Encode RoomConfig to binary */
fun RoomConfig.encode(): ByteArray {
    val builder = PacketBuilder()
    /* TODO: encode RoomId */
    builder.writeString(name)
    builder.writeU8(maxPlayers.toUByte())
    builder.writeBool(isPublic)
    builder.writeString(gameMode)
    settings ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<String> */
    } ?: builder.writeU8(0u)
    return builder.build()
}

/** Decode RoomConfig from binary */
fun RoomConfig.Companion.decode(data: ByteArray): RoomConfig? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return RoomConfig(
        id = null /* TODO: decode RoomId */,
        name = reader.readString(),
        maxPlayers = reader.readU8(),
        isPublic = reader.readBool(),
        gameMode = reader.readString(),
        settings = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<String> */ else null
    )
}

/** Encode GameState to binary */
fun GameState.encode(): ByteArray {
    val builder = PacketBuilder()
    builder.writeU64(tick.toULong())
    builder.writeU64(timestamp.toULong())
    /* TODO: encode Vec<Player> */
    /* TODO: encode RoomConfig */
    builder.writeBool(paused)
    return builder.build()
}

/** Decode GameState from binary */
fun GameState.Companion.decode(data: ByteArray): GameState? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return GameState(
        tick = reader.readU64(),
        timestamp = reader.readU64(),
        players = null /* TODO: decode Vec<Player> */,
        room = null /* TODO: decode RoomConfig */,
        paused = reader.readBool()
    )
}

/** Encode PlayerUpdate to binary */
fun PlayerUpdate.encode(): ByteArray {
    val builder = PacketBuilder()
    /* TODO: encode PlayerId */
    position ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<Position> */
    } ?: builder.writeU8(0u)
    velocity ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<Velocity> */
    } ?: builder.writeU8(0u)
    health ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<u8> */
    } ?: builder.writeU8(0u)
    score ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<u32> */
    } ?: builder.writeU8(0u)
    return builder.build()
}

/** Decode PlayerUpdate from binary */
fun PlayerUpdate.Companion.decode(data: ByteArray): PlayerUpdate? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return PlayerUpdate(
        id = null /* TODO: decode PlayerId */,
        position = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<Position> */ else null,
        velocity = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<Velocity> */ else null,
        health = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<u8> */ else null,
        score = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<u32> */ else null
    )
}

/** Encode Paginated to binary */
fun Paginated.encode(): ByteArray {
    val builder = PacketBuilder()
    /* TODO: encode Vec<T> */
    builder.writeU32(total.toUInt())
    builder.writeU32(page.toUInt())
    builder.writeU32(perPage.toUInt())
    return builder.build()
}

/** Decode Paginated from binary */
fun Paginated.Companion.decode(data: ByteArray): Paginated? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return Paginated(
        items = null /* TODO: decode Vec<T> */,
        total = reader.readU32(),
        page = reader.readU32(),
        perPage = reader.readU32()
    )
}

/** Encode ApiResult to binary */
fun ApiResult.encode(): ByteArray {
    val builder = PacketBuilder()
    builder.writeBool(success)
    data ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<T> */
    } ?: builder.writeU8(0u)
    error ?.let {
        builder.writeU8(1u)
        /* TODO: encode Option<String> */
    } ?: builder.writeU8(0u)
    builder.writeString(requestId)
    return builder.build()
}

/** Decode ApiResult from binary */
fun ApiResult.Companion.decode(data: ByteArray): ApiResult? {
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return ApiResult(
        success = reader.readBool(),
        data = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<T> */ else null,
        error = if (reader.readU8() != 0.toUByte()) null /* TODO: decode Option<String> */ else null,
        requestId = reader.readString()
    )
}
