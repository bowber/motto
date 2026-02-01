//! Rust Crate Backend Emitter
//!
//! Generates a Rust crate with:
//! - Type definitions matching the schema
//! - Router enum for match-based message routing
//! - Bitcode-compatible encode/decode implementations
//! - Zero-copy packet framing

use crate::emitters::{utils, Emitter, EmitterConfig, GeneratedFile};
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
        let mut files = Vec::new();

        // Generate lib.rs with types and router
        files.push(generate_lib(&config.manifest)?);

        // Generate codec.rs with encode/decode
        files.push(generate_codec(&config.manifest)?);

        // Generate Cargo.toml
        files.push(generate_cargo_toml(&config.manifest)?);

        // Generate tests
        files.push(generate_tests(&config.manifest)?);

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

fn generate_lib(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let mut content = String::new();

    // Header
    content.push_str(&generate_rust_header(
        manifest.meta.version_byte,
        &manifest.meta.fingerprint,
        &manifest.meta.generated_at,
    ));

    content.push_str(
        r#"
#![allow(dead_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub mod codec;

#[cfg(test)]
mod tests;

/// Protocol version byte - embedded in all packets
pub const PROTOCOL_VERSION_BYTE: u8 = "#,
    );
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
    s.push_str("\n");

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
        if let Some(msg) = msg {
            if let Some(docs) = &msg.docs {
                s.push_str(&format!("    /// Handle: {}\n", docs));
            }
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

fn generate_cargo_toml(manifest: &SchemaManifest) -> Result<GeneratedFile> {
    let name = utils::to_snake_case(&manifest.meta.name);
    let content = format!(
        r#"[package]
name = "{name}_schema"
version = "0.1.0"
edition = "2021"
description = "Generated Motto SDK schema types for {name}"
license = "MIT OR Apache-2.0"

# Motto schema metadata
[package.metadata.motto]
fingerprint = "{fingerprint}"
protocol_version = {version_byte}

[dependencies]
# No dependencies required - pure Rust types

[dev-dependencies]
# For testing encode/decode roundtrips
"#,
        name = name,
        fingerprint = &manifest.meta.fingerprint[..16],
        version_byte = manifest.meta.version_byte
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
// This file was generated by motto-cli from a Rust schema definition.
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
                    return format!("{}::{}", e.name, e.variants[0].name);
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

    s.push_str(&format!("#[test]\n"));
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
    s.push_str(&format!("#[test]\n"));
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
        s.push_str(&format!("#[test]\n"));
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
        s.push_str(&format!("#[test]\n"));
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
        s.push_str(&format!("#[test]\n"));
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
    s.push_str(&format!("#[test]\n"));
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
    s.push_str(&format!("#[test]\n"));
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
    s.push_str(&format!("#[test]\n"));
    s.push_str(&format!("fn test_{}_roundtrip() {{\n", router_fn_name));

    for v in &router.variants {
        // Only test non-generic message types
        if let Some(msg) = manifest.messages.iter().find(|m| m.name == v.message_type) {
            if msg.generics.is_empty() {
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
    s.push_str(&format!("#[test]\n"));
    s.push_str(&format!("fn test_{}_handler() {{\n", router_fn_name));
    s.push_str(&format!(
        "    let mut handler = Test{}Handler {{ calls: Vec::new() }};\n",
        router_name
    ));
    s.push_str("    \n");

    // Route first non-generic message
    for v in &router.variants {
        if let Some(msg) = manifest.messages.iter().find(|m| m.name == v.message_type) {
            if msg.generics.is_empty() {
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
