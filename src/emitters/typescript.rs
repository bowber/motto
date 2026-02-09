//! TypeScript/JavaScript Backend Emitter
//!
//! Generates ESM-compliant TypeScript with:
//! - Conditional exports for WASM or Native Addon (napi-rs)
//! - Zero-copy interface for packet framing
//! - 1-byte version header support

use crate::emitters::{utils, Emitter, EmitterConfig, GeneratedFile, TransportMode};
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
        let mut files = vec![
            // Generate main types file
            generate_types(&config.manifest)?,
            // Generate codec file
            generate_codec(&config.manifest)?,
            // Generate runtime file
            generate_runtime(&config.manifest, config.transport_mode)?,
            // Generate index file with exports
            generate_index(&config.manifest, config)?,
        ];

        // Generate WASM bindings if requested
        if config.wasm_bindings {
            files.push(generate_wasm_bindings(&config.manifest)?);
        }

        // Generate package.json
        files.push(generate_package_json(&config.manifest)?);
        // Generate tests
        files.push(generate_tests(&config.manifest)?);

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

    // Generate type aliases first (they're often used by other types)
    for alias in &manifest.type_aliases {
        content.push_str(&generate_type_alias(alias));
        content.push('\n');
    }

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

    // Generate implicit router type
    if let Some(router) = &manifest.router {
        content.push_str(&generate_router_type(router));
        content.push('\n');
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/types.ts"),
        content,
    })
}

/// Generate a type alias declaration
fn generate_type_alias(alias: &TypeAliasManifest) -> String {
    let mut s = String::new();

    if let Some(docs) = &alias.docs {
        s.push_str(&format!("/** {} */\n", docs));
    }

    let ts_type = rust_to_ts_type(&alias.target);
    s.push_str(&format!("export type {} = {};\n", alias.name, ts_type));

    s
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

/// Generate the implicit router type
fn generate_router_type(router: &RouterManifest) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &router.docs {
        for line in docs.lines() {
            s.push_str(&format!(" * {}\n", line));
        }
        s.insert_str(0, "/**\n");
        s.push_str(" */\n");
    }

    // Generate discriminated union type
    s.push_str(&format!("export type {} =\n", router.name));

    for (i, variant) in router.variants.iter().enumerate() {
        s.push_str(&format!(
            "  | {{ type: '{}'; data: {} }}",
            variant.name, variant.message_type
        ));
        if i < router.variants.len() - 1 {
            s.push('\n');
        }
    }
    s.push_str(";\n\n");

    // Generate discriminant constants for binary routing
    s.push_str(&format!(
        "/** Message type discriminants for {} */\n",
        router.name
    ));
    s.push_str(&format!("export const {}Type = {{\n", router.name));
    for variant in &router.variants {
        s.push_str(&format!(
            "  {}: {} as const,\n",
            variant.name, variant.discriminant
        ));
    }
    s.push_str("} as const;\n");

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

    // Generate encode/decode functions for type aliases
    for alias in &manifest.type_aliases {
        content.push_str(&generate_type_alias_codec(alias));
    }

    // Generate encode/decode functions for each enum
    for e in &manifest.enums {
        content.push_str(&generate_enum_codec(e));
    }

    // Generate encode/decode functions for each message
    for msg in &manifest.messages {
        content.push_str(&generate_message_codec(msg));
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/codec.ts"),
        content,
    })
}

/// Generate codec functions for type aliases
fn generate_type_alias_codec(alias: &TypeAliasManifest) -> String {
    let mut s = String::new();
    let name = &alias.name;
    let target = &alias.target;

    // Helper encode - just delegate to the underlying type
    s.push_str(&format!(
        "\n/** Encode {} (alias for {}) */\nfunction encode{}Fields(val: Types.{}, builder: PacketBuilder): void {{\n",
        name, target, name, name
    ));
    s.push_str(&format!("  {};\n", encode_field("val", target)));
    s.push_str("}\n");

    // Helper decode
    s.push_str(&format!(
        "\n/** Decode {} (alias for {}) */\nfunction decode{}Fields(view: PacketView): Types.{} {{\n",
        name, target, name, name
    ));
    s.push_str(&format!(
        "  return {} as Types.{};\n",
        decode_field(target),
        name
    ));
    s.push_str("}\n");

    s
}

/// Generate codec functions for enums
fn generate_enum_codec(e: &EnumManifest) -> String {
    let mut s = String::new();
    let name = &e.name;

    if e.is_simple {
        // Simple C-style enum: just encode/decode as the repr type
        let write_fn = match e.repr.as_str() {
            "u8" | "i8" => "writeU8",
            "u16" | "i16" => "writeU16",
            "u32" | "i32" => "writeU32",
            _ => "writeU8",
        };
        let read_fn = match e.repr.as_str() {
            "u8" | "i8" => "readU8",
            "u16" | "i16" => "readU16",
            "u32" | "i32" => "readU32",
            _ => "readU8",
        };

        // Helper encode
        s.push_str(&format!(
            "\n/** Encode {} enum (for nested types) */\nfunction encode{}Fields(val: Types.{}, builder: PacketBuilder): void {{\n",
            name, name, name
        ));
        s.push_str(&format!("  builder.{}(val);\n", write_fn));
        s.push_str("}\n");

        // Helper decode
        s.push_str(&format!(
            "\n/** Decode {} enum (for nested types) */\nfunction decode{}Fields(view: PacketView): Types.{} {{\n",
            name, name, name
        ));
        s.push_str(&format!("  return view.{}() as Types.{};\n", read_fn, name));
        s.push_str("}\n");
    } else {
        // Tagged union: encode discriminant + variant data
        s.push_str(&format!(
            "\n/** Encode {} union (for nested types) */\nfunction encode{}Fields(val: Types.{}, builder: PacketBuilder): void {{\n",
            name, name, name
        ));
        s.push_str("  switch (val.type) {\n");

        for v in &e.variants {
            s.push_str(&format!("    case '{}':\n", v.name));
            s.push_str(&format!("      builder.writeU8({});\n", v.discriminant));

            match &v.data {
                VariantData::Unit => {}
                VariantData::Tuple { types } => {
                    for (i, t) in types.iter().enumerate() {
                        s.push_str(&format!(
                            "      {};\n",
                            encode_field(&format!("val._{}", i), t)
                        ));
                    }
                }
                VariantData::Struct { fields } => {
                    for f in fields {
                        let field_name = utils::escape_reserved(&f.name, TS_RESERVED);
                        s.push_str(&format!(
                            "      {};\n",
                            encode_field(&format!("val.{}", field_name), &f.type_ref)
                        ));
                    }
                }
            }
            s.push_str("      break;\n");
        }

        s.push_str("  }\n}\n");

        // Helper decode
        s.push_str(&format!(
            "\n/** Decode {} union (for nested types) */\nfunction decode{}Fields(view: PacketView): Types.{} {{\n",
            name, name, name
        ));
        s.push_str("  const tag = view.readU8();\n");
        s.push_str("  switch (tag) {\n");

        for v in &e.variants {
            s.push_str(&format!("    case {}:\n", v.discriminant));

            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!(
                        "      return {{ type: '{}' }} as Types.{};\n",
                        v.name, name
                    ));
                }
                VariantData::Tuple { types } => {
                    let fields: Vec<String> = types
                        .iter()
                        .enumerate()
                        .map(|(i, t)| format!("_{}: {}", i, decode_field(t)))
                        .collect();
                    s.push_str(&format!(
                        "      return {{ type: '{}', {} }} as Types.{};\n",
                        v.name,
                        fields.join(", "),
                        name
                    ));
                }
                VariantData::Struct { fields } => {
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let field_name = utils::escape_reserved(&f.name, TS_RESERVED);
                            format!("{}: {}", field_name, decode_field(&f.type_ref))
                        })
                        .collect();
                    s.push_str(&format!(
                        "      return {{ type: '{}', {} }} as Types.{};\n",
                        v.name,
                        field_strs.join(", "),
                        name
                    ));
                }
            }
        }

        s.push_str(&format!(
            "    default:\n      throw new Error(`Unknown {} tag: ${{tag}}`);\n",
            name
        ));
        s.push_str("  }\n}\n");
    }

    s
}

fn generate_message_codec(msg: &MessageDef) -> String {
    let mut s = String::new();

    let name = &msg.name;
    let _camel_name = utils::to_camel_case(name);

    // Helper encode function (takes builder as parameter, for nested encoding)
    s.push_str(&format!(
        "\n/** Encode {} fields to a PacketBuilder (for nested types) */\nfunction encode{}Fields(msg: Types.{}, builder: PacketBuilder): void {{\n",
        name, name, name
    ));

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
    s.push_str("}\n");

    // Main encode function (creates PacketBuilder, returns Uint8Array)
    s.push_str(&format!(
        "\n/** Encode {} to binary */\nexport function encode{}(msg: Types.{}): Uint8Array {{\n",
        name, name, name
    ));
    s.push_str("  const builder = new PacketBuilder();\n");
    s.push_str(&format!("  encode{}Fields(msg, builder);\n", name));
    s.push_str("  return builder.build();\n}\n");

    // Helper decode function (takes view as parameter, for nested decoding)
    s.push_str(&format!(
        "\n/** Decode {} fields from a PacketView (for nested types) */\nfunction decode{}Fields(view: PacketView): Types.{} {{\n",
        name, name, name
    ));
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

    // Main decode function (creates PacketView, skips version byte)
    s.push_str(&format!(
        "\n/** Decode {} from binary */\nexport function decode{}(data: Uint8Array): Types.{} {{\n",
        name, name, name
    ));
    s.push_str("  const view = new PacketView(data);\n");
    s.push_str("  // Skip version byte\n");
    s.push_str("  view.skip(1);\n");
    s.push_str(&format!("  return decode{}Fields(view);\n", name));
    s.push_str("}\n");

    s
}

fn encode_field(accessor: &str, type_ref: &str) -> String {
    // Handle generic types
    if let Some((name, inner)) = parse_generic_type(type_ref) {
        match name {
            "Vec" => {
                // Array: write length, then each element
                return format!(
                    "{{ builder.writeU32({accessor}.length); for (const item of {accessor}) {{ {encode}; }} }}",
                    accessor = accessor,
                    encode = encode_field("item", inner)
                );
            }
            "Option" => {
                // Optional: presence byte + value (should be handled at call site)
                return encode_field(accessor, inner);
            }
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    return format!(
                        "{{ builder.writeU32({accessor}.size); for (const [k, v] of {accessor}) {{ {key_encode}; {val_encode}; }} }}",
                        accessor = accessor,
                        key_encode = encode_field("k", parts[0]),
                        val_encode = encode_field("v", parts[1])
                    );
                }
            }
            _ => {}
        }
    }

    // Handle primitives
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
        _ => {
            // Custom type - call the encode fields helper function
            format!("encode{}Fields({}, builder)", type_ref, accessor)
        }
    }
}

fn decode_field(type_ref: &str) -> String {
    // Handle generic types
    if let Some((name, inner)) = parse_generic_type(type_ref) {
        match name {
            "Vec" => {
                // Array: read length, then each element
                return format!(
                    "(() => {{ const len = view.readU32(); const arr: {}[] = []; for (let i = 0; i < len; i++) {{ arr.push({}); }} return arr; }})()",
                    rust_to_ts_type(inner),
                    decode_field(inner)
                );
            }
            "Option" => {
                // Optional: presence already read at call site
                return decode_field(inner);
            }
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    return format!(
                        "(() => {{ const len = view.readU32(); const map = new Map<{}, {}>(); for (let i = 0; i < len; i++) {{ map.set({}, {}); }} return map; }})()",
                        rust_to_ts_type(parts[0]),
                        rust_to_ts_type(parts[1]),
                        decode_field(parts[0]),
                        decode_field(parts[1])
                    );
                }
            }
            _ => {}
        }
    }

    // Handle primitives
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
        _ => {
            // Custom type - call the decode fields helper function
            format!("decode{}Fields(view)", type_ref)
        }
    }
}

/// Parse a generic type like "Vec<T>" into ("Vec", "T")
fn parse_generic_type(type_ref: &str) -> Option<(&str, &str)> {
    if let Some(inner_start) = type_ref.find('<') {
        let name = &type_ref[..inner_start];
        let inner = &type_ref[inner_start + 1..type_ref.len() - 1];
        Some((name, inner))
    } else {
        None
    }
}

fn generate_runtime(
    manifest: &SchemaManifest,
    transport_mode: TransportMode,
) -> Result<GeneratedFile> {
    let transport_class = match transport_mode {
        TransportMode::Ffi => {
            r#"/**
 * FFI-backed transport using the motto transport core.
 * Requires the motto native library (libmotto_transport) to be available.
 */
export class MottoTransport {{
  private ffi: any;
  private handle: any;
  private _state: ConnectionState = ConnectionState.Disconnected;
  private url: string;
  private retryConfig: RetryConfig;

  constructor(url: string, retryConfig: RetryConfig = DEFAULT_RETRY_CONFIG) {{
    this.url = url;
    this.retryConfig = retryConfig;
    // Dynamic import of ffi-napi for Node.js environments
    try {{
      const ffi = require('ffi-napi');
      const ref = require('ref-napi');
      this.ffi = ffi.Library('libmotto_transport', {{
        'motto_transport_new': ['pointer', ['string']],
        'motto_transport_free': ['void', ['pointer']],
        'motto_transport_connect': ['int', ['pointer']],
        'motto_transport_close': ['void', ['pointer']],
        'motto_transport_send': ['int', ['pointer', 'pointer', 'size_t']],
        'motto_transport_recv': ['int', ['pointer', 'pointer', 'pointer']],
        'motto_transport_recv_free': ['void', ['pointer', 'size_t']],
        'motto_transport_state': ['uint8', ['pointer']],
        'motto_transport_last_error': ['string', ['pointer']],
      }});
    }} catch (e) {{
      throw new Error('FFI transport requires ffi-napi and ref-napi packages. Install with: npm install ffi-napi ref-napi');
    }}
  }}

  async connect(): Promise<void> {{
    if (!this.handle) {{
      this.handle = this.ffi.motto_transport_new(this.url);
      if (this.handle.isNull()) {{
        throw new Error('Failed to create transport handle');
      }}
    }}
    this._state = ConnectionState.Connecting;
    const rc = this.ffi.motto_transport_connect(this.handle);
    if (rc !== 0) {{
      this._state = ConnectionState.Error;
      throw new Error(this.ffi.motto_transport_last_error(this.handle) || 'Connection failed');
    }}
    this._state = ConnectionState.Connected;
  }}

  async sendDatagram(data: Uint8Array): Promise<void> {{
    if (!this.handle) throw new Error('Not connected');
    const buf = Buffer.from(data);
    const rc = this.ffi.motto_transport_send(this.handle, buf, buf.length);
    if (rc !== 0) {{
      throw new Error(this.ffi.motto_transport_last_error(this.handle) || 'Send failed');
    }}
  }}

  async *receiveDatagram(): AsyncGenerator<Uint8Array> {{
    // Note: FFI recv is blocking; in production, run in a worker thread
    while (this._state === ConnectionState.Connected) {{
      // Placeholder: actual implementation would use worker_threads
      yield new Uint8Array(0);
      break;
    }}
  }}

  get state(): ConnectionState {{
    if (this.handle) {{
      const s = this.ffi.motto_transport_state(this.handle);
      return s as ConnectionState;
    }}
    return this._state;
  }}

  async close(): Promise<void> {{
    if (this.handle) {{
      this.ffi.motto_transport_close(this.handle);
      this.ffi.motto_transport_free(this.handle);
      this.handle = null;
    }}
    this._state = ConnectionState.Disconnected;
  }}
}}"#
        }
        TransportMode::Runtime => {
            r#"/** WebTransport connection wrapper */
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
}}"#
        }
    };

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

{}
"#,
        utils::generate_header(
            manifest.meta.version_byte,
            &manifest.meta.fingerprint,
            &manifest.meta.generated_at
        ),
        manifest.meta.version_byte,
        transport_class
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
    "dev": "tsup src/index.ts --format esm,cjs --dts --watch",
    "test": "vitest run"
  }},
  "files": [
    "dist",
    "src"
  ],
  "devDependencies": {{
    "tsup": "^8.0.0",
    "typescript": "^5.0.0",
    "vitest": "^2.1.9"
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

fn generate_tests(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let content = format!(
        r#"import {{ describe, expect, it }} from "vitest";
import {{ PacketBuilder, PacketView, PROTOCOL_VERSION_BYTE }} from "../src/codec";

describe("codec smoke tests", () => {{
  it("writes and reads the version header", () => {{
    const builder = new PacketBuilder();
    const data = builder.build();
    const view = new PacketView(data);

    expect(view.getVersionByte()).toBe(PROTOCOL_VERSION_BYTE);
    expect(view.validateVersion()).toBe(true);
  }});

  it("pins generated protocol version", () => {{
    expect(PROTOCOL_VERSION_BYTE).toBe({});
  }});
}});
"#,
        manifest.meta.version_byte
    );

    Ok(GeneratedFile {
        path: PathBuf::from("tests/codec.test.ts"),
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
                if !parts.is_empty() {
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
