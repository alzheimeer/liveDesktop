// Audio playback
// Platform-agnostic interface for audio playback

/// Trait for audio playback implementations
/// 
/// This trait defines the common interface for audio playback across platforms.
/// Implementations include:
/// - Windows: VBCablePlayback (routes to VB-Cable Output)
/// - macOS: VirtualAudioPlayback (routes to Virtual Audio Endpoint)
pub trait AudioPlayback {
    /// Start playback on the audio device.
    fn start(&mut self) -> Result<(), String>;
    
    /// Stop playback.
    fn stop(&mut self) -> Result<(), String>;
    
    /// Write audio samples to the playback buffer.
    /// 
    /// # Arguments
    /// * `samples` - PCM16 samples to write (expected format: 24kHz mono)
    fn write_buffer(&mut self, samples: &[i16]) -> Result<(), String>;
    
    /// Get the current playback latency in milliseconds.
    fn get_latency_ms(&self) -> u32;
    
    /// Check if playback is currently active.
    fn is_active(&self) -> bool;
    
    /// Check if the playback device has been disconnected.
    fn is_disconnected(&self) -> bool;
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::AudioPlayback;
    use crate::audio::windows::vbcable::VBCablePlayback;
/// Wrapper around VBCablePlayback that implements AudioPlayback trait
    pub struct VBCablePlaybackWrapper {
        inner: Option<VBCablePlayback>,
    }

    impl VBCablePlaybackWrapper {
        /// Create a new VBCablePlaybackWrapper (playback not started yet)
        pub fn new() -> Self {
            Self { inner: None }
        }

        /// Check if VB-Cable Output is available for playback
        pub fn is_available() -> bool {
            crate::audio::windows::vbcable::is_output_available()
        }
    }

    impl Default for VBCablePlaybackWrapper {
        fn default() -> Self {
            Self::new()
        }
    }

    impl AudioPlayback for VBCablePlaybackWrapper {
        fn start(&mut self) -> Result<(), String> {
            if self.inner.is_some() {
                return Ok(()); // Already started
            }

            match VBCablePlayback::start() {
                Ok(playback) => {
                    self.inner = Some(playback);
                    Ok(())
                }
                Err(e) => Err(format!("{}", e)),
            }
        }

        fn stop(&mut self) -> Result<(), String> {
            if let Some(ref mut playback) = self.inner {
                playback.stop().map_err(|e| format!("{}", e))?;
            }
            self.inner = None;
            Ok(())
        }

        fn write_buffer(&mut self, samples: &[i16]) -> Result<(), String> {
            match &self.inner {
                Some(playback) => {
                    playback.write_samples(samples).map_err(|e| format!("{}", e))
                }
                None => Err("Playback not started".to_string()),
            }
        }

        fn get_latency_ms(&self) -> u32 {
            self.inner.as_ref().map(|p| p.get_latency_ms()).unwrap_or(0)
        }

        fn is_active(&self) -> bool {
            self.inner.as_ref().map(|p| p.is_active()).unwrap_or(false)
        }

        fn is_disconnected(&self) -> bool {
            self.inner.as_ref().map(|p| p.is_disconnected()).unwrap_or(false)
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::VBCablePlaybackWrapper;
