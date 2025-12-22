//! # Batch Compression
//!
//! This module provides compression for event batches. All batches are compressed
//! with Zstd (level 1) for storage efficiency.
//!
//! ## Why No Encryption?
//!
//! SpiteDB uses SQLite which is embedded/in-process. There is no:
//! - Database server with network exposure
//! - DBA with independent access
//! - Separate authentication layer
//!
//! All data access goes through the application, which means:
//! - Volume encryption (infrastructure) handles "at rest" protection
//! - Application-layer tenant scoping handles isolation
//! - No additional encryption layer needed
//!
//! For regulatory requirements that mandate application-level encryption,
//! consider encrypting at the application layer before passing data to SpiteDB.

use crate::error::{Error, Result};

// =============================================================================
// Constants
// =============================================================================

/// Codec identifier for Zstd compression (level 1).
pub const CODEC_ZSTD_L1: i32 = 1;

/// Cipher identifier - kept for schema compatibility but no encryption is performed.
pub const CIPHER_AES256GCM: i32 = 0;

/// Nonce size in bytes - kept for API compatibility.
pub const AES_GCM_NONCE_SIZE: usize = 12;

/// Zstd compression level (1 = fastest).
pub const ZSTD_COMPRESSION_LEVEL: i32 = 1;

// =============================================================================
// Batch Compressor (formerly BatchCryptor)
// =============================================================================

/// Handles compression of batch data.
///
/// # Design Decision: Compression Only
///
/// This type performs Zstd compression only. No encryption is applied because:
/// 1. SQLite is embedded/in-process - no network exposure
/// 2. Volume encryption handles data-at-rest protection
/// 3. Application-layer tenant scoping handles isolation
///
/// For backward compatibility, this is still named BatchCryptor and maintains
/// the same API surface.
pub struct BatchCryptor {
    // No fields needed - compression only
}

impl BatchCryptor {
    /// Creates a new BatchCryptor.
    ///
    /// The key provider parameter is ignored - no encryption is performed.
    pub fn new<T>(_key_provider: T) -> Self {
        Self {}
    }

    /// Creates a BatchCryptor (no environment variable needed).
    ///
    /// This always succeeds since no encryption key is required.
    pub fn from_env() -> Result<Self> {
        Ok(Self {})
    }

    /// Creates a new BatchCryptor (for compatibility).
    pub fn clone_with_same_key(&self) -> Self {
        Self {}
    }

    /// Compresses batch data.
    ///
    /// # Arguments
    ///
    /// * `plaintext` - Raw batch data (concatenated event payloads)
    /// * `_batch_id` - Batch identifier (ignored, kept for API compatibility)
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - Compressed data
    /// - Zero nonce (kept for API compatibility)
    pub fn seal(
        &self,
        plaintext: &[u8],
        _batch_id: i64,
    ) -> Result<(Vec<u8>, [u8; AES_GCM_NONCE_SIZE])> {
        // Compress with Zstd level 1
        let compressed = zstd::encode_all(plaintext, ZSTD_COMPRESSION_LEVEL)
            .map_err(|e| Error::Compression(e.to_string()))?;

        // Return zero nonce for API compatibility
        let nonce = [0u8; AES_GCM_NONCE_SIZE];

        Ok((compressed, nonce))
    }

    /// Decompresses batch data.
    ///
    /// # Arguments
    ///
    /// * `compressed` - Compressed batch data
    /// * `_nonce` - Ignored (kept for API compatibility)
    /// * `_batch_id` - Batch identifier (ignored, kept for API compatibility)
    ///
    /// # Returns
    ///
    /// Decompressed plaintext.
    pub fn open(
        &self,
        compressed: &[u8],
        _nonce: &[u8; AES_GCM_NONCE_SIZE],
        _batch_id: i64,
    ) -> Result<Vec<u8>> {
        // Decompress with Zstd
        let plaintext = zstd::decode_all(compressed)
            .map_err(|e| Error::Compression(e.to_string()))?;

        Ok(plaintext)
    }
}

// =============================================================================
// Legacy Types (kept for API compatibility)
// =============================================================================

/// Legacy key provider trait - kept for API compatibility but not used.
pub trait KeyProvider: Send + Sync {
    fn get_master_key(&self) -> Result<[u8; 32]>;
    fn derive_batch_key(&self, _batch_id: i64, _nonce: &[u8; AES_GCM_NONCE_SIZE]) -> Result<[u8; 32]>;
}

/// Legacy environment key provider - kept for API compatibility.
pub struct EnvKeyProvider {
    key: [u8; 32],
}

impl EnvKeyProvider {
    /// Creates a provider with a specific key (for testing compatibility).
    pub fn from_key(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Creates from environment - now a no-op that returns a dummy key.
    pub fn from_env() -> Result<Self> {
        Ok(Self { key: [0u8; 32] })
    }
}

impl KeyProvider for EnvKeyProvider {
    fn get_master_key(&self) -> Result<[u8; 32]> {
        Ok(self.key)
    }

    fn derive_batch_key(&self, _batch_id: i64, _nonce: &[u8; AES_GCM_NONCE_SIZE]) -> Result<[u8; 32]> {
        Ok(self.key)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_open_roundtrip() {
        let cryptor = BatchCryptor::from_env().unwrap();
        let plaintext = b"Hello, world! This is test data for compression.";
        let batch_id = 12345i64;

        let (compressed, nonce) = cryptor.seal(plaintext, batch_id).unwrap();
        let decompressed = cryptor.open(&compressed, &nonce, batch_id).unwrap();

        assert_eq!(decompressed, plaintext);
    }

    #[test]
    fn test_seal_open_empty_data() {
        let cryptor = BatchCryptor::from_env().unwrap();
        let plaintext = b"";
        let batch_id = 1i64;

        let (compressed, nonce) = cryptor.seal(plaintext, batch_id).unwrap();
        let decompressed = cryptor.open(&compressed, &nonce, batch_id).unwrap();

        assert_eq!(decompressed, plaintext);
    }

    #[test]
    fn test_seal_open_large_data() {
        let cryptor = BatchCryptor::from_env().unwrap();
        let plaintext: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        let batch_id = 999i64;

        let (compressed, nonce) = cryptor.seal(&plaintext, batch_id).unwrap();

        // Compressed should be smaller than original for repetitive data
        assert!(compressed.len() < plaintext.len());

        let decompressed = cryptor.open(&compressed, &nonce, batch_id).unwrap();
        assert_eq!(decompressed, plaintext);
    }

    #[test]
    fn test_from_env_always_succeeds() {
        // from_env() should always succeed since no key is required
        let result = BatchCryptor::from_env();
        assert!(result.is_ok());
    }

    #[test]
    fn test_clone_with_same_key() {
        let cryptor = BatchCryptor::from_env().unwrap();
        let cloned = cryptor.clone_with_same_key();
        
        // Both should work the same
        let plaintext = b"test data";
        let (compressed, nonce) = cryptor.seal(plaintext, 1).unwrap();
        let decompressed = cloned.open(&compressed, &nonce, 1).unwrap();
        assert_eq!(decompressed, plaintext);
    }
}