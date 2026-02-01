//! Core module - Static Analysis Frontend
//!
//! Uses `syn` to parse Rust AST and extract schema definitions.

pub mod fingerprint;
pub mod parser;
pub mod types;
pub mod visitor;
