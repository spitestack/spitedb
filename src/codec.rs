//! # Event Batch Encoding and Decoding
//!
//! This module provides the codec for encoding events into batches and decoding
//! them back. The format is minimal - just raw concatenated payloads.
//!
//! ## Batch Format
//!
//! ```text
//! [event1_data][event2_data][event3_data]...
//! ```
//!
//! All metadata (stream_id, global_pos, stream_rev, timestamp) lives in the
//! `event_index` and `batches` tables, not in the blob. This enables efficient
//! queries and keeps the storage format simple.
//!
//! ## Future Extensions
//!
//! The codec and cipher fields in batches support future additions:
//! - Codec 1: LZ4 compression
//! - Codec 2: Zstd compression
//! - Cipher 1: AES-256-GCM encryption

use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::EventData;

// =============================================================================
// Constants
// =============================================================================

/// Codec identifier for batch compression.
///
/// Currently only uncompressed (0) is supported.
pub const CODEC_NONE: i32 = 0;

/// Cipher identifier for batch encryption.
///
/// Currently only plaintext (0) is supported.
pub const CIPHER_NONE: i32 = 0;

// =============================================================================
// Encoding
// =============================================================================

/// Encodes events into a batch blob.
///
/// The blob is just raw concatenated payloads with no headers. All metadata
/// lives in the database tables.
///
/// # Returns
///
/// A tuple of:
/// - The encoded blob data (raw concatenated payloads)
/// - A vec of (byte_offset, byte_len) for each event in the batch
///
/// # Arguments
///
/// * `events` - Events to encode
pub fn encode_batch(events: &[EventData]) -> (Vec<u8>, Vec<(usize, usize)>) {
    let mut data = Vec::new();
    let mut offsets = Vec::new();

    for event in events {
        let start_offset = data.len();
        data.extend_from_slice(&event.data);
        let len = event.data.len();
        offsets.push((start_offset, len));
    }

    (data, offsets)
}

// =============================================================================
// Decoding
// =============================================================================

/// Decodes a single event's data from a batch blob.
///
/// This just slices the raw bytes - no parsing needed since there are no headers.
///
/// # Arguments
///
/// * `batch_data` - The raw batch blob
/// * `offset` - Byte offset of this event within the batch
/// * `len` - Byte length of this event
///
/// # Returns
///
/// The raw event data bytes.
pub fn decode_event_data(batch_data: &[u8], offset: usize, len: usize) -> Vec<u8> {
    batch_data[offset..offset + len].to_vec()
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Computes a checksum for batch data.
///
/// Uses XXH3-64 for consistency with stream hashing. XXH3 is extremely fast
/// and provides good distribution for integrity checking.
pub fn compute_checksum(data: &[u8]) -> Vec<u8> {
    let hash = xxhash_rust::xxh3::xxh3_64(data);
    hash.to_le_bytes().to_vec()
}

/// Returns the current time in milliseconds since Unix epoch.
pub fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let events = vec![
            EventData::new(b"event 1 data".to_vec()),
            EventData::new(b"event 2 data".to_vec()),
        ];

        let (blob, offsets) = encode_batch(&events);

        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], (0, 12)); // "event 1 data" = 12 bytes
        assert_eq!(offsets[1], (12, 12)); // "event 2 data" = 12 bytes

        // Decode first event
        let data1 = decode_event_data(&blob, offsets[0].0, offsets[0].1);
        assert_eq!(data1, b"event 1 data");

        // Decode second event
        let data2 = decode_event_data(&blob, offsets[1].0, offsets[1].1);
        assert_eq!(data2, b"event 2 data");
    }

    #[test]
    fn test_encode_single_event() {
        let events = vec![EventData::new(b"hello".to_vec())];

        let (blob, offsets) = encode_batch(&events);

        assert_eq!(blob, b"hello");
        assert_eq!(offsets, vec![(0, 5)]);
    }

    #[test]
    fn test_encode_empty_events() {
        let events: Vec<EventData> = vec![];

        let (blob, offsets) = encode_batch(&events);

        assert!(blob.is_empty());
        assert!(offsets.is_empty());
    }

    #[test]
    fn test_checksum_deterministic() {
        let data = b"test data for checksum";
        let checksum1 = compute_checksum(data);
        let checksum2 = compute_checksum(data);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_checksum_different_data() {
        let checksum1 = compute_checksum(b"data1");
        let checksum2 = compute_checksum(b"data2");
        assert_ne!(checksum1, checksum2);
    }
}
