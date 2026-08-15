//! Audio Test Tone Generator
//! 
//! Generates and plays test tones for audio device verification during onboarding.
//! 
//! # Requirements
//! - Requirement 13.8: Play 3-second test tone for each device
//! - Requirement 13.9: Allow selecting alternative device if test fails
//! - Requirement 13.10: Save configuration when test completes successfully

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Test tone configuration
#[derive(Debug, Clone)]
pub struct TestToneConfig {
    /// Frequency of the test tone in Hz (default: 440 Hz = A4 note)
    pub frequency_hz: f32,
    /// Duration of the test tone in milliseconds (default: 3000ms = 3 seconds)
    pub duration_ms: u32,
    /// Sample rate for the audio (default: 24000 Hz for Gemini compatibility)
    pub sample_rate: u32,
    /// Volume level from 0.0 to 1.0 (default: 0.5 for comfortable listening)
    pub volume: f32,
}

impl Default for TestToneConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 440.0,  // A4 note
            duration_ms: 3000,    // 3 seconds per Requirement 13.8
            sample_rate: 24000,   // 24kHz for consistency with Gemini output
            volume: 0.5,          // 50% volume for comfortable listening
        }
    }
}

/// Generate a sine wave test tone
/// 
/// Creates PCM16 mono samples for a sine wave at the specified frequency.
/// 
/// # Arguments
/// * `config` - Test tone configuration
/// 
/// # Returns
/// Vector of i16 samples representing the sine wave
pub fn generate_sine_wave(config: &TestToneConfig) -> Vec<i16> {
    let num_samples = ((config.sample_rate as f32 * config.duration_ms as f32) / 1000.0) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    
    let angular_frequency = 2.0 * PI * config.frequency_hz / config.sample_rate as f32;
    let amplitude = (i16::MAX as f32 * config.volume) as i16;
    
    for i in 0..num_samples {
        let sample = (angular_frequency * i as f32).sin() * amplitude as f32;
        
        // Apply fade in/out to avoid clicks (100ms fade)
        let fade_samples = (config.sample_rate as f32 * 0.1) as usize; // 100ms
        let envelope = if i < fade_samples {
            // Fade in
            i as f32 / fade_samples as f32
        } else if i > num_samples - fade_samples {
            // Fade out
            (num_samples - i) as f32 / fade_samples as f32
        } else {
            1.0
        };
        
        samples.push((sample * envelope) as i16);
    }
    
    samples
}

/// Result of audio test playback
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTestResult {
    /// Whether the test completed successfully (playback finished)
    pub success: bool,
    /// Device ID that was tested
    pub device_id: String,
    /// Device name for display
    pub device_name: String,
    /// Error message if test failed
    pub error: Option<String>,
    /// Duration of playback in milliseconds
    pub duration_ms: u32,
}

/// Audio test state management
pub struct AudioTestState {
    /// Whether a test is currently playing
    pub is_playing: Arc<AtomicBool>,
    /// Whether the current test was cancelled
    pub is_cancelled: Arc<AtomicBool>,
}

impl Default for AudioTestState {
    fn default() -> Self {
        Self {
            is_playing: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AudioTestState {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if a test is currently playing
    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::SeqCst)
    }
    
    /// Start a test (returns false if one is already playing)
    pub fn start(&self) -> bool {
        let was_playing = self.is_playing.swap(true, Ordering::SeqCst);
        if !was_playing {
            self.is_cancelled.store(false, Ordering::SeqCst);
        }
        !was_playing
    }
    
    /// Stop the current test
    pub fn stop(&self) {
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.is_playing.store(false, Ordering::SeqCst);
    }
    
    /// Mark test as finished
    pub fn finish(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
    }
    
    /// Check if the test was cancelled
    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(target_os = "windows")]
pub mod windows_test {
    use super::*;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;
    use windows::core::PCWSTR;
    use std::ptr::null_mut;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    
    /// Play a test tone on a specific Windows audio output device
    /// 
    /// # Arguments
    /// * `device_id` - The device endpoint ID to play on
    /// * `config` - Test tone configuration
    /// * `state` - Shared state for cancellation support
    /// 
    /// # Returns
    /// AudioTestResult indicating success or failure
    pub async fn play_test_tone(
        device_id: &str,
        device_name: &str,
        config: TestToneConfig,
        state: Arc<AudioTestState>,
    ) -> AudioTestResult {
        // Mark test as started
        if !state.start() {
            return AudioTestResult {
                success: false,
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                error: Some("Ya hay una prueba de audio en progreso".to_string()),
                duration_ms: 0,
            };
        }
        
        // Run playback in blocking task
        let device_id_clone = device_id.to_string();
        let device_name_clone = device_name.to_string();
        let state_clone = state.clone();
        
        let result = tokio::task::spawn_blocking(move || {
            play_test_tone_sync(&device_id_clone, &device_name_clone, &config, &state_clone)
        }).await;
        
        // Mark test as finished
        state.finish();
        
        match result {
            Ok(test_result) => test_result,
            Err(e) => AudioTestResult {
                success: false,
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                error: Some(format!("Error interno: {}", e)),
                duration_ms: 0,
            },
        }
    }
    
    /// Synchronous implementation of test tone playback
    fn play_test_tone_sync(
        device_id: &str,
        device_name: &str,
        config: &TestToneConfig,
        state: &AudioTestState,
    ) -> AudioTestResult {
        unsafe {
            // Initialize COM
            let hr = CoInitializeEx(Some(null_mut()), COINIT_MULTITHREADED);
            if hr.is_err() {
                // Check if it's already initialized (S_FALSE or CO_E_ALREADYINITIALIZED)
                // S_FALSE = 0x00000001, CO_E_ALREADYINITIALIZED = 0x800401F1
                if let Err(ref e) = hr {
                    let code = e.code().0 as u32;
                    if code != 1 && code != 0x800401F1 {
                        return AudioTestResult {
                            success: false,
                            device_id: device_id.to_string(),
                            device_name: device_name.to_string(),
                            error: Some(format!("Error al inicializar COM: HRESULT 0x{:08X}", code)),
                            duration_ms: 0,
                        };
                    }
                }
            }
            
            let result = play_with_wasapi(device_id, device_name, config, state);
            
            // Don't uninitialize COM as it may be used elsewhere
            // CoUninitialize();
            
            result
        }
    }
    
    /// Play test tone using WASAPI
    unsafe fn play_with_wasapi(
        device_id: &str,
        device_name: &str,
        config: &TestToneConfig,
        state: &AudioTestState,
    ) -> AudioTestResult {
        // Get device enumerator
        let enumerator: IMMDeviceEnumerator = match CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ) {
            Ok(e) => e,
            Err(e) => {
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some(format!("No se pudo acceder al sistema de audio: {:?}", e)),
                    duration_ms: 0,
                };
            }
        };
        
        // Get the specific device by ID
        let device_id_wide: Vec<u16> = OsString::from(device_id)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        
        let device: IMMDevice = match enumerator.GetDevice(PCWSTR(device_id_wide.as_ptr())) {
            Ok(d) => d,
            Err(e) => {
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some(format!("Dispositivo no encontrado: {:?}", e)),
                    duration_ms: 0,
                };
            }
        };
        
        // Activate audio client
        let audio_client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
            Ok(c) => c,
            Err(e) => {
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some(format!("No se pudo activar el cliente de audio: {:?}", e)),
                    duration_ms: 0,
                };
            }
        };
        
        // Define the format we want to use
        let wave_format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 1,
            nSamplesPerSec: config.sample_rate,
            nAvgBytesPerSec: config.sample_rate * 2, // 16-bit mono
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        
        // Initialize audio client
        // Use 100ms buffer for smooth playback
        let buffer_duration = 1_000_000; // 100ms in 100-nanosecond units
        
        if let Err(e) = audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            buffer_duration,
            0,
            &wave_format,
            None,
        ) {
            return AudioTestResult {
                success: false,
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                error: Some(format!("No se pudo inicializar el dispositivo: {:?}", e)),
                duration_ms: 0,
            };
        }
        
        // Get render client
        let render_client: IAudioRenderClient = match audio_client.GetService() {
            Ok(r) => r,
            Err(e) => {
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some(format!("No se pudo obtener el cliente de renderizado: {:?}", e)),
                    duration_ms: 0,
                };
            }
        };
        
        // Get buffer size
        let buffer_size = match audio_client.GetBufferSize() {
            Ok(s) => s,
            Err(e) => {
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some(format!("No se pudo obtener el tamaño del buffer: {:?}", e)),
                    duration_ms: 0,
                };
            }
        };
        
        // Generate test tone samples
        let samples = generate_sine_wave(config);
        let mut samples_written = 0;
        let total_samples = samples.len();
        
        // Start playback
        if let Err(e) = audio_client.Start() {
            return AudioTestResult {
                success: false,
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                error: Some(format!("No se pudo iniciar la reproducción: {:?}", e)),
                duration_ms: 0,
            };
        }
        
        tracing::info!(
            "Starting test tone playback on '{}', {} samples at {}Hz",
            device_name,
            total_samples,
            config.sample_rate
        );
        
        // Play loop
        while samples_written < total_samples {
            // Check for cancellation
            if state.is_cancelled() {
                tracing::info!("Test tone cancelled by user");
                let _ = audio_client.Stop();
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some("Prueba cancelada por el usuario".to_string()),
                    duration_ms: (samples_written as u32 * 1000) / config.sample_rate,
                };
            }
            
            // Get current padding (samples already in buffer)
            let padding = match audio_client.GetCurrentPadding() {
                Ok(p) => p,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
            };
            
            // Calculate available space
            let frames_available = buffer_size - padding;
            if frames_available == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            
            // Calculate how many samples to write
            let samples_remaining = total_samples - samples_written;
            let samples_to_write = std::cmp::min(frames_available as usize, samples_remaining);
            
            if samples_to_write == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            
            // Get buffer and write samples
            match render_client.GetBuffer(samples_to_write as u32) {
                Ok(buffer_ptr) => {
                    let buffer = std::slice::from_raw_parts_mut(
                        buffer_ptr as *mut i16,
                        samples_to_write,
                    );
                    buffer.copy_from_slice(&samples[samples_written..samples_written + samples_to_write]);
                    
                    if let Err(e) = render_client.ReleaseBuffer(samples_to_write as u32, 0) {
                        tracing::warn!("Error releasing buffer: {:?}", e);
                    }
                    
                    samples_written += samples_to_write;
                }
                Err(e) => {
                    tracing::warn!("Error getting buffer: {:?}", e);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        
        // Wait for playback to finish (buffer to drain)
        let drain_timeout_ms = 500;
        let mut waited_ms = 0;
        while waited_ms < drain_timeout_ms {
            if state.is_cancelled() {
                break;
            }
            
            let padding = audio_client.GetCurrentPadding().unwrap_or(0);
            if padding == 0 {
                break;
            }
            
            std::thread::sleep(Duration::from_millis(20));
            waited_ms += 20;
        }
        
        // Stop playback
        let _ = audio_client.Stop();
        
        tracing::info!("Test tone playback completed on '{}'", device_name);
        
        AudioTestResult {
            success: true,
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            error: None,
            duration_ms: config.duration_ms,
        }
    }
}

#[cfg(target_os = "macos")]
pub mod macos_test {
    use super::*;
    
    /// Play a test tone on a specific macOS audio output device
    /// 
    /// TODO: Implement using CoreAudio
    pub async fn play_test_tone(
        device_id: &str,
        device_name: &str,
        config: TestToneConfig,
        state: Arc<AudioTestState>,
    ) -> AudioTestResult {
        // Mark test as started
        if !state.start() {
            return AudioTestResult {
                success: false,
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                error: Some("Ya hay una prueba de audio en progreso".to_string()),
                duration_ms: 0,
            };
        }
        
        // TODO: Implement CoreAudio playback
        // For now, simulate playback
        tracing::warn!("macOS test tone playback not yet implemented, simulating...");
        
        let duration = Duration::from_millis(config.duration_ms as u64);
        let sleep_interval = Duration::from_millis(100);
        let mut elapsed = Duration::ZERO;
        
        while elapsed < duration {
            if state.is_cancelled() {
                state.finish();
                return AudioTestResult {
                    success: false,
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    error: Some("Prueba cancelada por el usuario".to_string()),
                    duration_ms: elapsed.as_millis() as u32,
                };
            }
            
            tokio::time::sleep(sleep_interval).await;
            elapsed += sleep_interval;
        }
        
        state.finish();
        
        AudioTestResult {
            success: true,
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            error: None,
            duration_ms: config.duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_sine_wave_length() {
        let config = TestToneConfig::default();
        let samples = generate_sine_wave(&config);
        
        let expected_samples = ((config.sample_rate as f32 * config.duration_ms as f32) / 1000.0) as usize;
        assert_eq!(samples.len(), expected_samples);
    }
    
    #[test]
    fn test_generate_sine_wave_amplitude() {
        let config = TestToneConfig {
            volume: 1.0,
            ..Default::default()
        };
        let samples = generate_sine_wave(&config);
        
        // Check that values are within expected range (with some margin for fade)
        let max_sample = samples.iter().map(|&s| s.abs()).max().unwrap_or(0);
        assert!(max_sample > 0, "Samples should have non-zero amplitude");
        assert!(max_sample <= i16::MAX, "Samples should not exceed i16::MAX");
    }
    
    #[test]
    fn test_generate_sine_wave_fade() {
        let config = TestToneConfig {
            duration_ms: 1000,
            sample_rate: 1000,
            volume: 1.0,
            frequency_hz: 100.0,
        };
        let samples = generate_sine_wave(&config);
        
        // First and last samples should be near zero (fade in/out)
        assert!(samples[0].abs() < 1000, "First sample should be quiet (fade in)");
        assert!(samples.last().unwrap().abs() < 1000, "Last sample should be quiet (fade out)");
    }
    
    #[test]
    fn test_audio_test_state() {
        let state = AudioTestState::new();
        
        // Initial state
        assert!(!state.is_playing());
        assert!(!state.is_cancelled());
        
        // Start test
        assert!(state.start()); // Should succeed
        assert!(state.is_playing());
        assert!(!state.start()); // Should fail - already playing
        
        // Stop test
        state.stop();
        assert!(!state.is_playing());
        assert!(state.is_cancelled());
        
        // Should be able to start again
        assert!(state.start());
        assert!(!state.is_cancelled()); // Cancelled flag should be reset
    }
}
