//! Integration tests for the Rust emitter
//!
//! These tests verify that the Rust emitter generates valid, compilable code
//! and that the generated tests pass.

use motto::core::parser::SchemaParser;
use motto::emitters::rust::RustEmitter;
use motto::emitters::{Emitter, EmitterConfig};
use motto::ir::generator::IrGenerator;
use std::process::Command;
use tempfile::TempDir;

/// Test that the Rust emitter generates valid code for a simple schema
#[test]
fn test_rust_emitter_simple_schema() {
    let source = r#"
        pub struct Position {
            pub x: f32,
            pub y: f32,
        }

        pub struct Player {
            pub id: u64,
            pub name: String,
            pub position: Position,
            pub health: u8,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    let emitter = RustEmitter;
    let files = emitter.emit(&config).unwrap();

    // Verify expected files are generated
    let file_paths: Vec<_> = files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert!(file_paths.contains(&"src/lib.rs".to_string()));
    assert!(file_paths.contains(&"src/codec.rs".to_string()));
    assert!(file_paths.contains(&"src/tests.rs".to_string()));
    assert!(file_paths.contains(&"Cargo.toml".to_string()));

    // Verify lib.rs contains expected content
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/lib.rs")
        .unwrap();
    assert!(lib_rs.content.contains("pub struct Position"));
    assert!(lib_rs.content.contains("pub struct Player"));
    assert!(lib_rs.content.contains("pub const PROTOCOL_VERSION_BYTE"));
    assert!(lib_rs.content.contains("mod tests"));
}

/// Test that the Rust emitter generates router enum
#[test]
fn test_rust_emitter_generates_router() {
    let source = r#"
        pub struct Message1 {
            pub data: String,
        }

        pub struct Message2 {
            pub value: u32,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    let emitter = RustEmitter;
    let files = emitter.emit(&config).unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/lib.rs")
        .unwrap();

    // Check router enum is generated (name comes from schema.name which defaults to "schema")
    assert!(lib_rs.content.contains("pub enum SchemaRouter"));
    assert!(lib_rs.content.contains("Message1(Message1)"));
    assert!(lib_rs.content.contains("Message2(Message2)"));

    // Check handler trait is generated
    // Note: Message1 -> message1 (snake_case doesn't insert underscore before numbers)
    assert!(lib_rs.content.contains("pub trait SchemaRouterHandler"));
    assert!(lib_rs.content.contains("fn handle_message1"));
    assert!(lib_rs.content.contains("fn handle_message2"));
}

/// Test that generic types are excluded from router
#[test]
fn test_rust_emitter_excludes_generics_from_router() {
    let source = r#"
        pub struct SimpleMessage {
            pub data: String,
        }

        pub struct GenericWrapper<T> {
            pub item: T,
            pub count: u32,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    let emitter = RustEmitter;
    let files = emitter.emit(&config).unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/lib.rs")
        .unwrap();

    // Check router contains SimpleMessage but not GenericWrapper
    assert!(lib_rs.content.contains("SimpleMessage(SimpleMessage)"));
    assert!(!lib_rs.content.contains("GenericWrapper(GenericWrapper)"));
}

/// Test that enums are properly generated
#[test]
fn test_rust_emitter_enum_generation() {
    let source = r#"
        #[repr(u8)]
        pub enum Status {
            Pending = 0,
            Active = 1,
            Completed = 2,
        }

        pub enum Event {
            Started,
            Updated { value: u32 },
            Finished(String),
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    let emitter = RustEmitter;
    let files = emitter.emit(&config).unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/lib.rs")
        .unwrap();

    // Check simple enum
    assert!(lib_rs.content.contains("#[repr(u8)]"));
    assert!(lib_rs.content.contains("pub enum Status"));
    assert!(lib_rs.content.contains("Pending = 0"));
    assert!(lib_rs.content.contains("Active = 1"));
    assert!(lib_rs.content.contains("Completed = 2"));

    // Check complex enum
    assert!(lib_rs.content.contains("pub enum Event"));
    assert!(lib_rs.content.contains("Started"));
    assert!(lib_rs.content.contains("Updated {"));
    assert!(lib_rs.content.contains("Finished(String)"));
}

/// Test that type aliases are generated
#[test]
fn test_rust_emitter_type_aliases() {
    let source = r#"
        pub type UserId = u64;
        pub type Score = u32;

        pub struct User {
            pub id: UserId,
            pub score: Score,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    let emitter = RustEmitter;
    let files = emitter.emit(&config).unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/lib.rs")
        .unwrap();

    assert!(lib_rs.content.contains("pub type UserId = u64"));
    assert!(lib_rs.content.contains("pub type Score = u32"));
}

/// Test that generated code actually compiles (requires cargo)
#[test]
#[ignore] // Run with: cargo test -- --ignored
fn test_rust_emitter_compiles() {
    let source = r#"
        pub type PlayerId = u64;

        #[repr(u8)]
        pub enum Status {
            Offline = 0,
            Online = 1,
        }

        pub struct Position {
            pub x: f32,
            pub y: f32,
        }

        pub struct Player {
            pub id: PlayerId,
            pub name: String,
            pub position: Position,
            pub status: Status,
            pub score: Option<u32>,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    std::fs::create_dir_all(&rust_dir).unwrap();

    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    // Use the emit function to write files
    motto::emitters::rust::emit(&config).unwrap();

    // Try to compile the generated code
    let output = Command::new("cargo")
        .args(["build"])
        .current_dir(&rust_dir)
        .output()
        .expect("Failed to run cargo build");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("Generated Rust code failed to compile");
    }
}

/// Test that generated tests pass (requires cargo)
#[test]
#[ignore] // Run with: cargo test -- --ignored
fn test_rust_emitter_tests_pass() {
    let source = r#"
        pub struct Position {
            pub x: f32,
            pub y: f32,
        }

        pub struct Player {
            pub id: u64,
            pub name: String,
            pub position: Position,
            pub health: u8,
        }
    "#;

    let parser = SchemaParser::new();
    let schema = parser.parse(source).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    std::fs::create_dir_all(&rust_dir).unwrap();

    let config = EmitterConfig {
        output_dir: temp_dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    };

    motto::emitters::rust::emit(&config).unwrap();

    // Run the generated tests
    let output = Command::new("cargo")
        .args(["test"])
        .current_dir(&rust_dir)
        .output()
        .expect("Failed to run cargo test");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("Generated Rust tests failed");
    }
}
