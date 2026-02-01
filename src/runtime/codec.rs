//! Codec - Bitcode-based serialization
//!
//! Provides efficient binary serialization using the bitcode crate,
//! with a 1-byte version header for protocol versioning.

use bitcode::{Decode, Encode};
use std::io::{Read, Write};

/// The protocol version byte (derived from motto.lock)
pub const PROTOCOL_VERSION: u8 = crate::PROTOCOL_VERSION_BYTE;

/// Codec for Motto protocol serialization
pub struct MottoCodec {
    /// Whether to include version header
    include_header: bool,
}

impl MottoCodec {
    /// Create a new codec with version header enabled
    pub fn new() -> Self {
        Self {
            include_header: true,
        }
    }

    /// Create a codec without version header (for nested encoding)
    pub fn without_header() -> Self {
        Self {
            include_header: false,
        }
    }

    /// Encode a value to bytes with version header
    pub fn encode<T: Encode>(&self, value: &T) -> Vec<u8> {
        let data = bitcode::encode(value);

        if self.include_header {
            let mut result = Vec::with_capacity(1 + data.len());
            result.push(PROTOCOL_VERSION);
            result.extend_from_slice(&data);
            result
        } else {
            data
        }
    }

    /// Decode bytes to a value, validating version header
    pub fn decode<'a, T: Decode<'a>>(&self, bytes: &'a [u8]) -> Result<T, CodecError> {
        if self.include_header {
            if bytes.is_empty() {
                return Err(CodecError::EmptyInput);
            }

            let version = bytes[0];
            if version != PROTOCOL_VERSION {
                return Err(CodecError::VersionMismatch {
                    expected: PROTOCOL_VERSION,
                    found: version,
                });
            }

            bitcode::decode(&bytes[1..]).map_err(|e| CodecError::DecodeError(e.to_string()))
        } else {
            bitcode::decode(bytes).map_err(|e| CodecError::DecodeError(e.to_string()))
        }
    }

    /// Encode a value to a writer
    pub fn encode_to<T: Encode, W: Write>(&self, value: &T, mut writer: W) -> std::io::Result<()> {
        let data = self.encode(value);
        writer.write_all(&data)
    }

    /// Decode from a reader
    pub fn decode_from<T: for<'a> Decode<'a>, R: Read>(
        &self,
        mut reader: R,
    ) -> Result<T, CodecError> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        bitcode::decode(&data).map_err(|e| CodecError::DecodeError(e.to_string()))
    }

    /// Get just the version byte from encoded data
    pub fn peek_version(bytes: &[u8]) -> Option<u8> {
        bytes.first().copied()
    }

    /// Validate that the version byte matches
    pub fn validate_version(bytes: &[u8]) -> bool {
        Self::peek_version(bytes) == Some(PROTOCOL_VERSION)
    }
}

impl Default for MottoCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Codec errors
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("Empty input")]
    EmptyInput,

    #[error("Version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u8, found: u8 },

    #[error("Decode error: {0}")]
    DecodeError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Zero-copy buffer view for packet parsing
pub struct PacketView<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PacketView<'a> {
    /// Create a new packet view
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// Get the version byte
    pub fn version(&self) -> Option<u8> {
        self.data.first().copied()
    }

    /// Skip the version byte
    pub fn skip_version(&mut self) {
        if !self.data.is_empty() {
            self.offset = 1;
        }
    }

    /// Read a u8
    pub fn read_u8(&mut self) -> Option<u8> {
        if self.offset < self.data.len() {
            let val = self.data[self.offset];
            self.offset += 1;
            Some(val)
        } else {
            None
        }
    }

    /// Read a u16 (little-endian)
    pub fn read_u16(&mut self) -> Option<u16> {
        if self.offset + 2 <= self.data.len() {
            let val = u16::from_le_bytes([self.data[self.offset], self.data[self.offset + 1]]);
            self.offset += 2;
            Some(val)
        } else {
            None
        }
    }

    /// Read a u32 (little-endian)
    pub fn read_u32(&mut self) -> Option<u32> {
        if self.offset + 4 <= self.data.len() {
            let val = u32::from_le_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ]);
            self.offset += 4;
            Some(val)
        } else {
            None
        }
    }

    /// Read a u64 (little-endian)
    pub fn read_u64(&mut self) -> Option<u64> {
        if self.offset + 8 <= self.data.len() {
            let val = u64::from_le_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
                self.data[self.offset + 4],
                self.data[self.offset + 5],
                self.data[self.offset + 6],
                self.data[self.offset + 7],
            ]);
            self.offset += 8;
            Some(val)
        } else {
            None
        }
    }

    /// Read a f32 (little-endian)
    pub fn read_f32(&mut self) -> Option<f32> {
        self.read_u32().map(f32::from_bits)
    }

    /// Read a f64 (little-endian)
    pub fn read_f64(&mut self) -> Option<f64> {
        self.read_u64().map(f64::from_bits)
    }

    /// Read a length-prefixed string
    pub fn read_string(&mut self) -> Option<&'a str> {
        let len = self.read_u32()? as usize;
        if self.offset + len <= self.data.len() {
            let s = std::str::from_utf8(&self.data[self.offset..self.offset + len]).ok()?;
            self.offset += len;
            Some(s)
        } else {
            None
        }
    }

    /// Read raw bytes
    pub fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.offset + len <= self.data.len() {
            let bytes = &self.data[self.offset..self.offset + len];
            self.offset += len;
            Some(bytes)
        } else {
            None
        }
    }

    /// Remaining bytes
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    /// Current offset
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Packet builder for zero-copy encoding
pub struct PacketBuilder {
    data: Vec<u8>,
}

impl PacketBuilder {
    /// Create a new packet builder with version header
    pub fn new() -> Self {
        let mut data = Vec::with_capacity(256);
        data.push(PROTOCOL_VERSION);
        Self { data }
    }

    /// Create without version header
    pub fn without_header() -> Self {
        Self {
            data: Vec::with_capacity(256),
        }
    }

    /// Write a u8
    pub fn write_u8(&mut self, val: u8) {
        self.data.push(val);
    }

    /// Write a u16 (little-endian)
    pub fn write_u16(&mut self, val: u16) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write a u32 (little-endian)
    pub fn write_u32(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write a u64 (little-endian)
    pub fn write_u64(&mut self, val: u64) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write a f32 (little-endian)
    pub fn write_f32(&mut self, val: f32) {
        self.write_u32(val.to_bits());
    }

    /// Write a f64 (little-endian)
    pub fn write_f64(&mut self, val: f64) {
        self.write_u64(val.to_bits());
    }

    /// Write a length-prefixed string
    pub fn write_string(&mut self, s: &str) {
        self.write_u32(s.len() as u32);
        self.data.extend_from_slice(s.as_bytes());
    }

    /// Write raw bytes
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Build the final packet
    pub fn build(self) -> Vec<u8> {
        self.data
    }

    /// Get current length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Default for PacketBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcode::{Decode, Encode};

    #[derive(Debug, Clone, PartialEq, Encode, Decode)]
    struct TestMessage {
        id: u64,
        name: String,
        values: Vec<f32>,
    }

    #[test]
    fn test_codec_roundtrip() {
        let codec = MottoCodec::new();
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
            values: vec![1.0, 2.0, 3.0],
        };

        let encoded = codec.encode(&msg);
        assert_eq!(encoded[0], PROTOCOL_VERSION);

        let decoded: TestMessage = codec.decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_version_validation() {
        let encoded = vec![0xFF, 1, 2, 3]; // Wrong version
        assert!(!MottoCodec::validate_version(&encoded));

        let encoded = vec![PROTOCOL_VERSION, 1, 2, 3];
        assert!(MottoCodec::validate_version(&encoded));
    }

    #[test]
    fn test_packet_view() {
        let mut builder = PacketBuilder::new();
        builder.write_u32(42);
        builder.write_string("hello");
        builder.write_f64(3.14);

        let data = builder.build();
        let mut view = PacketView::new(&data);

        assert_eq!(view.version(), Some(PROTOCOL_VERSION));
        view.skip_version();

        assert_eq!(view.read_u32(), Some(42));
        assert_eq!(view.read_string(), Some("hello"));
        assert_eq!(view.read_f64(), Some(3.14));
    }
}
