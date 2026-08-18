//! Gemini Live Protocol Definitions
//!
//! Message formats for WebSocket communication with Gemini Live API.
//! All messages are JSON-serialized according to the Gemini Live API specification.

use serde::{Deserialize, Serialize};

// ==========================================
// Setup Messages (Client → Server)
// ==========================================

/// Initial setup message sent on connection
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupMessage {
    pub setup: SetupConfig,
}

/// Setup configuration for the translation session
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupConfig {
    /// Model name (e.g., "models/gemini-3.5-live-translate-preview")
    pub model: String,
    /// Generation configuration for the session
    pub generation_config: GenerationConfig,
}

/// Generation configuration for audio output
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Response modalities (e.g., ["AUDIO"])
    pub response_modalities: Vec<String>,
    /// Speech configuration for TTS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<SpeechConfig>,
    /// Translation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_config: Option<TranslationConfig>,
}

/// Translation configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationConfig {
    pub target_language_code: String,
    pub echo_target_language: bool,
}

/// Speech configuration for text-to-speech output
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    /// Voice configuration
    pub voice_config: VoiceConfig,
    /// Target language code (ISO 639-1, e.g., "es")
    pub language_code: String,
}

/// Voice configuration for TTS
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    /// Prebuilt voice configuration
    pub prebuilt_voice_config: PrebuiltVoiceConfig,
}

/// Prebuilt voice selection
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    /// Voice name (e.g., "Aoede", "Puck", "Charon", "Fenrir", "Kore")
    pub voice_name: String,
}

// ==========================================
// Audio Input Messages (Client → Server)
// ==========================================

/// Audio input message containing PCM data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputMessage {
    pub realtime_input: RealtimeInput,
}

/// Real-time audio input wrapper
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeInput {
    /// Media chunks containing audio data
    pub media_chunks: Vec<MediaChunk>,
}

/// Single media chunk with encoded audio
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaChunk {
    /// MIME type (e.g., "audio/pcm;rate=16000")
    pub mime_type: String,
    /// Base64-encoded PCM audio data
    pub data: String,
}

// ==========================================
// Server Response Messages (Server → Client)
// ==========================================

/// Server response message
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerResponse {
    /// True when setup is complete
    pub setup_complete: Option<bool>,
    /// Content from the model
    pub server_content: Option<ServerContent>,
    /// Error information if any
    pub error: Option<ServerError>,
}

/// Server error information
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerError {
    /// Error code
    pub code: Option<i32>,
    /// Error message
    pub message: Option<String>,
    /// Error status
    pub status: Option<String>,
}

/// Content from the server/model
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerContent {
    /// Model turn with generated content
    pub model_turn: Option<ModelTurn>,
    /// True when the model's turn is complete
    pub turn_complete: Option<bool>,
    /// True when the model was interrupted
    pub interrupted: Option<bool>,
}

/// Model turn containing generated parts
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelTurn {
    /// Parts of the response (audio, text, etc.)
    pub parts: Vec<Part>,
}

/// Single part of a model response
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Part {
    /// Inline data (for audio responses)
    pub inline_data: Option<InlineData>,
    /// Text content (for text responses)
    pub text: Option<String>,
}

/// Inline data containing audio or other binary content
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    /// MIME type (e.g., "audio/pcm;rate=24000")
    pub mime_type: String,
    /// Base64-encoded data
    pub data: String,
}

// ==========================================
// Tool/Function Messages (Future use)
// ==========================================

/// Tool call from the model (for future translation tools)
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    /// Function call details
    pub function_call: Option<FunctionCall>,
}

/// Function call details
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments as JSON
    pub args: serde_json::Value,
}

/// Tool response to send back to the model
#[derive(Debug, Clone, Serialize)]
pub struct ToolResponse {
    pub tool_response: ToolResponseContent,
}

/// Tool response content
#[derive(Debug, Clone, Serialize)]
pub struct ToolResponseContent {
    pub function_responses: Vec<FunctionResponse>,
}

/// Single function response
#[derive(Debug, Clone, Serialize)]
pub struct FunctionResponse {
    /// Correlation ID from the function call
    pub id: String,
    /// Response content
    pub response: serde_json::Value,
}

// ==========================================
// Client Control Messages
// ==========================================

/// End of turn marker (signals end of user input)
#[derive(Debug, Clone, Serialize)]
pub struct EndOfTurn {
    pub client_content: ClientContent,
}

/// Client content for turn completion
#[derive(Debug, Clone, Serialize)]
pub struct ClientContent {
    pub turn_complete: bool,
}

impl EndOfTurn {
    /// Create a new end-of-turn message
    pub fn new() -> Self {
        Self {
            client_content: ClientContent { turn_complete: true },
        }
    }
}

impl Default for EndOfTurn {
    fn default() -> Self {
        Self::new()
    }
}

// ==========================================
// Helper Functions
// ==========================================

/// Available voice names for Gemini Live TTS
pub const AVAILABLE_VOICES: &[&str] = &[
    "Aoede",   // Female, warm
    "Puck",    // Male, energetic
    "Charon",  // Male, deep
    "Fenrir",  // Male, authoritative
    "Kore",    // Female, bright
];

/// Check if a voice name is valid
pub fn is_valid_voice(voice_name: &str) -> bool {
    AVAILABLE_VOICES.iter().any(|&v| v.eq_ignore_ascii_case(voice_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_message_serialization() {
        let setup = SetupMessage {
            setup: SetupConfig {
                model: "models/gemini-3.5-live-translate-preview".to_string(),
                generation_config: GenerationConfig {
                    response_modalities: vec!["AUDIO".to_string()],
                    speech_config: SpeechConfig {
                        voice_config: VoiceConfig {
                            prebuilt_voice_config: PrebuiltVoiceConfig {
                                voice_name: "Aoede".to_string(),
                            },
                        },
                        language_code: "es".to_string(),
                    },
                },
            },
        };

        let json = serde_json::to_string(&setup).unwrap();
        assert!(json.contains("gemini-3.5-live-translate-preview"));
        assert!(json.contains("AUDIO"));
        assert!(json.contains("Aoede"));
        assert!(json.contains("es"));
    }

    #[test]
    fn test_audio_input_message_serialization() {
        let audio_msg = AudioInputMessage {
            realtime_input: RealtimeInput {
                media_chunks: vec![MediaChunk {
                    mime_type: "audio/pcm;rate=16000".to_string(),
                    data: "dGVzdCBkYXRh".to_string(), // "test data" in base64
                }],
            },
        };

        let json = serde_json::to_string(&audio_msg).unwrap();
        assert!(json.contains("realtime_input"));
        assert!(json.contains("media_chunks"));
        assert!(json.contains("audio/pcm;rate=16000"));
    }

    #[test]
    fn test_server_response_deserialization() {
        let json = r#"{"setup_complete": true}"#;
        let response: ServerResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.setup_complete, Some(true));
    }

    #[test]
    fn test_server_content_with_audio() {
        let json = r#"{
            "server_content": {
                "model_turn": {
                    "parts": [{
                        "inline_data": {
                            "mime_type": "audio/pcm;rate=24000",
                            "data": "AAAA"
                        }
                    }]
                },
                "turn_complete": true
            }
        }"#;

        let response: ServerResponse = serde_json::from_str(json).unwrap();
        let content = response.server_content.unwrap();
        assert_eq!(content.turn_complete, Some(true));
        
        let turn = content.model_turn.unwrap();
        assert_eq!(turn.parts.len(), 1);
        
        let inline = turn.parts[0].inline_data.as_ref().unwrap();
        assert!(inline.mime_type.contains("24000"));
    }

    #[test]
    fn test_end_of_turn_serialization() {
        let end = EndOfTurn::new();
        let json = serde_json::to_string(&end).unwrap();
        
        assert!(json.contains("client_content"));
        assert!(json.contains("turn_complete"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_valid_voices() {
        assert!(is_valid_voice("Aoede"));
        assert!(is_valid_voice("PUCK")); // Case insensitive
        assert!(is_valid_voice("kore"));
        assert!(!is_valid_voice("InvalidVoice"));
    }

    #[test]
    fn test_server_response_with_error() {
        let json = r#"{
            "error": {
                "code": 401,
                "message": "Invalid API key",
                "status": "UNAUTHENTICATED"
            }
        }"#;

        let response: ServerResponse = serde_json::from_str(json).unwrap();
        let error = response.error.unwrap();
        assert_eq!(error.code, Some(401));
        assert!(error.message.unwrap().contains("Invalid"));
    }
}
