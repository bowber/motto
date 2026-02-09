//! Runtime Module
//!
//! Contains the core runtime components bundled with generated SDKs:
//! - Bitcode serialization
//! - Zstd compression
//! - State machine
//! - Retry logic
//! - WebTransport layer

pub mod codec;
pub mod compression;
#[cfg(feature = "ffi-transport")]
pub mod ffi;
pub mod sniff;
pub mod state;
pub mod transport;

pub use codec::MottoCodec;
pub use compression::ZstdCompressor;
pub use state::StateMachine;
pub use transport::WebSocketClient;
#[cfg(feature = "webtransport")]
pub mod webtransport;
#[cfg(feature = "webtransport")]
pub use webtransport::WebTransportClient;

/// Main runtime entry point
pub struct MottoRuntime {
    /// Codec for serialization/deserialization
    pub codec: MottoCodec,
    /// Compressor for data compression
    pub compressor: ZstdCompressor,
}

impl MottoRuntime {
    /// Create a new runtime with default settings
    pub fn new() -> Self {
        Self {
            codec: MottoCodec::new(),
            compressor: ZstdCompressor::new(3),
        }
    }

    /// Create a runtime with custom compression level
    pub fn with_compression_level(level: i32) -> Self {
        Self {
            codec: MottoCodec::new(),
            compressor: ZstdCompressor::new(level),
        }
    }
}

impl Default for MottoRuntime {
    fn default() -> Self {
        Self::new()
    }
}
