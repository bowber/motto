//! Schema Parser - Static Analysis Frontend using syn
//!
//! Parses Rust source code to extract struct and enum definitions.
//! By default, includes all public structs and enums - no annotations required.

use crate::core::types::*;
use crate::core::visitor::SchemaVisitor;
use anyhow::{Context, Result};
use syn::parse_file;

/// Parser for Rust schema files
pub struct SchemaParser {
    /// Only include types with these attributes (empty = include all)
    required_attributes: Vec<String>,
    /// Only include public types
    pub_only: bool,
}

impl SchemaParser {
    /// Create a new parser with default settings (includes all public types)
    ///
    /// This aligns with Motto's "zero dependencies in schema" philosophy -
    /// you don't need `#[derive(Serialize, Deserialize)]` or any other annotations.
    pub fn new() -> Self {
        Self {
            required_attributes: Vec::new(),
            pub_only: true,
        }
    }

    /// Create a parser that requires serde or motto attributes
    ///
    /// Only includes types with:
    /// - `#[derive(Serialize)]` or `#[derive(Deserialize)]`
    /// - `#[derive(bitcode::...)]`
    /// - `#[motto]` or `#[motto::...]`
    pub fn strict() -> Self {
        Self {
            required_attributes: vec![
                "derive(Serialize".to_string(),
                "derive(Deserialize".to_string(),
                "derive(bitcode::".to_string(),
                "motto".to_string(),
            ],
            pub_only: true,
        }
    }

    /// Create a parser that includes ALL types (even private ones)
    pub fn include_all() -> Self {
        Self {
            required_attributes: Vec::new(),
            pub_only: false,
        }
    }

    /// Parse a Rust source string into a Schema
    pub fn parse(&self, source: &str) -> Result<Schema> {
        let syntax = parse_file(source).context("Failed to parse Rust source")?;

        let mut visitor = SchemaVisitor::new(&self.required_attributes, self.pub_only);
        visitor.visit_file(&syntax);

        Ok(visitor.into_schema())
    }

    /// Parse a file by path
    pub fn parse_file(&self, path: &std::path::Path) -> Result<Schema> {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        let mut schema = self.parse(&source)?;

        // Set schema name from file stem
        if let Some(stem) = path.file_stem() {
            schema.name = stem.to_string_lossy().to_string();
        }

        Ok(schema)
    }
}

impl Default for SchemaParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_struct_default_mode() {
        // Default mode: no serde required!
        let source = r#"
            /// A simple message
            pub struct Message {
                /// The message ID
                pub id: u64,
                /// The content
                pub content: String,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.structs.len(), 1);
        let msg = &schema.structs[0];
        assert_eq!(msg.name, "Message");
        assert_eq!(msg.fields.len(), 2);
        assert!(msg.serializable); // All types serializable in default mode
    }

    #[test]
    fn test_parse_ignores_private_structs() {
        let source = r#"
            /// Public message
            pub struct PublicMessage {
                pub id: u64,
            }

            /// Private helper (should be ignored)
            struct PrivateHelper {
                data: Vec<u8>,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.structs.len(), 1);
        assert_eq!(schema.structs[0].name, "PublicMessage");
    }

    #[test]
    fn test_parse_strict_mode_requires_serde() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            /// With serde - should be included
            #[derive(Serialize, Deserialize)]
            pub struct WithSerde {
                pub id: u64,
            }

            /// Without serde - should be excluded in strict mode
            pub struct WithoutSerde {
                pub id: u64,
            }
        "#;

        let parser = SchemaParser::strict();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.structs.len(), 1);
        assert_eq!(schema.structs[0].name, "WithSerde");
    }

    #[test]
    fn test_parse_enum_default_mode() {
        let source = r#"
            #[repr(u8)]
            pub enum Status {
                Pending = 0,
                Active = 1,
                Completed = 2,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.enums.len(), 1);
        let status = &schema.enums[0];
        assert_eq!(status.name, "Status");
        assert_eq!(status.variants.len(), 3);
        assert_eq!(status.repr, Some("u8".to_string()));
        assert!(status.serializable);
    }

    #[test]
    fn test_parse_complex_enum() {
        let source = r#"
            pub enum Event {
                /// Player joined
                Join { player_id: u64, name: String },
                /// Player moved
                Move(u64, f32, f32),
                /// Player left
                Leave,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.enums.len(), 1);
        let event = &schema.enums[0];
        assert_eq!(event.variants.len(), 3);

        match &event.variants[0].kind {
            VariantKind::Struct(fields) => {
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("Expected struct variant"),
        }

        match &event.variants[1].kind {
            VariantKind::Tuple(types) => {
                assert_eq!(types.len(), 3);
            }
            _ => panic!("Expected tuple variant"),
        }

        assert!(matches!(event.variants[2].kind, VariantKind::Unit));
    }

    #[test]
    fn test_parse_generics() {
        let source = r#"
            pub struct Container<T> {
                pub items: Vec<T>,
                pub metadata: Option<String>,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.structs.len(), 1);
        let container = &schema.structs[0];
        assert_eq!(container.generics.len(), 1);
        assert_eq!(container.generics[0].name, "T");
    }

    #[test]
    fn test_parse_type_aliases() {
        let source = r#"
            pub type PlayerId = u64;
            pub type RoomId = u32;
            type PrivateId = u16;  // Should be ignored (private)
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        assert_eq!(schema.type_aliases.len(), 2);
        assert_eq!(schema.type_aliases[0].name, "PlayerId");
        assert_eq!(schema.type_aliases[1].name, "RoomId");
    }

    #[test]
    fn test_include_all_mode() {
        let source = r#"
            pub struct PublicStruct { pub id: u64 }
            struct PrivateStruct { id: u64 }
        "#;

        let parser = SchemaParser::include_all();
        let schema = parser.parse(source).unwrap();

        // include_all should get both public and private
        assert_eq!(schema.structs.len(), 2);
    }
}
