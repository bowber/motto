//! Type definitions for parsed schema elements

use serde::{Deserialize, Serialize};

/// A complete parsed schema containing all types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Name of the schema (derived from module name or file)
    pub name: String,
    /// All struct definitions
    pub structs: Vec<StructDef>,
    /// All enum definitions
    pub enums: Vec<EnumDef>,
    /// All type aliases
    pub type_aliases: Vec<TypeAlias>,
    /// Documentation comments
    pub docs: Option<String>,
}

impl Schema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            structs: Vec::new(),
            enums: Vec::new(),
            type_aliases: Vec::new(),
            docs: None,
        }
    }
}

/// A struct definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    /// Struct name
    pub name: String,
    /// Fields in the struct
    pub fields: Vec<FieldDef>,
    /// Attributes on the struct
    pub attributes: Vec<Attribute>,
    /// Documentation
    pub docs: Option<String>,
    /// Generics if any
    pub generics: Vec<GenericParam>,
    /// Is this struct marked for serialization?
    pub serializable: bool,
}

/// A field in a struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    /// Field name
    pub name: String,
    /// Field type
    pub ty: TypeRef,
    /// Field attributes
    pub attributes: Vec<Attribute>,
    /// Documentation
    pub docs: Option<String>,
    /// Is this field optional?
    pub optional: bool,
    /// Default value expression (if any)
    pub default: Option<String>,
}

/// An enum definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    /// Enum name
    pub name: String,
    /// Variants
    pub variants: Vec<EnumVariant>,
    /// Attributes
    pub attributes: Vec<Attribute>,
    /// Documentation
    pub docs: Option<String>,
    /// Generics if any
    pub generics: Vec<GenericParam>,
    /// Is this enum marked for serialization?
    pub serializable: bool,
    /// Representation hint (u8, i32, etc.)
    pub repr: Option<String>,
}

/// A variant of an enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    /// Variant name
    pub name: String,
    /// Variant kind
    pub kind: VariantKind,
    /// Discriminant value (for C-style enums)
    pub discriminant: Option<i64>,
    /// Attributes
    pub attributes: Vec<Attribute>,
    /// Documentation
    pub docs: Option<String>,
}

/// Kind of enum variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VariantKind {
    /// Unit variant: `Foo`
    Unit,
    /// Tuple variant: `Foo(i32, String)`
    Tuple(Vec<TypeRef>),
    /// Struct variant: `Foo { x: i32, y: String }`
    Struct(Vec<FieldDef>),
}

/// A type reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    /// The base type name
    pub name: String,
    /// Generic arguments if any
    pub generics: Vec<TypeRef>,
    /// Is this a reference?
    pub is_ref: bool,
    /// Is this mutable?
    pub is_mut: bool,
    /// Array/slice info
    pub array: Option<ArrayInfo>,
}

impl TypeRef {
    pub fn simple(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            generics: Vec::new(),
            is_ref: false,
            is_mut: false,
            array: None,
        }
    }

    pub fn with_generics(name: impl Into<String>, generics: Vec<TypeRef>) -> Self {
        Self {
            name: name.into(),
            generics,
            is_ref: false,
            is_mut: false,
            array: None,
        }
    }

    /// Check if this type is a primitive
    pub fn is_primitive(&self) -> bool {
        matches!(
            self.name.as_str(),
            "u8" | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "f32"
                | "f64"
                | "bool"
                | "char"
        )
    }

    /// Check if this type is a string type
    pub fn is_string(&self) -> bool {
        matches!(self.name.as_str(), "String" | "str" | "&str")
    }

    /// Check if this type is a collection
    pub fn is_collection(&self) -> bool {
        matches!(
            self.name.as_str(),
            "Vec" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "VecDeque"
        )
    }

    /// Check if this type is Option
    pub fn is_option(&self) -> bool {
        self.name == "Option"
    }

    /// Check if this type is Result
    pub fn is_result(&self) -> bool {
        self.name == "Result"
    }
}

/// Array/slice information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayInfo {
    /// Fixed size (for arrays) or None (for slices/Vec)
    pub size: Option<usize>,
}

/// A type alias
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAlias {
    /// Alias name
    pub name: String,
    /// Target type
    pub target: TypeRef,
    /// Documentation
    pub docs: Option<String>,
}

/// An attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    /// Attribute path (e.g., "serde" or "motto::field")
    pub path: String,
    /// Arguments if any
    pub args: Option<String>,
}

impl Attribute {
    pub fn is_serde(&self) -> bool {
        self.path == "serde" || self.path.starts_with("serde::")
    }

    pub fn is_motto(&self) -> bool {
        self.path == "motto" || self.path.starts_with("motto::")
    }

    pub fn is_repr(&self) -> bool {
        self.path == "repr"
    }
}

/// A generic parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    /// Parameter name
    pub name: String,
    /// Bounds
    pub bounds: Vec<String>,
}

/// Primitive type sizes for bit-alignment calculations
#[derive(Debug, Clone, Copy)]
pub struct TypeSize {
    pub bytes: usize,
    pub alignment: usize,
}

impl TypeSize {
    pub fn for_type(ty: &TypeRef) -> Option<Self> {
        match ty.name.as_str() {
            "u8" | "i8" | "bool" => Some(Self {
                bytes: 1,
                alignment: 1,
            }),
            "u16" | "i16" => Some(Self {
                bytes: 2,
                alignment: 2,
            }),
            "u32" | "i32" | "f32" | "char" => Some(Self {
                bytes: 4,
                alignment: 4,
            }),
            "u64" | "i64" | "f64" => Some(Self {
                bytes: 8,
                alignment: 8,
            }),
            "u128" | "i128" => Some(Self {
                bytes: 16,
                alignment: 16,
            }),
            "usize" | "isize" => Some(Self {
                bytes: 8,
                alignment: 8,
            }), // Assume 64-bit
            _ => None,
        }
    }
}
