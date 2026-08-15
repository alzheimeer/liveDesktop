//! Gemini Live WebSocket Client
//!
//! Direct connection to Gemini Live API for real-time voice-to-voice translation.
//! Uses tokio-tungstenite for WebSocket communication with 5-second connection timeout.
//!
//! # Requirements
//!
//! - Requirement 6.1: WebSocket connection using tokio-tungstenite with 5s timeout
//! - Requirement 6.2: Use model `gemini-3.5-live-translate-preview` with API v1alpha

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{protocol::Message, Error as WsError},
};

use super::protocol::{
    AudioInputMessage, MediaChunk, RealtimeInput, ServerResponse, SetupConfig, SetupMessage,
    GenerationConfig, SpeechConfig, VoiceConfig, PrebuiltVoiceConfig,
};

/// WebSocket URL for Gemini Live API v1alpha
pub const GEMINI_WS_URL: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent";

/// Model name for live translation
pub const MODEL: &str = "gemini-3.5-live-translate-preview";

/// Alternative model for fallback
pub const MODEL_FALLBACK: &str = "gemini-2.0-flash-live-001";

/// Chunk size in milliseconds for audio streaming
pub const CHUNK_SIZE_MS: u32 = 20;

/// Chunk size in bytes for 16kHz mono PCM16 (320 samples × 2 bytes)
pub const CHUNK_SIZE_BYTES: usize = 640;

/// Connection timeout in seconds
const CONNECTION_TIMEOUT_SECS: u64 = 5;

/// Audio MIME type for 16kHz PCM input
const AUDIO_MIME_TYPE_INPUT: &str = "audio/pcm;rate=16000";

/// Audio MIME type for 24kHz PCM output
#[allow(dead_code)]
const AUDIO_MIME_TYPE_OUTPUT: &str = "audio/pcm;rate=24000";

/// Configuration for Gemini Live connection
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    /// Source language ISO 639-1 code (e.g., "en")
    pub source_lang: String,
    /// Target language ISO 639-1 code (e.g., "es")
    pub target_lang: String,
    /// Authentication token (ephemeral token or BYOK API key)
    pub token: String,
    /// Voice name for TTS output (optional)
    pub voice_name: Option<String>,
}

impl GeminiConfig {
    /// Create a new configuration with required fields
    pub fn new(source_lang: impl Into<String>, target_lang: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            source_lang: source_lang.into(),
            target_lang: target_lang.into(),
            token: token.into(),
            voice_name: None,
        }
    }

    /// Set voice name for TTS output
    pub fn with_voice(mut self, voice_name: impl Into<String>) -> Self {
        self.voice_name = Some(voice_name.into());
        self
    }
}

/// Errors that can occur during Gemini Live operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeminiError {
    /// WebSocket connection failed
    ConnectionFailed(String),
    /// Authentication failed (invalid token)
    AuthenticationFailed,
    /// Setup message failed
    SetupFailed(String),
    /// Error sending audio data
    SendError(String),
    /// Error receiving audio data
    ReceiveError(String),
    /// Connection timeout exceeded
    Timeout,
    /// Connection closed unexpectedly
    ConnectionClosed,
    /// Invalid response from server
    InvalidResponse(String),
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeminiError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            GeminiError::AuthenticationFailed => write!(f, "Authentication failed"),
            GeminiError::SetupFailed(msg) => write!(f, "Setup failed: {}", msg),
            GeminiError::SendError(msg) => write!(f, "Send error: {}", msg),
            GeminiError::ReceiveError(msg) => write!(f, "Receive error: {}", msg),
            GeminiError::Timeout => write!(f, "Connection timeout"),
            GeminiError::ConnectionClosed => write!(f, "Connection closed"),
            GeminiError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for GeminiError {}

/// Type alias for the WebSocket stream
type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Gemini Live WebSocket client
///
/// Handles bidirectional streaming communication with Gemini Live API
/// for real-time voice-to-voice translation.
pub struct GeminiLiveClient {
    /// Configuration for the connection
    config: GeminiConfig,
    /// WebSocket stream (Some when connected)
    ws_stream: Option<WsStream>,
    /// Whether the setup has been completed
    setup_complete: bool,
}

impl GeminiLiveClient {
    /// Create a new client with the given configuration
    ///
    /// The client is not connected until `connect()` is called.
    pub fn new(config: GeminiConfig) -> Self {
        Self {
            config,
            ws_stream: None,
            setup_complete: false,
        }
    }

    /// Build the WebSocket URL with authentication
    fn build_ws_url(&self) -> String {
        format!("{}?key={}", GEMINI_WS_URL, self.config.token)
    }

    /// Connect to Gemini Live with a 5-second timeout
    ///
    /// Establishes WebSocket connection and sends the setup message
    /// to configure the translation session.
    ///
    /// # Errors
    ///
    /// Returns `GeminiError::Timeout` if connection takes longer than 5 seconds.
    /// Returns `GeminiError::ConnectionFailed` for network or WebSocket errors.
    /// Returns `GeminiError::SetupFailed` if the setup message is rejected.
    pub async fn connect(&mut self) -> Result<(), GeminiError> {
        let url = self.build_ws_url();

        // Connect with timeout
        let connect_future = connect_async(&url);
        let result = timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECS), connect_future)
            .await
            .map_err(|_| GeminiError::Timeout)?
            .map_err(|e| self.map_ws_error(e))?;

        let (ws_stream, _response) = result;
        self.ws_stream = Some(ws_stream);

        // Send setup message
        self.send_setup_message().await?;

        // Wait for setup complete response
        self.wait_for_setup_complete().await?;

        tracing::info!(
            target: "gemini",
            "Connected to Gemini Live: {} -> {}",
            self.config.source_lang,
            self.config.target_lang
        );

        Ok(())
    }

    /// Send the setup message to configure the translation session
    async fn send_setup_message(&mut self) -> Result<(), GeminiError> {
        let voice_name = self.config.voice_name.clone()
            .unwrap_or_else(|| "Aoede".to_string());

        let setup_msg = SetupMessage {
            setup: SetupConfig {
                model: format!("models/{}", MODEL),
                generation_config: GenerationConfig {
                    response_modalities: vec!["AUDIO".to_string()],
                    speech_config: SpeechConfig {
                        voice_config: VoiceConfig {
                            prebuilt_voice_config: PrebuiltVoiceConfig {
                                voice_name,
                            },
                        },
                        language_code: self.config.target_lang.clone(),
                    },
                },
            },
        };

        let json = serde_json::to_string(&setup_msg)
            .map_err(|e| GeminiError::SetupFailed(format!("Failed to serialize setup: {}", e)))?;

        self.send_text(&json).await?;

        tracing::debug!(target: "gemini", "Setup message sent");
        Ok(())
    }

    /// Wait for the setup complete message from the server
    async fn wait_for_setup_complete(&mut self) -> Result<(), GeminiError> {
        let timeout_duration = Duration::from_secs(CONNECTION_TIMEOUT_SECS);

        let receive_future = async {
            loop {
                if let Some(msg) = self.receive_message().await? {
                    if let Message::Text(text) = msg {
                        let response: ServerResponse = serde_json::from_str(&text)
                            .map_err(|e| GeminiError::InvalidResponse(format!("Parse error: {}", e)))?;

                        if response.setup_complete == Some(true) {
                            self.setup_complete = true;
                            tracing::debug!(target: "gemini", "Setup complete received");
                            return Ok(());
                        }
                    }
                }
            }
        };

        timeout(timeout_duration, receive_future)
            .await
            .map_err(|_| GeminiError::SetupFailed("Setup timeout".to_string()))?
    }

    /// Send audio chunk to Gemini Live
    ///
    /// # Arguments
    ///
    /// * `samples` - PCM16 audio samples at 16kHz mono
    ///
    /// # Requirements
    ///
    /// Audio should be sent in 20ms chunks (320 samples = 640 bytes).
    pub async fn send_audio(&mut self, samples: &[i16]) -> Result<(), GeminiError> {
        if !self.is_connected() {
            return Err(GeminiError::ConnectionClosed);
        }

        if !self.setup_complete {
            return Err(GeminiError::SetupFailed("Setup not complete".to_string()));
        }

        // Convert samples to bytes (little-endian PCM16)
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        // Base64 encode the audio data
        let encoded = BASE64.encode(&bytes);

        // Create audio input message
        let audio_msg = AudioInputMessage {
            realtime_input: RealtimeInput {
                media_chunks: vec![MediaChunk {
                    mime_type: AUDIO_MIME_TYPE_INPUT.to_string(),
                    data: encoded,
                }],
            },
        };

        let json = serde_json::to_string(&audio_msg)
            .map_err(|e| GeminiError::SendError(format!("Serialize error: {}", e)))?;

        self.send_text(&json).await
    }

    /// Receive translated audio from Gemini Live
    ///
    /// # Returns
    ///
    /// - `Ok(Some(samples))` - Translated audio samples at 24kHz PCM16
    /// - `Ok(None)` - No audio available (turn not complete, or other message type)
    /// - `Err(_)` - Connection or parsing error
    pub async fn receive_audio(&mut self) -> Result<Option<Vec<i16>>, GeminiError> {
        if !self.is_connected() {
            return Err(GeminiError::ConnectionClosed);
        }

        let msg = self.receive_message().await?;

        match msg {
            Some(Message::Text(text)) => {
                let response: ServerResponse = serde_json::from_str(&text)
                    .map_err(|e| GeminiError::InvalidResponse(format!("Parse error: {}", e)))?;

                // Extract audio from server content
                if let Some(content) = response.server_content {
                    if let Some(model_turn) = content.model_turn {
                        for part in model_turn.parts {
                            if let Some(inline_data) = part.inline_data {
                                if inline_data.mime_type.starts_with("audio/pcm") {
                                    // Decode base64 audio
                                    let bytes = BASE64.decode(&inline_data.data)
                                        .map_err(|e| GeminiError::ReceiveError(format!("Base64 decode error: {}", e)))?;

                                    // Convert bytes to samples (little-endian PCM16)
                                    let samples: Vec<i16> = bytes
                                        .chunks_exact(2)
                                        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                                        .collect();

                                    return Ok(Some(samples));
                                }
                            }
                        }
                    }
                }

                Ok(None)
            }
            Some(Message::Binary(data)) => {
                // Some responses may come as binary PCM directly
                if data.len() % 2 == 0 {
                    let samples: Vec<i16> = data
                        .chunks_exact(2)
                        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                        .collect();
                    return Ok(Some(samples));
                }
                Ok(None)
            }
            Some(Message::Close(_)) => {
                self.ws_stream = None;
                self.setup_complete = false;
                Err(GeminiError::ConnectionClosed)
            }
            _ => Ok(None),
        }
    }

    /// Receive audio with a timeout
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait for audio in milliseconds
    pub async fn receive_audio_timeout(&mut self, timeout_ms: u64) -> Result<Option<Vec<i16>>, GeminiError> {
        match timeout(Duration::from_millis(timeout_ms), self.receive_audio()).await {
            Ok(result) => result,
            Err(_) => Ok(None), // Timeout returns None, not an error
        }
    }

    /// Close the connection gracefully
    pub async fn close(&mut self) -> Result<(), GeminiError> {
        if let Some(ref mut ws) = self.ws_stream {
            let _ = ws.close(None).await;
        }
        self.ws_stream = None;
        self.setup_complete = false;

        tracing::info!(target: "gemini", "Connection closed");
        Ok(())
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.ws_stream.is_some()
    }

    /// Check if setup has been completed
    pub fn is_setup_complete(&self) -> bool {
        self.setup_complete
    }

    /// Get the current configuration
    pub fn config(&self) -> &GeminiConfig {
        &self.config
    }

    /// Update the configuration (requires reconnection)
    pub fn set_config(&mut self, config: GeminiConfig) {
        self.config = config;
    }

    // ========================================
    // Private helper methods
    // ========================================

    /// Send a text message through the WebSocket
    async fn send_text(&mut self, text: &str) -> Result<(), GeminiError> {
        if let Some(ref mut ws) = self.ws_stream {
            ws.send(Message::Text(text.to_string()))
                .await
                .map_err(|e| GeminiError::SendError(format!("WebSocket send error: {}", e)))?;
            Ok(())
        } else {
            Err(GeminiError::ConnectionClosed)
        }
    }

    /// Receive a message from the WebSocket
    async fn receive_message(&mut self) -> Result<Option<Message>, GeminiError> {
        if let Some(ref mut ws) = self.ws_stream {
            match ws.next().await {
                Some(Ok(msg)) => Ok(Some(msg)),
                Some(Err(e)) => Err(GeminiError::ReceiveError(format!("WebSocket receive error: {}", e))),
                None => {
                    self.ws_stream = None;
                    self.setup_complete = false;
                    Err(GeminiError::ConnectionClosed)
                }
            }
        } else {
            Err(GeminiError::ConnectionClosed)
        }
    }

    /// Map WebSocket errors to GeminiError
    fn map_ws_error(&self, error: WsError) -> GeminiError {
        match error {
            WsError::Http(response) => {
                let status = response.status();
                if status == 401 || status == 403 {
                    GeminiError::AuthenticationFailed
                } else {
                    GeminiError::ConnectionFailed(format!("HTTP error: {}", status))
                }
            }
            WsError::ConnectionClosed => GeminiError::ConnectionClosed,
            WsError::Tls(e) => GeminiError::ConnectionFailed(format!("TLS error: {}", e)),
            WsError::Io(e) => GeminiError::ConnectionFailed(format!("IO error: {}", e)),
            _ => GeminiError::ConnectionFailed(format!("WebSocket error: {}", error)),
        }
    }
}

impl Drop for GeminiLiveClient {
    fn drop(&mut self) {
        // Note: We can't do async cleanup in Drop, but the WebSocket
        // will be closed when dropped anyway
        self.ws_stream = None;
        self.setup_complete = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_config_new() {
        let config = GeminiConfig::new("en", "es", "test_token");
        
        assert_eq!(config.source_lang, "en");
        assert_eq!(config.target_lang, "es");
        assert_eq!(config.token, "test_token");
        assert!(config.voice_name.is_none());
    }

    #[test]
    fn test_gemini_config_with_voice() {
        let config = GeminiConfig::new("en", "es", "test_token")
            .with_voice("Puck");
        
        assert_eq!(config.voice_name, Some("Puck".to_string()));
    }

    #[test]
    fn test_build_ws_url() {
        let config = GeminiConfig::new("en", "es", "my_api_key_123");
        let client = GeminiLiveClient::new(config);
        
        let url = client.build_ws_url();
        
        assert!(url.starts_with(GEMINI_WS_URL));
        assert!(url.contains("key=my_api_key_123"));
    }

    #[test]
    fn test_client_initial_state() {
        let config = GeminiConfig::new("en", "es", "token");
        let client = GeminiLiveClient::new(config);
        
        assert!(!client.is_connected());
        assert!(!client.is_setup_complete());
    }

    #[test]
    fn test_gemini_error_display() {
        let errors = vec![
            (GeminiError::Timeout, "Connection timeout"),
            (GeminiError::AuthenticationFailed, "Authentication failed"),
            (GeminiError::ConnectionClosed, "Connection closed"),
        ];

        for (error, expected_substr) in errors {
            let display = format!("{}", error);
            assert!(
                display.contains(expected_substr),
                "Expected '{}' to contain '{}'",
                display,
                expected_substr
            );
        }
    }

    #[test]
    fn test_constants() {
        // Verify constants match requirements
        assert_eq!(CHUNK_SIZE_MS, 20);
        assert_eq!(CHUNK_SIZE_BYTES, 640); // 320 samples * 2 bytes
        assert!(GEMINI_WS_URL.contains("v1alpha"));
        assert!(MODEL.contains("live"));
    }

    #[test]
    fn test_audio_conversion() {
        // Test that audio samples can be converted to bytes correctly
        let samples: Vec<i16> = vec![0, 100, -100, i16::MAX, i16::MIN];
        
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        
        assert_eq!(bytes.len(), samples.len() * 2);
        
        // Convert back
        let recovered: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        
        assert_eq!(samples, recovered);
    }
}
