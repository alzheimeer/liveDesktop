//! Audio Engine module
//!
//! Core audio capture, playback, and processing for translation channels.
//!
//! # Architecture
//!
//! - `engine` - Main AudioEngine managing two channels (system and user)
//! - `capture` - Platform-agnostic audio capture abstraction
//! - `playback` - Audio playback trait
//! - `resampler` - Sample rate conversion for Gemini compatibility
//! - `chunker` - Audio chunking for Gemini Live API (20ms / 640 bytes)
//! - `test_tone` - Audio test tone generation and playback for onboarding
//! - `windows` - Windows-specific WASAPI and VB-Cable integration
//! - `macos` - macOS-specific ScreenCaptureKit integration (planned)

pub mod engine;
pub mod capture;
pub mod playback;
pub mod resampler;
pub mod chunker;
pub mod test_tone;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

// Re-export main types for convenient access
pub use engine::{
    AudioDevice, AudioEngine, AudioMetrics, ChannelConfig, ChannelState, ChannelType,
    DeviceEvent, DeviceEventType, EngineState, PauseReason,
};

// Re-export chunker types
pub use chunker::{
    AudioChunker, ChunkIterator, CHUNK_BYTES, CHUNK_DURATION_MS, CHUNK_SAMPLES,
    bytes_to_samples, samples_to_bytes,
};
