//! motto.lock - Version Tracking File
//!
//! The motto.lock file serves as the immutable versioning authority,
//! tracking the schema fingerprint and protocol version.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The motto.lock file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MottoLock {
    /// Lock file format version
    pub format_version: u8,
    /// Schema version (semver-like)
    pub version: Version,
    /// Schema fingerprint (SHA-256)
    pub fingerprint: String,
    /// Protocol version byte (derived from minor version)
    pub protocol_byte: u8,
    /// Timestamp of last update
    pub updated_at: String,
    /// History of schema changes
    #[serde(default)]
    pub history: Vec<LockHistoryEntry>,
}

/// Semantic version for the schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub major: u16,
    pub minor: u8,
    pub patch: u16,
}

impl Version {
    pub fn new(major: u16, minor: u8, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// An entry in the lock history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHistoryEntry {
    /// Version at this point
    pub version: String,
    /// Fingerprint at this point
    pub fingerprint: String,
    /// Timestamp
    pub timestamp: String,
    /// Description of changes (optional)
    pub description: Option<String>,
}

impl MottoLock {
    /// Create a new lock file with default values
    pub fn new() -> Self {
        Self {
            format_version: 1,
            version: Version::new(0, 1, 0),
            fingerprint: String::new(),
            protocol_byte: 1,
            updated_at: chrono::Utc::now().to_rfc3339(),
            history: Vec::new(),
        }
    }

    /// Load from a string
    pub fn from_str(content: &str) -> Result<Self> {
        // Support both TOML and JSON formats
        if content.trim().starts_with('{') {
            serde_json::from_str(content).context("Failed to parse motto.lock as JSON")
        } else {
            toml::from_str(content).context("Failed to parse motto.lock as TOML")
        }
    }

    /// Serialize to TOML string
    pub fn to_string(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize motto.lock")
    }

    /// Get the version string
    pub fn version(&self) -> String {
        self.version.to_string()
    }

    /// Get the fingerprint
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Set the fingerprint
    pub fn set_fingerprint(&mut self, fp: impl Into<String>) {
        let new_fp = fp.into();

        // Add to history if fingerprint changed
        if !self.fingerprint.is_empty() && self.fingerprint != new_fp {
            self.history.push(LockHistoryEntry {
                version: self.version.to_string(),
                fingerprint: self.fingerprint.clone(),
                timestamp: self.updated_at.clone(),
                description: None,
            });
        }

        self.fingerprint = new_fp;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Bump major version (breaking change)
    pub fn bump_major(&mut self) {
        self.version.major += 1;
        self.version.minor = 0;
        self.version.patch = 0;
        self.update_protocol_byte();
    }

    /// Bump minor version (new features, backward compatible)
    pub fn bump_minor(&mut self) {
        self.version.minor = self.version.minor.wrapping_add(1);
        self.version.patch = 0;
        self.update_protocol_byte();
    }

    /// Bump patch version (bug fixes)
    pub fn bump_patch(&mut self) {
        self.version.patch += 1;
    }

    /// Update protocol byte from minor version
    fn update_protocol_byte(&mut self) {
        // Protocol byte is derived from minor version
        // This allows 256 protocol versions before major bump required
        self.protocol_byte = self.version.minor;
    }
}

impl Default for MottoLock {
    fn default() -> Self {
        Self::new()
    }
}

// We need toml for lock file serialization
use chrono;
use toml;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_roundtrip() {
        let mut lock = MottoLock::new();
        lock.set_fingerprint("abc123def456");
        lock.bump_minor();

        let serialized = lock.to_string().unwrap();
        let deserialized = MottoLock::from_str(&serialized).unwrap();

        assert_eq!(deserialized.version.major, 0);
        assert_eq!(deserialized.version.minor, 2); // Started at 0.1.0, bumped to 0.2.0
        assert_eq!(deserialized.fingerprint, "abc123def456");
    }

    #[test]
    fn test_version_bumping() {
        let mut lock = MottoLock::new();
        assert_eq!(lock.version.to_string(), "0.1.0");

        lock.bump_patch();
        assert_eq!(lock.version.to_string(), "0.1.1");

        lock.bump_minor();
        assert_eq!(lock.version.to_string(), "0.2.0");
        assert_eq!(lock.protocol_byte, 2);

        lock.bump_major();
        assert_eq!(lock.version.to_string(), "1.0.0");
        assert_eq!(lock.protocol_byte, 0);
    }
}
