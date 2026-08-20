// macOS Audio Module
// ScreenCaptureKit and Virtual Audio Endpoint implementations

#[cfg(target_os = "macos")]
pub mod screencapture;

#[cfg(target_os = "macos")]
pub mod virtual_audio;

// Re-exports for macOS (explicit to avoid ambiguity)
#[cfg(target_os = "macos")]
pub use screencapture::{ScreenCaptureAudio, MacOSVersion, ScreenCaptureError, PermissionStatus, StreamConfiguration, AudioSampleBuffer};

#[cfg(target_os = "macos")]
pub use virtual_audio::{VirtualAudioEndpoint, AudioResampler, BlackHoleInstructions, VirtualAudioError, VirtualAudioStatus, VirtualAudioConfig, AudioDeviceInfo, enumerate_audio_devices};
