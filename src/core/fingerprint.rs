//! Schema Fingerprinting using SHA-256
//!
//! Computes a deterministic fingerprint of the schema layout
//! to detect breaking changes.

use crate::core::types::Schema;
use sha2::{Digest, Sha256};

/// A deterministic fingerprint of a schema
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFingerprint {
    hash: String,
}

impl SchemaFingerprint {
    /// Compute a fingerprint from a schema
    pub fn compute(schema: &Schema) -> Self {
        let mut hasher = Sha256::new();

        // Hash schema name
        hasher.update(schema.name.as_bytes());
        hasher.update(b"\x00");

        // Hash structs in deterministic order
        let mut struct_names: Vec<_> = schema.structs.iter().map(|s| &s.name).collect();
        struct_names.sort();

        for name in struct_names {
            let s = schema.structs.iter().find(|s| &s.name == name).unwrap();
            Self::hash_struct(&mut hasher, s);
        }

        // Hash enums in deterministic order
        let mut enum_names: Vec<_> = schema.enums.iter().map(|e| &e.name).collect();
        enum_names.sort();

        for name in enum_names {
            let e = schema.enums.iter().find(|e| &e.name == name).unwrap();
            Self::hash_enum(&mut hasher, e);
        }

        // Hash type aliases
        let mut alias_names: Vec<_> = schema.type_aliases.iter().map(|t| &t.name).collect();
        alias_names.sort();

        for name in alias_names {
            let t = schema
                .type_aliases
                .iter()
                .find(|t| &t.name == name)
                .unwrap();
            hasher.update(format!("type:{}={}", t.name, type_ref_signature(&t.target)).as_bytes());
            hasher.update(b"\x00");
        }

        let result = hasher.finalize();
        Self {
            hash: hex::encode(result),
        }
    }

    fn hash_struct(hasher: &mut Sha256, s: &crate::core::types::StructDef) {
        hasher.update(format!("struct:{}", s.name).as_bytes());
        hasher.update(b"\x00");

        // Hash generics
        for g in &s.generics {
            hasher.update(format!("generic:{}", g.name).as_bytes());
            hasher.update(b"\x00");
        }

        // Hash fields in order (order matters for binary layout!)
        for field in &s.fields {
            hasher.update(
                format!(
                    "field:{}:{}:{}",
                    field.name,
                    type_ref_signature(&field.ty),
                    field.optional
                )
                .as_bytes(),
            );
            hasher.update(b"\x00");
        }

        hasher.update(b"\x01"); // Struct delimiter
    }

    fn hash_enum(hasher: &mut Sha256, e: &crate::core::types::EnumDef) {
        hasher.update(format!("enum:{}", e.name).as_bytes());
        hasher.update(b"\x00");

        // Hash repr
        if let Some(repr) = &e.repr {
            hasher.update(format!("repr:{}", repr).as_bytes());
            hasher.update(b"\x00");
        }

        // Hash generics
        for g in &e.generics {
            hasher.update(format!("generic:{}", g.name).as_bytes());
            hasher.update(b"\x00");
        }

        // Hash variants in order (order matters for discriminants!)
        for (idx, variant) in e.variants.iter().enumerate() {
            let disc = variant.discriminant.unwrap_or(idx as i64);
            hasher.update(format!("variant:{}:{}", variant.name, disc).as_bytes());

            match &variant.kind {
                crate::core::types::VariantKind::Unit => {
                    hasher.update(b":unit");
                }
                crate::core::types::VariantKind::Tuple(types) => {
                    hasher.update(b":tuple(");
                    for t in types {
                        hasher.update(type_ref_signature(t).as_bytes());
                        hasher.update(b",");
                    }
                    hasher.update(b")");
                }
                crate::core::types::VariantKind::Struct(fields) => {
                    hasher.update(b":struct{");
                    for f in fields {
                        hasher.update(
                            format!("{}:{},", f.name, type_ref_signature(&f.ty)).as_bytes(),
                        );
                    }
                    hasher.update(b"}");
                }
            }
            hasher.update(b"\x00");
        }

        hasher.update(b"\x02"); // Enum delimiter
    }

    /// Get the full hash string
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Get a short version of the hash (first 16 chars)
    pub fn short(&self) -> &str {
        &self.hash[..16]
    }

    /// Derive the protocol version byte from this fingerprint
    /// Uses first byte of hash
    pub fn version_byte(&self) -> u8 {
        u8::from_str_radix(&self.hash[..2], 16).unwrap_or(0)
    }
}

/// Create a signature string for a type reference
fn type_ref_signature(ty: &crate::core::types::TypeRef) -> String {
    let mut sig = ty.name.clone();

    if !ty.generics.is_empty() {
        sig.push('<');
        sig.push_str(
            &ty.generics
                .iter()
                .map(type_ref_signature)
                .collect::<Vec<_>>()
                .join(","),
        );
        sig.push('>');
    }

    if ty.is_ref {
        sig = format!("&{}{}", if ty.is_mut { "mut " } else { "" }, sig);
    }

    if let Some(arr) = &ty.array {
        if let Some(size) = arr.size {
            sig = format!("[{};{}]", sig, size);
        } else {
            sig = format!("[{}]", sig);
        }
    }

    sig
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::SchemaParser;

    #[test]
    fn test_fingerprint_stability() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
            pub struct Message {
                pub id: u64,
                pub content: String,
            }
        "#;

        let parser = SchemaParser::new();
        let schema1 = parser.parse(source).unwrap();
        let schema2 = parser.parse(source).unwrap();

        let fp1 = SchemaFingerprint::compute(&schema1);
        let fp2 = SchemaFingerprint::compute(&schema2);

        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_changes_on_field_change() {
        let source1 = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
            pub struct Message {
                pub id: u64,
                pub content: String,
            }
        "#;

        let source2 = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
            pub struct Message {
                pub id: u32,  // Changed from u64
                pub content: String,
            }
        "#;

        let parser = SchemaParser::new();
        let fp1 = SchemaFingerprint::compute(&parser.parse(source1).unwrap());
        let fp2 = SchemaFingerprint::compute(&parser.parse(source2).unwrap());

        assert_ne!(fp1, fp2);
    }
}
