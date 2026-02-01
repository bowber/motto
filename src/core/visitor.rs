//! AST Visitor for extracting schema definitions

use crate::core::types::*;
use syn::{
    Attribute as SynAttr, Expr, Fields, File, GenericParam as SynGenericParam, Item, Type, Variant,
    Visibility,
};

/// Visitor that extracts schema information from syn AST
pub struct SchemaVisitor {
    schema: Schema,
    required_attrs: Vec<String>,
    /// Only include public types
    pub_only: bool,
}

impl SchemaVisitor {
    pub fn new(required_attrs: &[String], pub_only: bool) -> Self {
        Self {
            schema: Schema::new("schema"),
            required_attrs: required_attrs.to_vec(),
            pub_only,
        }
    }

    pub fn visit_file(&mut self, file: &File) {
        // Extract module-level docs
        self.schema.docs = extract_docs(&file.attrs);

        for item in &file.items {
            match item {
                Item::Struct(s) => self.visit_struct(s),
                Item::Enum(e) => self.visit_enum(e),
                Item::Type(t) => self.visit_type_alias(t),
                _ => {}
            }
        }
    }

    fn visit_struct(&mut self, s: &syn::ItemStruct) {
        // Check visibility
        if self.pub_only && !is_public(&s.vis) {
            return;
        }

        let attrs = convert_attributes(&s.attrs);
        let serializable = self.check_serializable(&attrs);

        // Skip if we require specific attributes and none match
        if !self.required_attrs.is_empty() && !serializable {
            return;
        }

        let fields = match &s.fields {
            Fields::Named(named) => named
                .named
                .iter()
                .map(|f| FieldDef {
                    name: f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default(),
                    ty: convert_type(&f.ty),
                    attributes: convert_attributes(&f.attrs),
                    docs: extract_docs(&f.attrs),
                    optional: is_optional_type(&f.ty),
                    default: extract_default(&f.attrs),
                })
                .collect(),
            Fields::Unnamed(unnamed) => unnamed
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, f)| FieldDef {
                    name: format!("_{}", i),
                    ty: convert_type(&f.ty),
                    attributes: convert_attributes(&f.attrs),
                    docs: extract_docs(&f.attrs),
                    optional: is_optional_type(&f.ty),
                    default: None,
                })
                .collect(),
            Fields::Unit => Vec::new(),
        };

        let generics = convert_generics(&s.generics);

        self.schema.structs.push(StructDef {
            name: s.ident.to_string(),
            fields,
            attributes: attrs,
            docs: extract_docs(&s.attrs),
            generics,
            // In default mode (no required attrs), all types are considered serializable
            serializable: self.required_attrs.is_empty() || serializable,
        });
    }

    fn visit_enum(&mut self, e: &syn::ItemEnum) {
        // Check visibility
        if self.pub_only && !is_public(&e.vis) {
            return;
        }

        let attrs = convert_attributes(&e.attrs);
        let serializable = self.check_serializable(&attrs);

        // Skip if we require specific attributes and none match
        if !self.required_attrs.is_empty() && !serializable {
            return;
        }

        let variants = e.variants.iter().map(|v| convert_variant(v)).collect();

        let generics = convert_generics(&e.generics);
        let repr = extract_repr(&e.attrs);

        self.schema.enums.push(EnumDef {
            name: e.ident.to_string(),
            variants,
            attributes: attrs,
            docs: extract_docs(&e.attrs),
            generics,
            // In default mode (no required attrs), all types are considered serializable
            serializable: self.required_attrs.is_empty() || serializable,
            repr,
        });
    }

    fn visit_type_alias(&mut self, t: &syn::ItemType) {
        // Check visibility for type aliases too
        if self.pub_only && !is_public(&t.vis) {
            return;
        }

        self.schema.type_aliases.push(TypeAlias {
            name: t.ident.to_string(),
            target: convert_type(&t.ty),
            docs: extract_docs(&t.attrs),
        });
    }

    fn check_serializable(&self, attrs: &[Attribute]) -> bool {
        // Check for derive(Serialize, Deserialize) or motto attribute
        for attr in attrs {
            // Check derive macro for Serialize/Deserialize
            if attr.path == "derive" {
                if let Some(args) = &attr.args {
                    if args.contains("Serialize")
                        || args.contains("Deserialize")
                        || args.contains("bitcode")
                    {
                        return true;
                    }
                }
            }
            // Check for motto attribute
            if attr.path == "motto" || attr.path.starts_with("motto::") {
                return true;
            }
        }
        false
    }

    pub fn into_schema(self) -> Schema {
        self.schema
    }
}

fn convert_attributes(attrs: &[SynAttr]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter_map(|attr| {
            let path = attr
                .path()
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            // Skip doc attributes (handled separately)
            if path == "doc" {
                return None;
            }

            let args = match &attr.meta {
                syn::Meta::Path(_) => None,
                syn::Meta::List(list) => Some(list.tokens.to_string()),
                syn::Meta::NameValue(nv) => Some(nv.value.to_token_stream().to_string()),
            };

            Some(Attribute { path, args })
        })
        .collect()
}

fn extract_docs(attrs: &[SynAttr]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &attr.meta {
                    if let Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn extract_repr(attrs: &[SynAttr]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            if let syn::Meta::List(list) = &attr.meta {
                return Some(list.tokens.to_string());
            }
        }
    }
    None
}

fn extract_default(attrs: &[SynAttr]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("serde") {
            if let syn::Meta::List(list) = &attr.meta {
                let tokens = list.tokens.to_string();
                if tokens.contains("default") {
                    // Extract the default value if specified
                    if let Some(start) = tokens.find("default = ") {
                        let rest = &tokens[start + 10..];
                        if let Some(end) = rest.find([',', ')']) {
                            return Some(rest[..end].trim_matches('"').to_string());
                        }
                    }
                    return Some("Default::default()".to_string());
                }
            }
        }
    }
    None
}

fn convert_type(ty: &Type) -> TypeRef {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<_> = type_path.path.segments.iter().collect();

            if segments.is_empty() {
                return TypeRef::simple("unknown");
            }

            let last = segments.last().unwrap();
            let name = last.ident.to_string();

            let generics = match &last.arguments {
                syn::PathArguments::AngleBracketed(args) => args
                    .args
                    .iter()
                    .filter_map(|arg| {
                        if let syn::GenericArgument::Type(t) = arg {
                            Some(convert_type(t))
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            };

            TypeRef {
                name,
                generics,
                is_ref: false,
                is_mut: false,
                array: None,
            }
        }
        Type::Reference(ref_type) => {
            let mut inner = convert_type(&ref_type.elem);
            inner.is_ref = true;
            inner.is_mut = ref_type.mutability.is_some();
            inner
        }
        Type::Array(arr) => {
            let mut inner = convert_type(&arr.elem);
            inner.array = Some(ArrayInfo {
                size: extract_array_size(&arr.len),
            });
            inner
        }
        Type::Slice(slice) => {
            let mut inner = convert_type(&slice.elem);
            inner.array = Some(ArrayInfo { size: None });
            inner
        }
        Type::Tuple(tuple) => {
            if tuple.elems.is_empty() {
                TypeRef::simple("()")
            } else {
                TypeRef {
                    name: "tuple".to_string(),
                    generics: tuple.elems.iter().map(convert_type).collect(),
                    is_ref: false,
                    is_mut: false,
                    array: None,
                }
            }
        }
        _ => TypeRef::simple("unknown"),
    }
}

fn extract_array_size(expr: &Expr) -> Option<usize> {
    if let Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(i),
        ..
    }) = expr
    {
        i.base10_parse().ok()
    } else {
        None
    }
}

fn is_optional_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(last) = type_path.path.segments.last() {
            return last.ident == "Option";
        }
    }
    false
}

fn convert_variant(v: &Variant) -> EnumVariant {
    let kind = match &v.fields {
        Fields::Unit => VariantKind::Unit,
        Fields::Unnamed(unnamed) => VariantKind::Tuple(
            unnamed
                .unnamed
                .iter()
                .map(|f| convert_type(&f.ty))
                .collect(),
        ),
        Fields::Named(named) => VariantKind::Struct(
            named
                .named
                .iter()
                .map(|f| FieldDef {
                    name: f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default(),
                    ty: convert_type(&f.ty),
                    attributes: convert_attributes(&f.attrs),
                    docs: extract_docs(&f.attrs),
                    optional: is_optional_type(&f.ty),
                    default: None,
                })
                .collect(),
        ),
    };

    let discriminant = v.discriminant.as_ref().and_then(|(_, expr)| {
        if let Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(i),
            ..
        }) = expr
        {
            i.base10_parse().ok()
        } else {
            None
        }
    });

    EnumVariant {
        name: v.ident.to_string(),
        kind,
        discriminant,
        attributes: convert_attributes(&v.attrs),
        docs: extract_docs(&v.attrs),
    }
}

fn convert_generics(generics: &syn::Generics) -> Vec<GenericParam> {
    generics
        .params
        .iter()
        .filter_map(|p| {
            if let SynGenericParam::Type(tp) = p {
                let bounds: Vec<String> = tp
                    .bounds
                    .iter()
                    .map(|b| b.to_token_stream().to_string())
                    .collect();
                Some(GenericParam {
                    name: tp.ident.to_string(),
                    bounds,
                })
            } else {
                None
            }
        })
        .collect()
}

use quote::ToTokens;

/// Check if a visibility is public (pub, pub(crate), pub(super), etc.)
fn is_public(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}
