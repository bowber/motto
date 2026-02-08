//! Unity/C# Backend Emitter
//!
//! Generates C# SDK for Unity with:
//! - Unsafe pointers for memory-efficient DllImport
//! - WebAssembly.instantiate support
//! - Zero-copy packet framing

use crate::emitters::{Emitter, EmitterConfig, GeneratedFile, utils};
use crate::ir::manifest::*;
use anyhow::Result;
use std::path::PathBuf;

/// C# reserved words
const CSHARP_RESERVED: &[&str] = &[
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

pub struct UnityEmitter;

impl Emitter for UnityEmitter {
    fn platform(&self) -> &'static str {
        "unity"
    }

    fn extension(&self) -> &'static str {
        "cs"
    }

    fn emit(&self, config: &EmitterConfig) -> Result<Vec<GeneratedFile>> {
        let files = vec![
            generate_types(&config.manifest)?,
            generate_codec(&config.manifest)?,
            generate_runtime(&config.manifest)?,
            generate_native_bridge(&config.manifest)?,
            generate_asmdef(&config.manifest)?,
            generate_tests(&config.manifest)?,
        ];

        Ok(files)
    }
}

/// Emit Unity SDK
pub fn emit(config: &EmitterConfig) -> Result<()> {
    let emitter = UnityEmitter;
    let files = emitter.emit(config)?;

    let unity_dir = config.output_dir.join("unity").join("MottoSDK");
    std::fs::create_dir_all(&unity_dir)?;

    for file in files {
        let path = unity_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }

    Ok(())
}

fn csharp_header(manifest: &SchemaManifest) -> String {
    format!(
        r#"/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0x{:02X}
 * Schema Fingerprint: {}
 * Generated At: {}
 */

using System;
using System.Runtime.InteropServices;
using System.Text;
"#,
        manifest.meta.version_byte,
        &manifest.meta.fingerprint[..16],
        manifest.meta.generated_at
    )
}

fn generate_types(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = csharp_header(manifest);

    content.push_str("\nnamespace Motto.SDK\n{\n");

    // Generate enums
    for e in &manifest.enums {
        content.push_str(&generate_enum_type(e));
        content.push('\n');
    }

    // Generate structs
    for msg in &manifest.messages {
        content.push_str(&generate_struct(msg));
        content.push('\n');
    }

    content.push_str("}\n");

    Ok(GeneratedFile {
        path: PathBuf::from("Runtime/Types.cs"),
        content,
    })
}

fn generate_enum_type(e: &EnumManifest) -> String {
    let mut s = String::new();

    if let Some(docs) = &e.docs {
        s.push_str(&format!("    /// <summary>{}</summary>\n", docs));
    }

    if e.is_simple {
        // C-style enum
        let base_type = rust_to_csharp_type(&e.repr);
        s.push_str(&format!(
            "    public enum {} : {}\n    {{\n",
            e.name, base_type
        ));
        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("        /// <summary>{}</summary>\n", docs));
            }
            s.push_str(&format!("        {} = {},\n", v.name, v.discriminant));
        }
        s.push_str("    }\n");
    } else {
        // Tagged union -> abstract base class with derived types
        s.push_str(&format!("    public abstract class {}\n    {{\n", e.name));
        s.push_str("        public abstract byte Tag { get; }\n");
        s.push_str("    }\n\n");

        for (idx, v) in e.variants.iter().enumerate() {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /// <summary>{}</summary>\n", docs));
            }
            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!(
                        "    public sealed class {}_{} : {}\n    {{\n        public override byte Tag => {};\n    }}\n\n",
                        e.name, v.name, e.name, idx
                    ));
                }
                VariantData::Tuple { types } => {
                    s.push_str(&format!(
                        "    public sealed class {}_{} : {}\n    {{\n        public override byte Tag => {};\n",
                        e.name, v.name, e.name, idx
                    ));
                    for (i, t) in types.iter().enumerate() {
                        s.push_str(&format!(
                            "        public {} Item{} {{ get; set; }}\n",
                            rust_to_csharp_type(t),
                            i + 1
                        ));
                    }
                    s.push_str("    }\n\n");
                }
                VariantData::Struct { fields } => {
                    s.push_str(&format!(
                        "    public sealed class {}_{} : {}\n    {{\n        public override byte Tag => {};\n",
                        e.name, v.name, e.name, idx
                    ));
                    for f in fields {
                        s.push_str(&format!(
                            "        public {} {} {{ get; set; }}\n",
                            rust_to_csharp_type(&f.type_ref),
                            utils::to_pascal_case(&f.name)
                        ));
                    }
                    s.push_str("    }\n\n");
                }
            }
        }
    }

    s
}

fn generate_struct(msg: &MessageDef) -> String {
    let mut s = String::new();

    if let Some(docs) = &msg.docs {
        s.push_str(&format!("    /// <summary>{}</summary>\n", docs));
    }

    // Use struct for fixed-size, class for variable-size
    let type_keyword = if msg.fixed_size.is_some() {
        "struct"
    } else {
        "class"
    };
    let struct_layout = if msg.fixed_size.is_some() {
        format!(
            "    [StructLayout(LayoutKind.Sequential, Pack = {})]\n",
            msg.alignment
        )
    } else {
        String::new()
    };

    let generics = if msg.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", msg.generics.join(", "))
    };

    s.push_str(&struct_layout);
    s.push_str(&format!(
        "    public {} {}{}\n    {{\n",
        type_keyword, msg.name, generics
    ));

    for field in &msg.fields {
        if let Some(docs) = &field.docs {
            s.push_str(&format!("        /// <summary>{}</summary>\n", docs));
        }
        let field_name = utils::to_pascal_case(&escape_csharp(&field.name));
        let field_type = rust_to_csharp_type(&field.type_ref);
        let nullable = if field.optional && !is_value_type(&field.type_ref) {
            "?"
        } else {
            ""
        };
        s.push_str(&format!(
            "        public {}{} {} {{ get; set; }}\n",
            field_type, nullable, field_name
        ));
    }

    s.push_str("    }\n");
    s
}

fn generate_codec(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = csharp_header(manifest);

    content.push_str(&format!(
        r#"
namespace Motto.SDK
{{
    /// <summary>Protocol version byte embedded in all packets</summary>
    public static class Protocol
    {{
        public const byte VersionByte = 0x{:02X};
        public const string Fingerprint = "{}";
    }}

    /// <summary>Zero-copy packet reader using Span</summary>
    public ref struct PacketReader
    {{
        private ReadOnlySpan<byte> _data;
        private int _offset;

        public PacketReader(ReadOnlySpan<byte> data)
        {{
            _data = data;
            _offset = 0;
        }}

        public bool ValidateVersion()
        {{
            return _data.Length > 0 && _data[0] == Protocol.VersionByte;
        }}

        public void SkipVersionByte()
        {{
            _offset = 1;
        }}

        public byte ReadU8()
        {{
            return _data[_offset++];
        }}

        public ushort ReadU16()
        {{
            var value = BitConverter.ToUInt16(_data.Slice(_offset, 2));
            _offset += 2;
            return value;
        }}

        public uint ReadU32()
        {{
            var value = BitConverter.ToUInt32(_data.Slice(_offset, 4));
            _offset += 4;
            return value;
        }}

        public ulong ReadU64()
        {{
            var value = BitConverter.ToUInt64(_data.Slice(_offset, 8));
            _offset += 8;
            return value;
        }}

        public float ReadF32()
        {{
            var value = BitConverter.ToSingle(_data.Slice(_offset, 4));
            _offset += 4;
            return value;
        }}

        public double ReadF64()
        {{
            var value = BitConverter.ToDouble(_data.Slice(_offset, 8));
            _offset += 8;
            return value;
        }}

        public bool ReadBool()
        {{
            return ReadU8() != 0;
        }}

        public string ReadString()
        {{
            var length = (int)ReadU32();
            var str = Encoding.UTF8.GetString(_data.Slice(_offset, length));
            _offset += length;
            return str;
        }}

        public int Remaining => _data.Length - _offset;
    }}

    /// <summary>Packet builder for encoding</summary>
    public class PacketBuilder
    {{
        private byte[] _buffer;
        private int _offset;

        public PacketBuilder(int initialCapacity = 256)
        {{
            _buffer = new byte[initialCapacity];
            // Write version byte header
            WriteU8(Protocol.VersionByte);
        }}

        private void EnsureCapacity(int need)
        {{
            if (_offset + need > _buffer.Length)
            {{
                var newSize = Math.Max(_buffer.Length * 2, _offset + need);
                Array.Resize(ref _buffer, newSize);
            }}
        }}

        public void WriteU8(byte value)
        {{
            EnsureCapacity(1);
            _buffer[_offset++] = value;
        }}

        public void WriteU16(ushort value)
        {{
            EnsureCapacity(2);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 2;
        }}

        public void WriteU32(uint value)
        {{
            EnsureCapacity(4);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 4;
        }}

        public void WriteU64(ulong value)
        {{
            EnsureCapacity(8);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 8;
        }}

        public void WriteF32(float value)
        {{
            EnsureCapacity(4);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 4;
        }}

        public void WriteF64(double value)
        {{
            EnsureCapacity(8);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 8;
        }}

        public void WriteBool(bool value)
        {{
            WriteU8((byte)(value ? 1 : 0));
        }}

        public void WriteString(string value)
        {{
            var bytes = Encoding.UTF8.GetBytes(value);
            WriteU32((uint)bytes.Length);
            EnsureCapacity(bytes.Length);
            bytes.CopyTo(_buffer.AsSpan(_offset));
            _offset += bytes.Length;
        }}

        public byte[] Build()
        {{
            var result = new byte[_offset];
            Array.Copy(_buffer, result, _offset);
            return result;
        }}
    }}
"#,
        manifest.meta.version_byte,
        &manifest.meta.fingerprint[..16]
    ));

    // Generate encode/decode for each message
    for msg in &manifest.messages {
        content.push_str(&generate_message_codec(msg));
    }

    content.push_str("}\n");

    Ok(GeneratedFile {
        path: PathBuf::from("Runtime/Codec.cs"),
        content,
    })
}

fn generate_message_codec(msg: &MessageDef) -> String {
    let name = &msg.name;
    let mut s = String::new();

    s.push_str(&format!(
        r#"
    /// <summary>Codec extensions for {}</summary>
    public static class {}Codec
    {{
        public static byte[] Encode(this {} msg)
        {{
            var builder = new PacketBuilder();
"#,
        name, name, name
    ));

    for field in &msg.fields {
        let field_name = utils::to_pascal_case(&escape_csharp(&field.name));
        if field.optional {
            s.push_str(&format!(
                r#"            if (msg.{} != null)
            {{
                builder.WriteU8(1);
                {};
            }}
            else
            {{
                builder.WriteU8(0);
            }}
"#,
                field_name,
                encode_csharp_field(&format!("msg.{}", field_name), &field.type_ref)
            ));
        } else {
            s.push_str(&format!(
                "            {};\n",
                encode_csharp_field(&format!("msg.{}", field_name), &field.type_ref)
            ));
        }
    }

    s.push_str("            return builder.Build();\n        }\n\n");

    // Decode
    s.push_str(&format!(
        r#"        public static {} Decode(ReadOnlySpan<byte> data)
        {{
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new {}
            {{
"#,
        name, name
    ));

    for field in &msg.fields {
        let field_name = utils::to_pascal_case(&escape_csharp(&field.name));
        if field.optional {
            s.push_str(&format!(
                "                {} = reader.ReadU8() != 0 ? {} : default,\n",
                field_name,
                decode_csharp_field(&field.type_ref)
            ));
        } else {
            s.push_str(&format!(
                "                {} = {},\n",
                field_name,
                decode_csharp_field(&field.type_ref)
            ));
        }
    }

    s.push_str("            };\n        }\n    }\n");

    s
}

fn encode_csharp_field(accessor: &str, type_ref: &str) -> String {
    match type_ref {
        "u8" => format!("builder.WriteU8({})", accessor),
        "u16" => format!("builder.WriteU16({})", accessor),
        "u32" => format!("builder.WriteU32({})", accessor),
        "u64" => format!("builder.WriteU64({})", accessor),
        "i8" => format!("builder.WriteU8((byte){})", accessor),
        "i16" => format!("builder.WriteU16((ushort){})", accessor),
        "i32" => format!("builder.WriteU32((uint){})", accessor),
        "i64" => format!("builder.WriteU64((ulong){})", accessor),
        "f32" => format!("builder.WriteF32({})", accessor),
        "f64" => format!("builder.WriteF64({})", accessor),
        "bool" => format!("builder.WriteBool({})", accessor),
        "String" => format!("builder.WriteString({})", accessor),
        _ => format!("/* TODO: encode {} */", type_ref),
    }
}

fn decode_csharp_field(type_ref: &str) -> String {
    match type_ref {
        "u8" => "reader.ReadU8()".to_string(),
        "u16" => "reader.ReadU16()".to_string(),
        "u32" => "reader.ReadU32()".to_string(),
        "u64" => "reader.ReadU64()".to_string(),
        "i8" => "(sbyte)reader.ReadU8()".to_string(),
        "i16" => "(short)reader.ReadU16()".to_string(),
        "i32" => "(int)reader.ReadU32()".to_string(),
        "i64" => "(long)reader.ReadU64()".to_string(),
        "f32" => "reader.ReadF32()".to_string(),
        "f64" => "reader.ReadF64()".to_string(),
        "bool" => "reader.ReadBool()".to_string(),
        "String" => "reader.ReadString()".to_string(),
        _ => format!("default /* TODO: decode {} */", type_ref),
    }
}

fn generate_runtime(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{}
using System.Threading.Tasks;
using UnityEngine;

namespace Motto.SDK
{{
    /// <summary>Connection state</summary>
    public enum ConnectionState
    {{
        Disconnected,
        Connecting,
        Connected,
        Reconnecting,
        Error
    }}

    /// <summary>Retry configuration</summary>
    [System.Serializable]
    public class RetryConfig
    {{
        public int MaxRetries = 5;
        public int InitialDelayMs = 100;
        public int MaxDelayMs = 30000;
        public float BackoffMultiplier = 2.0f;
    }}

    /// <summary>Calculate retry delay with exponential backoff</summary>
    public static class RetryHelper
    {{
        public static int CalculateDelay(int attempt, RetryConfig config)
        {{
            var delay = config.InitialDelayMs * Mathf.Pow(config.BackoffMultiplier, attempt);
            return Mathf.Min((int)delay, config.MaxDelayMs);
        }}
    }}

    /// <summary>Transport interface</summary>
    public interface IMottoTransport
    {{
        ConnectionState State {{ get; }}
        Task ConnectAsync();
        Task DisconnectAsync();
        Task SendAsync(byte[] data);
        event System.Action<byte[]> OnReceived;
    }}

    /// <summary>WebSocket transport for Unity</summary>
    public class MottoWebSocketTransport : IMottoTransport
    {{
        private readonly string _url;
        private readonly RetryConfig _retryConfig;
        private int _retryAttempt;

        public ConnectionState State {{ get; private set; }} = ConnectionState.Disconnected;
        public event System.Action<byte[]> OnReceived;

        public MottoWebSocketTransport(string url, RetryConfig retryConfig = null)
        {{
            _url = url;
            _retryConfig = retryConfig ?? new RetryConfig();
        }}

        public async Task ConnectAsync()
        {{
            State = ConnectionState.Connecting;

            try
            {{
                // TODO: Implement actual WebSocket connection
                // For Unity, consider using NativeWebSocket or UnityWebSocket
                await Task.Delay(100); // Placeholder
                State = ConnectionState.Connected;
                _retryAttempt = 0;
            }}
            catch (System.Exception)
            {{
                State = ConnectionState.Error;
                throw;
            }}
        }}

        public async Task ReconnectAsync()
        {{
            if (_retryAttempt >= _retryConfig.MaxRetries)
            {{
                throw new System.Exception("Max retry attempts exceeded");
            }}

            State = ConnectionState.Reconnecting;
            var delay = RetryHelper.CalculateDelay(_retryAttempt, _retryConfig);
            _retryAttempt++;

            await Task.Delay(delay);
            await ConnectAsync();
        }}

        public async Task DisconnectAsync()
        {{
            // TODO: Close WebSocket
            await Task.CompletedTask;
            State = ConnectionState.Disconnected;
        }}

        public async Task SendAsync(byte[] data)
        {{
            if (State != ConnectionState.Connected)
            {{
                throw new System.InvalidOperationException("Not connected");
            }}

            // TODO: Send via WebSocket
            await Task.CompletedTask;
        }}
    }}
}}
"#,
        csharp_header(manifest)
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Runtime/Runtime.cs"),
        content,
    })
}

fn generate_native_bridge(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{}
using System.Runtime.InteropServices;

namespace Motto.SDK
{{
    /// <summary>Native bridge for DllImport or WebAssembly</summary>
    public static unsafe class NativeBridge
    {{
        private const string DllName = "motto_native";

#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")]
        private static extern int motto_wasm_init(byte* wasmBytes, int wasmLength);

        [DllImport("__Internal")]
        private static extern int motto_wasm_encode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport("__Internal")]
        private static extern int motto_wasm_decode(byte* input, int inputLength, byte* output, int outputCapacity);
#else
        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_init();

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_encode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_decode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_compress(byte* input, int inputLength, byte* output, int outputCapacity, int level);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_decompress(byte* input, int inputLength, byte* output, int outputCapacity);
#endif

        /// <summary>Initialize native library</summary>
        public static bool Initialize()
        {{
#if UNITY_WEBGL && !UNITY_EDITOR
            // WASM initialization handled by JavaScript
            return true;
#else
            return motto_native_init() == 0;
#endif
        }}

        /// <summary>Encode using native implementation (zero-copy)</summary>
        public static int Encode(ReadOnlySpan<byte> input, Span<byte> output)
        {{
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {{
#if UNITY_WEBGL && !UNITY_EDITOR
                return motto_wasm_encode(inputPtr, input.Length, outputPtr, output.Length);
#else
                return motto_native_encode(inputPtr, input.Length, outputPtr, output.Length);
#endif
            }}
        }}

        /// <summary>Decode using native implementation (zero-copy)</summary>
        public static int Decode(ReadOnlySpan<byte> input, Span<byte> output)
        {{
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {{
#if UNITY_WEBGL && !UNITY_EDITOR
                return motto_wasm_decode(inputPtr, input.Length, outputPtr, output.Length);
#else
                return motto_native_decode(inputPtr, input.Length, outputPtr, output.Length);
#endif
            }}
        }}

#if !UNITY_WEBGL || UNITY_EDITOR
        /// <summary>Compress with Zstd (native only)</summary>
        public static int Compress(ReadOnlySpan<byte> input, Span<byte> output, int level = 3)
        {{
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {{
                return motto_native_compress(inputPtr, input.Length, outputPtr, output.Length, level);
            }}
        }}

        /// <summary>Decompress with Zstd (native only)</summary>
        public static int Decompress(ReadOnlySpan<byte> input, Span<byte> output)
        {{
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {{
                return motto_native_decompress(inputPtr, input.Length, outputPtr, output.Length);
            }}
        }}
#endif
    }}
}}
"#,
        csharp_header(manifest)
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Runtime/NativeBridge.cs"),
        content,
    })
}

fn generate_asmdef(_manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = r#"{
    "name": "Motto.SDK",
    "rootNamespace": "Motto.SDK",
    "references": [],
    "includePlatforms": [],
    "excludePlatforms": [],
    "allowUnsafeCode": true,
    "overrideReferences": false,
    "precompiledReferences": [],
    "autoReferenced": true,
    "defineConstraints": [],
    "versionDefines": [],
    "noEngineReferences": false
}
"#
    .to_string();

    Ok(GeneratedFile {
        path: PathBuf::from("Motto.SDK.asmdef"),
        content,
    })
}

fn generate_tests(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"using NUnit.Framework;

namespace Motto.SDK.Tests
{{
    public class CodecTests
    {{
        [Test]
        public void ProtocolVersionMatchesManifest()
        {{
            Assert.AreEqual(0x{:02X}, Protocol.VersionByte);
        }}

        [Test]
        public void PacketBuilderWritesHeader()
        {{
            var builder = new PacketBuilder();
            var data = builder.Build();

            Assert.Greater(data.Length, 0);
            Assert.AreEqual(Protocol.VersionByte, data[0]);
        }}
    }}
}}
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Runtime/Tests/CodecTests.cs"),
        content,
    })
}

fn rust_to_csharp_type(rust_type: &str) -> String {
    if let Some(inner_start) = rust_type.find('<') {
        let name = &rust_type[..inner_start];
        let inner = &rust_type[inner_start + 1..rust_type.len() - 1];

        match name {
            "Vec" => format!("{}[]", rust_to_csharp_type(inner)),
            "Option" => {
                let inner_type = rust_to_csharp_type(inner);
                if is_value_type(inner) {
                    format!("{}?", inner_type)
                } else {
                    inner_type
                }
            }
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "System.Collections.Generic.Dictionary<{}, {}>",
                        rust_to_csharp_type(parts[0]),
                        rust_to_csharp_type(parts[1])
                    )
                } else {
                    "System.Collections.Generic.Dictionary<object, object>".to_string()
                }
            }
            "HashSet" | "BTreeSet" => {
                format!(
                    "System.Collections.Generic.HashSet<{}>",
                    rust_to_csharp_type(inner)
                )
            }
            _ => rust_type.to_string(),
        }
    } else {
        match rust_type {
            "u8" => "byte".to_string(),
            "u16" => "ushort".to_string(),
            "u32" => "uint".to_string(),
            "u64" => "ulong".to_string(),
            "i8" => "sbyte".to_string(),
            "i16" => "short".to_string(),
            "i32" => "int".to_string(),
            "i64" => "long".to_string(),
            "f32" => "float".to_string(),
            "f64" => "double".to_string(),
            "bool" => "bool".to_string(),
            "String" | "str" => "string".to_string(),
            "char" => "char".to_string(),
            "()" => "void".to_string(),
            _ => rust_type.to_string(),
        }
    }
}

fn is_value_type(rust_type: &str) -> bool {
    matches!(
        rust_type,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "f32"
            | "f64"
            | "bool"
            | "char"
    )
}

fn escape_csharp(name: &str) -> String {
    if CSHARP_RESERVED.contains(&name) {
        format!("@{}", name)
    } else {
        name.to_string()
    }
}
