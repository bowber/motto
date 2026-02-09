//! Rust Crate Backend Emitter
//!
//! Generates a Rust crate with:
//! - Type definitions matching the schema
//! - Router enum for match-based message routing
//! - Bitcode-compatible encode/decode implementations
//! - Zero-copy packet framing

use crate::emitters::{Emitter, EmitterConfig, GeneratedFile, TransportMode, utils};
use crate::ir::manifest::*;
use anyhow::Result;
use std::path::PathBuf;

/// Rust reserved words
const RUST_RESERVED: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

pub struct RustEmitter;

impl Emitter for RustEmitter {
    fn platform(&self) -> &'static str {
        "rust"
    }

    fn extension(&self) -> &'static str {
        "rs"
    }

    fn emit(&self, config: &EmitterConfig) -> Result<Vec<GeneratedFile>> {
        let files = vec![
            // Generate lib.rs with types and router
            generate_lib(&config.manifest, config.transport_mode)?,
            // Generate codec.rs with encode/decode
            generate_codec(&config.manifest)?,
            // Generate Cargo.toml
            generate_cargo_toml(&config.manifest, config.transport_mode)?,
            // Generate tests
            generate_tests(&config.manifest)?,
            // Generate transport modules (feature-gated)
            generate_transport_common(&config.manifest)?,
            generate_webtransport(&config.manifest, config.transport_mode)?,
            generate_websocket(&config.manifest, config.transport_mode)?,
        ];

        Ok(files)
    }
}

/// Emit Rust SDK (convenience function)
pub fn emit(config: &EmitterConfig) -> Result<()> {
    let emitter = RustEmitter;
    let files = emitter.emit(config)?;

    let rust_dir = config.output_dir.join("rust");
    std::fs::create_dir_all(&rust_dir)?;

    for file in files {
        let path = rust_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }

    Ok(())
}

fn generate_lib(manifest: &SchemaManifest, transport_mode: TransportMode) -> Result<GeneratedFile> {
    let mut content = String::new();

    // Header
    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    let ffi_module_decl = if transport_mode == TransportMode::Ffi {
        r#"
#[cfg(feature = "ffi-transport")]
pub mod ffi_transport;
"#
    } else {
        ""
    };

    content.push_str(&format!(
        r#"
#![allow(dead_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub mod codec;

#[cfg(test)]
mod tests;

// Transport module (common types for webtransport and websocket)
#[cfg(any(feature = "webtransport", feature = "websocket"))]
pub mod transport;

#[cfg(feature = "webtransport")]
pub mod webtransport;

#[cfg(feature = "websocket")]
pub mod websocket;
{ffi_module_decl}
// Re-export transport types when features are enabled
#[cfg(any(feature = "webtransport", feature = "websocket"))]
pub use transport::{{TransportConfig, TransportError, ConnectionState}};

#[cfg(feature = "webtransport")]
pub use webtransport::WebTransportClient;

#[cfg(feature = "websocket")]
pub use websocket::WebSocketClient;

/// Protocol version byte - embedded in all packets
pub const PROTOCOL_VERSION_BYTE: u8 = "#,
        ffi_module_decl = ffi_module_decl,
    ));
    content.push_str(&format!("0x{:02X};\n\n", manifest.meta.version_byte));

    content.push_str(&format!(
        "/// Schema fingerprint for validation\npub const SCHEMA_FINGERPRINT: &str = \"{}\";\n\n",
        manifest.meta.fingerprint
    ));

    // Generate type aliases
    for alias in &manifest.type_aliases {
        if let Some(docs) = &alias.docs {
            content.push_str(&format!("/// {}\n", docs));
        }
        content.push_str(&format!(
            "pub type {} = {};\n\n",
            alias.name,
            rust_type_to_rust(&alias.target)
        ));
    }

    // Generate enums
    for e in &manifest.enums {
        content.push_str(&generate_enum_def(e));
        content.push('\n');
    }

    // Generate structs
    for msg in &manifest.messages {
        content.push_str(&generate_struct_def(msg));
        content.push('\n');
    }

    // Generate router enum
    if let Some(router) = &manifest.router {
        content.push_str(&generate_router_enum(router, manifest));
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/lib.rs"),
        content,
    })
}

fn generate_enum_def(e: &EnumManifest) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &e.docs {
        s.push_str(&format!("/// {}\n", docs));
    }

    if e.is_simple {
        // C-style enum with repr - can be Copy since it's just discriminants
        s.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        s.push_str(&format!("#[repr({})]\n", e.repr));
        s.push_str(&format!("pub enum {} {{\n", e.name));

        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /// {}\n", docs));
            }
            s.push_str(&format!("    {} = {},\n", v.name, v.discriminant));
        }
        s.push_str("}\n");
    } else {
        // Tagged union enum - can't be Copy due to potential heap data
        s.push_str("#[derive(Debug, Clone, PartialEq)]\n");
        s.push_str(&format!("pub enum {} {{\n", e.name));

        for v in &e.variants {
            if let Some(docs) = &v.docs {
                s.push_str(&format!("    /// {}\n", docs));
            }

            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!("    {},\n", v.name));
                }
                VariantData::Tuple { types } => {
                    let type_list = types
                        .iter()
                        .map(|t| rust_type_to_rust(t))
                        .collect::<Vec<_>>()
                        .join(", ");
                    s.push_str(&format!("    {}({}),\n", v.name, type_list));
                }
                VariantData::Struct { fields } => {
                    s.push_str(&format!("    {} {{\n", v.name));
                    for f in fields {
                        let field_name = utils::escape_reserved(&f.name, RUST_RESERVED);
                        if let Some(docs) = &f.docs {
                            s.push_str(&format!("        /// {}\n", docs));
                        }
                        s.push_str(&format!(
                            "        {}: {},\n",
                            field_name,
                            rust_type_to_rust(&f.type_ref)
                        ));
                    }
                    s.push_str("    },\n");
                }
            }
        }
        s.push_str("}\n");
    }

    s
}

fn generate_struct_def(msg: &MessageDef) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &msg.docs {
        s.push_str(&format!("/// {}\n", docs));
    }

    // Derive macros
    s.push_str("#[derive(Debug, Clone, PartialEq)]\n");

    // Generic parameters
    let generics = if msg.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", msg.generics.join(", "))
    };

    s.push_str(&format!("pub struct {}{} {{\n", msg.name, generics));

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, RUST_RESERVED);

        if let Some(docs) = &field.docs {
            s.push_str(&format!("    /// {}\n", docs));
        }

        let field_type = rust_type_to_rust(&field.type_ref);
        s.push_str(&format!("    pub {}: {},\n", field_name, field_type));
    }

    s.push_str("}\n");
    s
}

fn generate_router_enum(router: &RouterManifest, manifest: &SchemaManifest) -> String {
    let mut s = String::new();

    // Add docs
    if let Some(docs) = &router.docs {
        for line in docs.lines() {
            s.push_str(&format!("/// {}\n", line));
        }
    }

    s.push_str("#[derive(Debug, Clone, PartialEq)]\n");
    s.push_str(&format!("pub enum {} {{\n", router.name));

    for variant in &router.variants {
        if let Some(docs) = &variant.docs {
            s.push_str(&format!("    /// {}\n", docs));
        }
        s.push_str(&format!(
            "    {}({}),\n",
            variant.name, variant.message_type
        ));
    }

    s.push_str("}\n\n");

    // Generate discriminant constants
    s.push_str(&format!("impl {} {{\n", router.name));
    for variant in &router.variants {
        s.push_str(&format!(
            "    pub const {}_TAG: u16 = {};\n",
            utils::to_snake_case(&variant.name).to_uppercase(),
            variant.discriminant
        ));
    }
    s.push('\n');

    // Generate tag() method
    s.push_str("    /// Get the discriminant tag for this message\n");
    s.push_str("    pub fn tag(&self) -> u16 {\n");
    s.push_str("        match self {\n");
    for variant in &router.variants {
        s.push_str(&format!(
            "            Self::{}(_) => Self::{}_TAG,\n",
            variant.name,
            utils::to_snake_case(&variant.name).to_uppercase()
        ));
    }
    s.push_str("        }\n");
    s.push_str("    }\n\n");

    // Generate from_tag() constructor
    s.push_str("    /// Get the message type name from a tag\n");
    s.push_str("    pub fn type_name_from_tag(tag: u16) -> Option<&'static str> {\n");
    s.push_str("        match tag {\n");
    for variant in &router.variants {
        s.push_str(&format!(
            "            Self::{}_TAG => Some(\"{}\"),\n",
            utils::to_snake_case(&variant.name).to_uppercase(),
            variant.name
        ));
    }
    s.push_str("            _ => None,\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n\n");

    // Generate convenient match helper with handler trait
    s.push_str(&format!(
        "/// Handler trait for routing {} messages\n",
        router.name
    ));
    s.push_str(&format!("pub trait {}Handler {{\n", router.name));
    s.push_str("    type Output;\n\n");

    for variant in &router.variants {
        // Find the message definition to get docs
        let msg = manifest
            .messages
            .iter()
            .find(|m| m.name == variant.message_type);
        if let Some(msg) = msg
            && let Some(docs) = &msg.docs
        {
            s.push_str(&format!("    /// Handle: {}\n", docs));
        }
        s.push_str(&format!(
            "    fn handle_{}(&mut self, msg: {}) -> Self::Output;\n",
            utils::to_snake_case(&variant.name),
            variant.message_type
        ));
    }
    s.push_str("}\n\n");

    // Generate route method
    s.push_str(&format!("impl {} {{\n", router.name));
    s.push_str("    /// Route this message to the appropriate handler\n");
    s.push_str(&format!(
        "    pub fn route<H: {}Handler>(self, handler: &mut H) -> H::Output {{\n",
        router.name
    ));
    s.push_str("        match self {\n");
    for variant in &router.variants {
        s.push_str(&format!(
            "            Self::{}(msg) => handler.handle_{}(msg),\n",
            variant.name,
            utils::to_snake_case(&variant.name)
        ));
    }
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n");

    s
}

fn generate_codec(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(
        r#"
//! Binary codec for encoding/decoding messages

use super::*;
use std::io::{Read, Write, Result as IoResult, Error as IoError, ErrorKind};

/// Trait for types that can be encoded to binary
pub trait Encode {
    fn encode<W: Write>(&self, writer: &mut W) -> IoResult<()>;
    
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION_BYTE);
        self.encode(&mut buf).expect("Vec write cannot fail");
        buf
    }
}

/// Trait for types that can be decoded from binary
pub trait Decode: Sized {
    fn decode<R: Read>(reader: &mut R) -> IoResult<Self>;
    
    fn from_bytes(bytes: &[u8]) -> IoResult<Self> {
        if bytes.is_empty() {
            return Err(IoError::new(ErrorKind::InvalidData, "Empty buffer"));
        }
        if bytes[0] != PROTOCOL_VERSION_BYTE {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                format!("Version mismatch: expected 0x{:02X}, got 0x{:02X}", 
                    PROTOCOL_VERSION_BYTE, bytes[0])
            ));
        }
        Self::decode(&mut &bytes[1..])
    }
}

// Primitive type implementations
impl Encode for u8 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&[*self]) }
}
impl Decode for u8 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 1];
        r.read_exact(&mut buf)?;
        Ok(buf[0])
    }
}

impl Encode for u16 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for u16 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for u32 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for u32 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for u64 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for u64 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 8];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for i8 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&[*self as u8]) }
}
impl Decode for i8 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> { Ok(u8::decode(r)? as i8) }
}

impl Encode for i16 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for i16 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for i32 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for i32 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for i64 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for i64 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 8];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for f32 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for f32 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for f64 {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { w.write_all(&self.to_le_bytes()) }
}
impl Decode for f64 {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 8];
        r.read_exact(&mut buf)?;
        Ok(Self::from_le_bytes(buf))
    }
}

impl Encode for bool {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> { (*self as u8).encode(w) }
}
impl Decode for bool {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> { Ok(u8::decode(r)? != 0) }
}

impl Encode for String {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {
        (self.len() as u32).encode(w)?;
        w.write_all(self.as_bytes())
    }
}
impl Decode for String {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let len = u32::decode(r)? as usize;
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|e| IoError::new(ErrorKind::InvalidData, e))
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {
        (self.len() as u32).encode(w)?;
        for item in self {
            item.encode(w)?;
        }
        Ok(())
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        let len = u32::decode(r)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode(r)?);
        }
        Ok(vec)
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {
        match self {
            Some(v) => {
                1u8.encode(w)?;
                v.encode(w)
            }
            None => 0u8.encode(w),
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {
        match u8::decode(r)? {
            0 => Ok(None),
            _ => Ok(Some(T::decode(r)?)),
        }
    }
}

"#,
    );

    // Generate Encode/Decode for enums
    for e in &manifest.enums {
        content.push_str(&generate_enum_codec(e));
    }

    // Generate Encode/Decode for structs
    for msg in &manifest.messages {
        content.push_str(&generate_struct_codec(msg));
    }

    // Generate Encode/Decode for router
    if let Some(router) = &manifest.router {
        content.push_str(&generate_router_codec(router));
    }

    Ok(GeneratedFile {
        path: PathBuf::from("src/codec.rs"),
        content,
    })
}

fn generate_enum_codec(e: &EnumManifest) -> String {
    let mut s = String::new();
    let name = &e.name;

    if e.is_simple {
        // Simple C-style enum
        s.push_str(&format!("impl Encode for {} {{\n", name));
        s.push_str("    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {\n");
        s.push_str(&format!("        (*self as {}).encode(w)\n", e.repr));
        s.push_str("    }\n}\n\n");

        s.push_str(&format!("impl Decode for {} {{\n", name));
        s.push_str("    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {\n");
        s.push_str(&format!("        let val = {}::decode(r)?;\n", e.repr));
        s.push_str("        match val {\n");
        for v in &e.variants {
            s.push_str(&format!(
                "            {} => Ok(Self::{}),\n",
                v.discriminant, v.name
            ));
        }
        s.push_str(&format!(
            "            _ => Err(IoError::new(ErrorKind::InvalidData, format!(\"Unknown {} value: {{}}\", val))),\n",
            name
        ));
        s.push_str("        }\n");
        s.push_str("    }\n}\n\n");
    } else {
        // Tagged union enum
        s.push_str(&format!("impl Encode for {} {{\n", name));
        s.push_str("    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {\n");
        s.push_str("        match self {\n");

        for v in &e.variants {
            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!(
                        "            Self::{} => {}u8.encode(w),\n",
                        v.name, v.discriminant
                    ));
                }
                VariantData::Tuple { types } => {
                    let bindings: Vec<String> =
                        (0..types.len()).map(|i| format!("v{}", i)).collect();
                    s.push_str(&format!(
                        "            Self::{}({}) => {{\n",
                        v.name,
                        bindings.join(", ")
                    ));
                    s.push_str(&format!(
                        "                {}u8.encode(w)?;\n",
                        v.discriminant
                    ));
                    for b in &bindings {
                        s.push_str(&format!("                {}.encode(w)?;\n", b));
                    }
                    s.push_str("                Ok(())\n");
                    s.push_str("            }\n");
                }
                VariantData::Struct { fields } => {
                    let bindings: Vec<String> = fields
                        .iter()
                        .map(|f| utils::escape_reserved(&f.name, RUST_RESERVED))
                        .collect();
                    s.push_str(&format!(
                        "            Self::{} {{ {} }} => {{\n",
                        v.name,
                        bindings.join(", ")
                    ));
                    s.push_str(&format!(
                        "                {}u8.encode(w)?;\n",
                        v.discriminant
                    ));
                    for b in &bindings {
                        s.push_str(&format!("                {}.encode(w)?;\n", b));
                    }
                    s.push_str("                Ok(())\n");
                    s.push_str("            }\n");
                }
            }
        }

        s.push_str("        }\n");
        s.push_str("    }\n}\n\n");

        // Decode
        s.push_str(&format!("impl Decode for {} {{\n", name));
        s.push_str("    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {\n");
        s.push_str("        let tag = u8::decode(r)?;\n");
        s.push_str("        match tag {\n");

        for v in &e.variants {
            s.push_str(&format!("            {} => ", v.discriminant));
            match &v.data {
                VariantData::Unit => {
                    s.push_str(&format!("Ok(Self::{}),\n", v.name));
                }
                VariantData::Tuple { types } => {
                    s.push_str(&format!("Ok(Self::{}(\n", v.name));
                    for _ in types {
                        s.push_str("                Decode::decode(r)?,\n");
                    }
                    s.push_str("            )),\n");
                }
                VariantData::Struct { fields } => {
                    s.push_str(&format!("Ok(Self::{} {{\n", v.name));
                    for f in fields {
                        let field_name = utils::escape_reserved(&f.name, RUST_RESERVED);
                        s.push_str(&format!(
                            "                {}: Decode::decode(r)?,\n",
                            field_name
                        ));
                    }
                    s.push_str("            }),\n");
                }
            }
        }

        s.push_str(&format!(
            "            _ => Err(IoError::new(ErrorKind::InvalidData, format!(\"Unknown {} tag: {{}}\", tag))),\n",
            name
        ));
        s.push_str("        }\n");
        s.push_str("    }\n}\n\n");
    }

    s
}

fn generate_struct_codec(msg: &MessageDef) -> String {
    let mut s = String::new();
    let name = &msg.name;

    // Handle generics
    let (generics, where_clause) = if msg.generics.is_empty() {
        (String::new(), String::new())
    } else {
        let g = format!("<{}>", msg.generics.join(", "));
        let bounds: Vec<String> = msg
            .generics
            .iter()
            .map(|g| format!("{}: Encode", g))
            .collect();
        let w = format!(" where {}", bounds.join(", "));
        (g, w)
    };

    // Encode
    s.push_str(&format!(
        "impl{} Encode for {}{}{} {{\n",
        generics, name, generics, where_clause
    ));
    s.push_str("    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {\n");

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, RUST_RESERVED);
        s.push_str(&format!("        self.{}.encode(w)?;\n", field_name));
    }

    s.push_str("        Ok(())\n");
    s.push_str("    }\n}\n\n");

    // Decode
    let decode_where = if msg.generics.is_empty() {
        String::new()
    } else {
        let bounds: Vec<String> = msg
            .generics
            .iter()
            .map(|g| format!("{}: Decode", g))
            .collect();
        format!(" where {}", bounds.join(", "))
    };

    s.push_str(&format!(
        "impl{} Decode for {}{}{} {{\n",
        generics, name, generics, decode_where
    ));
    s.push_str("    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {\n");
    s.push_str("        Ok(Self {\n");

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, RUST_RESERVED);
        s.push_str(&format!(
            "            {}: Decode::decode(r)?,\n",
            field_name
        ));
    }

    s.push_str("        })\n");
    s.push_str("    }\n}\n\n");

    s
}

fn generate_router_codec(router: &RouterManifest) -> String {
    let mut s = String::new();
    let name = &router.name;

    // Encode
    s.push_str(&format!("impl Encode for {} {{\n", name));
    s.push_str("    fn encode<W: Write>(&self, w: &mut W) -> IoResult<()> {\n");
    s.push_str("        self.tag().encode(w)?;\n");
    s.push_str("        match self {\n");

    for v in &router.variants {
        s.push_str(&format!(
            "            Self::{}(msg) => msg.encode(w),\n",
            v.name
        ));
    }

    s.push_str("        }\n");
    s.push_str("    }\n}\n\n");

    // Decode
    s.push_str(&format!("impl Decode for {} {{\n", name));
    s.push_str("    fn decode<R: Read>(r: &mut R) -> IoResult<Self> {\n");
    s.push_str("        let tag = u16::decode(r)?;\n");
    s.push_str("        match tag {\n");

    for v in &router.variants {
        s.push_str(&format!(
            "            Self::{}_TAG => Ok(Self::{}(Decode::decode(r)?)),\n",
            utils::to_snake_case(&v.name).to_uppercase(),
            v.name
        ));
    }

    s.push_str(&format!(
        "            _ => Err(IoError::new(ErrorKind::InvalidData, format!(\"Unknown {} tag: {{}}\", tag))),\n",
        name
    ));
    s.push_str("        }\n");
    s.push_str("    }\n}\n\n");

    s
}

fn generate_cargo_toml(
    manifest: &SchemaManifest,
    transport_mode: TransportMode,
) -> Result<GeneratedFile> {
    let name = utils::to_snake_case(&manifest.meta.name);

    let default_features = if transport_mode == TransportMode::Ffi {
        r#"default = ["core", "webtransport", "websocket", "ffi-transport"]"#
    } else {
        r#"default = ["core", "webtransport", "websocket"]"#
    };

    let content = format!(
        r#"[package]
name = "{name}_sdk"
version = "0.1.0"
edition = "2021"
description = "Generated Motto SDK for {name}"
license = "MIT OR Apache-2.0"

# Motto schema metadata
[package.metadata.motto]
fingerprint = "{fingerprint}"
protocol_version = {version_byte}

[features]
{default_features}
core = []
webtransport = ["core", "dep:tokio", "dep:wtransport"]
websocket = ["core", "dep:tokio", "dep:tokio-tungstenite"]
ffi-transport = ["core", "dep:libloading"]

# WASM features (automatically enabled by target)
wasm = ["dep:wasm-bindgen", "dep:wasm-bindgen-futures", "dep:js-sys", "dep:web-sys"]

[dependencies]
# Core has no dependencies - pure Rust types + codec

# Native async runtime (for transport features)
tokio = {{ version = "1", features = ["rt", "sync", "time"], optional = true }}

# Native WebTransport
wtransport = {{ version = "0.6", optional = true }}

# Native WebSocket
tokio-tungstenite = {{ version = "0.24", optional = true }}

# FFI transport (shared Rust transport core via C ABI)
libloading = {{ version = "0.8", optional = true }}

# WASM bindings (auto-enabled on wasm32 target)
wasm-bindgen = {{ version = "0.2", optional = true }}
wasm-bindgen-futures = {{ version = "0.4", optional = true }}
js-sys = {{ version = "0.3", optional = true }}
web-sys = {{ version = "0.3", optional = true, features = [
    "WebTransport", "WebTransportDatagramDuplexStream",
    "ReadableStream", "WritableStream", "ReadableStreamDefaultReader",
    "WritableStreamDefaultWriter", "WebSocket", "MessageEvent", "BinaryType",
    "CloseEvent", "ErrorEvent", "Window"
] }}

[dev-dependencies]
# For testing

# Auto-enable wasm feature on wasm32 target
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = {{ version = "0.2" }}
wasm-bindgen-futures = {{ version = "0.4" }}
js-sys = {{ version = "0.3" }}
web-sys = {{ version = "0.3", features = [
    "WebTransport", "WebTransportDatagramDuplexStream",
    "ReadableStream", "WritableStream", "ReadableStreamDefaultReader",
    "WritableStreamDefaultWriter", "WebSocket", "MessageEvent", "BinaryType",
    "CloseEvent", "ErrorEvent", "Window"
] }}
"#,
        name = name,
        fingerprint = &manifest.meta.fingerprint[..16],
        version_byte = manifest.meta.version_byte,
        default_features = default_features,
    );

    Ok(GeneratedFile {
        path: PathBuf::from("Cargo.toml"),
        content,
    })
}

fn generate_rust_header(version_byte: u8, fingerprint: &str, timestamp: &str) -> String {
    format!(
        r#"// ============================================================================
// MOTTO GENERATED CODE - DO NOT EDIT
// 
// This file was generated by motto from a Rust schema definition.
// Any changes will be overwritten on next generation.
//
// Protocol Version Byte: 0x{:02X}
// Schema Fingerprint: {}
// Generated At: {}
// ============================================================================
"#,
        version_byte, fingerprint, timestamp
    )
}

/// Convert a manifest type reference to a Rust type
fn rust_type_to_rust(type_ref: &str) -> String {
    // Handle generic types
    if let Some(inner_start) = type_ref.find('<') {
        let name = &type_ref[..inner_start];
        let inner = &type_ref[inner_start + 1..type_ref.len() - 1];

        match name {
            "Vec" => format!("Vec<{}>", rust_type_to_rust(inner)),
            "Option" => format!("Option<{}>", rust_type_to_rust(inner)),
            "HashMap" | "BTreeMap" => {
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "std::collections::HashMap<{}, {}>",
                        rust_type_to_rust(parts[0]),
                        rust_type_to_rust(parts[1])
                    )
                } else {
                    type_ref.to_string()
                }
            }
            _ => type_ref.to_string(),
        }
    } else {
        type_ref.to_string()
    }
}

/// Generate test file for the SDK
fn generate_tests(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(
        r#"
//! Generated tests for encode/decode roundtrips and router functionality.
//!
//! Run with: `cargo test`

use super::*;
use super::codec::{Encode, Decode};

// ============================================================================
// Helper: Create test instances with sample data
// ============================================================================

"#,
    );

    // Generate helper functions to create test instances for each non-generic struct
    for msg in &manifest.messages {
        if !msg.generics.is_empty() {
            continue; // Skip generic types
        }
        content.push_str(&generate_test_instance_fn(msg, manifest));
    }

    // Generate roundtrip tests for each struct
    content.push_str(
        r#"
// ============================================================================
// Roundtrip Tests: Encode -> Decode -> Compare
// ============================================================================

"#,
    );

    for msg in &manifest.messages {
        if !msg.generics.is_empty() {
            continue;
        }
        content.push_str(&generate_roundtrip_test(msg));
    }

    // Generate enum tests
    content.push_str(
        r#"
// ============================================================================
// Enum Serialization Tests
// ============================================================================

"#,
    );

    for e in &manifest.enums {
        if !e.generics.is_empty() {
            continue;
        }
        content.push_str(&generate_enum_tests(e));
    }

    // Generate router tests if router exists
    if let Some(router) = &manifest.router {
        content.push_str(
            r#"
// ============================================================================
// Router Tests: Tag values, routing, handler trait
// ============================================================================

"#,
        );
        content.push_str(&generate_router_tests(router, manifest));
    }

    // Generate version byte tests
    content.push_str(&generate_version_tests(manifest));

    Ok(GeneratedFile {
        path: PathBuf::from("src/tests.rs"),
        content,
    })
}

/// Generate a function that creates a test instance of a struct
fn generate_test_instance_fn(msg: &MessageDef, manifest: &SchemaManifest) -> String {
    let mut s = String::new();
    let fn_name = utils::to_snake_case(&msg.name);

    s.push_str(&format!("/// Create a test instance of {}\n", msg.name));
    s.push_str(&format!(
        "fn create_test_{}() -> {} {{\n",
        fn_name, msg.name
    ));
    s.push_str(&format!("    {} {{\n", msg.name));

    for field in &msg.fields {
        let field_name = utils::escape_reserved(&field.name, RUST_RESERVED);
        let value = generate_test_value(&field.type_ref, &field.name, manifest);
        s.push_str(&format!("        {}: {},\n", field_name, value));
    }

    s.push_str("    }\n");
    s.push_str("}\n\n");

    s
}

// ============================================================================
// Transport Module Generators
// ============================================================================

/// Generate common transport traits and types
fn generate_transport_common(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(
        r#"
//! Common transport traits and types.
//!
//! This module provides the shared abstractions used by both WebTransport
//! and WebSocket implementations.

use std::future::Future;
use std::pin::Pin;

/// Transport error types
#[derive(Debug)]
pub enum TransportError {
    /// Connection failed
    ConnectionFailed(String),
    /// Connection closed
    Disconnected,
    /// Failed to send data
    SendFailed(String),
    /// Failed to receive data
    ReceiveFailed(String),
    /// Codec error (encode/decode failure)
    CodecError(String),
    /// Version mismatch
    VersionMismatch { expected: u8, got: u8 },
    /// Connection timeout
    Timeout,
    /// Other error
    Other(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            TransportError::Disconnected => write!(f, "Disconnected"),
            TransportError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            TransportError::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
            TransportError::CodecError(msg) => write!(f, "Codec error: {}", msg),
            TransportError::VersionMismatch { expected, got } => {
                write!(f, "Version mismatch: expected 0x{:02X}, got 0x{:02X}", expected, got)
            }
            TransportError::Timeout => write!(f, "Timeout"),
            TransportError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Transport trait - implemented by WebTransport and WebSocket clients
pub trait Transport {
    type Error;
    
    /// Send raw bytes
    fn send(&self, data: &[u8]) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>>;
    
    /// Receive raw bytes
    fn recv(&self) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Self::Error>> + Send + '_>>;
    
    /// Close the connection
    fn close(&self) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>>;
    
    /// Get current connection state
    fn state(&self) -> ConnectionState;
}

/// Configuration for transport clients
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Server URL
    pub url: String,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts in milliseconds
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection delay in milliseconds
    pub max_reconnect_delay_ms: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            connect_timeout_ms: 10_000,
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            reconnect_delay_ms: 1_000,
            max_reconnect_delay_ms: 30_000,
        }
    }
}

impl TransportConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}
"#,
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/transport.rs"),
        content,
    })
}

/// Generate WebTransport module with native/WASM support
fn generate_webtransport(
    manifest: &SchemaManifest,
    transport_mode: TransportMode,
) -> Result<GeneratedFile> {
    let router_name = manifest
        .router
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_else(|| "Router".to_string());

    let mut content = String::new();

    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(&format!(
        r#"
//! WebTransport client implementation.
//!
//! This module provides a WebTransport client that works on both native
//! and WASM targets. The implementation is automatically selected at
//! compile time based on the target architecture.
//!
//! # Native (non-WASM)
//! Uses the `wtransport` crate for pure Rust WebTransport.
//!
//! # WASM
//! Uses `wasm-bindgen` to access the browser's WebTransport API.
//!
//! # Example
//! ```ignore
//! use {name}_schema::{{WebTransportClient, {router}}};
//! use {name}_schema::transport::TransportConfig;
//!
//! let config = TransportConfig::new("https://example.com:4433");
//! let client = WebTransportClient::connect(config).await?;
//!
//! // Send a message
//! client.send(&my_message).await?;
//!
//! // Receive and route messages
//! let msg: {router} = client.recv().await?;
//! ```

use crate::transport::{{TransportError, TransportConfig, ConnectionState}};
use crate::codec::{{Encode, Decode}};
use crate::PROTOCOL_VERSION_BYTE;
"#,
        name = utils::to_snake_case(&manifest.meta.name),
        router = router_name,
    ));

    // Native implementation
    if transport_mode == TransportMode::Ffi {
        content.push_str(
            r#"
// ============================================================================
// Native Implementation (non-WASM) — FFI-backed
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::ptr;

    // FFI declarations for the motto transport core
    extern "C" {
        fn motto_transport_new(url: *const c_char) -> *mut std::ffi::c_void;
        fn motto_transport_free(handle: *mut std::ffi::c_void);
        fn motto_transport_connect(handle: *mut std::ffi::c_void) -> i32;
        fn motto_transport_close(handle: *mut std::ffi::c_void);
        fn motto_transport_send(handle: *mut std::ffi::c_void, data: *const u8, data_len: usize) -> i32;
        fn motto_transport_recv(handle: *mut std::ffi::c_void, out_data: *mut *mut u8, out_len: *mut usize) -> i32;
        fn motto_transport_recv_free(data: *mut u8, len: usize);
        fn motto_transport_state(handle: *mut std::ffi::c_void) -> u8;
        fn motto_transport_last_error(handle: *mut std::ffi::c_void) -> *const c_char;
    }

    pub struct WebTransportClient {
        handle: *mut std::ffi::c_void,
    }

    impl WebTransportClient {
        pub fn new(url: &str) -> Result<Self, String> {
            let c_url = CString::new(url).map_err(|e| e.to_string())?;
            let handle = unsafe { motto_transport_new(c_url.as_ptr()) };
            if handle.is_null() {
                return Err("Failed to create transport handle".to_string());
            }
            Ok(Self { handle })
        }

        pub fn connect(&self) -> Result<(), String> {
            let rc = unsafe { motto_transport_connect(self.handle) };
            if rc != 0 {
                return Err(self.last_error());
            }
            Ok(())
        }

        pub fn send(&self, data: &[u8]) -> Result<(), String> {
            let rc = unsafe { motto_transport_send(self.handle, data.as_ptr(), data.len()) };
            if rc != 0 {
                return Err(self.last_error());
            }
            Ok(())
        }

        pub fn recv(&self) -> Result<Vec<u8>, String> {
            let mut out_data: *mut u8 = ptr::null_mut();
            let mut out_len: usize = 0;
            let rc = unsafe { motto_transport_recv(self.handle, &mut out_data, &mut out_len) };
            if rc != 0 {
                return Err(self.last_error());
            }
            let data = unsafe { std::slice::from_raw_parts(out_data, out_len).to_vec() };
            unsafe { motto_transport_recv_free(out_data, out_len) };
            Ok(data)
        }

        pub fn close(&self) {
            unsafe { motto_transport_close(self.handle) };
        }

        pub fn state(&self) -> u8 {
            unsafe { motto_transport_state(self.handle) }
        }

        fn last_error(&self) -> String {
            let ptr = unsafe { motto_transport_last_error(self.handle) };
            if ptr.is_null() {
                "Unknown error".to_string()
            } else {
                unsafe { std::ffi::CStr::from_ptr(ptr) }.to_string_lossy().to_string()
            }
        }
    }

    impl Drop for WebTransportClient {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe { motto_transport_free(self.handle) };
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::WebTransportClient;
"#,
        );
    } else {
        content.push_str(
            r#"
// ============================================================================
// Native Implementation (non-WASM)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// WebTransport client for native platforms
    pub struct WebTransportClient {
        config: TransportConfig,
        state: Arc<RwLock<ConnectionState>>,
        // Connection handle would be stored here
        // connection: Option<wtransport::Connection>,
    }

    impl WebTransportClient {
        /// Create a new WebTransport client (not connected)
        pub fn new(config: TransportConfig) -> Self {
            Self {
                config,
                state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            }
        }

        /// Connect to the server
        pub async fn connect(config: TransportConfig) -> Result<Self, TransportError> {
            let client = Self::new(config);
            client.do_connect().await?;
            Ok(client)
        }

        async fn do_connect(&self) -> Result<(), TransportError> {
            *self.state.write().await = ConnectionState::Connecting;
            
            // Native WebTransport is not yet implemented.
            // Add `wtransport` to your Cargo.toml and implement the connection logic here.
            // See https://docs.rs/wtransport for the client API.
            //
            // Example:
            // let endpoint = wtransport::Endpoint::client(
            //     wtransport::ClientConfig::builder()
            //         .with_bind_default()
            //         .with_no_cert_validation()
            //         .build()
            // )?;
            // let connection = endpoint.connect(&self.config.url).await?;
            
            *self.state.write().await = ConnectionState::Disconnected;
            Err(TransportError::ConnectionFailed(
                "Native WebTransport not yet implemented. Add `wtransport` crate and implement do_connect().".into()
            ))
        }

        /// Send an encodable message
        pub async fn send<T: Encode>(&self, msg: &T) -> Result<(), TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            let mut buf = vec![PROTOCOL_VERSION_BYTE];
            msg.encode(&mut buf)
                .map_err(|e| TransportError::CodecError(e.to_string()))?;
            
            self.send_raw(&buf).await
        }

        /// Send raw bytes
        pub async fn send_raw(&self, data: &[u8]) -> Result<(), TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            // Native WebTransport send not yet implemented.
            // self.connection.send_datagram(data).await?;
            let _ = data;
            Err(TransportError::SendFailed(
                "Native WebTransport send not yet implemented.".into()
            ))
        }

        /// Receive and decode a message
        pub async fn recv<T: Decode>(&self) -> Result<T, TransportError> {
            let data = self.recv_raw().await?;
            
            if data.is_empty() {
                return Err(TransportError::ReceiveFailed("Empty packet".into()));
            }

            if data[0] != PROTOCOL_VERSION_BYTE {
                return Err(TransportError::VersionMismatch {
                    expected: PROTOCOL_VERSION_BYTE,
                    got: data[0],
                });
            }

            T::decode(&mut &data[1..])
                .map_err(|e| TransportError::CodecError(e.to_string()))
        }

        /// Receive raw bytes
        pub async fn recv_raw(&self) -> Result<Vec<u8>, TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            // Native WebTransport recv not yet implemented.
            // let data = self.connection.receive_datagram().await?;
            Err(TransportError::ReceiveFailed(
                "Native WebTransport recv not yet implemented.".into()
            ))
        }

        /// Close the connection
        pub async fn close(&self) -> Result<(), TransportError> {
            *self.state.write().await = ConnectionState::Disconnected;
            // TODO: Close wtransport connection
            Ok(())
        }

        /// Get current connection state
        pub async fn state(&self) -> ConnectionState {
            *self.state.read().await
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::WebTransportClient;
"#,
        );
    }

    // WASM implementation
    content.push_str(
        r#"
// ============================================================================
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{WebTransport as JsWebTransport, WebTransportDatagramDuplexStream};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// WebTransport client for WASM (browser)
    pub struct WebTransportClient {
        config: TransportConfig,
        state: Rc<RefCell<ConnectionState>>,
        transport: Rc<RefCell<Option<JsWebTransport>>>,
    }

    impl WebTransportClient {
        /// Create a new WebTransport client (not connected)
        pub fn new(config: TransportConfig) -> Self {
            Self {
                config,
                state: Rc::new(RefCell::new(ConnectionState::Disconnected)),
                transport: Rc::new(RefCell::new(None)),
            }
        }

        /// Connect to the server
        pub async fn connect(config: TransportConfig) -> Result<Self, TransportError> {
            let client = Self::new(config);
            client.do_connect().await?;
            Ok(client)
        }

        async fn do_connect(&self) -> Result<(), TransportError> {
            *self.state.borrow_mut() = ConnectionState::Connecting;

            // Create WebTransport instance via browser API
            let transport = JsWebTransport::new(&self.config.url)
                .map_err(|e| TransportError::ConnectionFailed(format!("{:?}", e)))?;

            // Wait for connection to be ready
            let ready_promise = transport.ready();
            JsFuture::from(ready_promise)
                .await
                .map_err(|e| TransportError::ConnectionFailed(format!("{:?}", e)))?;

            *self.transport.borrow_mut() = Some(transport);
            *self.state.borrow_mut() = ConnectionState::Connected;
            Ok(())
        }

        /// Send an encodable message
        pub async fn send<T: Encode>(&self, msg: &T) -> Result<(), TransportError> {
            if *self.state.borrow() != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            let mut buf = vec![PROTOCOL_VERSION_BYTE];
            msg.encode(&mut buf)
                .map_err(|e| TransportError::CodecError(e.to_string()))?;
            
            self.send_raw(&buf).await
        }

        /// Send raw bytes
        pub async fn send_raw(&self, data: &[u8]) -> Result<(), TransportError> {
            let transport = self.transport.borrow();
            let transport = transport.as_ref().ok_or(TransportError::Disconnected)?;

            let datagrams: WebTransportDatagramDuplexStream = transport.datagrams();
            let writable = datagrams.writable();
            
            let writer = writable
                .get_writer()
                .map_err(|e| TransportError::SendFailed(format!("{:?}", e)))?;

            let uint8_array = js_sys::Uint8Array::from(data);
            let write_promise = writer.write_with_chunk(&uint8_array);
            
            JsFuture::from(write_promise)
                .await
                .map_err(|e| TransportError::SendFailed(format!("{:?}", e)))?;

            writer.release_lock();
            Ok(())
        }

        /// Receive and decode a message
        pub async fn recv<T: Decode>(&self) -> Result<T, TransportError> {
            let data = self.recv_raw().await?;
            
            if data.is_empty() {
                return Err(TransportError::ReceiveFailed("Empty packet".into()));
            }

            if data[0] != PROTOCOL_VERSION_BYTE {
                return Err(TransportError::VersionMismatch {
                    expected: PROTOCOL_VERSION_BYTE,
                    got: data[0],
                });
            }

            T::decode(&mut &data[1..])
                .map_err(|e| TransportError::CodecError(e.to_string()))
        }

        /// Receive raw bytes
        pub async fn recv_raw(&self) -> Result<Vec<u8>, TransportError> {
            let transport = self.transport.borrow();
            let transport = transport.as_ref().ok_or(TransportError::Disconnected)?;

            let datagrams: WebTransportDatagramDuplexStream = transport.datagrams();
            let readable = datagrams.readable();
            
            let reader = readable
                .get_reader()
                .map_err(|e| TransportError::ReceiveFailed(format!("{:?}", e)))?
                .unchecked_into::<web_sys::ReadableStreamDefaultReader>();

            let read_promise = reader.read();
            let result = JsFuture::from(read_promise)
                .await
                .map_err(|e| TransportError::ReceiveFailed(format!("{:?}", e)))?;

            reader.release_lock();

            // Extract value from ReadableStreamReadResult
            let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
                .map_err(|e| TransportError::ReceiveFailed(format!("{:?}", e)))?;

            if value.is_undefined() {
                return Err(TransportError::Disconnected);
            }

            let uint8_array = value.unchecked_into::<js_sys::Uint8Array>();
            Ok(uint8_array.to_vec())
        }

        /// Close the connection
        pub async fn close(&self) -> Result<(), TransportError> {
            if let Some(transport) = self.transport.borrow().as_ref() {
                transport.close();
            }
            *self.transport.borrow_mut() = None;
            *self.state.borrow_mut() = ConnectionState::Disconnected;
            Ok(())
        }

        /// Get current connection state
        pub fn state(&self) -> ConnectionState {
            *self.state.borrow()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WebTransportClient;
"#,
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/webtransport.rs"),
        content,
    })
}

/// Generate WebSocket module with native/WASM support
fn generate_websocket(
    manifest: &SchemaManifest,
    transport_mode: TransportMode,
) -> Result<GeneratedFile> {
    let router_name = manifest
        .router
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_else(|| "Router".to_string());

    let mut content = String::new();

    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(&format!(
        r#"
//! WebSocket client implementation.
//!
//! This module provides a WebSocket client that works on both native
//! and WASM targets. The implementation is automatically selected at
//! compile time based on the target architecture.
//!
//! # Native (non-WASM)
//! Uses `tokio-tungstenite` for async WebSocket support.
//!
//! # WASM
//! Uses `wasm-bindgen` to access the browser's WebSocket API.
//!
//! # Example
//! ```ignore
//! use {name}_schema::{{WebSocketClient, {router}}};
//! use {name}_schema::transport::TransportConfig;
//!
//! let config = TransportConfig::new("wss://example.com/ws");
//! let client = WebSocketClient::connect(config).await?;
//!
//! // Send a message
//! client.send(&my_message).await?;
//!
//! // Receive and route messages
//! let msg: {router} = client.recv().await?;
//! ```

use crate::transport::{{TransportError, TransportConfig, ConnectionState}};
use crate::codec::{{Encode, Decode}};
use crate::PROTOCOL_VERSION_BYTE;
"#,
        name = utils::to_snake_case(&manifest.meta.name),
        router = router_name,
    ));

    // Native implementation
    if transport_mode == TransportMode::Ffi {
        content.push_str(
            r#"
// ============================================================================
// Native Implementation (non-WASM) — FFI-backed
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::ptr;

    // FFI declarations for the motto transport core
    extern "C" {
        fn motto_transport_new(url: *const c_char) -> *mut std::ffi::c_void;
        fn motto_transport_free(handle: *mut std::ffi::c_void);
        fn motto_transport_connect(handle: *mut std::ffi::c_void) -> i32;
        fn motto_transport_close(handle: *mut std::ffi::c_void);
        fn motto_transport_send(handle: *mut std::ffi::c_void, data: *const u8, data_len: usize) -> i32;
        fn motto_transport_recv(handle: *mut std::ffi::c_void, out_data: *mut *mut u8, out_len: *mut usize) -> i32;
        fn motto_transport_recv_free(data: *mut u8, len: usize);
        fn motto_transport_state(handle: *mut std::ffi::c_void) -> u8;
        fn motto_transport_last_error(handle: *mut std::ffi::c_void) -> *const c_char;
    }

    pub struct WebSocketClient {
        handle: *mut std::ffi::c_void,
    }

    impl WebSocketClient {
        pub fn new(url: &str) -> Result<Self, String> {
            let c_url = CString::new(url).map_err(|e| e.to_string())?;
            let handle = unsafe { motto_transport_new(c_url.as_ptr()) };
            if handle.is_null() {
                return Err("Failed to create transport handle".to_string());
            }
            Ok(Self { handle })
        }

        pub fn connect(&self) -> Result<(), String> {
            let rc = unsafe { motto_transport_connect(self.handle) };
            if rc != 0 {
                return Err(self.last_error());
            }
            Ok(())
        }

        pub fn send(&self, data: &[u8]) -> Result<(), String> {
            let rc = unsafe { motto_transport_send(self.handle, data.as_ptr(), data.len()) };
            if rc != 0 {
                return Err(self.last_error());
            }
            Ok(())
        }

        pub fn recv(&self) -> Result<Vec<u8>, String> {
            let mut out_data: *mut u8 = ptr::null_mut();
            let mut out_len: usize = 0;
            let rc = unsafe { motto_transport_recv(self.handle, &mut out_data, &mut out_len) };
            if rc != 0 {
                return Err(self.last_error());
            }
            let data = unsafe { std::slice::from_raw_parts(out_data, out_len).to_vec() };
            unsafe { motto_transport_recv_free(out_data, out_len) };
            Ok(data)
        }

        pub fn close(&self) {
            unsafe { motto_transport_close(self.handle) };
        }

        pub fn state(&self) -> u8 {
            unsafe { motto_transport_state(self.handle) }
        }

        fn last_error(&self) -> String {
            let ptr = unsafe { motto_transport_last_error(self.handle) };
            if ptr.is_null() {
                "Unknown error".to_string()
            } else {
                unsafe { std::ffi::CStr::from_ptr(ptr) }.to_string_lossy().to_string()
            }
        }
    }

    impl Drop for WebSocketClient {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe { motto_transport_free(self.handle) };
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::WebSocketClient;
"#,
        );
    } else {
        content.push_str(
            r#"
// ============================================================================
// Native Implementation (non-WASM)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// WebSocket client for native platforms
    pub struct WebSocketClient {
        config: TransportConfig,
        state: Arc<RwLock<ConnectionState>>,
        // WebSocket connection would be stored here
        // ws: Option<tokio_tungstenite::WebSocketStream<...>>,
    }

    impl WebSocketClient {
        /// Create a new WebSocket client (not connected)
        pub fn new(config: TransportConfig) -> Self {
            Self {
                config,
                state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            }
        }

        /// Connect to the server
        pub async fn connect(config: TransportConfig) -> Result<Self, TransportError> {
            let client = Self::new(config);
            client.do_connect().await?;
            Ok(client)
        }

        async fn do_connect(&self) -> Result<(), TransportError> {
            *self.state.write().await = ConnectionState::Connecting;
            
            // Native WebSocket is not yet implemented.
            // Add `tokio-tungstenite` to your Cargo.toml and implement the connection logic here.
            // See https://docs.rs/tokio-tungstenite for the client API.
            //
            // Example:
            // let (ws_stream, _) = tokio_tungstenite::connect_async(&self.config.url)
            //     .await
            //     .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
            
            *self.state.write().await = ConnectionState::Disconnected;
            Err(TransportError::ConnectionFailed(
                "Native WebSocket not yet implemented. Add `tokio-tungstenite` crate and implement do_connect().".into()
            ))
        }

        /// Send an encodable message
        pub async fn send<T: Encode>(&self, msg: &T) -> Result<(), TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            let mut buf = vec![PROTOCOL_VERSION_BYTE];
            msg.encode(&mut buf)
                .map_err(|e| TransportError::CodecError(e.to_string()))?;
            
            self.send_raw(&buf).await
        }

        /// Send raw bytes
        pub async fn send_raw(&self, data: &[u8]) -> Result<(), TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            // Native WebSocket send not yet implemented.
            // use tokio_tungstenite::tungstenite::Message;
            // self.ws.send(Message::Binary(data.to_vec())).await?;
            let _ = data;
            Err(TransportError::SendFailed(
                "Native WebSocket send not yet implemented.".into()
            ))
        }

        /// Receive and decode a message
        pub async fn recv<T: Decode>(&self) -> Result<T, TransportError> {
            let data = self.recv_raw().await?;
            
            if data.is_empty() {
                return Err(TransportError::ReceiveFailed("Empty packet".into()));
            }

            if data[0] != PROTOCOL_VERSION_BYTE {
                return Err(TransportError::VersionMismatch {
                    expected: PROTOCOL_VERSION_BYTE,
                    got: data[0],
                });
            }

            T::decode(&mut &data[1..])
                .map_err(|e| TransportError::CodecError(e.to_string()))
        }

        /// Receive raw bytes
        pub async fn recv_raw(&self) -> Result<Vec<u8>, TransportError> {
            let state = *self.state.read().await;
            if state != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            // Native WebSocket recv not yet implemented.
            Err(TransportError::ReceiveFailed(
                "Native WebSocket recv not yet implemented.".into()
            ))
        }

        /// Close the connection
        pub async fn close(&self) -> Result<(), TransportError> {
            *self.state.write().await = ConnectionState::Disconnected;
            // TODO: Close WebSocket connection
            Ok(())
        }

        /// Get current connection state
        pub async fn state(&self) -> ConnectionState {
            *self.state.read().await
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::WebSocketClient;
"#,
        );
    }

    // WASM implementation
    content.push_str(
        r#"
// ============================================================================
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{WebSocket as JsWebSocket, MessageEvent, BinaryType};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::collections::VecDeque;

    /// WebSocket client for WASM (browser)
    pub struct WebSocketClient {
        config: TransportConfig,
        state: Rc<RefCell<ConnectionState>>,
        socket: Rc<RefCell<Option<JsWebSocket>>>,
        recv_queue: Rc<RefCell<VecDeque<Vec<u8>>>>,
    }

    impl WebSocketClient {
        /// Create a new WebSocket client (not connected)
        pub fn new(config: TransportConfig) -> Self {
            Self {
                config,
                state: Rc::new(RefCell::new(ConnectionState::Disconnected)),
                socket: Rc::new(RefCell::new(None)),
                recv_queue: Rc::new(RefCell::new(VecDeque::new())),
            }
        }

        /// Connect to the server
        pub async fn connect(config: TransportConfig) -> Result<Self, TransportError> {
            let client = Self::new(config);
            client.do_connect().await?;
            Ok(client)
        }

        async fn do_connect(&self) -> Result<(), TransportError> {
            *self.state.borrow_mut() = ConnectionState::Connecting;

            // Create WebSocket via browser API
            let socket = JsWebSocket::new(&self.config.url)
                .map_err(|e| TransportError::ConnectionFailed(format!("{:?}", e)))?;

            // Set binary type to arraybuffer for efficient data transfer
            socket.set_binary_type(BinaryType::Arraybuffer);

            // Set up message handler
            let recv_queue = self.recv_queue.clone();
            let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&abuf);
                    recv_queue.borrow_mut().push_back(array.to_vec());
                }
            }) as Box<dyn FnMut(MessageEvent)>);
            socket.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget(); // Prevent closure from being dropped

            // Set up open handler
            let state = self.state.clone();
            let onopen_callback = Closure::wrap(Box::new(move |_| {
                *state.borrow_mut() = ConnectionState::Connected;
            }) as Box<dyn FnMut(JsValue)>);
            socket.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
            onopen_callback.forget();

            // Set up close handler
            let state = self.state.clone();
            let onclose_callback = Closure::wrap(Box::new(move |_| {
                *state.borrow_mut() = ConnectionState::Disconnected;
            }) as Box<dyn FnMut(JsValue)>);
            socket.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
            onclose_callback.forget();

            *self.socket.borrow_mut() = Some(socket);

            // Wait for connection (simple polling - in production use promises)
            for _ in 0..100 {
                if *self.state.borrow() == ConnectionState::Connected {
                    return Ok(());
                }
                // Small delay
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    let _ = web_sys::window()
                        .unwrap()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
                });
                wasm_bindgen_futures::JsFuture::from(promise).await.ok();
            }

            Err(TransportError::Timeout)
        }

        /// Send an encodable message
        pub async fn send<T: Encode>(&self, msg: &T) -> Result<(), TransportError> {
            if *self.state.borrow() != ConnectionState::Connected {
                return Err(TransportError::Disconnected);
            }

            let mut buf = vec![PROTOCOL_VERSION_BYTE];
            msg.encode(&mut buf)
                .map_err(|e| TransportError::CodecError(e.to_string()))?;
            
            self.send_raw(&buf).await
        }

        /// Send raw bytes
        pub async fn send_raw(&self, data: &[u8]) -> Result<(), TransportError> {
            let socket = self.socket.borrow();
            let socket = socket.as_ref().ok_or(TransportError::Disconnected)?;

            socket
                .send_with_u8_array(data)
                .map_err(|e| TransportError::SendFailed(format!("{:?}", e)))
        }

        /// Receive and decode a message
        pub async fn recv<T: Decode>(&self) -> Result<T, TransportError> {
            let data = self.recv_raw().await?;
            
            if data.is_empty() {
                return Err(TransportError::ReceiveFailed("Empty packet".into()));
            }

            if data[0] != PROTOCOL_VERSION_BYTE {
                return Err(TransportError::VersionMismatch {
                    expected: PROTOCOL_VERSION_BYTE,
                    got: data[0],
                });
            }

            T::decode(&mut &data[1..])
                .map_err(|e| TransportError::CodecError(e.to_string()))
        }

        /// Receive raw bytes (waits for data with polling)
        pub async fn recv_raw(&self) -> Result<Vec<u8>, TransportError> {
            // Poll for data with timeout
            for _ in 0..200 { // ~10 seconds with 50ms delay
                if let Some(data) = self.recv_queue.borrow_mut().pop_front() {
                    return Ok(data);
                }

                if *self.state.borrow() != ConnectionState::Connected {
                    return Err(TransportError::Disconnected);
                }

                // Small delay
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    let _ = web_sys::window()
                        .unwrap()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
                });
                wasm_bindgen_futures::JsFuture::from(promise).await.ok();
            }

            Err(TransportError::Timeout)
        }

        /// Try to receive without blocking
        pub fn try_recv_raw(&self) -> Option<Vec<u8>> {
            self.recv_queue.borrow_mut().pop_front()
        }

        /// Close the connection
        pub async fn close(&self) -> Result<(), TransportError> {
            if let Some(socket) = self.socket.borrow().as_ref() {
                socket.close().ok();
            }
            *self.socket.borrow_mut() = None;
            *self.state.borrow_mut() = ConnectionState::Disconnected;
            Ok(())
        }

        /// Get current connection state
        pub fn state(&self) -> ConnectionState {
            *self.state.borrow()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WebSocketClient;
"#,
    );

    Ok(GeneratedFile {
        path: PathBuf::from("src/websocket.rs"),
        content,
    })
}
/// Generate a test value for a given type
fn generate_test_value(type_ref: &str, field_name: &str, manifest: &SchemaManifest) -> String {
    // Check if it's an Option type
    if type_ref.starts_with("Option<") {
        let inner = &type_ref[7..type_ref.len() - 1];
        let inner_value = generate_test_value(inner, field_name, manifest);
        return format!("Some({})", inner_value);
    }

    // Check if it's a Vec type
    if type_ref.starts_with("Vec<") {
        let inner = &type_ref[4..type_ref.len() - 1];
        let inner_value = generate_test_value(inner, field_name, manifest);
        return format!("vec![{}]", inner_value);
    }

    // Primitives
    match type_ref {
        "u8" => "42u8".to_string(),
        "u16" => "1234u16".to_string(),
        "u32" => "123456u32".to_string(),
        "u64" => "123456789u64".to_string(),
        "i8" => "-42i8".to_string(),
        "i16" => "-1234i16".to_string(),
        "i32" => "-123456i32".to_string(),
        "i64" => "-123456789i64".to_string(),
        "f32" => "3.14159f32".to_string(),
        "f64" => "2.718281828f64".to_string(),
        "bool" => "true".to_string(),
        "String" => format!("\"test_{}\".to_string()", field_name),
        _ => {
            // Check if it's a type alias
            if let Some(alias) = manifest.type_aliases.iter().find(|a| a.name == type_ref) {
                return generate_test_value(&alias.target, field_name, manifest);
            }

            // Check if it's an enum
            if let Some(e) = manifest.enums.iter().find(|e| e.name == type_ref) {
                if e.is_simple && !e.variants.is_empty() {
                    format!("{}::{}", e.name, e.variants[0].name)
                } else if !e.variants.is_empty() {
                    // For complex enums, use the first unit variant if available
                    for v in &e.variants {
                        if matches!(v.data, VariantData::Unit) {
                            return format!("{}::{}", e.name, v.name);
                        }
                    }
                    // Otherwise create a simple variant
                    let v = &e.variants[0];
                    match &v.data {
                        VariantData::Unit => format!("{}::{}", e.name, v.name),
                        VariantData::Tuple { types } => {
                            let values: Vec<String> = types
                                .iter()
                                .enumerate()
                                .map(|(i, t)| {
                                    generate_test_value(
                                        t,
                                        &format!("{}_{}", field_name, i),
                                        manifest,
                                    )
                                })
                                .collect();
                            format!("{}::{}({})", e.name, v.name, values.join(", "))
                        }
                        VariantData::Struct { fields } => {
                            let values: Vec<String> = fields
                                .iter()
                                .map(|f| {
                                    let val = generate_test_value(&f.type_ref, &f.name, manifest);
                                    format!("{}: {}", f.name, val)
                                })
                                .collect();
                            format!("{}::{} {{ {} }}", e.name, v.name, values.join(", "))
                        }
                    }
                } else {
                    format!("Default::default() /* {} */", type_ref)
                }
            }
            // Check if it's a struct we know about
            else if let Some(msg) = manifest.messages.iter().find(|m| m.name == type_ref) {
                if msg.generics.is_empty() {
                    format!("create_test_{}()", utils::to_snake_case(type_ref))
                } else {
                    format!("Default::default() /* generic {} */", type_ref)
                }
            } else {
                format!("Default::default() /* unknown {} */", type_ref)
            }
        }
    }
}

/// Generate a roundtrip test for a struct
fn generate_roundtrip_test(msg: &MessageDef) -> String {
    let mut s = String::new();
    let fn_name = utils::to_snake_case(&msg.name);

    s.push_str("#[test]\n");
    s.push_str(&format!("fn test_{}_roundtrip() {{\n", fn_name));
    s.push_str(&format!("    let original = create_test_{}();\n", fn_name));
    s.push_str("    \n");
    s.push_str("    // Encode to bytes\n");
    s.push_str("    let encoded = original.to_bytes();\n");
    s.push_str("    \n");
    s.push_str("    // Verify version byte is present\n");
    s.push_str("    assert!(!encoded.is_empty(), \"Encoded bytes should not be empty\");\n");
    s.push_str("    assert_eq!(encoded[0], PROTOCOL_VERSION_BYTE, \"First byte should be version byte\");\n");
    s.push_str("    \n");
    s.push_str("    // Decode back\n");
    s.push_str(&format!(
        "    let decoded = {}::from_bytes(&encoded).expect(\"Decode should succeed\");\n",
        msg.name
    ));
    s.push_str("    \n");
    s.push_str("    // Compare\n");
    s.push_str("    assert_eq!(original, decoded, \"Roundtrip should preserve data\");\n");
    s.push_str("}\n\n");

    // Also add a raw encode/decode test (without version byte wrapper)
    s.push_str("#[test]\n");
    s.push_str(&format!("fn test_{}_encode_decode() {{\n", fn_name));
    s.push_str(&format!("    let original = create_test_{}();\n", fn_name));
    s.push_str("    \n");
    s.push_str("    // Encode to buffer\n");
    s.push_str("    let mut buffer = Vec::new();\n");
    s.push_str("    original.encode(&mut buffer).expect(\"Encode should succeed\");\n");
    s.push_str("    \n");
    s.push_str("    // Decode from buffer\n");
    s.push_str("    let mut reader = buffer.as_slice();\n");
    s.push_str(&format!(
        "    let decoded = {}::decode(&mut reader).expect(\"Decode should succeed\");\n",
        msg.name
    ));
    s.push_str("    \n");
    s.push_str("    // Compare\n");
    s.push_str("    assert_eq!(original, decoded);\n");
    s.push_str("}\n\n");

    s
}

/// Generate tests for an enum
fn generate_enum_tests(e: &EnumManifest) -> String {
    let mut s = String::new();
    let enum_name = &e.name;
    let fn_name = utils::to_snake_case(enum_name);

    if e.is_simple {
        // Test all variants of simple enum
        s.push_str("#[test]\n");
        s.push_str(&format!("fn test_{}_variants() {{\n", fn_name));

        for v in &e.variants {
            s.push_str(&format!("    // Test {}::{}\n", enum_name, v.name));
            s.push_str(&format!("    let variant = {}::{};\n", enum_name, v.name));
            s.push_str("    let mut buffer = Vec::new();\n");
            s.push_str("    variant.encode(&mut buffer).expect(\"Encode should succeed\");\n");
            s.push_str("    let mut reader = buffer.as_slice();\n");
            s.push_str(&format!(
                "    let decoded = {}::decode(&mut reader).expect(\"Decode should succeed\");\n",
                enum_name
            ));
            s.push_str("    assert_eq!(variant, decoded);\n");
            s.push_str("    \n");
        }

        s.push_str("}\n\n");

        // Test discriminant values
        s.push_str("#[test]\n");
        s.push_str(&format!("fn test_{}_discriminants() {{\n", fn_name));
        for v in &e.variants {
            s.push_str(&format!(
                "    assert_eq!({}::{} as {}, {});\n",
                enum_name, v.name, e.repr, v.discriminant
            ));
        }
        s.push_str("}\n\n");
    } else {
        // Test complex enum variants
        s.push_str("#[test]\n");
        s.push_str(&format!("fn test_{}_roundtrip() {{\n", fn_name));

        // Test unit variants
        for v in &e.variants {
            if matches!(v.data, VariantData::Unit) {
                s.push_str(&format!("    // Test {}::{}\n", enum_name, v.name));
                s.push_str(&format!("    let variant = {}::{};\n", enum_name, v.name));
                s.push_str("    let mut buffer = Vec::new();\n");
                s.push_str("    variant.encode(&mut buffer).expect(\"Encode should succeed\");\n");
                s.push_str("    let mut reader = buffer.as_slice();\n");
                s.push_str(&format!(
                    "    let decoded = {}::decode(&mut reader).expect(\"Decode should succeed\");\n",
                    enum_name
                ));
                s.push_str("    assert_eq!(variant, decoded);\n");
                s.push_str("    \n");
            }
        }

        s.push_str("}\n\n");
    }

    s
}

/// Generate router tests
fn generate_router_tests(router: &RouterManifest, manifest: &SchemaManifest) -> String {
    let mut s = String::new();
    let router_name = &router.name;
    let router_fn_name = utils::to_snake_case(router_name);

    // Test tag values are unique and correct
    s.push_str("#[test]\n");
    s.push_str(&format!("fn test_{}_tag_values() {{\n", router_fn_name));
    s.push_str("    // Verify each variant has the expected tag\n");

    for v in &router.variants {
        let tag_const = format!(
            "{}::{}_TAG",
            router_name,
            utils::to_snake_case(&v.name).to_uppercase()
        );
        s.push_str(&format!(
            "    assert_eq!({}, {}, \"Tag for {} should be {}\");\n",
            tag_const, v.discriminant, v.name, v.discriminant
        ));
    }

    s.push_str("}\n\n");

    // Test type_name_from_tag
    s.push_str("#[test]\n");
    s.push_str(&format!(
        "fn test_{}_type_name_from_tag() {{\n",
        router_fn_name
    ));

    for v in &router.variants {
        s.push_str(&format!(
            "    assert_eq!({}::type_name_from_tag({}), Some(\"{}\"));\n",
            router_name, v.discriminant, v.name
        ));
    }

    s.push_str(&format!(
        "    assert_eq!({}::type_name_from_tag(9999), None);\n",
        router_name
    ));
    s.push_str("}\n\n");

    // Test router roundtrip for each variant
    s.push_str("#[test]\n");
    s.push_str(&format!("fn test_{}_roundtrip() {{\n", router_fn_name));

    for v in &router.variants {
        // Only test non-generic message types
        if let Some(msg) = manifest.messages.iter().find(|m| m.name == v.message_type)
            && msg.generics.is_empty()
        {
            let msg_fn_name = utils::to_snake_case(&v.message_type);
            s.push_str(&format!("    // Test {}::{}\n", router_name, v.name));
            s.push_str(&format!(
                "    let msg = {}::{}(create_test_{}());\n",
                router_name, v.name, msg_fn_name
            ));
            s.push_str(&format!(
                "    assert_eq!(msg.tag(), {}::{}_TAG);\n",
                router_name,
                utils::to_snake_case(&v.name).to_uppercase()
            ));
            s.push_str("    let encoded = msg.to_bytes();\n");
            s.push_str(&format!(
                "    let decoded = {}::from_bytes(&encoded).expect(\"Decode should succeed\");\n",
                router_name
            ));
            s.push_str("    assert_eq!(msg, decoded);\n");
            s.push_str("    \n");
        }
    }

    s.push_str("}\n\n");

    // Test handler trait
    s.push_str(&format!("/// Test handler for {}\n", router_name));
    s.push_str(&format!("struct Test{}Handler {{\n", router_name));
    s.push_str("    calls: Vec<String>,\n");
    s.push_str("}\n\n");

    s.push_str(&format!(
        "impl {}Handler for Test{}Handler {{\n",
        router_name, router_name
    ));
    s.push_str("    type Output = ();\n\n");

    for v in &router.variants {
        let handler_fn = utils::to_snake_case(&v.name);
        s.push_str(&format!(
            "    fn handle_{}(&mut self, _msg: {}) -> Self::Output {{\n",
            handler_fn, v.message_type
        ));
        s.push_str(&format!(
            "        self.calls.push(\"{}\".to_string());\n",
            v.name
        ));
        s.push_str("    }\n");
    }

    s.push_str("}\n\n");

    // Test that handler is called correctly
    s.push_str("#[test]\n");
    s.push_str(&format!("fn test_{}_handler() {{\n", router_fn_name));
    s.push_str(&format!(
        "    let mut handler = Test{}Handler {{ calls: Vec::new() }};\n",
        router_name
    ));
    s.push_str("    \n");

    // Route first non-generic message
    for v in &router.variants {
        if let Some(msg) = manifest.messages.iter().find(|m| m.name == v.message_type)
            && msg.generics.is_empty()
        {
            let msg_fn_name = utils::to_snake_case(&v.message_type);
            s.push_str(&format!(
                "    let msg = {}::{}(create_test_{}());\n",
                router_name, v.name, msg_fn_name
            ));
            s.push_str("    msg.route(&mut handler);\n");
            s.push_str(&format!(
                "    assert_eq!(handler.calls.last(), Some(&\"{}\".to_string()));\n",
                v.name
            ));
            s.push_str("    \n");
            break; // Just test one for brevity
        }
    }

    s.push_str("}\n\n");

    s
}

/// Generate version byte validation tests
fn generate_version_tests(manifest: &SchemaManifest) -> String {
    let mut s = String::new();

    s.push_str(
        r#"
// ============================================================================
// Version Byte Tests
// ============================================================================

#[test]
fn test_protocol_version_byte() {
"#,
    );

    s.push_str(&format!(
        "    assert_eq!(PROTOCOL_VERSION_BYTE, 0x{:02X});\n",
        manifest.meta.version_byte
    ));

    s.push_str("}\n\n");

    s.push_str("#[test]\n");
    s.push_str("fn test_version_mismatch_rejected() {\n");
    s.push_str("    // Create a packet with wrong version byte\n");
    s.push_str("    let bad_packet = vec![0x00, 0x01, 0x02, 0x03];\n");
    s.push_str("    \n");

    // Use first non-generic message for test
    if let Some(msg) = manifest.messages.iter().find(|m| m.generics.is_empty()) {
        s.push_str(&format!(
            "    let result = {}::from_bytes(&bad_packet);\n",
            msg.name
        ));
        s.push_str(
            "    assert!(result.is_err(), \"Should reject packet with wrong version byte\");\n",
        );
    }

    s.push_str("}\n\n");

    s.push_str("#[test]\n");
    s.push_str("fn test_empty_packet_rejected() {\n");
    s.push_str("    let empty_packet: Vec<u8> = vec![];\n");

    if let Some(msg) = manifest.messages.iter().find(|m| m.generics.is_empty()) {
        s.push_str(&format!(
            "    let result = {}::from_bytes(&empty_packet);\n",
            msg.name
        ));
        s.push_str("    assert!(result.is_err(), \"Should reject empty packet\");\n");
    }

    s.push_str("}\n\n");

    s
}
