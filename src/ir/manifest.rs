//! Schema Manifest - Language-agnostic IR
//!
//! This manifest format includes all information needed
//! for backend emitters to generate platform-specific code.

use crate::core::types::TypeRef;
use serde::{Deserialize, Serialize};

/// The complete schema manifest (IR)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaManifest {
    /// Schema metadata
    pub meta: ManifestMeta,
    /// All type definitions
    pub types: Vec<TypeDef>,
    /// All message definitions (structs marked for transport)
    pub messages: Vec<MessageDef>,
    /// All enum definitions
    pub enums: Vec<EnumManifest>,
    /// Type aliases (e.g., `type PlayerId = u64`)
    pub type_aliases: Vec<TypeAliasManifest>,
    /// Implicit router enum (auto-generated from all message types)
    /// This allows type-safe routing of incoming messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<RouterManifest>,
}

/// Metadata about the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMeta {
    /// Schema name
    pub name: String,
    /// Schema fingerprint (SHA-256)
    pub fingerprint: String,
    /// Protocol version byte
    pub version_byte: u8,
    /// Generation timestamp
    pub generated_at: String,
    /// Motto version used
    pub motto_version: String,
}

/// A type definition in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    /// Type name
    pub name: String,
    /// Kind of type
    pub kind: TypeKind,
    /// Size in bytes (if known)
    pub size: Option<usize>,
    /// Alignment requirement
    pub alignment: Option<usize>,
    /// Documentation
    pub docs: Option<String>,
    /// Generic parameters
    pub generics: Vec<String>,
}

/// Kind of type in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TypeKind {
    /// Primitive type (u8, i32, f64, bool, etc.)
    Primitive {
        rust_type: String,
        wire_type: WireType,
    },
    /// String type
    String,
    /// Optional wrapper
    Optional { inner: String },
    /// Array or Vec
    Array {
        element: String,
        fixed_size: Option<usize>,
    },
    /// HashMap or similar
    Map { key: String, value: String },
    /// Reference to another defined type
    Reference { target: String },
    /// Tuple type
    Tuple { elements: Vec<String> },
}

/// Wire type for primitive encoding
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WireType {
    /// Varint encoding (for small integers)
    Varint,
    /// Fixed 1 byte
    Fixed8,
    /// Fixed 2 bytes
    Fixed16,
    /// Fixed 4 bytes
    Fixed32,
    /// Fixed 8 bytes
    Fixed64,
    /// Fixed 16 bytes
    Fixed128,
    /// Length-prefixed bytes
    LengthPrefixed,
}

impl WireType {
    pub fn for_rust_type(ty: &str) -> Self {
        match ty {
            "u8" | "i8" | "bool" => Self::Fixed8,
            "u16" | "i16" => Self::Fixed16,
            "u32" | "i32" | "f32" | "char" => Self::Fixed32,
            "u64" | "i64" | "f64" => Self::Fixed64,
            "u128" | "i128" => Self::Fixed128,
            "usize" | "isize" => Self::Fixed64, // Assume 64-bit
            _ => Self::LengthPrefixed,
        }
    }

    pub fn byte_size(&self) -> Option<usize> {
        match self {
            Self::Fixed8 => Some(1),
            Self::Fixed16 => Some(2),
            Self::Fixed32 => Some(4),
            Self::Fixed64 => Some(8),
            Self::Fixed128 => Some(16),
            Self::Varint | Self::LengthPrefixed => None,
        }
    }
}

/// A message definition (struct intended for wire transport)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDef {
    /// Message name
    pub name: String,
    /// Fields
    pub fields: Vec<FieldManifest>,
    /// Total size in bytes (if fixed)
    pub fixed_size: Option<usize>,
    /// Minimum size in bytes
    pub min_size: usize,
    /// Maximum alignment requirement
    pub alignment: usize,
    /// Documentation
    pub docs: Option<String>,
    /// Generic parameters
    pub generics: Vec<String>,
}

/// A field in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldManifest {
    /// Field name
    pub name: String,
    /// Field index (for backward compatibility)
    pub index: usize,
    /// Type reference
    pub type_ref: String,
    /// Wire type
    pub wire_type: WireType,
    /// Offset from start of message (if fixed layout)
    pub offset: Option<usize>,
    /// Size in bytes (if fixed)
    pub size: Option<usize>,
    /// Is this field optional?
    pub optional: bool,
    /// Default value (if any)
    pub default: Option<String>,
    /// Documentation
    pub docs: Option<String>,
}

/// An enum in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumManifest {
    /// Enum name
    pub name: String,
    /// Representation type (u8, u16, etc.)
    pub repr: String,
    /// Variants
    pub variants: Vec<VariantManifest>,
    /// Is this a simple C-style enum?
    pub is_simple: bool,
    /// Documentation
    pub docs: Option<String>,
    /// Generic parameters
    pub generics: Vec<String>,
}

/// A variant in an enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantManifest {
    /// Variant name
    pub name: String,
    /// Discriminant value
    pub discriminant: i64,
    /// Variant data (if not unit)
    pub data: VariantData,
    /// Documentation
    pub docs: Option<String>,
}

/// Data associated with an enum variant
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VariantData {
    /// No data (unit variant)
    Unit,
    /// Tuple data
    Tuple { types: Vec<String> },
    /// Struct data
    Struct { fields: Vec<FieldManifest> },
}

/// Implicit router manifest - auto-generated enum for message routing
///
/// This creates a `SchemaRouter` enum that wraps all message types,
/// allowing type-safe message routing without manual enum definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterManifest {
    /// Router enum name (e.g., "ExampleSchemaRouter")
    pub name: String,
    /// All message variants
    pub variants: Vec<RouterVariant>,
    /// Documentation
    pub docs: Option<String>,
}

/// A variant in the router enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterVariant {
    /// Variant name (same as the struct name)
    pub name: String,
    /// The message type this variant wraps
    pub message_type: String,
    /// Discriminant value (auto-assigned)
    pub discriminant: u16,
    /// Documentation (copied from struct)
    pub docs: Option<String>,
}

/// Type alias manifest (e.g., `type PlayerId = u64`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAliasManifest {
    /// Alias name
    pub name: String,
    /// Target type
    pub target: String,
    /// Documentation
    pub docs: Option<String>,
}

impl SchemaManifest {
    /// Serialize to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Serialize to BSON
    pub fn to_bson(&self) -> bson::ser::Result<Vec<u8>> {
        bson::to_vec(self)
    }

    /// Deserialize from BSON
    pub fn from_bson(bytes: &[u8]) -> bson::de::Result<Self> {
        bson::from_slice(bytes)
    }
}

/// Convert a TypeRef to a string for the manifest
pub fn type_ref_to_string(ty: &TypeRef) -> String {
    let mut s = ty.name.clone();

    if !ty.generics.is_empty() {
        s.push('<');
        s.push_str(
            &ty.generics
                .iter()
                .map(type_ref_to_string)
                .collect::<Vec<_>>()
                .join(", "),
        );
        s.push('>');
    }

    if let Some(arr) = &ty.array {
        if let Some(size) = arr.size {
            s = format!("[{};{}]", s, size);
        } else {
            s = format!("[{}]", s);
        }
    }

    s
}
