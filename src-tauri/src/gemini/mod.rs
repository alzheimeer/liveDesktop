//! Gemini Live Client Module
//!
//! WebSocket connection to Gemini Live API for real-time voice-to-voice translation.
//!
//! # Modules
//!
//! - `client` - WebSocket client for Gemini Live API
//! - `protocol` - Message protocol definitions (JSON serialization)
//! - `session` - Session management with auto-reconnect

pub mod client;
pub mod protocol;
pub mod session;

// Re-export main types for convenience
pub use client::{GeminiConfig, GeminiError, GeminiLiveClient, GEMINI_WS_URL, MODEL};
pub use session::{ChannelType, GeminiSessionManager};
