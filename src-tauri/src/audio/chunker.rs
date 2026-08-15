//! Audio Chunker for Gemini Live API
//!
//! Divides audio into 20ms chunks (640 bytes = 320 samples × 2 bytes) for sending
//! to Gemini Live WebSocket API.
//!
//! # Requirements
//!
//! - Requirement 6.3: Audio chunks of 20ms (320 samples at 16kHz, 640 bytes)
//! - All chunks except the final one must be exactly 640 bytes
//! - Audio format: PCM16 mono @ 16kHz

/// Chunk size in samples (20ms at 16kHz)
pub const CHUNK_SAMPLES: usize = 320;

/// Chunk size in bytes (320 samples × 2 bytes per sample)
pub const CHUNK_BYTES: usize = 640;

/// Duration of each chunk in milliseconds
pub const CHUNK_DURATION_MS: u32 = 20;

/// Audio chunker for Gemini Live API
///
/// Buffers incoming PCM16 samples and emits fixed-size chunks of 320 samples
/// (640 bytes) for transmission to Gemini Live.
///
/// # Example
///
/// ```ignore
/// let mut chunker = AudioChunker::new();
///
/// // Push samples from capture
/// let samples: Vec<i16> = capture_audio();
/// for chunk in chunker.push_samples(&samples) {
///     send_to_gemini(&chunk);
/// }
///
/// // At end of stream, flush remaining samples
/// if let Some(final_chunk) = chunker.flush() {
///     send_to_gemini(&final_chunk);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AudioChunker {
    /// Internal buffer for accumulating samples
    buffer: Vec<i16>,
}

impl Default for AudioChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioChunker {
    /// Create a new AudioChunker with empty buffer
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(CHUNK_SAMPLES * 2), // Pre-allocate for efficiency
        }
    }

    /// Push samples into the chunker and return complete chunks
    ///
    /// Returns an iterator over complete 320-sample chunks. Any remaining
    /// samples that don't fill a complete chunk are buffered for the next call.
    ///
    /// # Arguments
    ///
    /// * `samples` - PCM16 samples to add to the buffer
    ///
    /// # Returns
    ///
    /// Iterator of complete chunks, each containing exactly 320 samples (640 bytes)
    pub fn push_samples(&mut self, samples: &[i16]) -> ChunkIterator<'_> {
        self.buffer.extend_from_slice(samples);
        ChunkIterator { chunker: self }
    }

    /// Flush the remaining buffer, returning the final (potentially smaller) chunk
    ///
    /// Call this at the end of an audio stream to retrieve any buffered samples
    /// that haven't formed a complete chunk. The returned chunk may be smaller
    /// than 320 samples.
    ///
    /// # Returns
    ///
    /// - `Some(Vec<i16>)` if there are buffered samples (1 to 319 samples)
    /// - `None` if the buffer is empty
    pub fn flush(&mut self) -> Option<Vec<i16>> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }

    /// Reset the chunker, clearing any buffered samples
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Get the number of samples currently buffered
    pub fn buffered_samples(&self) -> usize {
        self.buffer.len()
    }

    /// Get the number of bytes currently buffered
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len() * 2
    }

    /// Check if there are enough samples for at least one complete chunk
    pub fn has_complete_chunk(&self) -> bool {
        self.buffer.len() >= CHUNK_SAMPLES
    }

    /// Extract one complete chunk if available
    ///
    /// Returns exactly 320 samples (640 bytes) if available, None otherwise.
    fn take_chunk(&mut self) -> Option<Vec<i16>> {
        if self.buffer.len() >= CHUNK_SAMPLES {
            let chunk: Vec<i16> = self.buffer.drain(..CHUNK_SAMPLES).collect();
            Some(chunk)
        } else {
            None
        }
    }
}

/// Iterator over complete audio chunks
///
/// This iterator yields chunks of exactly 320 samples (640 bytes) until
/// the buffer has fewer than 320 samples remaining.
pub struct ChunkIterator<'a> {
    chunker: &'a mut AudioChunker,
}

impl<'a> Iterator for ChunkIterator<'a> {
    type Item = Vec<i16>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunker.take_chunk()
    }
}

/// Convert a chunk of i16 samples to raw bytes (little-endian PCM16)
///
/// Gemini Live expects audio data as raw bytes in little-endian format.
///
/// # Arguments
///
/// * `samples` - PCM16 samples to convert
///
/// # Returns
///
/// Raw bytes in little-endian PCM16 format
pub fn samples_to_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Convert raw bytes (little-endian PCM16) to i16 samples
///
/// # Arguments
///
/// * `bytes` - Raw PCM16 bytes in little-endian format
///
/// # Returns
///
/// PCM16 samples
///
/// # Panics
///
/// Panics if the byte length is not even (each sample requires 2 bytes)
pub fn bytes_to_samples(bytes: &[u8]) -> Vec<i16> {
    assert!(bytes.len() % 2 == 0, "Byte length must be even for PCM16 conversion");
    bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // Property-Based Tests (proptest)
    // ============================================
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        // **Property 2: Audio Chunks Have Correct Size**
        // 
        // Verifica que cada chunk no-final tiene exactamente 320 samples (640 bytes for PCM16).
        // El chunk final puede tener entre 1-320 samples.
        // 
        // **Validates: Requirements 6.3**
        // 
        // Invariant: For any input buffer of N samples:
        // - All non-final chunks have exactly CHUNK_SAMPLES (320) samples
        // - All non-final chunks convert to exactly CHUNK_BYTES (640) bytes
        // - Final chunk (if any) has 1..=320 samples
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(500))]

            #[test]
            fn prop_non_final_chunks_have_exact_size(
                // Generate random audio buffers of varying sizes
                samples in prop::collection::vec(any::<i16>(), 0..10000usize)
            ) {
                let mut chunker = AudioChunker::new();
                
                // Push all samples and collect complete chunks
                let complete_chunks: Vec<Vec<i16>> = chunker.push_samples(&samples).collect();
                
                // Property: All complete (non-final) chunks must have exactly 320 samples
                for (idx, chunk) in complete_chunks.iter().enumerate() {
                    prop_assert_eq!(
                        chunk.len(), 
                        CHUNK_SAMPLES,
                        "Non-final chunk {} has {} samples, expected exactly {}",
                        idx, chunk.len(), CHUNK_SAMPLES
                    );
                    
                    // Also verify byte conversion produces exactly 640 bytes
                    let bytes = samples_to_bytes(chunk);
                    prop_assert_eq!(
                        bytes.len(),
                        CHUNK_BYTES,
                        "Non-final chunk {} converts to {} bytes, expected exactly {}",
                        idx, bytes.len(), CHUNK_BYTES
                    );
                }
                
                // Property: Final chunk (from flush) has 1-320 samples if present
                if let Some(final_chunk) = chunker.flush() {
                    prop_assert!(
                        !final_chunk.is_empty() && final_chunk.len() <= CHUNK_SAMPLES,
                        "Final chunk has {} samples, expected 1..={}",
                        final_chunk.len(), CHUNK_SAMPLES
                    );
                }
            }

            #[test]
            fn prop_chunking_preserves_all_samples(
                samples in prop::collection::vec(any::<i16>(), 0..5000usize)
            ) {
                let mut chunker = AudioChunker::new();
                
                // Collect all complete chunks
                let complete_chunks: Vec<Vec<i16>> = chunker.push_samples(&samples).collect();
                
                // Get any remaining samples
                let remaining = chunker.flush();
                
                // Reconstruct all samples
                let mut reconstructed: Vec<i16> = Vec::new();
                for chunk in complete_chunks {
                    reconstructed.extend(chunk);
                }
                if let Some(final_chunk) = remaining {
                    reconstructed.extend(final_chunk);
                }
                
                // Property: All input samples must be preserved in output
                prop_assert_eq!(
                    reconstructed, samples,
                    "Chunking lost or corrupted samples"
                );
            }

            #[test]
            fn prop_chunk_count_is_correct(
                samples in prop::collection::vec(any::<i16>(), 0..10000usize)
            ) {
                let mut chunker = AudioChunker::new();
                
                let complete_chunks: Vec<Vec<i16>> = chunker.push_samples(&samples).collect();
                let remaining = chunker.flush();
                
                // Property: Number of complete chunks equals floor(N / 320)
                let expected_complete = samples.len() / CHUNK_SAMPLES;
                prop_assert_eq!(
                    complete_chunks.len(),
                    expected_complete,
                    "Expected {} complete chunks, got {}",
                    expected_complete, complete_chunks.len()
                );
                
                // Property: Remaining samples equals N mod 320
                let expected_remaining = samples.len() % CHUNK_SAMPLES;
                let actual_remaining = remaining.as_ref().map(|v| v.len()).unwrap_or(0);
                prop_assert_eq!(
                    actual_remaining,
                    expected_remaining,
                    "Expected {} remaining samples, got {}",
                    expected_remaining, actual_remaining
                );
            }

            #[test]
            fn prop_incremental_push_same_result(
                // Generate a sequence of push sizes
                push_sizes in prop::collection::vec(1..500usize, 1..20usize)
            ) {
                // Total samples to generate
                let total: usize = push_sizes.iter().sum();
                let all_samples: Vec<i16> = (0..total as i16).collect();
                
                // Method 1: Single push
                let mut chunker1 = AudioChunker::new();
                let chunks1: Vec<Vec<i16>> = chunker1.push_samples(&all_samples).collect();
                let remaining1 = chunker1.flush();
                
                // Method 2: Incremental pushes
                let mut chunker2 = AudioChunker::new();
                let mut chunks2: Vec<Vec<i16>> = Vec::new();
                let mut pos = 0;
                for size in push_sizes {
                    let end = (pos + size).min(total);
                    let chunk_iter = chunker2.push_samples(&all_samples[pos..end]);
                    chunks2.extend(chunk_iter);
                    pos = end;
                }
                let remaining2 = chunker2.flush();
                
                // Property: Both methods must produce identical results
                prop_assert_eq!(chunks1.len(), chunks2.len());
                for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
                    prop_assert_eq!(c1, c2);
                }
                prop_assert_eq!(remaining1, remaining2);
            }
        }
    }

    // ============================================
    // Unit Tests
    // ============================================

    #[test]
    fn test_chunk_constants() {
        // Verify the constants are correct for 20ms @ 16kHz
        // 16000 samples/sec * 0.020 sec = 320 samples
        assert_eq!(CHUNK_SAMPLES, 320);
        // 320 samples * 2 bytes/sample = 640 bytes
        assert_eq!(CHUNK_BYTES, 640);
        assert_eq!(CHUNK_DURATION_MS, 20);
    }

    #[test]
    fn test_new_chunker_is_empty() {
        let chunker = AudioChunker::new();
        assert_eq!(chunker.buffered_samples(), 0);
        assert_eq!(chunker.buffered_bytes(), 0);
        assert!(!chunker.has_complete_chunk());
    }

    #[test]
    fn test_push_less_than_chunk() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..100).collect();
        
        let chunks: Vec<_> = chunker.push_samples(&samples).collect();
        
        assert!(chunks.is_empty(), "Should not emit any chunks with only 100 samples");
        assert_eq!(chunker.buffered_samples(), 100);
        assert!(!chunker.has_complete_chunk());
    }

    #[test]
    fn test_push_exactly_one_chunk() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..320).map(|i| i as i16).collect();
        
        let chunks: Vec<_> = chunker.push_samples(&samples).collect();
        
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), CHUNK_SAMPLES);
        assert_eq!(chunker.buffered_samples(), 0);
    }

    #[test]
    fn test_push_multiple_chunks() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..1000).map(|i| i as i16).collect();
        
        let chunks: Vec<_> = chunker.push_samples(&samples).collect();
        
        // 1000 / 320 = 3 complete chunks, 40 samples remaining
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            assert_eq!(chunk.len(), CHUNK_SAMPLES);
        }
        assert_eq!(chunker.buffered_samples(), 1000 - (3 * 320)); // 40 samples
    }

    #[test]
    fn test_push_incremental() {
        let mut chunker = AudioChunker::new();
        
        // Push 200 samples - no complete chunk yet
        let samples1: Vec<i16> = (0..200).map(|i| i as i16).collect();
        let chunks1: Vec<_> = chunker.push_samples(&samples1).collect();
        assert!(chunks1.is_empty());
        assert_eq!(chunker.buffered_samples(), 200);
        
        // Push 200 more samples - now we have 400, should emit 1 chunk
        let samples2: Vec<i16> = (200..400).map(|i| i as i16).collect();
        let chunks2: Vec<_> = chunker.push_samples(&samples2).collect();
        assert_eq!(chunks2.len(), 1);
        assert_eq!(chunks2[0].len(), CHUNK_SAMPLES);
        assert_eq!(chunker.buffered_samples(), 80); // 400 - 320 = 80
    }

    #[test]
    fn test_flush_with_remaining_samples() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..500).map(|i| i as i16).collect();
        
        let _chunks: Vec<_> = chunker.push_samples(&samples).collect();
        // 500 / 320 = 1 complete chunk, 180 remaining
        
        let final_chunk = chunker.flush();
        assert!(final_chunk.is_some());
        assert_eq!(final_chunk.unwrap().len(), 180);
        assert_eq!(chunker.buffered_samples(), 0);
    }

    #[test]
    fn test_flush_empty_buffer() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..320).map(|i| i as i16).collect();
        
        let _chunks: Vec<_> = chunker.push_samples(&samples).collect();
        // Exactly one chunk consumed, nothing remaining
        
        let final_chunk = chunker.flush();
        assert!(final_chunk.is_none());
    }

    #[test]
    fn test_reset() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..100).collect();
        let _: Vec<_> = chunker.push_samples(&samples).collect();
        
        assert_eq!(chunker.buffered_samples(), 100);
        
        chunker.reset();
        
        assert_eq!(chunker.buffered_samples(), 0);
        assert!(!chunker.has_complete_chunk());
    }

    #[test]
    fn test_samples_to_bytes() {
        let samples: Vec<i16> = vec![0x0102, 0x0304, -1];
        let bytes = samples_to_bytes(&samples);
        
        assert_eq!(bytes.len(), 6);
        // 0x0102 in little-endian
        assert_eq!(bytes[0], 0x02);
        assert_eq!(bytes[1], 0x01);
        // 0x0304 in little-endian
        assert_eq!(bytes[2], 0x04);
        assert_eq!(bytes[3], 0x03);
        // -1 as i16 = 0xFFFF
        assert_eq!(bytes[4], 0xFF);
        assert_eq!(bytes[5], 0xFF);
    }

    #[test]
    fn test_bytes_to_samples() {
        let bytes: Vec<u8> = vec![0x02, 0x01, 0x04, 0x03, 0xFF, 0xFF];
        let samples = bytes_to_samples(&bytes);
        
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0], 0x0102);
        assert_eq!(samples[1], 0x0304);
        assert_eq!(samples[2], -1);
    }

    #[test]
    fn test_samples_bytes_roundtrip() {
        let original: Vec<i16> = vec![0, 100, -100, i16::MAX, i16::MIN, 12345, -12345];
        let bytes = samples_to_bytes(&original);
        let recovered = bytes_to_samples(&bytes);
        
        assert_eq!(original, recovered);
    }

    #[test]
    #[should_panic(expected = "Byte length must be even")]
    fn test_bytes_to_samples_odd_length() {
        let bytes: Vec<u8> = vec![0x00, 0x01, 0x02];
        bytes_to_samples(&bytes);
    }

    #[test]
    fn test_chunk_data_integrity() {
        let mut chunker = AudioChunker::new();
        
        // Create sequential samples
        let samples: Vec<i16> = (0..1000).map(|i| i as i16).collect();
        
        // Collect all complete chunks
        let chunks: Vec<_> = chunker.push_samples(&samples).collect();
        
        // Verify chunk data is correct and in order
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            for (sample_idx, &sample) in chunk.iter().enumerate() {
                let expected = ((chunk_idx * CHUNK_SAMPLES) + sample_idx) as i16;
                assert_eq!(sample, expected, 
                    "Mismatch at chunk {}, sample {}: expected {}, got {}", 
                    chunk_idx, sample_idx, expected, sample);
            }
        }
        
        // Verify remaining buffer data
        let remaining = chunker.flush().unwrap();
        let start_idx = chunks.len() * CHUNK_SAMPLES;
        for (i, &sample) in remaining.iter().enumerate() {
            let expected = (start_idx + i) as i16;
            assert_eq!(sample, expected);
        }
    }

    #[test]
    fn test_complete_chunk_has_correct_byte_size() {
        let mut chunker = AudioChunker::new();
        let samples: Vec<i16> = (0..640).map(|i| i as i16).collect();
        
        let chunks: Vec<_> = chunker.push_samples(&samples).collect();
        
        for chunk in chunks {
            let bytes = samples_to_bytes(&chunk);
            assert_eq!(bytes.len(), CHUNK_BYTES, 
                "Complete chunk should have exactly {} bytes", CHUNK_BYTES);
        }
    }
}
