//! TypeScript/JavaScript Backend Emitter
//!
//! Generates ESM-compliant TypeScript with:
//! - Conditional exports for WASM or Native Addon (napi-rs)
//! - Zero-copy interface for packet framing
//! - 1-byte version header support

use crate::emitters::{utils, Emitter, EmitterConfig, GeneratedFile};
use crate::ir::manifest::*;
use anyhow::Result;
use std::path::PathBuf;

/// TypeScript reserved words
const TS_RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "as",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "any",
    "boolean",
    "number",
    "string",
    "symbol",
    "type",
    "from",
    "of",
    "namespace",
    "module",
    "declare",
    "abstract",
    "readonly",
    "async",
    "await",
];

pub struct TypeScriptEmitter;

impl Emitter for TypeScriptEmitter {
    fn platform(&self) -> &'static str {
        "typescript"
    }

    fn extension(&self) -> &'static str {
        "ts"
    }

    fn emit(&self, config: &EmitterConfig) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();

        // Generate main types file
        files.push(generate_types(&config.manifest)?);

        // Generate codec file
        files.push(generate_codec(&config.manifest)?);

        // Generate runtime file
        files.push(generate_runtime(&config.manifest)?);

        // Generate index file with exports
        files.push(generate_index(&config.manifest, config)?);

        // Generate WASM bindings if requested
        if config.wasm_bindings {
            files.push(generate_wasm_bindings(&config.manifest)?);
        }

        // Generate package.json
        files.push(generate_package_json(&config.manifest)?);

        Ok(files)
    }
}

/// Emit TypeScript SDK (convenience function)
pub fn emit(config: &EmitterConfig) -> Result<()> {
    let emitter = TypeScriptEmitter;
    let files = emitter.emit(config)?;

    let ts_dir = config.output_dir.join("typescript");
    std::fs::create_dir_all(&ts_dir)?;

    for file in files {
        let path = ts_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }

    Ok(())
}

fn generate_types(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    // Header
    content.push_str(&utils::generate_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str("\n// Type Definitions\n\n");

    // Generate enums
    for e in &manifest.enums {
        content.push_str(&generate_enum_type(e));
        content.push('\n');
    }

    // Generate interfaces for messages
    for msg in &manifest.messages {
        content.push_str(&generate_message_interface(msg));
        content.push('\n');
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/types.ts"),
        content,
    })
}

fn generate_enum_type(e: &EnumManifest) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &e.docs {
        s.push_str(&format!("/** {} */\n", docs));
    }

    if e.is_simple {
        // C-style enum -> TypeScript enum
        s.push_str(&format!("export enum {} {{\n", e.name));
        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("  /** {} */\n", docs));
            }
            s.push_str(&format!("  {} = {},\n", v.name, v.discriminant));
        }
        s.push_str("}\n");
    } else {
        // Tagged union -> discriminated union type
        s.push_str(&format!("export type {} =\n", e.name));
        for (i, v) in e.variants.iter().enumerate() {
            let variant_type = match &v.data {
                VariantData::Unit => {
                    format!("  | {{ type: '{}' }}", v.name)
                }
                VariantData::Tuple { types } => {
                    let fields: String = types
                        .iter()
                        .enumerate()
                        .map(|(i, t)| format!("_{}: {}", i, rust_to_ts_type(t)))
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("  | {{ type: '{}'; {} }}", v.name, fields)
                }
                VariantData::Struct { fields } => {
                    let field_strs: String = fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                utils::escape_reserved(&f.name, TS_RESERVED),
                                rust_to_ts_type(&f.type_ref)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("  | {{ type: '{}'; {} }}", v.name, field_strs)
                }
            };
            s.push_str(&variant_type);
            if i < e.variants.len() - 1 {
                s.push('\n');
            }
        }
        s.push_str(";\n");
    }

    s
}

fn generate_message_interface(msg: &MessageDef) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &msg.docs {
        s.push_str(&format!("/** {} */\n", docs));
    }

    // Generate interface
    let generics = if msg.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", msg.generics.join(", "))
    };

    s.push_str(&format!("export interface {}{} {{\n", msg.name, generics));

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, TS_RESERVED);
        let optional = if field.optional { "?" } else { "" };
        let ts_type = rust_to_ts_type(&field.type_ref);

        if let Some(docs) = &field.docs {
            s.push_str(&format!("  /** {} */\n", docs));
        }
        s.push_str(&format!("  {}{}: {};\n", field_name, optional, ts_type));
    }

    s.push_str("}\n");
    s
}

fn generate_codec(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    content.push_str(&utils::generate_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(
        r#"
import type * as Types from './types';

// Protocol version byte - embedded in all packets
export const PROTOCOL_VERSION_BYTE = "#,
    );
    content.push_str(&format!("0x{:02X};\n", manifest.meta.version_byte));

    content.push_str(
        r#"
// Schema fingerprint for validation
export const SCHEMA_FINGERPRINT = '"#,
    );
    content.push_str(&manifest.meta.fingerprint);
    content.push_str("';\n\n");

    // Buffer utilities
    content.push_str(
        r#"
/** Zero-copy buffer view for packet framing */
export class PacketView {
  private view: DataView;
  private offset: number = 0;

  constructor(buffer: ArrayBuffer | Uint8Array) {
    if (buffer instanceof Uint8Array) {
      this.view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    } else {
      this.view = new DataView(buffer);
    }
  }

  /** Get protocol version byte from packet header */
  getVersionByte(): number {
    return this.view.getUint8(0);
  }

  /** Validate version matches expected */
  validateVersion(): boolean {
    return this.getVersionByte() === PROTOCOL_VERSION_BYTE;
  }

  /** Read u8 at current offset */
  readU8(): number {
    const val = this.view.getUint8(this.offset);
    this.offset += 1;
    return val;
  }

  /** Read u16 (little-endian) */
  readU16(): number {
    const val = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return val;
  }

  /** Read u32 (little-endian) */
  readU32(): number {
    const val = this.view.getUint32(this.offset, true);
    this.offset += 4;
    return val;
  }

  /** Read u64 as BigInt (little-endian) */
  readU64(): bigint {
    const val = this.view.getBigUint64(this.offset, true);
    this.offset += 8;
    return val;
  }

  /** Read f32 (little-endian) */
  readF32(): number {
    const val = this.view.getFloat32(this.offset, true);
    this.offset += 4;
    return val;
  }

  /** Read f64 (little-endian) */
  readF64(): number {
    const val = this.view.getFloat64(this.offset, true);
    this.offset += 8;
    return val;
  }

  /** Read length-prefixed string (u32 length + UTF-8 bytes) */
  readString(): string {
    const len = this.readU32();
    const bytes = new Uint8Array(this.view.buffer, this.view.byteOffset + this.offset, len);
    this.offset += len;
    return new TextDecoder().decode(bytes);
  }

  /** Read boolean */
  readBool(): boolean {
    return this.readU8() !== 0;
  }

  /** Skip bytes */
  skip(n: number): void {
    this.offset += n;
  }

  /** Get current offset */
  getOffset(): number {
    return this.offset;
  }

  /** Set offset */
  setOffset(offset: number): void {
    this.offset = offset;
  }

  /** Get remaining bytes */
  remaining(): number {
    return this.view.byteLength - this.offset;
  }
}

/** Packet builder for zero-copy encoding */
export class PacketBuilder {
  private buffer: Uint8Array;
  private view: DataView;
  private offset: number = 0;

  constructor(initialSize: number = 256) {
    this.buffer = new Uint8Array(initialSize);
    this.view = new DataView(this.buffer.buffer);
    // Write version byte header
    this.writeU8(PROTOCOL_VERSION_BYTE);
  }

  private ensureCapacity(need: number): void {
    if (this.offset + need > this.buffer.length) {
      const newSize = Math.max(this.buffer.length * 2, this.offset + need);
      const newBuffer = new Uint8Array(newSize);
      newBuffer.set(this.buffer);
      this.buffer = newBuffer;
      this.view = new DataView(this.buffer.buffer);
    }
  }

  writeU8(val: number): void {
    this.ensureCapacity(1);
    this.view.setUint8(this.offset, val);
    this.offset += 1;
  }

  writeU16(val: number): void {
    this.ensureCapacity(2);
    this.view.setUint16(this.offset, val, true);
    this.offset += 2;
  }

  writeU32(val: number): void {
    this.ensureCapacity(4);
    this.view.setUint32(this.offset, val, true);
    this.offset += 4;
  }

  writeU64(val: bigint): void {
    this.ensureCapacity(8);
    this.view.setBigUint64(this.offset, val, true);
    this.offset += 8;
  }

  writeF32(val: number): void {
    this.ensureCapacity(4);
    this.view.setFloat32(this.offset, val, true);
    this.offset += 4;
  }

  writeF64(val: number): void {
    this.ensureCapacity(8);
    this.view.setFloat64(this.offset, val, true);
    this.offset += 8;
  }

  writeString(val: string): void {
    const bytes = new TextEncoder().encode(val);
    this.writeU32(bytes.length);
    this.ensureCapacity(bytes.length);
    this.buffer.set(bytes, this.offset);
    this.offset += bytes.length;
  }

  writeBool(val: boolean): void {
    this.writeU8(val ? 1 : 0);
  }

  /** Get the built packet as a Uint8Array (trimmed to actual size) */
  build(): Uint8Array {
    return this.buffer.slice(0, this.offset);
  }
}
"#,
    );

    // Generate encode/decode functions for each message
    for msg in &manifest.messages {
        content.push_str(&generate_message_codec(msg));
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/codec.ts"),
        content,
    })
}

fn generate_message_codec(msg: &MessageDef) -> String {
    let mut s = String::new();

    let name = &msg.name;
    let _camel_name = utils::to_camel_case(name);

    // Encode function
    s.push_str(&format!(
        "\n/** Encode {} to binary */\nexport function encode{}(msg: Types.{}): Uint8Array {{\n",
        name, name, name
    ));
    s.push_str("  const builder = new PacketBuilder();\n");

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, TS_RESERVED);
        let accessor = format!("msg.{}", field_name);

        if field.optional {
            s.push_str(&format!("  if ({} !== undefined) {{\n", accessor));
            s.push_str("    builder.writeU8(1);\n");
            s.push_str(&format!(
                "    {};\n",
                encode_field(&accessor, &field.type_ref)
            ));
            s.push_str("  } else {\n");
            s.push_str("    builder.writeU8(0);\n");
            s.push_str("  }\n");
        } else {
            s.push_str(&format!(
                "  {};\n",
                encode_field(&accessor, &field.type_ref)
            ));
        }
    }

    s.push_str("  return builder.build();\n}\n");

    // Decode function
    s.push_str(&format!(
        "\n/** Decode {} from binary */\nexport function decode{}(data: Uint8Array): Types.{} {{\n",
        name, name, name
    ));
    s.push_str("  const view = new PacketView(data);\n");
    s.push_str("  // Skip version byte\n");
    s.push_str("  view.skip(1);\n\n");
    s.push_str("  return {\n");

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, TS_RESERVED);

        if field.optional {
            s.push_str(&format!(
                "    {}: view.readU8() ? {} : undefined,\n",
                field_name,
                decode_field(&field.type_ref)
            ));
        } else {
            s.push_str(&format!(
                "    {}: {},\n",
                field_name,
                decode_field(&field.type_ref)
            ));
        }
    }

    s.push_str("  };\n}\n");

    s
}

fn encode_field(accessor: &str, type_ref: &str) -> String {
    match type_ref {
        "u8" => format!("builder.writeU8({})", accessor),
        "u16" => format!("builder.writeU16({})", accessor),
        "u32" => format!("builder.writeU32({})", accessor),
        "u64" => format!("builder.writeU64(BigInt({}))", accessor),
        "i8" => format!("builder.writeU8({} & 0xFF)", accessor),
        "i16" => format!("builder.writeU16({} & 0xFFFF)", accessor),
        "i32" => format!("builder.writeU32({} >>> 0)", accessor),
        "i64" => format!("builder.writeU64(BigInt({}))", accessor),
        "f32" => format!("builder.writeF32({})", accessor),
        "f64" => format!("builder.writeF64({})", accessor),
        "bool" => format!("builder.writeBool({})", accessor),
        "String" | "str" => format!("builder.writeString({})", accessor),
        _ => format!("/* TODO: encode {} */", type_ref),
    }
}

fn decode_field(type_ref: &str) -> String {
    match type_ref {
        "u8" => "view.readU8()".to_string(),
        "u16" => "view.readU16()".to_string(),
        "u32" => "view.readU32()".to_string(),
        "u64" => "view.readU64()".to_string(),
        "i8" => "(view.readU8() << 24) >> 24".to_string(),
        "i16" => "(view.readU16() << 16) >> 16".to_string(),
        "i32" => "view.readU32() | 0".to_string(),
        "i64" => "view.readU64()".to_string(),
        "f32" => "view.readF32()".to_string(),
        "f64" => "view.readF64()".to_string(),
        "bool" => "view.readBool()".to_string(),
        "String" | "str" => "view.readString()".to_string(),
        _ => format!("undefined /* TODO: decode {} */", type_ref),
    }
}

fn generate_runtime(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{}

// Motto Runtime - State Machine & Transport Layer

export const PROTOCOL_VERSION = 0x{:02X};

/** Connection state machine */
export enum ConnectionState {{
  Disconnected = 0,
  Connecting = 1,
  Connected = 2,
  Reconnecting = 3,
  Error = 4,
}}

/** Retry configuration */
export interface RetryConfig {{
  maxRetries: number;
  initialDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}}

export const DEFAULT_RETRY_CONFIG: RetryConfig = {{
  maxRetries: 5,
  initialDelayMs: 100,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
}};

/** Calculate retry delay with exponential backoff */
export function calculateRetryDelay(attempt: number, config: RetryConfig = DEFAULT_RETRY_CONFIG): number {{
  const delay = config.initialDelayMs * Math.pow(config.backoffMultiplier, attempt);
  return Math.min(delay, config.maxDelayMs);
}}

/** Decompress zstd data (requires external zstd library) */
export async function decompressZstd(data: Uint8Array): Promise<Uint8Array> {{
  // This is a placeholder - actual implementation depends on runtime
  // For browser: use zstd-wasm
  // For Node.js: use zstd-napi
  throw new Error('Zstd decompression not implemented. Import @aspect/zstd or similar.');
}}

/** Compress data with zstd (requires external zstd library) */
export async function compressZstd(data: Uint8Array, level: number = 3): Promise<Uint8Array> {{
  // This is a placeholder - actual implementation depends on runtime
  throw new Error('Zstd compression not implemented. Import @aspect/zstd or similar.');
}}

/** WebTransport connection wrapper */
export class MottoTransport {{
  private transport: WebTransport | null = null;
  private state: ConnectionState = ConnectionState.Disconnected;
  private retryAttempt: number = 0;
  private retryConfig: RetryConfig;

  constructor(
    private url: string,
    retryConfig: RetryConfig = DEFAULT_RETRY_CONFIG
  ) {{
    this.retryConfig = retryConfig;
  }}

  async connect(): Promise<void> {{
    if (this.state === ConnectionState.Connected) return;

    this.state = ConnectionState.Connecting;
    
    try {{
      this.transport = new WebTransport(this.url);
      await this.transport.ready;
      this.state = ConnectionState.Connected;
      this.retryAttempt = 0;
    }} catch (error) {{
      this.state = ConnectionState.Error;
      throw error;
    }}
  }}

  async reconnect(): Promise<void> {{
    if (this.retryAttempt >= this.retryConfig.maxRetries) {{
      throw new Error('Max retry attempts exceeded');
    }}

    this.state = ConnectionState.Reconnecting;
    const delay = calculateRetryDelay(this.retryAttempt, this.retryConfig);
    this.retryAttempt++;

    await new Promise(resolve => setTimeout(resolve, delay));
    await this.connect();
  }}

  async sendDatagram(data: Uint8Array): Promise<void> {{
    if (!this.transport || this.state !== ConnectionState.Connected) {{
      throw new Error('Not connected');
    }}

    const writer = this.transport.datagrams.writable.getWriter();
    await writer.write(data);
    writer.releaseLock();
  }}

  async *receiveDatagram(): AsyncGenerator<Uint8Array> {{
    if (!this.transport || this.state !== ConnectionState.Connected) {{
      throw new Error('Not connected');
    }}

    const reader = this.transport.datagrams.readable.getReader();
    try {{
      while (true) {{
        const {{ value, done }} = await reader.read();
        if (done) break;
        yield value;
      }}
    }} finally {{
      reader.releaseLock();
    }}
  }}

  getState(): ConnectionState {{
    return this.state;
  }}

  async close(): Promise<void> {{
    if (this.transport) {{
      this.transport.close();
      this.transport = null;
    }}
    this.state = ConnectionState.Disconnected;
  }}
}}
"#,
        utils::generate_header(
            manifest.meta.version_byte,
            &manifest.meta.fingerprint,
            &manifest.meta.generated_at
        ),
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/runtime.ts"),
        content,
    })
}

fn generate_index(manifest: &SchemaManifest, config: &EmitterConfig) -> Result<GeneratedFile> {
    let mut content = format!(
        r#"{}

// Motto SDK - Auto-generated TypeScript bindings

// Re-export all types
export * from './types';

// Re-export codec functions
export * from './codec';

// Re-export runtime
export * from './runtime';
"#,
        utils::generate_header(
            manifest.meta.version_byte,
            &manifest.meta.fingerprint,
            &manifest.meta.generated_at
        )
    );

    if config.wasm_bindings {
        content.push_str("\n// WASM bindings\nexport * from './wasm';\n");
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/index.ts"),
        content,
    })
}

fn generate_wasm_bindings(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{}

// WASM Bindings - Glue code for WebAssembly runtime

let wasmInstance: WebAssembly.Instance | null = null;
let wasmMemory: WebAssembly.Memory | null = null;

/** Initialize WASM module */
export async function initWasm(wasmUrl: string): Promise<void> {{
  const response = await fetch(wasmUrl);
  const bytes = await response.arrayBuffer();
  
  const {{ instance }} = await WebAssembly.instantiate(bytes, {{
    env: {{
      // Add any imports needed by the WASM module
    }},
  }});
  
  wasmInstance = instance;
  wasmMemory = instance.exports.memory as WebAssembly.Memory;
}}

/** Check if WASM is initialized */
export function isWasmReady(): boolean {{
  return wasmInstance !== null;
}}

/** Get WASM memory buffer */
export function getWasmMemory(): Uint8Array | null {{
  if (!wasmMemory) return null;
  return new Uint8Array(wasmMemory.buffer);
}}

/** Allocate memory in WASM heap */
export function allocate(size: number): number {{
  if (!wasmInstance) throw new Error('WASM not initialized');
  const alloc = wasmInstance.exports.alloc as (size: number) => number;
  return alloc(size);
}}

/** Free memory in WASM heap */
export function deallocate(ptr: number, size: number): void {{
  if (!wasmInstance) throw new Error('WASM not initialized');
  const dealloc = wasmInstance.exports.dealloc as (ptr: number, size: number) => void;
  dealloc(ptr, size);
}}

/** Copy data to WASM memory */
export function copyToWasm(data: Uint8Array): number {{
  const ptr = allocate(data.length);
  const memory = getWasmMemory();
  if (memory) {{
    memory.set(data, ptr);
  }}
  return ptr;
}}

/** Copy data from WASM memory */
export function copyFromWasm(ptr: number, length: number): Uint8Array {{
  const memory = getWasmMemory();
  if (!memory) throw new Error('WASM not initialized');
  return memory.slice(ptr, ptr + length);
}}
"#,
        utils::generate_header(
            manifest.meta.version_byte,
            &manifest.meta.fingerprint,
            &manifest.meta.generated_at
        )
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/wasm.ts"),
        content,
    })
}

fn generate_package_json(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"{{
  "name": "@motto/{name}",
  "version": "0.1.0",
  "description": "Generated Motto SDK for {name}",
  "type": "module",
  "main": "./dist/index.js",
  "module": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "exports": {{
    ".": {{
      "types": "./dist/index.d.ts",
      "import": "./dist/index.js",
      "require": "./dist/index.cjs"
    }}
  }},
  "scripts": {{
    "build": "tsup src/index.ts --format esm,cjs --dts",
    "dev": "tsup src/index.ts --format esm,cjs --dts --watch"
  }},
  "files": [
    "dist",
    "src"
  ],
  "devDependencies": {{
    "tsup": "^8.0.0",
    "typescript": "^5.0.0"
  }},
  "motto": {{
    "fingerprint": "{fingerprint}",
    "protocolVersion": {version_byte}
  }}
}}
"#,
        name = utils::to_camel_case(&manifest.meta.name),
        fingerprint = &manifest.meta.fingerprint[..16],
        version_byte = manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("package.json"),
        content,
    })
}

/// Convert Rust type to TypeScript type
fn rust_to_ts_type(rust_type: &str) -> String {
    // Parse generic types like Vec<T>, Option<T>, etc.
    if let Some(inner_start) = rust_type.find('<') {
        let name = &rust_type[..inner_start];
        let inner = &rust_type[inner_start + 1..rust_type.len() - 1];

        match name {
            "Vec" => format!("{}[]", rust_to_ts_type(inner)),
            "Option" => format!("{} | undefined", rust_to_ts_type(inner)),
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "Map<{}, {}>",
                        rust_to_ts_type(parts[0]),
                        rust_to_ts_type(parts[1])
                    )
                } else {
                    "Map<unknown, unknown>".to_string()
                }
            }
            "HashSet" | "BTreeSet" => format!("Set<{}>", rust_to_ts_type(inner)),
            "Result" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 1 {
                    rust_to_ts_type(parts[0])
                } else {
                    "unknown".to_string()
                }
            }
            _ => rust_type.to_string(),
        }
    } else {
        // Simple types
        match rust_type {
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" | "usize" | "isize" => {
                "number".to_string()
            }
            "u64" | "i64" | "u128" | "i128" => "bigint".to_string(),
            "bool" => "boolean".to_string(),
            "String" | "str" | "&str" => "string".to_string(),
            "char" => "string".to_string(),
            "()" => "void".to_string(),
            _ => rust_type.to_string(),
        }
    }
}
