//! Kotlin Backend Emitter
//!
//! Generates Kotlin SDK for Android/JVM with:
//! - kotlinx.serialization support
//! - Coroutine-based transport
//! - Zero-copy packet framing

use crate::emitters::{Emitter, EmitterConfig, GeneratedFile, utils};
use crate::ir::manifest::*;
use anyhow::Result;
use std::path::PathBuf;

/// Kotlin reserved words
const KOTLIN_RESERVED: &[&str] = &[
    "as",
    "break",
    "class",
    "continue",
    "do",
    "else",
    "false",
    "for",
    "fun",
    "if",
    "in",
    "interface",
    "is",
    "null",
    "object",
    "package",
    "return",
    "super",
    "this",
    "throw",
    "true",
    "try",
    "typealias",
    "typeof",
    "val",
    "var",
    "when",
    "while",
];

pub struct KotlinEmitter;

impl Emitter for KotlinEmitter {
    fn platform(&self) -> &'static str {
        "kotlin"
    }

    fn extension(&self) -> &'static str {
        "kt"
    }

    fn emit(&self, config: &EmitterConfig) -> Result<Vec<GeneratedFile>> {
        let files = vec![
            generate_types(&config.manifest)?,
            generate_codec(&config.manifest)?,
            generate_runtime(&config.manifest)?,
            generate_build_gradle(&config.manifest)?,
            generate_tests(&config.manifest)?,
        ];

        Ok(files)
    }
}

/// Emit Kotlin SDK
pub fn emit(config: &EmitterConfig) -> Result<()> {
    let emitter = KotlinEmitter;
    let files = emitter.emit(config)?;

    let kotlin_dir = config.output_dir.join("kotlin");
    std::fs::create_dir_all(&kotlin_dir)?;

    for file in files {
        let path = kotlin_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }

    Ok(())
}

fn kotlin_header(manifest: &SchemaManifest) -> String {
    format!(
        r#"/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0x{:02X}
 * Schema Fingerprint: {}
 * Generated At: {}
 */

package io.motto.sdk
"#,
        manifest.meta.version_byte,
        &manifest.meta.fingerprint[..16],
        manifest.meta.generated_at
    )
}

fn generate_types(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = kotlin_header(manifest);

    content.push_str("\nimport kotlinx.serialization.*\n\n");

    // Generate enums
    for e in &manifest.enums {
        content.push_str(&generate_enum_type(e));
        content.push('\n');
    }

    // Generate data classes
    for msg in &manifest.messages {
        content.push_str(&generate_data_class(msg));
        content.push('\n');
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/main/kotlin/io/motto/sdk/Types.kt"),
        content,
    })
}

fn generate_enum_type(e: &EnumManifest) -> String {
    let mut s = String::new();

    if let Some(docs) = &e.docs {
        s.push_str(&format!("/** {} */\n", docs));
    }

    if e.is_simple {
        // C-style enum
        s.push_str("@Serializable\n");
        s.push_str(&format!(
            "enum class {}(val value: {}) {{\n",
            e.name,
            rust_to_kotlin_type(&e.repr)
        ));
        for (i, v) in e.variants.iter().enumerate() {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /** {} */\n", docs));
            }
            let comma = if i < e.variants.len() - 1 { "," } else { ";" };
            s.push_str(&format!(
                "    {}({}){}\n",
                v.name.to_uppercase(),
                v.discriminant,
                comma
            ));
        }
        s.push_str("}\n");
    } else {
        // Sealed class for tagged unions
        s.push_str("@Serializable\n");
        s.push_str(&format!("sealed class {} {{\n", e.name));
        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /** {} */\n", docs));
            }
            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!(
                        "    @Serializable\n    data object {} : {}()\n",
                        v.name, e.name
                    ));
                }
                VariantData::Tuple { types } => {
                    let params: Vec<String> = types
                        .iter()
                        .enumerate()
                        .map(|(i, t)| format!("val _{}: {}", i, rust_to_kotlin_type(t)))
                        .collect();
                    s.push_str(&format!(
                        "    @Serializable\n    data class {}({}) : {}()\n",
                        v.name,
                        params.join(", "),
                        e.name
                    ));
                }
                VariantData::Struct { fields } => {
                    let params: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            format!(
                                "val {}: {}",
                                escape_kotlin(&f.name),
                                rust_to_kotlin_type(&f.type_ref)
                            )
                        })
                        .collect();
                    s.push_str(&format!(
                        "    @Serializable\n    data class {}({}) : {}()\n",
                        v.name,
                        params.join(", "),
                        e.name
                    ));
                }
            }
        }
        s.push_str("}\n");
    }

    s
}

fn generate_data_class(msg: &MessageDef) -> String {
    let mut s = String::new();

    if let Some(docs) = &msg.docs {
        s.push_str(&format!("/** {} */\n", docs));
    }

    let generics = if msg.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", msg.generics.join(", "))
    };

    s.push_str("@Serializable\n");
    s.push_str(&format!("data class {}{} (\n", msg.name, generics));

    let fields: Vec<String> = msg
        .fields
        .iter()
        .map(|f| {
            let field_name = escape_kotlin(&f.name);
            let field_type = rust_to_kotlin_type(&f.type_ref);
            let nullable = if f.optional { "?" } else { "" };
            let default = if f.optional { " = null" } else { "" };

            let doc = f
                .docs
                .as_ref()
                .map(|d| format!("    /** {} */\n", d))
                .unwrap_or_default();
            format!(
                "{}    val {}: {}{}{}",
                doc, field_name, field_type, nullable, default
            )
        })
        .collect();

    s.push_str(&fields.join(",\n"));
    s.push_str("\n)\n");

    s
}

fn generate_codec(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = kotlin_header(manifest);

    content.push_str(&format!(
        r#"
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Protocol version byte embedded in all packets */
const val PROTOCOL_VERSION_BYTE: Byte = 0x{:02X}.toByte()

/** Schema fingerprint for validation */
const val SCHEMA_FINGERPRINT = "{}"

/** Zero-copy packet reader */
class PacketReader(private val buffer: ByteBuffer) {{
    init {{
        buffer.order(ByteOrder.LITTLE_ENDIAN)
    }}
    
    constructor(data: ByteArray) : this(ByteBuffer.wrap(data))
    
    fun validateVersion(): Boolean {{
        if (buffer.remaining() < 1) return false
        return buffer.get(0) == PROTOCOL_VERSION_BYTE
    }}
    
    fun skipVersionByte() {{
        buffer.position(1)
    }}
    
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
    
    fun readString(): String {{
        val length = buffer.int
        val bytes = ByteArray(length)
        buffer.get(bytes)
        return String(bytes, Charsets.UTF_8)
    }}
}}

/** Packet builder for encoding */
class PacketBuilder(initialCapacity: Int = 256) {{
    private var buffer = ByteBuffer.allocate(initialCapacity).order(ByteOrder.LITTLE_ENDIAN)
    
    init {{
        // Write version byte header
        buffer.put(PROTOCOL_VERSION_BYTE)
    }}
    
    private fun ensureCapacity(need: Int) {{
        if (buffer.remaining() < need) {{
            val newCapacity = maxOf(buffer.capacity() * 2, buffer.position() + need)
            val newBuffer = ByteBuffer.allocate(newCapacity).order(ByteOrder.LITTLE_ENDIAN)
            buffer.flip()
            newBuffer.put(buffer)
            buffer = newBuffer
        }}
    }}
    
    fun writeU8(value: UByte) {{
        ensureCapacity(1)
        buffer.put(value.toByte())
    }}
    
    fun writeU16(value: UShort) {{
        ensureCapacity(2)
        buffer.putShort(value.toShort())
    }}
    
    fun writeU32(value: UInt) {{
        ensureCapacity(4)
        buffer.putInt(value.toInt())
    }}
    
    fun writeU64(value: ULong) {{
        ensureCapacity(8)
        buffer.putLong(value.toLong())
    }}
    
    fun writeI8(value: Byte) {{
        ensureCapacity(1)
        buffer.put(value)
    }}
    
    fun writeI16(value: Short) {{
        ensureCapacity(2)
        buffer.putShort(value)
    }}
    
    fun writeI32(value: Int) {{
        ensureCapacity(4)
        buffer.putInt(value)
    }}
    
    fun writeI64(value: Long) {{
        ensureCapacity(8)
        buffer.putLong(value)
    }}
    
    fun writeF32(value: Float) {{
        ensureCapacity(4)
        buffer.putFloat(value)
    }}
    
    fun writeF64(value: Double) {{
        ensureCapacity(8)
        buffer.putDouble(value)
    }}
    
    fun writeBool(value: Boolean) {{
        ensureCapacity(1)
        buffer.put(if (value) 1.toByte() else 0.toByte())
    }}
    
    fun writeString(value: String) {{
        val bytes = value.toByteArray(Charsets.UTF_8)
        ensureCapacity(4 + bytes.size)
        buffer.putInt(bytes.size)
        buffer.put(bytes)
    }}
    
    fun build(): ByteArray {{
        val result = ByteArray(buffer.position())
        buffer.flip()
        buffer.get(result)
        return result
    }}
}}
"#,
        manifest.meta.version_byte,
        &manifest.meta.fingerprint[..16]
    ));

    // Generate encode/decode extensions for each message
    for msg in &manifest.messages {
        content.push_str(&generate_message_codec(msg));
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/main/kotlin/io/motto/sdk/Codec.kt"),
        content,
    })
}

fn generate_message_codec(msg: &MessageDef) -> String {
    let name = &msg.name;
    let mut s = String::new();

    // Encode extension
    s.push_str(&format!(
        r#"
/** Encode {} to binary */
fun {}.encode(): ByteArray {{
    val builder = PacketBuilder()
"#,
        name, name
    ));

    for field in &msg.fields {
        let field_name = escape_kotlin(&field.name);
        if field.optional {
            s.push_str(&format!(
                r#"    {} ?.let {{
        builder.writeU8(1u)
        {}
    }} ?: builder.writeU8(0u)
"#,
                field_name,
                encode_kotlin_field("it", &field.type_ref)
            ));
        } else {
            s.push_str(&format!(
                "    {}\n",
                encode_kotlin_field(&field_name, &field.type_ref)
            ));
        }
    }

    s.push_str("    return builder.build()\n}\n");

    // Decode companion
    s.push_str(&format!(
        r#"
/** Decode {} from binary */
fun {}.Companion.decode(data: ByteArray): {}? {{
    val reader = PacketReader(data)
    if (!reader.validateVersion()) return null
    reader.skipVersionByte()
    
    return {}(
"#,
        name, name, name, name
    ));

    let decode_fields: Vec<String> = msg
        .fields
        .iter()
        .map(|f| {
            let field_name = escape_kotlin(&f.name);
            if f.optional {
                format!(
                    "        {} = if (reader.readU8() != 0.toUByte()) {} else null",
                    field_name,
                    decode_kotlin_field(&f.type_ref)
                )
            } else {
                format!(
                    "        {} = {}",
                    field_name,
                    decode_kotlin_field(&f.type_ref)
                )
            }
        })
        .collect();

    s.push_str(&decode_fields.join(",\n"));
    s.push_str("\n    )\n}\n");

    s
}

fn encode_kotlin_field(accessor: &str, type_ref: &str) -> String {
    match type_ref {
        "u8" => format!("builder.writeU8({}.toUByte())", accessor),
        "u16" => format!("builder.writeU16({}.toUShort())", accessor),
        "u32" => format!("builder.writeU32({}.toUInt())", accessor),
        "u64" => format!("builder.writeU64({}.toULong())", accessor),
        "i8" => format!("builder.writeI8({})", accessor),
        "i16" => format!("builder.writeI16({})", accessor),
        "i32" => format!("builder.writeI32({})", accessor),
        "i64" => format!("builder.writeI64({})", accessor),
        "f32" => format!("builder.writeF32({})", accessor),
        "f64" => format!("builder.writeF64({})", accessor),
        "bool" => format!("builder.writeBool({})", accessor),
        "String" => format!("builder.writeString({})", accessor),
        _ => format!("/* TODO: encode {} */", type_ref),
    }
}

fn decode_kotlin_field(type_ref: &str) -> String {
    match type_ref {
        "u8" => "reader.readU8()".to_string(),
        "u16" => "reader.readU16()".to_string(),
        "u32" => "reader.readU32()".to_string(),
        "u64" => "reader.readU64()".to_string(),
        "i8" => "reader.readI8()".to_string(),
        "i16" => "reader.readI16()".to_string(),
        "i32" => "reader.readI32()".to_string(),
        "i64" => "reader.readI64()".to_string(),
        "f32" => "reader.readF32()".to_string(),
        "f64" => "reader.readF64()".to_string(),
        "bool" => "reader.readBool()".to_string(),
        "String" => "reader.readString()".to_string(),
        _ => format!("null /* TODO: decode {} */", type_ref),
    }
}

fn generate_runtime(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{}

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import java.io.IOException
import kotlin.math.min
import kotlin.math.pow

/** Protocol version constant */
const val PROTOCOL_VERSION: Int = 0x{:02X}

/** Connection state */
enum class ConnectionState {{
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    RECONNECTING,
    ERROR
}}

/** Retry configuration */
data class RetryConfig(
    val maxRetries: Int = 5,
    val initialDelayMs: Long = 100,
    val maxDelayMs: Long = 30000,
    val backoffMultiplier: Double = 2.0
)

/** Calculate retry delay with exponential backoff */
fun calculateRetryDelay(attempt: Int, config: RetryConfig = RetryConfig()): Long {{
    val delay = config.initialDelayMs * config.backoffMultiplier.pow(attempt.toDouble())
    return min(delay.toLong(), config.maxDelayMs)
}}

/** Transport interface */
interface MottoTransport {{
    val state: StateFlow<ConnectionState>
    suspend fun connect()
    suspend fun disconnect()
    suspend fun send(data: ByteArray)
    fun receive(): Flow<ByteArray>
}}

/** WebSocket-based transport implementation */
class MottoWebSocketTransport(
    private val url: String,
    private val retryConfig: RetryConfig = RetryConfig()
) : MottoTransport {{
    
    private val _state = MutableStateFlow(ConnectionState.DISCONNECTED)
    override val state: StateFlow<ConnectionState> = _state.asStateFlow()
    
    private var retryAttempt = 0
    
    override suspend fun connect() {{
        _state.value = ConnectionState.CONNECTING
        
        try {{
            // TODO: Implement actual WebSocket/WebTransport connection
            // This is a placeholder
            _state.value = ConnectionState.CONNECTED
            retryAttempt = 0
        }} catch (e: IOException) {{
            _state.value = ConnectionState.ERROR
            throw e
        }}
    }}
    
    suspend fun reconnect() {{
        if (retryAttempt >= retryConfig.maxRetries) {{
            throw IOException("Max retry attempts exceeded")
        }}
        
        _state.value = ConnectionState.RECONNECTING
        val delay = calculateRetryDelay(retryAttempt, retryConfig)
        retryAttempt++
        
        delay(delay)
        connect()
    }}
    
    override suspend fun disconnect() {{
        // TODO: Close connection
        _state.value = ConnectionState.DISCONNECTED
    }}
    
    override suspend fun send(data: ByteArray) {{
        if (_state.value != ConnectionState.CONNECTED) {{
            throw IOException("Not connected")
        }}
        // TODO: Send data
    }}
    
    override fun receive(): Flow<ByteArray> = flow {{
        // TODO: Receive data
    }}
}}
"#,
        kotlin_header(manifest),
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/main/kotlin/io/motto/sdk/Runtime.kt"),
        content,
    })
}

fn generate_build_gradle(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"// MOTTO GENERATED - Protocol Version: 0x{:02X}

plugins {{
    kotlin("jvm") version "1.9.20"
    kotlin("plugin.serialization") version "1.9.20"
}}

group = "io.motto"
version = "0.1.0"

repositories {{
    mavenCentral()
}}

dependencies {{
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.3")
    
    testImplementation(kotlin("test"))
}}

tasks.test {{
    useJUnitPlatform()
}}

kotlin {{
    jvmToolchain(17)
}}
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("build.gradle.kts"),
        content,
    })
}

fn generate_tests(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"package io.motto.sdk

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class MottoSdkTests {{
    @Test
    fun protocolVersionByteMatchesManifest() {{
        assertEquals(0x{:02X}.toByte(), PROTOCOL_VERSION_BYTE)
    }}

    @Test
    fun packetBuilderWritesHeader() {{
        val builder = PacketBuilder()
        val data = builder.build()
        val view = PacketView(data)

        assertEquals(PROTOCOL_VERSION_BYTE, view.getVersionByte())
        assertTrue(view.validateVersion())
    }}
}}
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/test/kotlin/io/motto/sdk/MottoSdkTests.kt"),
        content,
    })
}

fn rust_to_kotlin_type(rust_type: &str) -> String {
    if let Some(inner_start) = rust_type.find('<') {
        let name = &rust_type[..inner_start];
        let inner = &rust_type[inner_start + 1..rust_type.len() - 1];

        match name {
            "Vec" => format!("List<{}>", rust_to_kotlin_type(inner)),
            "Option" => format!("{}?", rust_to_kotlin_type(inner)),
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "Map<{}, {}>",
                        rust_to_kotlin_type(parts[0]),
                        rust_to_kotlin_type(parts[1])
                    )
                } else {
                    "Map<Any, Any>".to_string()
                }
            }
            "HashSet" | "BTreeSet" => format!("Set<{}>", rust_to_kotlin_type(inner)),
            _ => rust_type.to_string(),
        }
    } else {
        match rust_type {
            "u8" => "UByte".to_string(),
            "u16" => "UShort".to_string(),
            "u32" => "UInt".to_string(),
            "u64" => "ULong".to_string(),
            "i8" => "Byte".to_string(),
            "i16" => "Short".to_string(),
            "i32" => "Int".to_string(),
            "i64" => "Long".to_string(),
            "f32" => "Float".to_string(),
            "f64" => "Double".to_string(),
            "bool" => "Boolean".to_string(),
            "String" | "str" => "String".to_string(),
            "char" => "Char".to_string(),
            "()" => "Unit".to_string(),
            _ => rust_type.to_string(),
        }
    }
}

fn escape_kotlin(name: &str) -> String {
    if KOTLIN_RESERVED.contains(&name) {
        format!("`{}`", name)
    } else {
        utils::to_camel_case(name)
    }
}
