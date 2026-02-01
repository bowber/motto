//! Schema Parser - Static Analysis Frontend using syn
//!
//! Parses Rust source code to extract struct and enum definitions
//! that are annotated for serialization.

use crate::core::types::*;
use crate::core::visitor::SchemaVisitor;
use anyhow::{Context, Result};
use syn::parse_file;

/// Parser for Rust schema files
pub struct SchemaParser {
    /// Only include types with these attributes
    required_attributes: Vec<String>,
}

impl SchemaParser {
    /// Create a new parser with default settings
    pub fn new() -> Self {
        Self {
            required_attributes: vec![
                "derive(Serialize".to_string(),
                "derive(Deserialize".to_string(),
                "derive(bitcode::".to_string(),
                "motto".to_string(),
            ],
        }
    }

    /// Create a parser that includes all types regardless of attributes
    pub fn include_all() -> Self {
        Self {
            required_attributes: Vec::new(),
        }
    }

    /// Parse a Rust source string into a Schema
    pub fn parse(&self, source: &str) -> Result<Schema> {
        let syntax = parse_file(source).context("Failed to parse Rust source")?;

        let mut visitor = SchemaVisitor::new(&self.required_attributes);
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
    fn test_parse_simple_struct() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            /// A simple message
            #[derive(Serialize, Deserialize)]
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
        assert!(msg.serializable);
    }

    #[test]
    fn test_parse_enum() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
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
    }

    #[test]
    fn test_parse_complex_enum() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
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
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
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
}
