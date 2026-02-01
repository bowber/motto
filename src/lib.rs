//! # Motto - Compiler-as-a-Service for Multi-Platform SDK Generation
//!
//! Motto transforms Rust `schema.rs` files into platform-specific SDK toolkits
//! with a unified Single Source of Truth architecture.
//!
//! ## Architecture
//!
//! 1. **Static Analysis Frontend** - Parses Rust AST using `syn` to extract
//!    structs and enums annotated for serialization. Computes SHA-256 fingerprint.
//!
//! 2. **Intermediate Representation (IR)** - Language-agnostic JSON/BSON manifest
//!    with field offsets and bit-alignment requirements for Bitcode backend.
//!
//! 3. **Backend Emitters** - Generate platform-specific code:
//!    - TypeScript/WASM (ESM with wasm-bindgen or napi-rs)
//!    - Swift (iOS/macOS native)
//!    - Kotlin (Android/JVM native)
//!    - Unity (C# with unsafe DllImport)
//!
//! 4. **Runtime** - Bundled with SDK: bitcode, zstd, state machine,
//!    retry logic, WebTransport.

pub mod core;
pub mod emitters;
pub mod ir;
pub mod runtime;

pub use core::parser::SchemaParser;
pub use core::fingerprint::SchemaFingerprint;
pub use ir::generator::IrGenerator;
pub use ir::manifest::SchemaManifest;

/// Re-export commonly used types
pub mod prelude {
    pub use crate::core::parser::SchemaParser;
    pub use crate::core::fingerprint::SchemaFingerprint;
    pub use crate::core::types::*;
    pub use crate::ir::generator::IrGenerator;
    pub use crate::ir::manifest::SchemaManifest;
    pub use crate::emitters::EmitterConfig;
    pub use crate::runtime::MottoRuntime;
}

/// Version information derived from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocol version byte (derived from minor version of motto.lock)
/// This is embedded in all generated packets for sidecar routing
pub const PROTOCOL_VERSION_BYTE: u8 = 1;
