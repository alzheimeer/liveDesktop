//! BYOK (Bring Your Own Key) Gemini Connector
//!
//! Provides direct connection to Gemini Live API using user's own API key
//! stored securely in the OS keyring. This module ensures:
//! - API key is retrieved only from the OS keyring
//! - API key is transmitted ONLY to Gemini's servers (never to any backend)
//! - No logging of API key values
//!
//! # Security Guarantees
//!
//! - API key stored in Windows Credential Manager / macOS Keychain
//! - API key transmitted via secure WebSocket (wss://) directly to Gemini
//! - No intermediate servers involved in BYOK mode
//!
//! # Requirements
//!
//! - Requirement 8.4: API key transmitted ONLY to Gemini API
//! - Requirement 8.5: Direct connection to Gemini Live using API key from Keyring
//! - Requirement 23.4: API key never sent to any server other than Gemini

use crate::gemini::client::{GeminiConfig, GeminiError, GeminiLiveClient};
use crate::gemini::session::{ChannelType, GeminiSessionManager};

use super::keyring::KeyringManager;

/// Errors that can occur during BYOK connection
#[derive(Debug, Clone)]
pub enum ByokConnectionError {
    /// No BYOK API key is stored in the keyring
    NoApiKeyStored,
    /// Failed to retrieve API key from keyring
    KeyringAccessError(String),
    /// Gemini connection failed
    ConnectionError(GeminiError),
}

impl std::fmt::Display for ByokConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ByokConnectionError::NoApiKeyStored => {
                write!(f, "No BYOK API key stored. Please configure your API key first.")
            }
            ByokConnectionError::KeyringAccessError(msg) => {
                write!(f, "Failed to access OS keyring: {}", msg)
            }
            ByokConnectionError::ConnectionError(err) => {
                write!(f, "Gemini connection error: {}", err)
            }
        }
    }
}

impl std::error::Error for ByokConnectionError {}

impl From<GeminiError> for ByokConnectionError {
    fn from(err: GeminiError) -> Self {
        ByokConnectionError::ConnectionError(err)
    }
}

/// BYOK Gemini Connector
///
/// Handles direct connections to Gemini Live API using the user's own API key.
/// The API key is retrieved from the OS keyring and transmitted ONLY to Gemini.
///
/// # Security Model
///
/// ```text
/// ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
/// │   OS Keyring    │────▶│ ByokConnector   │────▶│  Gemini Live    │
/// │ (Credential Mgr │     │ (In-Memory)     │     │  (wss://)       │
/// │  / Keychain)    │     │                 │     │                 │
/// └─────────────────┘     └─────────────────┘     └─────────────────┘
///                              │
///                              ▼
///                         ❌ NO transmission to
///                            any other server
/// ```
pub struct ByokGeminiConnector {
    /// Whether BYOK mode is currently active
    byok_active: bool,
}

impl ByokGeminiConnector {
    /// Create a new BYOK connector
    pub fn new() -> Self {
        Self { byok_active: false }
    }

    /// Check if BYOK mode is available (API key exists in keyring)
    ///
    /// This method checks the OS keyring without returning the actual key value.
    pub fn is_byok_available() -> bool {
        KeyringManager::has_byok_key().unwrap_or(false)
    }

    /// Check if BYOK mode is currently active
    pub fn is_byok_active(&self) -> bool {
        self.byok_active
    }

    /// Retrieve the BYOK API key from OS keyring
    ///
    /// # Security Note
    ///
    /// This method retrieves the API key from the OS keyring.
    /// The returned key should ONLY be used to connect to Gemini's servers.
    /// NEVER log, transmit to other servers, or store the key in any other location.
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - The API key from the keyring
    /// - `Err(ByokConnectionError)` - If no key exists or keyring access failed
    fn retrieve_api_key() -> Result<String, ByokConnectionError> {
        match KeyringManager::get_byok_key() {
            Ok(Some(key)) => {
                // Log that we retrieved a key, but NEVER log the key value
                tracing::debug!(target: "byok", "Retrieved BYOK API key from keyring");
                Ok(key)
            }
            Ok(None) => {
                tracing::warn!(target: "byok", "No BYOK API key found in keyring");
                Err(ByokConnectionError::NoApiKeyStored)
            }
            Err(e) => {
                tracing::error!(target: "byok", "Failed to access keyring: {}", e);
                Err(ByokConnectionError::KeyringAccessError(e.to_string()))
            }
        }
    }

    /// Create a GeminiConfig using the BYOK API key
    ///
    /// # Arguments
    ///
    /// * `source_lang` - Source language ISO 639-1 code (e.g., "en")
    /// * `target_lang` - Target language ISO 639-1 code (e.g., "es")
    /// * `voice_name` - Optional voice name for TTS output
    ///
    /// # Security
    ///
    /// The API key is retrieved from OS keyring and used ONLY in the GeminiConfig.
    /// The GeminiConfig is then used to connect directly to Gemini's WebSocket servers.
    ///
    /// # Returns
    ///
    /// - `Ok(GeminiConfig)` - Configuration ready for direct Gemini connection
    /// - `Err(ByokConnectionError)` - If API key retrieval failed
    pub fn create_byok_config(
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
        voice_name: Option<String>,
    ) -> Result<GeminiConfig, ByokConnectionError> {
        // Retrieve API key from keyring - the ONLY source for BYOK keys
        let api_key = Self::retrieve_api_key()?;

        // Create config with the BYOK key as the token
        // This token will be sent ONLY to Gemini's WebSocket server
        let mut config = GeminiConfig::new(source_lang, target_lang, api_key);

        if let Some(voice) = voice_name {
            config = config.with_voice(voice);
        }

        tracing::info!(
            target: "byok",
            "Created BYOK config for {} -> {} translation",
            config.source_lang,
            config.target_lang
        );

        Ok(config)
    }

    /// Create a GeminiLiveClient connected directly to Gemini using BYOK
    ///
    /// # Arguments
    ///
    /// * `source_lang` - Source language ISO 639-1 code
    /// * `target_lang` - Target language ISO 639-1 code
    /// * `voice_name` - Optional voice name for TTS output
    ///
    /// # Security
    ///
    /// This method:
    /// 1. Retrieves API key from OS keyring
    /// 2. Creates GeminiConfig with the key
    /// 3. Connects directly to Gemini's WebSocket server
    /// 4. The API key is transmitted ONLY to wss://generativelanguage.googleapis.com
    ///
    /// # Returns
    ///
    /// - `Ok(GeminiLiveClient)` - Connected client ready for audio streaming
    /// - `Err(ByokConnectionError)` - If connection failed
    pub async fn create_connected_client(
        &mut self,
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
        voice_name: Option<String>,
    ) -> Result<GeminiLiveClient, ByokConnectionError> {
        let config = Self::create_byok_config(source_lang, target_lang, voice_name)?;

        let mut client = GeminiLiveClient::new(config);

        // Connect directly to Gemini - API key transmitted only here
        client.connect().await?;

        self.byok_active = true;

        tracing::info!(target: "byok", "BYOK client connected successfully to Gemini Live");

        Ok(client)
    }

    /// Create a session in the GeminiSessionManager using BYOK
    ///
    /// This method integrates with the existing session management infrastructure
    /// while ensuring the API key is used only for direct Gemini connections.
    ///
    /// # Arguments
    ///
    /// * `session_manager` - The session manager to create the session in
    /// * `channel` - The channel type (System or User)
    /// * `source_lang` - Source language ISO 639-1 code
    /// * `target_lang` - Target language ISO 639-1 code
    /// * `voice_name` - Optional voice name for TTS output
    ///
    /// # Security
    ///
    /// The GeminiSessionManager's create_session method uses the config's token
    /// (our BYOK API key) to connect directly to Gemini. No intermediate servers.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Session created successfully
    /// - `Err(ByokConnectionError)` - If session creation failed
    pub async fn create_byok_session(
        &mut self,
        session_manager: &mut GeminiSessionManager,
        channel: ChannelType,
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
        voice_name: Option<String>,
    ) -> Result<(), ByokConnectionError> {
        let config = Self::create_byok_config(source_lang, target_lang, voice_name)?;

        // Create session - the GeminiSessionManager will connect directly to Gemini
        session_manager.create_session(channel, config).await?;

        self.byok_active = true;

        tracing::info!(
            target: "byok",
            "BYOK session created for {} channel",
            channel
        );

        Ok(())
    }

    /// Create both system and user sessions using BYOK
    ///
    /// Convenience method to set up dual-channel translation with BYOK.
    ///
    /// # Arguments
    ///
    /// * `session_manager` - The session manager to create sessions in
    /// * `system_config` - (source_lang, target_lang) for system channel
    /// * `user_config` - (source_lang, target_lang) for user channel
    /// * `voice_name` - Optional voice name for both channels
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Both sessions created successfully
    /// - `Err(ByokConnectionError)` - If either session creation failed
    pub async fn create_dual_channel_sessions(
        &mut self,
        session_manager: &mut GeminiSessionManager,
        system_config: (&str, &str),
        user_config: (&str, &str),
        voice_name: Option<String>,
    ) -> Result<(), ByokConnectionError> {
        // Create system channel session (meeting → user)
        self.create_byok_session(
            session_manager,
            ChannelType::System,
            system_config.0,
            system_config.1,
            voice_name.clone(),
        )
        .await?;

        // Create user channel session (user → meeting)
        self.create_byok_session(
            session_manager,
            ChannelType::User,
            user_config.0,
            user_config.1,
            voice_name,
        )
        .await?;

        tracing::info!(
            target: "byok",
            "Dual-channel BYOK sessions created: System ({} -> {}), User ({} -> {})",
            system_config.0, system_config.1,
            user_config.0, user_config.1
        );

        Ok(())
    }

    /// Deactivate BYOK mode
    ///
    /// This should be called when switching away from BYOK mode or
    /// when the user removes their API key.
    pub fn deactivate(&mut self) {
        self.byok_active = false;
        tracing::info!(target: "byok", "BYOK mode deactivated");
    }
}

impl Default for ByokGeminiConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byok_connector_initial_state() {
        let connector = ByokGeminiConnector::new();
        assert!(!connector.is_byok_active());
    }

    #[test]
    fn test_byok_connection_error_display() {
        let errors = [
            (
                ByokConnectionError::NoApiKeyStored,
                "No BYOK API key stored",
            ),
            (
                ByokConnectionError::KeyringAccessError("test error".to_string()),
                "Failed to access OS keyring",
            ),
            (
                ByokConnectionError::ConnectionError(GeminiError::Timeout),
                "Gemini connection error",
            ),
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
    fn test_byok_connector_default() {
        let connector = ByokGeminiConnector::default();
        assert!(!connector.is_byok_active());
    }

    // Note: Integration tests that require actual keyring access
    // should be run separately with proper test credentials set up.
    // These tests should NOT be run in CI environments without proper
    // keyring mocking.
}
