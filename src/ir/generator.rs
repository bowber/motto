//! IR Generator - Converts parsed Schema to SchemaManifest
//!
//! Computes field offsets, alignment, and wire types for the Bitcode backend.

use crate::core::fingerprint::SchemaFingerprint;
use crate::core::types::{EnumDef, FieldDef, Schema, StructDef, TypeRef, TypeSize, VariantKind};
use crate::ir::manifest::*;
use anyhow::Result;
use chrono::Utc;

/// Generator for creating IR manifests from schemas
pub struct IrGenerator {
    /// Use packed layout (no padding)
    packed: bool,
}

impl IrGenerator {
    pub fn new() -> Self {
        Self { packed: false }
    }

    /// Create a generator that produces packed layouts
    pub fn packed() -> Self {
        Self { packed: true }
    }

    /// Generate a manifest from a schema
    pub fn generate(&self, schema: &Schema) -> Result<SchemaManifest> {
        let fingerprint = SchemaFingerprint::compute(schema);

        let meta = ManifestMeta {
            name: schema.name.clone(),
            fingerprint: fingerprint.hash().to_string(),
            version_byte: fingerprint.version_byte(),
            generated_at: Utc::now().to_rfc3339(),
            motto_version: crate::VERSION.to_string(),
        };

        // Generate type definitions for all referenced types
        let mut types = Vec::new();
        self.collect_types(schema, &mut types);

        // Generate message definitions from structs
        let messages = schema
            .structs
            .iter()
            .map(|s| self.generate_message(s))
            .collect();

        // Generate enum manifests
        let enums = schema.enums.iter().map(|e| self.generate_enum(e)).collect();

        Ok(SchemaManifest {
            meta,
            types,
            messages,
            enums,
        })
    }

    fn collect_types(&self, schema: &Schema, types: &mut Vec<TypeDef>) {
        // Add primitive types
        for prim in &[
            "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64",
            "bool", "char", "usize", "isize",
        ] {
            let size = TypeSize::for_type(&TypeRef::simple(*prim));
            types.push(TypeDef {
                name: prim.to_string(),
                kind: TypeKind::Primitive {
                    rust_type: prim.to_string(),
                    wire_type: WireType::for_rust_type(prim),
                },
                size: size.map(|s| s.bytes),
                alignment: size.map(|s| s.alignment),
                docs: None,
                generics: Vec::new(),
            });
        }

        // Add String type
        types.push(TypeDef {
            name: "String".to_string(),
            kind: TypeKind::String,
            size: None,
            alignment: Some(1),
            docs: None,
            generics: Vec::new(),
        });

        // Add type definitions for structs
        for s in &schema.structs {
            types.push(TypeDef {
                name: s.name.clone(),
                kind: TypeKind::Reference {
                    target: s.name.clone(),
                },
                size: self.compute_struct_size(s),
                alignment: self.compute_struct_alignment(s),
                docs: s.docs.clone(),
                generics: s.generics.iter().map(|g| g.name.clone()).collect(),
            });
        }

        // Add type definitions for enums
        for e in &schema.enums {
            let repr_size = match e.repr.as_deref() {
                Some("u8") | Some("i8") => Some(1),
                Some("u16") | Some("i16") => Some(2),
                Some("u32") | Some("i32") => Some(4),
                Some("u64") | Some("i64") => Some(8),
                _ => None, // Complex enum, variable size
            };

            types.push(TypeDef {
                name: e.name.clone(),
                kind: TypeKind::Reference {
                    target: e.name.clone(),
                },
                size: repr_size,
                alignment: repr_size,
                docs: e.docs.clone(),
                generics: e.generics.iter().map(|g| g.name.clone()).collect(),
            });
        }
    }

    fn generate_message(&self, s: &StructDef) -> MessageDef {
        let mut fields = Vec::new();
        let mut current_offset = 0usize;
        let mut max_alignment = 1usize;
        let mut all_fixed = true;

        for (idx, field) in s.fields.iter().enumerate() {
            let wire_type = self.wire_type_for_field(field);
            let field_size = wire_type.byte_size();
            let field_alignment = field_size.unwrap_or(1);

            // Compute offset with alignment
            let offset = if self.packed {
                current_offset
            } else {
                align_to(current_offset, field_alignment)
            };

            if field_size.is_none() {
                all_fixed = false;
            }

            fields.push(FieldManifest {
                name: field.name.clone(),
                index: idx,
                type_ref: type_ref_to_string(&field.ty),
                wire_type,
                offset: if all_fixed { Some(offset) } else { None },
                size: field_size,
                optional: field.optional,
                default: field.default.clone(),
                docs: field.docs.clone(),
            });

            if let Some(size) = field_size {
                current_offset = offset + size;
            }
            max_alignment = max_alignment.max(field_alignment);
        }

        // Final size with trailing padding (only for non-packed)
        let fixed_size = if all_fixed {
            if self.packed {
                Some(current_offset)
            } else {
                Some(align_to(current_offset, max_alignment))
            }
        } else {
            None
        };

        // Minimum size: sum of minimum sizes for each field
        let min_size = fields
            .iter()
            .map(|f| {
                if f.optional {
                    1 // Just the presence byte
                } else {
                    f.size.unwrap_or(1) // At least 1 byte for length prefix
                }
            })
            .sum();

        MessageDef {
            name: s.name.clone(),
            fields,
            fixed_size,
            min_size,
            alignment: max_alignment,
            docs: s.docs.clone(),
            generics: s.generics.iter().map(|g| g.name.clone()).collect(),
        }
    }

    fn generate_enum(&self, e: &EnumDef) -> EnumManifest {
        let repr = e.repr.clone().unwrap_or_else(|| "u8".to_string());

        let is_simple = e
            .variants
            .iter()
            .all(|v| matches!(v.kind, VariantKind::Unit));

        let variants = e
            .variants
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let discriminant = v.discriminant.unwrap_or(idx as i64);

                let data = match &v.kind {
                    VariantKind::Unit => VariantData::Unit,
                    VariantKind::Tuple(types) => VariantData::Tuple {
                        types: types.iter().map(type_ref_to_string).collect(),
                    },
                    VariantKind::Struct(fields) => VariantData::Struct {
                        fields: fields
                            .iter()
                            .enumerate()
                            .map(|(i, f)| FieldManifest {
                                name: f.name.clone(),
                                index: i,
                                type_ref: type_ref_to_string(&f.ty),
                                wire_type: self.wire_type_for_field(f),
                                offset: None,
                                size: None,
                                optional: f.optional,
                                default: f.default.clone(),
                                docs: f.docs.clone(),
                            })
                            .collect(),
                    },
                };

                VariantManifest {
                    name: v.name.clone(),
                    discriminant,
                    data,
                    docs: v.docs.clone(),
                }
            })
            .collect();

        EnumManifest {
            name: e.name.clone(),
            repr,
            variants,
            is_simple,
            docs: e.docs.clone(),
            generics: e.generics.iter().map(|g| g.name.clone()).collect(),
        }
    }

    fn wire_type_for_field(&self, field: &FieldDef) -> WireType {
        self.wire_type_for_type(&field.ty)
    }

    fn wire_type_for_type(&self, ty: &TypeRef) -> WireType {
        if ty.is_primitive() {
            WireType::for_rust_type(&ty.name)
        } else if ty.is_string() {
            WireType::LengthPrefixed
        } else if ty.is_option() {
            // Optional: 1 byte tag + optional value
            WireType::LengthPrefixed
        } else if ty.is_collection() {
            WireType::LengthPrefixed
        } else {
            // Complex type: use length-prefixed encoding
            WireType::LengthPrefixed
        }
    }

    fn compute_struct_size(&self, s: &StructDef) -> Option<usize> {
        let mut size = 0usize;
        let mut max_align = 1usize;

        for field in &s.fields {
            let field_size = TypeSize::for_type(&field.ty)?;

            if !self.packed {
                size = align_to(size, field_size.alignment);
            }

            size += field_size.bytes;
            max_align = max_align.max(field_size.alignment);
        }

        Some(align_to(size, max_align))
    }

    fn compute_struct_alignment(&self, s: &StructDef) -> Option<usize> {
        if self.packed {
            return Some(1);
        }

        s.fields
            .iter()
            .filter_map(|f| TypeSize::for_type(&f.ty).map(|s| s.alignment))
            .max()
    }
}

impl Default for IrGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Align a value to the given alignment
fn align_to(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) / alignment * alignment
}

// Import chrono for timestamp generation
use chrono;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::SchemaParser;

    #[test]
    fn test_generate_manifest() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
            pub struct Player {
                pub id: u64,
                pub x: f32,
                pub y: f32,
                pub health: u8,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        let generator = IrGenerator::new();
        let manifest = generator.generate(&schema).unwrap();

        assert_eq!(manifest.messages.len(), 1);
        let player = &manifest.messages[0];
        assert_eq!(player.name, "Player");
        assert_eq!(player.fields.len(), 4);

        // Check field offsets (with natural alignment)
        // u64 (8 bytes, align 8) at offset 0
        // f32 (4 bytes, align 4) at offset 8
        // f32 (4 bytes, align 4) at offset 12
        // u8 (1 byte, align 1) at offset 16
        // Total: 17 bytes, padded to 24 for u64 alignment
        assert_eq!(player.fields[0].offset, Some(0));
        assert_eq!(player.fields[1].offset, Some(8));
        assert_eq!(player.fields[2].offset, Some(12));
        assert_eq!(player.fields[3].offset, Some(16));
        assert_eq!(player.fixed_size, Some(24));
    }

    #[test]
    fn test_packed_layout() {
        let source = r#"
            use serde::{Serialize, Deserialize};

            #[derive(Serialize, Deserialize)]
            pub struct Packet {
                pub version: u8,
                pub id: u64,
                pub flags: u8,
            }
        "#;

        let parser = SchemaParser::new();
        let schema = parser.parse(source).unwrap();

        let generator = IrGenerator::packed();
        let manifest = generator.generate(&schema).unwrap();

        let packet = &manifest.messages[0];
        assert_eq!(packet.fields[0].offset, Some(0)); // u8 at 0
        assert_eq!(packet.fields[1].offset, Some(1)); // u64 at 1 (packed!)
        assert_eq!(packet.fields[2].offset, Some(9)); // u8 at 9
        assert_eq!(packet.fixed_size, Some(10)); // Total: 10 bytes
    }
}
