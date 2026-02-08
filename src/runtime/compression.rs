//! Compression - Zstd-based compression utilities

/// Zstd compressor with configurable compression level
pub struct ZstdCompressor {
    level: i32,
}

impl ZstdCompressor {
    /// Create a new compressor with the given level (1-22)
    pub fn new(level: i32) -> Self {
        Self {
            level: level.clamp(1, 22),
        }
    }

    /// Get the compression level
    pub fn level(&self) -> i32 {
        self.level
    }

    /// Compress data
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        zstd::encode_all(data, self.level).map_err(CompressionError::from)
    }

    /// Decompress data
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        zstd::decode_all(data).map_err(CompressionError::from)
    }

    /// Compress data with a size hint
    pub fn compress_with_hint(
        &self,
        data: &[u8],
        size_hint: usize,
    ) -> Result<Vec<u8>, CompressionError> {
        let mut output = Vec::with_capacity(size_hint);
        let mut encoder = zstd::Encoder::new(&mut output, self.level)?;
        std::io::copy(&mut std::io::Cursor::new(data), &mut encoder)?;
        encoder.finish()?;
        Ok(output)
    }

    /// Decompress data with a size hint
    pub fn decompress_with_hint(
        &self,
        data: &[u8],
        size_hint: usize,
    ) -> Result<Vec<u8>, CompressionError> {
        let mut output = Vec::with_capacity(size_hint);
        let mut decoder = zstd::Decoder::new(data)?;
        std::io::copy(&mut decoder, &mut output)?;
        Ok(output)
    }

    /// Estimate compressed size (rough estimate)
    pub fn estimate_compressed_size(&self, uncompressed_size: usize) -> usize {
        // Rough estimate: typical compression ratio is 2-4x for most data
        // At worst, compressed data can be slightly larger than input
        (uncompressed_size / 2).max(uncompressed_size.saturating_sub(100).max(1) + 18)
    }
}

impl Default for ZstdCompressor {
    fn default() -> Self {
        Self::new(3) // Default compression level
    }
}

/// Streaming compressor for large data
pub struct StreamingCompressor {
    level: i32,
}

impl StreamingCompressor {
    pub fn new(level: i32) -> Self {
        Self {
            level: level.clamp(1, 22),
        }
    }

    /// Create a streaming encoder
    pub fn encoder<'a, W: std::io::Write>(
        &self,
        writer: W,
    ) -> Result<zstd::Encoder<'a, W>, CompressionError> {
        zstd::Encoder::new(writer, self.level).map_err(CompressionError::from)
    }

    /// Create a streaming decoder
    pub fn decoder<'a, R: std::io::BufRead>(
        &self,
        reader: R,
    ) -> Result<zstd::Decoder<'a, std::io::BufReader<R>>, CompressionError> {
        zstd::Decoder::new(reader).map_err(CompressionError::from)
    }
}

/// Compression errors
#[derive(Debug, thiserror::Error)]
pub enum CompressionError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Compression level presets
pub mod levels {
    /// Fast compression, lower ratio
    pub const FAST: i32 = 1;
    /// Balanced compression (default)
    pub const DEFAULT: i32 = 3;
    /// Better compression, slower
    pub const BETTER: i32 = 9;
    /// Best compression, slowest
    pub const BEST: i32 = 19;
    /// Maximum compression (very slow)
    pub const MAX: i32 = 22;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_compress_decompress() {
        let compressor = ZstdCompressor::new(3);
        let data =
            b"Hello, World! This is a test message that should compress well when repeated. "
                .repeat(100);

        let compressed = compressor.compress(&data).unwrap();
        assert!(compressed.len() < data.len());

        let decompressed = compressor.decompress(&compressed).unwrap();
        assert_eq!(data, decompressed.as_slice());
    }

    #[test]
    fn test_compression_levels() {
        let data = b"Test data for compression".repeat(1000);

        let fast = ZstdCompressor::new(levels::FAST);
        let best = ZstdCompressor::new(levels::BEST);

        let fast_compressed = fast.compress(&data).unwrap();
        let best_compressed = best.compress(&data).unwrap();

        // Best should produce smaller output (usually)
        assert!(best_compressed.len() <= fast_compressed.len());
    }

    #[test]
    fn test_empty_data() {
        let compressor = ZstdCompressor::new(3);
        let data = b"";

        let compressed = compressor.compress(data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();
        assert_eq!(data, decompressed.as_slice());
    }
}
