//! Swift Backend Emitter
//!
//! Generates Swift SDK for iOS/macOS with:
//! - Native WebTransport support
//! - Codable protocol conformance
//! - Zero-copy packet framing

use crate::emitters::{utils, Emitter, EmitterConfig, GeneratedFile, TransportMode};
use crate::ir::manifest::*;
use anyhow::Result;
use std::path::PathBuf;

/// Swift reserved words
const SWIFT_RESERVED: &[&str] = &[
    "associatedtype",
    "class",
    "deinit",
    "enum",
    "extension",
    "fileprivate",
    "func",
    "import",
    "init",
    "inout",
    "internal",
    "let",
    "open",
    "operator",
    "private",
    "protocol",
    "public",
    "rethrows",
    "static",
    "struct",
    "subscript",
    "typealias",
    "var",
    "break",
    "case",
    "continue",
    "default",
    "defer",
    "do",
    "else",
    "fallthrough",
    "for",
    "guard",
    "if",
    "in",
    "repeat",
    "return",
    "switch",
    "where",
    "while",
    "as",
    "Any",
    "catch",
    "false",
    "is",
    "nil",
    "super",
    "self",
    "Self",
    "throw",
    "throws",
    "true",
    "try",
    "Type",
    "type",
];

pub struct SwiftEmitter;

impl Emitter for SwiftEmitter {
    fn platform(&self) -> &'static str {
        "swift"
    }

    fn extension(&self) -> &'static str {
        "swift"
    }

    fn emit(&self, config: &EmitterConfig) -> Result<Vec<GeneratedFile>> {
        let files = vec![
            generate_types(&config.manifest)?,
            generate_codec(&config.manifest)?,
            generate_runtime(&config.manifest, config.transport_mode)?,
            generate_package_swift(&config.manifest)?,
            generate_tests(&config.manifest)?,
        ];

        Ok(files)
    }
}

/// Emit Swift SDK
pub fn emit(config: &EmitterConfig) -> Result<()> {
    let emitter = SwiftEmitter;
    let files = emitter.emit(config)?;

    let swift_dir = config.output_dir.join("swift");
    std::fs::create_dir_all(&swift_dir)?;

    for file in files {
        let path = swift_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }

    Ok(())
}

fn swift_header(manifest: &SchemaManifest) -> String {
    format!(
        r#"//
// MOTTO GENERATED CODE - DO NOT EDIT
//
// Protocol Version: 0x{:02X}
// Schema Fingerprint: {}
// Generated At: {}
//

import Foundation
"#,
        manifest.meta.version_byte,
        &manifest.meta.fingerprint[..16],
        manifest.meta.generated_at
    )
}

fn generate_types(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = swift_header(manifest);

    content.push_str("\n// MARK: - Type Definitions\n\n");

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

    Ok(GeneratedFile {
        path: PathBuf::from("Sources/MottoSDK/Types.swift"),
        content,
    })
}

fn generate_enum_type(e: &EnumManifest) -> String {
    let mut s = String::new();

    if let Some(docs) = &e.docs {
        s.push_str(&format!("/// {}\n", docs));
    }

    if e.is_simple {
        // C-style enum
        let raw_type = rust_to_swift_type(&e.repr);
        s.push_str(&format!(
            "public enum {}: {}, Codable, Sendable {{\n",
            e.name, raw_type
        ));
        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /// {}\n", docs));
            }
            s.push_str(&format!(
                "    case {} = {}\n",
                utils::to_camel_case(&v.name),
                v.discriminant
            ));
        }
        s.push_str("}\n");
    } else {
        // Tagged union -> Swift enum with associated values
        s.push_str(&format!("public enum {}: Codable, Sendable {{\n", e.name));
        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /// {}\n", docs));
            }
            let case_name = utils::to_camel_case(&v.name);
            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!("    case {}\n", case_name));
                }
                VariantData::Tuple { types } => {
                    let params: Vec<String> = types.iter().map(|t| rust_to_swift_type(t)).collect();
                    s.push_str(&format!("    case {}({})\n", case_name, params.join(", ")));
                }
                VariantData::Struct { fields } => {
                    let params: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                escape_swift(&f.name),
                                rust_to_swift_type(&f.type_ref)
                            )
                        })
                        .collect();
                    s.push_str(&format!("    case {}({})\n", case_name, params.join(", ")));
                }
            }
        }
        s.push_str("}\n");
    }

    s
}

fn generate_struct(msg: &MessageDef) -> String {
    let mut s = String::new();

    if let Some(docs) = &msg.docs {
        s.push_str(&format!("/// {}\n", docs));
    }

    let generics = if msg.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", msg.generics.join(", "))
    };

    s.push_str(&format!(
        "public struct {}{}: Codable, Sendable {{\n",
        msg.name, generics
    ));

    for field in &msg.fields {
        if let Some(docs) = &field.docs {
            s.push_str(&format!("    /// {}\n", docs));
        }
        let field_name = escape_swift(&field.name);
        let field_type = rust_to_swift_type(&field.type_ref);
        let optional = if field.optional { "?" } else { "" };
        s.push_str(&format!(
            "    public var {}: {}{}\n",
            field_name, field_type, optional
        ));
    }

    // Generate initializer
    s.push_str("\n    public init(\n");
    let init_params: Vec<String> = msg
        .fields
        .iter()
        .map(|f| {
            let name = escape_swift(&f.name);
            let ty = rust_to_swift_type(&f.type_ref);
            let optional = if f.optional { "?" } else { "" };
            let default = if f.optional { " = nil" } else { "" };
            format!("        {}: {}{}{}", name, ty, optional, default)
        })
        .collect();
    s.push_str(&init_params.join(",\n"));
    s.push_str("\n    ) {\n");
    for field in &msg.fields {
        let name = escape_swift(&field.name);
        s.push_str(&format!("        self.{} = {}\n", name, name));
    }
    s.push_str("    }\n");

    s.push_str("}\n");
    s
}

fn generate_codec(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = swift_header(manifest);

    content.push_str(&format!(
        r#"
// MARK: - Codec

/// Protocol version byte embedded in all packets
public let PROTOCOL_VERSION_BYTE: UInt8 = 0x{:02X}

/// Schema fingerprint for validation
public let SCHEMA_FINGERPRINT = "{}"

/// Zero-copy packet reader
public struct PacketReader {{
    private var data: Data
    private var offset: Int = 0
    
    public init(data: Data) {{
        self.data = data
    }}
    
    public mutating func validateVersion() -> Bool {{
        guard data.count > 0 else {{ return false }}
        return data[0] == PROTOCOL_VERSION_BYTE
    }}
    
    public mutating func skipVersionByte() {{
        offset = 1
    }}
    
    public mutating func readU8() -> UInt8 {{
        let value = data[offset]
        offset += 1
        return value
    }}
    
    public mutating func readU16() -> UInt16 {{
        let value = data.withUnsafeBytes {{ ptr in
            ptr.load(fromByteOffset: offset, as: UInt16.self)
        }}
        offset += 2
        return UInt16(littleEndian: value)
    }}
    
    public mutating func readU32() -> UInt32 {{
        let value = data.withUnsafeBytes {{ ptr in
            ptr.load(fromByteOffset: offset, as: UInt32.self)
        }}
        offset += 4
        return UInt32(littleEndian: value)
    }}
    
    public mutating func readU64() -> UInt64 {{
        let value = data.withUnsafeBytes {{ ptr in
            ptr.load(fromByteOffset: offset, as: UInt64.self)
        }}
        offset += 8
        return UInt64(littleEndian: value)
    }}
    
    public mutating func readF32() -> Float {{
        let bits = readU32()
        return Float(bitPattern: bits)
    }}
    
    public mutating func readF64() -> Double {{
        let bits = readU64()
        return Double(bitPattern: bits)
    }}
    
    public mutating func readBool() -> Bool {{
        return readU8() != 0
    }}
    
    public mutating func readString() -> String {{
        let length = Int(readU32())
        let stringData = data.subdata(in: offset..<(offset + length))
        offset += length
        return String(data: stringData, encoding: .utf8) ?? ""
    }}
}}

/// Packet builder for encoding
public struct PacketBuilder {{
    private var data: Data
    
    public init(capacity: Int = 256) {{
        data = Data(capacity: capacity)
        // Write version byte header
        data.append(PROTOCOL_VERSION_BYTE)
    }}
    
    public mutating func writeU8(_ value: UInt8) {{
        data.append(value)
    }}
    
    public mutating func writeU16(_ value: UInt16) {{
        var v = value.littleEndian
        withUnsafeBytes(of: &v) {{ data.append(contentsOf: $0) }}
    }}
    
    public mutating func writeU32(_ value: UInt32) {{
        var v = value.littleEndian
        withUnsafeBytes(of: &v) {{ data.append(contentsOf: $0) }}
    }}
    
    public mutating func writeU64(_ value: UInt64) {{
        var v = value.littleEndian
        withUnsafeBytes(of: &v) {{ data.append(contentsOf: $0) }}
    }}
    
    public mutating func writeF32(_ value: Float) {{
        writeU32(value.bitPattern)
    }}
    
    public mutating func writeF64(_ value: Double) {{
        writeU64(value.bitPattern)
    }}
    
    public mutating func writeBool(_ value: Bool) {{
        writeU8(value ? 1 : 0)
    }}
    
    public mutating func writeString(_ value: String) {{
        let bytes = value.data(using: .utf8) ?? Data()
        writeU32(UInt32(bytes.count))
        data.append(bytes)
    }}
    
    public func build() -> Data {{
        return data
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

    Ok(GeneratedFile {
        path: PathBuf::from("Sources/MottoSDK/Codec.swift"),
        content,
    })
}

fn generate_message_codec(msg: &MessageDef) -> String {
    let name = &msg.name;
    let mut s = String::new();

    // Extension with encode/decode
    s.push_str(&format!(
        r#"
// MARK: - {} Codec

extension {} {{
    public func encode() -> Data {{
        var builder = PacketBuilder()
"#,
        name, name
    ));

    for field in &msg.fields {
        let field_name = escape_swift(&field.name);
        if field.optional {
            s.push_str(&format!(
                r#"        if let {} = self.{} {{
            builder.writeU8(1)
            {}
        }} else {{
            builder.writeU8(0)
        }}
"#,
                field_name,
                field_name,
                encode_swift_field(&field_name, &field.type_ref)
            ));
        } else {
            s.push_str(&format!(
                "        {}\n",
                encode_swift_field(&format!("self.{}", field_name), &field.type_ref)
            ));
        }
    }

    s.push_str("        return builder.build()\n    }\n\n");

    // Decode
    s.push_str(&format!(
        r#"    public static func decode(from data: Data) -> {}? {{
        var reader = PacketReader(data: data)
        guard reader.validateVersion() else {{ return nil }}
        reader.skipVersionByte()
        
        return {}(
"#,
        name, name
    ));

    let decode_params: Vec<String> = msg
        .fields
        .iter()
        .map(|f| {
            let field_name = escape_swift(&f.name);
            if f.optional {
                format!(
                    "            {}: reader.readU8() != 0 ? {} : nil",
                    field_name,
                    decode_swift_field(&f.type_ref)
                )
            } else {
                format!(
                    "            {}: {}",
                    field_name,
                    decode_swift_field(&f.type_ref)
                )
            }
        })
        .collect();

    s.push_str(&decode_params.join(",\n"));
    s.push_str("\n        )\n    }\n}\n");

    s
}

fn encode_swift_field(accessor: &str, type_ref: &str) -> String {
    match type_ref {
        "u8" => format!("builder.writeU8({})", accessor),
        "u16" => format!("builder.writeU16({})", accessor),
        "u32" => format!("builder.writeU32({})", accessor),
        "u64" => format!("builder.writeU64({})", accessor),
        "i8" => format!("builder.writeU8(UInt8(bitPattern: {}))", accessor),
        "i16" => format!("builder.writeU16(UInt16(bitPattern: {}))", accessor),
        "i32" => format!("builder.writeU32(UInt32(bitPattern: {}))", accessor),
        "i64" => format!("builder.writeU64(UInt64(bitPattern: {}))", accessor),
        "f32" => format!("builder.writeF32({})", accessor),
        "f64" => format!("builder.writeF64({})", accessor),
        "bool" => format!("builder.writeBool({})", accessor),
        "String" => format!("builder.writeString({})", accessor),
        _ => format!("/* TODO: encode {} */", type_ref),
    }
}

fn decode_swift_field(type_ref: &str) -> String {
    match type_ref {
        "u8" => "reader.readU8()".to_string(),
        "u16" => "reader.readU16()".to_string(),
        "u32" => "reader.readU32()".to_string(),
        "u64" => "reader.readU64()".to_string(),
        "i8" => "Int8(bitPattern: reader.readU8())".to_string(),
        "i16" => "Int16(bitPattern: reader.readU16())".to_string(),
        "i32" => "Int32(bitPattern: reader.readU32())".to_string(),
        "i64" => "Int64(bitPattern: reader.readU64())".to_string(),
        "f32" => "reader.readF32()".to_string(),
        "f64" => "reader.readF64()".to_string(),
        "bool" => "reader.readBool()".to_string(),
        "String" => "reader.readString()".to_string(),
        _ => format!("/* TODO: decode {} */", type_ref),
    }
}

fn generate_runtime(
    manifest: &SchemaManifest,
    transport_mode: TransportMode,
) -> Result<GeneratedFile> {
    let ffi_transport = if transport_mode == TransportMode::Ffi {
        r#"

// MARK: - FFI Transport (motto native core)

/// C function declarations for the motto transport core
/// Link with: -lmotto_transport
@_silgen_name("motto_transport_new")
func motto_transport_new(_ url: UnsafePointer<CChar>) -> OpaquePointer?
@_silgen_name("motto_transport_free")
func motto_transport_free(_ handle: OpaquePointer)
@_silgen_name("motto_transport_connect")
func motto_transport_connect(_ handle: OpaquePointer) -> Int32
@_silgen_name("motto_transport_close")
func motto_transport_close(_ handle: OpaquePointer)
@_silgen_name("motto_transport_send")
func motto_transport_send(_ handle: OpaquePointer, _ data: UnsafePointer<UInt8>, _ len: Int) -> Int32
@_silgen_name("motto_transport_recv")
func motto_transport_recv(_ handle: OpaquePointer, _ outData: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>, _ outLen: UnsafeMutablePointer<Int>) -> Int32
@_silgen_name("motto_transport_recv_free")
func motto_transport_recv_free(_ data: UnsafeMutablePointer<UInt8>, _ len: Int)
@_silgen_name("motto_transport_state")
func motto_transport_state(_ handle: OpaquePointer) -> UInt8
@_silgen_name("motto_transport_last_error")
func motto_transport_last_error(_ handle: OpaquePointer) -> UnsafePointer<CChar>?

/// FFI-backed transport using the motto native core library
public actor MottoFfiTransport: MottoTransportProtocol {{
    private let url: URL
    private let retryConfig: RetryConfig
    private var handle: OpaquePointer?
    public private(set) var state: ConnectionState = .disconnected

    public init(url: URL, retryConfig: RetryConfig = .default) {{
        self.url = url
        self.retryConfig = retryConfig
    }}

    public func connect() async throws {{
        state = .connecting
        let cUrl = url.absoluteString.withCString {{ ptr in
            motto_transport_new(ptr)
        }}
        guard let h = cUrl else {{
            state = .error(MottoError.connectionFailed("Failed to create FFI transport handle"))
            throw MottoError.connectionFailed("Failed to create FFI transport handle")
        }}
        self.handle = h
        let rc = motto_transport_connect(h)
        if rc != 0 {{
            state = .error(MottoError.connectionFailed("FFI connect failed"))
            throw MottoError.connectionFailed("FFI connect failed")
        }}
        state = .connected
    }}

    public func disconnect() async {{
        if let h = handle {{
            motto_transport_close(h)
            motto_transport_free(h)
            handle = nil
        }}
        state = .disconnected
    }}

    public func send(_ data: Data) async throws {{
        guard let h = handle else {{ throw MottoError.notConnected }}
        let rc = data.withUnsafeBytes {{ ptr in
            motto_transport_send(h, ptr.baseAddress!.assumingMemoryBound(to: UInt8.self), ptr.count)
        }}
        if rc != 0 {{ throw MottoError.sendFailed("FFI send failed") }}
    }}

    public func receive() async throws -> Data {{
        guard let h = handle else {{ throw MottoError.notConnected }}
        var outData: UnsafeMutablePointer<UInt8>? = nil
        var outLen: Int = 0
        let rc = motto_transport_recv(h, &outData, &outLen)
        if rc != 0 {{ throw MottoError.receiveFailed("FFI recv failed") }}
        guard let ptr = outData else {{ throw MottoError.receiveFailed("Null data") }}
        let data = Data(bytes: ptr, count: outLen)
        motto_transport_recv_free(ptr, outLen)
        return data
    }}

    deinit {{
        // Note: deinit cannot be async, so we synchronously free
        if let h = handle {{
            motto_transport_close(h)
            motto_transport_free(h)
        }}
    }}
}}

enum MottoError: Error {{
    case connectionFailed(String)
    case notConnected
    case sendFailed(String)
    case receiveFailed(String)
}}
"#
    } else {
        ""
    };

    let content = format!(
        r#"{}

// MARK: - Runtime

/// Connection state machine
public enum ConnectionState: Sendable {{
    case disconnected
    case connecting
    case connected
    case reconnecting
    case error(Error)
}}

/// Retry configuration
public struct RetryConfig: Sendable {{
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
    
    public init(maxRetries: Int, initialDelayMs: Int, maxDelayMs: Int, backoffMultiplier: Double) {{
        self.maxRetries = maxRetries
        self.initialDelayMs = initialDelayMs
        self.maxDelayMs = maxDelayMs
        self.backoffMultiplier = backoffMultiplier
    }}
}}

/// Calculate retry delay with exponential backoff
public func calculateRetryDelay(attempt: Int, config: RetryConfig = .default) -> Int {{
    let delay = Double(config.initialDelayMs) * pow(config.backoffMultiplier, Double(attempt))
    return min(Int(delay), config.maxDelayMs)
}}

/// Motto transport protocol
public protocol MottoTransportProtocol: AnyObject, Sendable {{
    var state: ConnectionState {{ get }}
    func connect() async throws
    func disconnect() async
    func send(_ data: Data) async throws
    func receive() async throws -> Data
}}

#if canImport(Network)
import Network

/// WebTransport-like connection using Network.framework
@available(iOS 15.0, macOS 12.0, *)
public actor MottoTransport: MottoTransportProtocol {{
    private let url: URL
    private let retryConfig: RetryConfig
    private var connection: NWConnection?
    private var retryAttempt: Int = 0
    
    public private(set) var state: ConnectionState = .disconnected
    
    public init(url: URL, retryConfig: RetryConfig = .default) {{
        self.url = url
        self.retryConfig = retryConfig
    }}
    
    public func connect() async throws {{
        state = .connecting
        
        // Note: This is a simplified TCP connection
        // Real WebTransport requires HTTP/3 + QUIC support
        let endpoint = NWEndpoint.url(url)!
        let parameters = NWParameters.tcp
        
        connection = NWConnection(to: endpoint, using: parameters)
        
        return try await withCheckedThrowingContinuation {{ continuation in
            connection?.stateUpdateHandler = {{ [weak self] newState in
                Task {{
                    switch newState {{
                    case .ready:
                        await self?.setState(.connected)
                        continuation.resume()
                    case .failed(let error):
                        await self?.setState(.error(error))
                        continuation.resume(throwing: error)
                    default:
                        break
                    }}
                }}
            }}
            connection?.start(queue: .global())
        }}
    }}
    
    private func setState(_ newState: ConnectionState) {{
        state = newState
    }}
    
    public func disconnect() async {{
        connection?.cancel()
        connection = nil
        state = .disconnected
    }}
    
    public func send(_ data: Data) async throws {{
        guard let connection = connection else {{
            throw NSError(domain: "MottoTransport", code: -1, userInfo: [NSLocalizedDescriptionKey: "Not connected"])
        }}
        
        return try await withCheckedThrowingContinuation {{ continuation in
            connection.send(content: data, completion: .contentProcessed {{ error in
                if let error = error {{
                    continuation.resume(throwing: error)
                }} else {{
                    continuation.resume()
                }}
            }})
        }}
    }}
    
    public func receive() async throws -> Data {{
        guard let connection = connection else {{
            throw NSError(domain: "MottoTransport", code: -1, userInfo: [NSLocalizedDescriptionKey: "Not connected"])
        }}
        
        return try await withCheckedThrowingContinuation {{ continuation in
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65535) {{ data, _, _, error in
                if let error = error {{
                    continuation.resume(throwing: error)
                }} else if let data = data {{
                    continuation.resume(returning: data)
                }} else {{
                    continuation.resume(throwing: NSError(domain: "MottoTransport", code: -2, userInfo: [NSLocalizedDescriptionKey: "No data received"]))
                }}
            }}
        }}
    }}
}}
#endif
{ffi_transport}
"#,
        swift_header(manifest),
        ffi_transport = ffi_transport
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Sources/MottoSDK/Runtime.swift"),
        content,
    })
}

fn generate_package_swift(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"// swift-tools-version:5.9
// MOTTO GENERATED - Protocol Version: 0x{:02X}

import PackageDescription

let package = Package(
    name: "MottoSDK",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
        .watchOS(.v8),
        .tvOS(.v15)
    ],
    products: [
        .library(
            name: "MottoSDK",
            targets: ["MottoSDK"]
        ),
    ],
    dependencies: [],
    targets: [
        .target(
            name: "MottoSDK",
            dependencies: [],
            swiftSettings: [
                .enableExperimentalFeature("StrictConcurrency")
            ]
        ),
        .testTarget(
            name: "MottoSDKTests",
            dependencies: ["MottoSDK"]
        ),
    ]
)
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Package.swift"),
        content,
    })
}

fn generate_tests(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"import XCTest
@testable import MottoSDK

final class MottoSDKTests: XCTestCase {{
    func testProtocolVersionByte() throws {{
        XCTAssertEqual(PROTOCOL_VERSION_BYTE, UInt8(0x{:02X}))
    }}

    func testPacketBuilderWritesHeader() throws {{
        var builder = PacketBuilder()
        let bytes = builder.build()
        XCTAssertEqual(bytes.first, PROTOCOL_VERSION_BYTE)
    }}
}}
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Tests/MottoSDKTests/MottoSDKTests.swift"),
        content,
    })
}

fn rust_to_swift_type(rust_type: &str) -> String {
    if let Some(inner_start) = rust_type.find('<') {
        let name = &rust_type[..inner_start];
        let inner = &rust_type[inner_start + 1..rust_type.len() - 1];

        match name {
            "Vec" => format!("[{}]", rust_to_swift_type(inner)),
            "Option" => format!("{}?", rust_to_swift_type(inner)),
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "[{}: {}]",
                        rust_to_swift_type(parts[0]),
                        rust_to_swift_type(parts[1])
                    )
                } else {
                    "[AnyHashable: Any]".to_string()
                }
            }
            "HashSet" | "BTreeSet" => format!("Set<{}>", rust_to_swift_type(inner)),
            _ => rust_type.to_string(),
        }
    } else {
        match rust_type {
            "u8" => "UInt8".to_string(),
            "u16" => "UInt16".to_string(),
            "u32" => "UInt32".to_string(),
            "u64" => "UInt64".to_string(),
            "i8" => "Int8".to_string(),
            "i16" => "Int16".to_string(),
            "i32" => "Int32".to_string(),
            "i64" => "Int64".to_string(),
            "f32" => "Float".to_string(),
            "f64" => "Double".to_string(),
            "bool" => "Bool".to_string(),
            "String" | "str" => "String".to_string(),
            "char" => "Character".to_string(),
            "()" => "Void".to_string(),
            _ => rust_type.to_string(),
        }
    }
}

fn escape_swift(name: &str) -> String {
    if SWIFT_RESERVED.contains(&name) {
        format!("`{}`", name)
    } else {
        utils::to_camel_case(name)
    }
}
