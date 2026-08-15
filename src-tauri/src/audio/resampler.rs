// Audio resampler using rubato crate
// Sample rate conversion for Gemini Live compatibility
//
// Requirements:
// - 2.3: Convert captured audio to PCM16 mono @ 16kHz for Gemini input
// - 3.4: Same conversion for macOS ScreenCaptureKit audio
// - Duration must be preserved (±1 sample tolerance)

use rubato::{FftFixedIn, Resampler};
use thiserror::Error;

/// Target sample rate for Gemini Live input
pub const GEMINI_INPUT_RATE: u32 = 16000;

/// Sample rate of Gemini Live output
pub const GEMINI_OUTPUT_RATE: u32 = 24000;

/// Errors that can occur during resampling
#[derive(Error, Debug)]
pub enum ResamplerError {
    #[error("Invalid source sample rate: {0}")]
    InvalidSourceRate(u32),
    
    #[error("Invalid target sample rate: {0}")]
    InvalidTargetRate(u32),
    
    #[error("Resampler initialization failed: {0}")]
    InitializationFailed(String),
    
    #[error("Resampling process failed: {0}")]
    ProcessingFailed(String),
    
    #[error("Empty input samples")]
    EmptyInput,
}

/// Audio resampler that converts audio to the target sample rate
pub struct AudioResampler {
    source_rate: u32,
    target_rate: u32,
    channels: usize,
    resampler: FftFixedIn<f32>,
}

impl AudioResampler {
    /// Create a new resampler for the given source and target rates
    /// 
    /// # Arguments
    /// * `source_rate` - The sample rate of the input audio
    /// * `target_rate` - The desired output sample rate
    /// * `channels` - Number of audio channels (1 for mono, 2 for stereo)
    pub fn new(source_rate: u32, target_rate: u32, channels: usize) -> Result<Self, ResamplerError> {
        if source_rate == 0 || source_rate > 192000 {
            return Err(ResamplerError::InvalidSourceRate(source_rate));
        }
        
        if target_rate == 0 || target_rate > 192000 {
            return Err(ResamplerError::InvalidTargetRate(target_rate));
        }
        
        if channels == 0 || channels > 2 {
            return Err(ResamplerError::InitializationFailed(
                format!("Invalid channel count: {}", channels)
            ));
        }
        
        // Use a chunk size that balances latency and efficiency
        // 1024 samples is a good balance for real-time audio
        let chunk_size = 1024;
        
        let resampler = FftFixedIn::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            chunk_size,
            2, // sub-chunks for better quality
            channels,
        ).map_err(|e| ResamplerError::InitializationFailed(e.to_string()))?;
        
        Ok(Self {
            source_rate,
            target_rate,
            channels,
            resampler,
        })
    }
    
    /// Create a resampler for converting to Gemini input format (16kHz mono)
    pub fn for_gemini_input(source_rate: u32, channels: usize) -> Result<Self, ResamplerError> {
        Self::new(source_rate, GEMINI_INPUT_RATE, channels)
    }
    
    /// Create a resampler for converting from Gemini output (24kHz)
    pub fn for_gemini_output(target_rate: u32) -> Result<Self, ResamplerError> {
        Self::new(GEMINI_OUTPUT_RATE, target_rate, 1) // Gemini output is mono
    }
    
    /// Calculate the expected output length for a given input length
    /// This preserves duration within ±1 sample tolerance
    pub fn calculate_output_length(&self, input_length: usize) -> usize {
        let ratio = self.target_rate as f64 / self.source_rate as f64;
        (input_length as f64 * ratio).round() as usize
    }
    
    /// Resample PCM16 audio samples
    /// 
    /// # Arguments
    /// * `samples` - Input audio samples in PCM16 format (interleaved if stereo)
    /// 
    /// # Returns
    /// Resampled audio samples in PCM16 format
    pub fn resample(&mut self, samples: &[i16]) -> Result<Vec<i16>, ResamplerError> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        
        // If rates are the same, just handle channel conversion
        if self.source_rate == self.target_rate {
            if self.channels == 1 {
                return Ok(samples.to_vec());
            }
            // Convert stereo to mono if needed
            return Ok(stereo_to_mono_i16(samples));
        }
        
        // Convert i16 samples to f32 for processing
        let f32_samples: Vec<f32> = samples.iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect();
        
        // Deinterleave if stereo
        let deinterleaved = if self.channels == 2 {
            deinterleave_stereo(&f32_samples)
        } else {
            vec![f32_samples]
        };
        
        // Process in chunks
        let chunk_size = self.resampler.input_frames_max();
        let mut output_frames: Vec<Vec<f32>> = vec![Vec::new(); self.channels];
        
        let frames_per_channel = deinterleaved[0].len();
        let mut pos = 0;
        
        while pos < frames_per_channel {
            let end = (pos + chunk_size).min(frames_per_channel);
            let chunk_len = end - pos;
            
            // Prepare input chunk with padding if necessary
            let input_chunk: Vec<Vec<f32>> = deinterleaved.iter()
                .map(|channel| {
                    let mut chunk = channel[pos..end].to_vec();
                    // Pad with zeros if needed
                    while chunk.len() < chunk_size {
                        chunk.push(0.0);
                    }
                    chunk
                })
                .collect();
            
            // Process the chunk
            let output = self.resampler.process(&input_chunk, None)
                .map_err(|e| ResamplerError::ProcessingFailed(e.to_string()))?;
            
            // Calculate how many output samples correspond to this input
            let expected_output = self.calculate_output_length(chunk_len);
            
            for (ch, out_ch) in output.iter().enumerate() {
                let take_count = expected_output.min(out_ch.len());
                output_frames[ch].extend_from_slice(&out_ch[..take_count]);
            }
            
            pos = end;
        }
        
        // Mix down to mono if stereo input
        let mono_output = if self.channels == 2 {
            mix_to_mono(&output_frames[0], &output_frames[1])
        } else {
            output_frames.into_iter().next().unwrap_or_default()
        };
        
        // Convert back to i16
        let result: Vec<i16> = mono_output.iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * i16::MAX as f32) as i16
            })
            .collect();
        
        Ok(result)
    }
    
    /// Get the source sample rate
    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }
    
    /// Get the target sample rate
    pub fn target_rate(&self) -> u32 {
        self.target_rate
    }
}

/// Simple linear interpolation resampler for when rubato fails or for simple cases
/// This is a fallback that still preserves duration accurately
pub fn linear_resample(samples: &[i16], source_rate: u32, target_rate: u32) -> Vec<i16> {
    if samples.is_empty() || source_rate == 0 || target_rate == 0 {
        return Vec::new();
    }
    
    if source_rate == target_rate {
        return samples.to_vec();
    }
    
    let ratio = source_rate as f64 / target_rate as f64;
    let output_len = ((samples.len() as f64 * target_rate as f64) / source_rate as f64).round() as usize;
    
    if output_len == 0 {
        return Vec::new();
    }
    
    let mut output = Vec::with_capacity(output_len);
    
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos.floor() as usize;
        let frac = src_pos - src_idx as f64;
        
        let sample = if src_idx + 1 < samples.len() {
            let s0 = samples[src_idx] as f64;
            let s1 = samples[src_idx + 1] as f64;
            (s0 + (s1 - s0) * frac) as i16
        } else if src_idx < samples.len() {
            samples[src_idx]
        } else {
            0
        };
        
        output.push(sample);
    }
    
    output
}

/// Convert stereo PCM16 samples to mono by averaging channels
fn stereo_to_mono_i16(samples: &[i16]) -> Vec<i16> {
    samples.chunks(2)
        .map(|chunk| {
            if chunk.len() == 2 {
                ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16
            } else {
                chunk[0]
            }
        })
        .collect()
}

/// Deinterleave stereo samples into separate channels
fn deinterleave_stereo(samples: &[f32]) -> Vec<Vec<f32>> {
    let mut left = Vec::with_capacity(samples.len() / 2);
    let mut right = Vec::with_capacity(samples.len() / 2);
    
    for chunk in samples.chunks(2) {
        if chunk.len() == 2 {
            left.push(chunk[0]);
            right.push(chunk[1]);
        } else {
            left.push(chunk[0]);
            right.push(chunk[0]);
        }
    }
    
    vec![left, right]
}

/// Mix two mono channels into one
fn mix_to_mono(left: &[f32], right: &[f32]) -> Vec<f32> {
    left.iter()
        .zip(right.iter())
        .map(|(&l, &r)| (l + r) / 2.0)
        .collect()
}

// ============================================================================
// Simple API functions (backward compatible with existing code)
// ============================================================================

/// Resample audio from source rate to target rate (simple API)
/// 
/// This function creates a new resampler for each call.
/// For repeated use with the same rates, use AudioResampler directly.
pub fn resample(samples: &[i16], source_rate: u32, target_rate: u32) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    
    if source_rate == target_rate {
        return samples.to_vec();
    }
    
    // Try using rubato resampler first
    match AudioResampler::new(source_rate, target_rate, 1) {
        Ok(mut resampler) => {
            resampler.resample(samples).unwrap_or_else(|_| {
                // Fallback to linear interpolation
                linear_resample(samples, source_rate, target_rate)
            })
        }
        Err(_) => {
            // Fallback to linear interpolation
            linear_resample(samples, source_rate, target_rate)
        }
    }
}

/// Resample stereo audio to mono at target rate
pub fn resample_stereo_to_mono(samples: &[i16], source_rate: u32, target_rate: u32) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    
    // First convert stereo to mono
    let mono = stereo_to_mono_i16(samples);
    
    // Then resample
    resample(&mono, source_rate, target_rate)
}

/// Resample to 16kHz mono for Gemini input (simple API)
pub fn resample_to_gemini_input(samples: &[i16], source_rate: u32) -> Vec<i16> {
    resample(samples, source_rate, GEMINI_INPUT_RATE)
}

/// Resample stereo to 16kHz mono for Gemini input
pub fn resample_stereo_to_gemini_input(samples: &[i16], source_rate: u32) -> Vec<i16> {
    resample_stereo_to_mono(samples, source_rate, GEMINI_INPUT_RATE)
}

/// Resample from 24kHz Gemini output to target rate (simple API)
pub fn resample_from_gemini_output(samples: &[i16], target_rate: u32) -> Vec<i16> {
    resample(samples, GEMINI_OUTPUT_RATE, target_rate)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_same_rate_passthrough() {
        let samples = vec![100, 200, 300, 400, 500];
        let result = resample(&samples, 16000, 16000);
        assert_eq!(result, samples);
    }
    
    #[test]
    fn test_empty_input() {
        let samples: Vec<i16> = vec![];
        let result = resample(&samples, 44100, 16000);
        assert!(result.is_empty());
    }
    
    #[test]
    fn test_duration_preservation_downsample() {
        // 48kHz -> 16kHz (3:1 ratio)
        let source_rate = 48000u32;
        let target_rate = 16000u32;
        let duration_seconds = 1.0;
        
        let input_samples = (source_rate as f64 * duration_seconds) as usize;
        let expected_output = (target_rate as f64 * duration_seconds) as usize;
        
        // Create a simple sine wave
        let samples: Vec<i16> = (0..input_samples)
            .map(|i| ((i as f64 / source_rate as f64 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 10000.0) as i16)
            .collect();
        
        let result = resample(&samples, source_rate, target_rate);
        
        // Duration should be preserved within ±1 sample
        let diff = (result.len() as i64 - expected_output as i64).abs();
        assert!(diff <= 1, "Duration not preserved: expected ~{}, got {}, diff {}", 
                expected_output, result.len(), diff);
    }
    
    #[test]
    fn test_duration_preservation_upsample() {
        // 8kHz -> 16kHz (1:2 ratio)
        let source_rate = 8000u32;
        let target_rate = 16000u32;
        let duration_seconds = 0.5;
        
        let input_samples = (source_rate as f64 * duration_seconds) as usize;
        let expected_output = (target_rate as f64 * duration_seconds) as usize;
        
        let samples: Vec<i16> = (0..input_samples)
            .map(|i| ((i as f64 / source_rate as f64 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 10000.0) as i16)
            .collect();
        
        let result = resample(&samples, source_rate, target_rate);
        
        let diff = (result.len() as i64 - expected_output as i64).abs();
        assert!(diff <= 1, "Duration not preserved: expected ~{}, got {}, diff {}",
                expected_output, result.len(), diff);
    }
    
    #[test]
    fn test_44100_to_16000() {
        // Common case: CD quality to Gemini input
        let source_rate = 44100u32;
        let target_rate = 16000u32;
        let input_samples = 44100; // 1 second
        let expected_output = 16000; // 1 second at target rate
        
        let samples: Vec<i16> = (0..input_samples)
            .map(|i| ((i as f64 / source_rate as f64 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 10000.0) as i16)
            .collect();
        
        let result = resample(&samples, source_rate, target_rate);
        
        let diff = (result.len() as i64 - expected_output as i64).abs();
        assert!(diff <= 1, "44100->16000 duration not preserved: expected ~{}, got {}",
                expected_output, result.len());
    }
    
    #[test]
    fn test_stereo_to_mono() {
        // Left: 1000, Right: 500 -> Mono: 750
        let stereo = vec![1000i16, 500, 2000, 1000, 3000, 1500];
        let mono = stereo_to_mono_i16(&stereo);
        assert_eq!(mono, vec![750, 1500, 2250]);
    }
    
    #[test]
    fn test_gemini_input_conversion() {
        let source_rate = 48000u32;
        let samples: Vec<i16> = vec![100; 4800]; // 100ms at 48kHz
        
        let result = resample_to_gemini_input(&samples, source_rate);
        
        // Should be approximately 1600 samples (100ms at 16kHz)
        let expected = 1600;
        let diff = (result.len() as i64 - expected as i64).abs();
        assert!(diff <= 1, "Gemini input conversion failed: expected ~{}, got {}",
                expected, result.len());
    }
    
    #[test]
    fn test_gemini_output_conversion() {
        let target_rate = 48000u32;
        let samples: Vec<i16> = vec![100; 2400]; // 100ms at 24kHz (Gemini output)
        
        let result = resample_from_gemini_output(&samples, target_rate);
        
        // Should be approximately 4800 samples (100ms at 48kHz)
        let expected = 4800;
        let diff = (result.len() as i64 - expected as i64).abs();
        assert!(diff <= 1, "Gemini output conversion failed: expected ~{}, got {}",
                expected, result.len());
    }
    
    #[test]
    fn test_linear_resample_basic() {
        let samples = vec![0i16, 1000, 0, -1000, 0];
        let result = linear_resample(&samples, 1000, 500);
        
        // Should have approximately half the samples
        assert!(result.len() >= 2 && result.len() <= 3);
    }
    
    #[test]
    fn test_audio_resampler_struct() {
        let mut resampler = AudioResampler::for_gemini_input(48000, 1).unwrap();
        
        let samples: Vec<i16> = (0..4800)
            .map(|i| ((i as f64 / 48000.0 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 10000.0) as i16)
            .collect();
        
        let result = resampler.resample(&samples).unwrap();
        
        // 4800 samples at 48kHz = 100ms, should become ~1600 samples at 16kHz
        let expected = 1600;
        let diff = (result.len() as i64 - expected as i64).abs();
        assert!(diff <= 1, "AudioResampler failed: expected ~{}, got {}",
                expected, result.len());
    }
    
    #[test]
    fn test_calculate_output_length() {
        let resampler = AudioResampler::new(48000, 16000, 1).unwrap();
        
        // 48000 samples (1 sec) -> 16000 samples
        assert_eq!(resampler.calculate_output_length(48000), 16000);
        
        // 4800 samples (100ms) -> 1600 samples  
        assert_eq!(resampler.calculate_output_length(4800), 1600);
    }
}
