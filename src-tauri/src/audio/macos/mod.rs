// macOS Audio Module
// ScreenCaptureKit and Virtual Audio Endpoint implementations

#[cfg(target_os = "macos")]
pub mod screencapture;

#[cfg(target_os = "macos")]
pub mod virtual_audio;

// Re-exports for macOS
#[cfg(target_os = "macos")]
pub use screencapture::*;

#[cfg(target_os = "macos")]
pub use virtual_audio::*;
