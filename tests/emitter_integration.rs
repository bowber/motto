//! Integration tests for non-Rust emitters (TypeScript, Swift, Kotlin, Unity)
//!
//! These tests verify that each emitter generates the expected files
//! with correct type names and protocol version constants.

use motto::core::parser::SchemaParser;
use motto::emitters::{Emitter, EmitterConfig};
use motto::ir::generator::IrGenerator;
use tempfile::TempDir;

/// Shared test schema used by all emitter tests
const TEST_SCHEMA: &str = r#"
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

    #[repr(u8)]
    pub enum Status {
        Offline = 0,
        Online = 1,
    }
"#;

/// Helper: parse the test schema and build an EmitterConfig pointing at the given temp dir.
fn create_test_config(dir: &TempDir) -> EmitterConfig {
    let parser = SchemaParser::new();
    let schema = parser.parse(TEST_SCHEMA).unwrap();

    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema).unwrap();

    EmitterConfig {
        output_dir: dir.path().to_path_buf(),
        wasm_bindings: false,
        native_bindings: false,
        manifest,
    }
}

/// Convenience: collect generated file paths as strings for assertion.
fn file_paths(files: &[motto::emitters::GeneratedFile]) -> Vec<String> {
    files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect()
}

// ============================================================================
// TypeScript emitter tests
// ============================================================================

#[test]
fn test_typescript_emitter_generates_files() {
    use motto::emitters::typescript::TypeScriptEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = TypeScriptEmitter;
    let files = emitter.emit(&config).unwrap();
    let paths = file_paths(&files);

    assert!(paths.contains(&"src/types.ts".to_string()));
    assert!(paths.contains(&"src/codec.ts".to_string()));
    assert!(paths.contains(&"src/runtime.ts".to_string()));
    assert!(paths.contains(&"src/index.ts".to_string()));
    assert!(paths.contains(&"package.json".to_string()));
    // WASM file should NOT be present when wasm_bindings is false
    assert!(!paths.contains(&"src/wasm.ts".to_string()));
}

#[test]
fn test_typescript_emitter_content() {
    use motto::emitters::typescript::TypeScriptEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = TypeScriptEmitter;
    let files = emitter.emit(&config).unwrap();

    let types = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/types.ts")
        .expect("types.ts should exist");

    // Verify type names appear in generated content
    assert!(
        types.content.contains("Position"),
        "types.ts should contain Position type"
    );
    assert!(
        types.content.contains("Player"),
        "types.ts should contain Player type"
    );
    assert!(
        types.content.contains("Status"),
        "types.ts should contain Status enum"
    );

    // Verify protocol version byte constant is present somewhere in the generated files
    let has_version = files.iter().any(|f| f.content.contains("PROTOCOL_VERSION"));
    assert!(
        has_version,
        "generated files should contain protocol version byte constant"
    );
}

#[test]
fn test_typescript_emitter_wasm_bindings() {
    use motto::emitters::typescript::TypeScriptEmitter;

    let dir = TempDir::new().unwrap();
    let mut config = create_test_config(&dir);
    config.wasm_bindings = true;

    let emitter = TypeScriptEmitter;
    let files = emitter.emit(&config).unwrap();
    let paths = file_paths(&files);

    // All standard files should still be present
    assert!(paths.contains(&"src/types.ts".to_string()));
    assert!(paths.contains(&"src/codec.ts".to_string()));
    assert!(paths.contains(&"src/runtime.ts".to_string()));
    assert!(paths.contains(&"src/index.ts".to_string()));
    assert!(paths.contains(&"package.json".to_string()));

    // WASM bindings file should now be present
    assert!(
        paths.contains(&"src/wasm.ts".to_string()),
        "src/wasm.ts should be generated when wasm_bindings is true"
    );
}

// ============================================================================
// Swift emitter tests
// ============================================================================

#[test]
fn test_swift_emitter_generates_files() {
    use motto::emitters::swift::SwiftEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = SwiftEmitter;
    let files = emitter.emit(&config).unwrap();
    let paths = file_paths(&files);

    assert!(paths.contains(&"Sources/MottoSDK/Types.swift".to_string()));
    assert!(paths.contains(&"Sources/MottoSDK/Codec.swift".to_string()));
    assert!(paths.contains(&"Sources/MottoSDK/Runtime.swift".to_string()));
    assert!(paths.contains(&"Package.swift".to_string()));
}

#[test]
fn test_swift_emitter_content() {
    use motto::emitters::swift::SwiftEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = SwiftEmitter;
    let files = emitter.emit(&config).unwrap();

    let types = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "Sources/MottoSDK/Types.swift")
        .expect("Types.swift should exist");

    assert!(
        types.content.contains("Position"),
        "Types.swift should contain Position type"
    );
    assert!(
        types.content.contains("Player"),
        "Types.swift should contain Player type"
    );
    assert!(
        types.content.contains("Status"),
        "Types.swift should contain Status enum"
    );

    let has_version = files.iter().any(|f| f.content.contains("PROTOCOL_VERSION"));
    assert!(
        has_version,
        "generated files should contain protocol version byte constant"
    );
}

// ============================================================================
// Kotlin emitter tests
// ============================================================================

#[test]
fn test_kotlin_emitter_generates_files() {
    use motto::emitters::kotlin::KotlinEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = KotlinEmitter;
    let files = emitter.emit(&config).unwrap();
    let paths = file_paths(&files);

    assert!(paths.contains(&"src/main/kotlin/io/motto/sdk/Types.kt".to_string()));
    assert!(paths.contains(&"src/main/kotlin/io/motto/sdk/Codec.kt".to_string()));
    assert!(paths.contains(&"src/main/kotlin/io/motto/sdk/Runtime.kt".to_string()));
    assert!(paths.contains(&"build.gradle.kts".to_string()));
}

#[test]
fn test_kotlin_emitter_content() {
    use motto::emitters::kotlin::KotlinEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = KotlinEmitter;
    let files = emitter.emit(&config).unwrap();

    let types = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "src/main/kotlin/io/motto/sdk/Types.kt")
        .expect("Types.kt should exist");

    assert!(
        types.content.contains("Position"),
        "Types.kt should contain Position type"
    );
    assert!(
        types.content.contains("Player"),
        "Types.kt should contain Player type"
    );
    assert!(
        types.content.contains("Status"),
        "Types.kt should contain Status enum"
    );

    let has_version = files.iter().any(|f| f.content.contains("PROTOCOL_VERSION"));
    assert!(
        has_version,
        "generated files should contain protocol version byte constant"
    );
}

// ============================================================================
// Unity emitter tests
// ============================================================================

#[test]
fn test_unity_emitter_generates_files() {
    use motto::emitters::unity::UnityEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = UnityEmitter;
    let files = emitter.emit(&config).unwrap();
    let paths = file_paths(&files);

    assert!(paths.contains(&"Runtime/Types.cs".to_string()));
    assert!(paths.contains(&"Runtime/Codec.cs".to_string()));
    assert!(paths.contains(&"Runtime/Runtime.cs".to_string()));
    assert!(paths.contains(&"Runtime/NativeBridge.cs".to_string()));
    assert!(paths.contains(&"Motto.SDK.asmdef".to_string()));
}

#[test]
fn test_unity_emitter_content() {
    use motto::emitters::unity::UnityEmitter;

    let dir = TempDir::new().unwrap();
    let config = create_test_config(&dir);

    let emitter = UnityEmitter;
    let files = emitter.emit(&config).unwrap();

    let types = files
        .iter()
        .find(|f| f.path.to_string_lossy() == "Runtime/Types.cs")
        .expect("Types.cs should exist");

    assert!(
        types.content.contains("Position"),
        "Types.cs should contain Position type"
    );
    assert!(
        types.content.contains("Player"),
        "Types.cs should contain Player type"
    );
    assert!(
        types.content.contains("Status"),
        "Types.cs should contain Status enum"
    );

    let has_version = files.iter().any(|f| f.content.contains("Protocol Version"));
    assert!(
        has_version,
        "generated files should contain protocol version byte constant"
    );
}
