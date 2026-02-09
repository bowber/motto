use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

// Helper to get a motto command
#[allow(deprecated)]
fn motto() -> Command {
    Command::cargo_bin("motto").unwrap()
}

#[test]
fn test_cli_version() {
    motto()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("motto"));
}

#[test]
fn test_cli_help() {
    motto()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Generate multi-platform SDKs"));
}

#[test]
fn test_cli_init() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();

    assert!(dir.path().join("src/schema.rs").exists());
    assert!(dir.path().join("motto.lock").exists());
    assert!(dir.path().join("generated").is_dir());
}

#[test]
fn test_cli_generate_all_targets() {
    let dir = TempDir::new().unwrap();
    // Init first
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();
    // Generate
    motto()
        .args([
            "generate",
            "--schema",
            dir.path().join("src/schema.rs").to_str().unwrap(),
            "--output",
            dir.path().join("generated").to_str().unwrap(),
        ])
        .assert()
        .success();

    // Check all target dirs exist (default features include all emitters)
    #[cfg(feature = "emitter-typescript")]
    assert!(dir.path().join("generated/typescript").is_dir());
    #[cfg(feature = "emitter-swift")]
    assert!(dir.path().join("generated/swift").is_dir());
    #[cfg(feature = "emitter-kotlin")]
    assert!(dir.path().join("generated/kotlin").is_dir());
    #[cfg(feature = "emitter-unity")]
    assert!(dir.path().join("generated/unity").is_dir());
    assert!(dir.path().join("generated/rust").is_dir());
}

#[cfg(feature = "emitter-typescript")]
#[test]
fn test_cli_generate_specific_targets() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();
    motto()
        .args([
            "generate",
            "--schema",
            dir.path().join("src/schema.rs").to_str().unwrap(),
            "--output",
            dir.path().join("generated").to_str().unwrap(),
            "--targets",
            "rust,typescript",
        ])
        .assert()
        .success();

    assert!(dir.path().join("generated/typescript").is_dir());
    assert!(dir.path().join("generated/rust").is_dir());
}

#[test]
fn test_cli_lock_and_check() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();

    let schema = dir.path().join("src/schema.rs");
    let lock = dir.path().join("motto.lock");

    // Lock the schema
    motto()
        .args([
            "lock",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--bump",
            "patch",
        ])
        .assert()
        .success();

    // Check should pass
    motto()
        .args([
            "check",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Strict check should also pass
    motto()
        .args([
            "check",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--strict",
        ])
        .assert()
        .success();
}

#[test]
fn test_cli_check_detects_mismatch() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();

    let schema = dir.path().join("src/schema.rs");
    let lock = dir.path().join("motto.lock");

    // Lock the schema
    motto()
        .args([
            "lock",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--bump",
            "patch",
        ])
        .assert()
        .success();

    // Modify schema to cause mismatch
    let mut content = fs::read_to_string(&schema).unwrap();
    content.push_str("\npub struct ExtraType { pub value: u64 }\n");
    fs::write(&schema, content).unwrap();

    // Non-strict check should still succeed (just warns)
    motto()
        .args([
            "check",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Strict check should fail
    motto()
        .args([
            "check",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--strict",
        ])
        .assert()
        .failure();
}

#[test]
fn test_cli_lock_version_bumps() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();

    let schema = dir.path().join("src/schema.rs");
    let lock = dir.path().join("motto.lock");

    // Patch bump: 0.1.0 -> 0.1.1
    motto()
        .args([
            "lock",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--bump",
            "patch",
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&lock).unwrap();
    assert!(content.contains("patch = 1"));

    // Minor bump: 0.1.1 -> 0.2.0
    motto()
        .args([
            "lock",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--bump",
            "minor",
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&lock).unwrap();
    assert!(content.contains("minor = 2"));

    // Major bump: 0.2.0 -> 1.0.0
    motto()
        .args([
            "lock",
            "--schema",
            schema.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--bump",
            "major",
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&lock).unwrap();
    assert!(content.contains("major = 1"));
}

#[test]
fn test_cli_generate_invalid_target() {
    let dir = TempDir::new().unwrap();
    motto()
        .args(["init", "--path", dir.path().to_str().unwrap()])
        .assert()
        .success();

    motto()
        .args([
            "generate",
            "--schema",
            dir.path().join("src/schema.rs").to_str().unwrap(),
            "--output",
            dir.path().join("generated").to_str().unwrap(),
            "--targets",
            "invalid_target",
        ])
        .assert()
        .failure();
}

#[test]
fn test_cli_generate_missing_schema() {
    let dir = TempDir::new().unwrap();
    motto()
        .args([
            "generate",
            "--schema",
            dir.path().join("nonexistent.rs").to_str().unwrap(),
            "--output",
            dir.path().join("generated").to_str().unwrap(),
        ])
        .assert()
        .failure();
}

// ─── Sniff command tests ─────────────────────────────────────────────────────

#[test]
fn test_cli_sniff_help() {
    motto()
        .args(["sniff", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("transport traffic"));
}

#[test]
fn test_cli_sniff_unknown_mode() {
    motto()
        .args([
            "sniff",
            "--mode",
            "invalid",
            "--upstream",
            "ws://localhost:9999",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unknown sniff mode"));
}

#[test]
fn test_cli_sniff_unknown_format() {
    motto()
        .args([
            "sniff",
            "--format",
            "xml",
            "--upstream",
            "ws://localhost:9999",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown format"));
}

#[test]
fn test_cli_sniff_tap_requires_upstream() {
    motto()
        .args(["sniff", "--mode", "tap"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--upstream"));
}

#[test]
fn test_cli_sniff_proxy_requires_upstream() {
    motto()
        .args(["sniff", "--mode", "proxy"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--upstream"));
}

#[test]
fn test_cli_sniff_decode_disabled_with_no_decode() {
    // --no-decode should skip schema loading entirely; fails only because
    // there is no upstream to connect to, not because of schema issues
    motto()
        .args(["sniff", "--no-decode", "--upstream", "ws://localhost:9999"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("sniff tap error"));
}
