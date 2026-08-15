//! Gemini Session Management
//!
//! Manages multiple WebSocket sessions with auto-reconnect capability.
//! Supports two simultaneous sessions: one for system channel (meeting → user)
//! and one for user channel (user → meeting).
//!
//! # Requirements
//!
//! - Requirement 6.5: Support two simultaneous Gemini Live sessions
//! - Requirement 6.6: Auto-reconnect up to 3 times with 1-second intervals

use std::time::Duration;
use tokio::time::sleep;

use super::client::{GeminiConfig, GeminiError, GeminiLiveClient};

/// Maximum number of reconnection attempts
const MAX_RECONNECT_ATTEMPTS: u8 = 3;

/// Interval between reconnection attempts in milliseconds
const RECONNECT_INTERVAL_MS: u64 = 1000;

/// Channel type for session management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    /// System channel: Meeting audio → User (translated)
    System,
    /// User channel: User microphone → Meeting (translated)
    User,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::System => write!(f, "System"),
            ChannelType::User => write!(f, "User"),
        }
    }
}

/// Session manager for multiple Gemini connections
///
/// Manages two independent WebSocket sessions to Gemini Live,
/// enabling bidirectional translation in meetings.
pub struct GeminiSessionManager {
    /// Session for system channel (meeting → user)
    system_session: Option<SessionState>,
    /// Session for user channel (user → meeting)
    user_session: Option<SessionState>,
    /// Maximum reconnection attempts per session
    max_reconnect_attempts: u8,
}

/// Internal state for a single session
struct SessionState {
    /// The WebSocket client
    client: GeminiLiveClient,
    /// Configuration used to create this session
    config: GeminiConfig,
    /// Number of consecutive reconnection failures
    reconnect_failures: u8,
}

impl GeminiSessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            system_session: None,
            user_session: None,
            max_reconnect_attempts: MAX_RECONNECT_ATTEMPTS,
        }
    }

    /// Create a new session manager with custom reconnection settings
    pub fn with_max_reconnects(max_attempts: u8) -> Self {
        Self {
            system_session: None,
            user_session: None,
            max_reconnect_attempts: max_attempts,
        }
    }

    /// Create a new session for the specified channel
    ///
    /// If a session already exists for this channel, it will be closed first.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel type (System or User)
    /// * `config` - Configuration for the Gemini connection
    ///
    /// # Returns
    ///
    /// `Ok(())` if the session was created successfully, `Err(GeminiError)` otherwise.
    pub async fn create_session(
        &mut self,
        channel: ChannelType,
        config: GeminiConfig,
    ) -> Result<(), GeminiError> {
        // Close existing session if any
        self.close_session(channel).await?;

        // Create and connect new client
        let mut client = GeminiLiveClient::new(config.clone());
        client.connect().await?;

        let state = SessionState {
            client,
            config,
            reconnect_failures: 0,
        };

        match channel {
            ChannelType::System => self.system_session = Some(state),
            ChannelType::User => self.user_session = Some(state),
        }

        tracing::info!(target: "gemini", "Session created for {} channel", channel);
        Ok(())
    }

    /// Close a session for the specified channel
    pub async fn close_session(&mut self, channel: ChannelType) -> Result<(), GeminiError> {
        let session = match channel {
            ChannelType::System => self.system_session.take(),
            ChannelType::User => self.user_session.take(),
        };

        if let Some(mut state) = session {
            state.client.close().await?;
            tracing::info!(target: "gemini", "Session closed for {} channel", channel);
        }

        Ok(())
    }

    /// Close all sessions
    pub async fn close_all(&mut self) -> Result<(), GeminiError> {
        self.close_session(ChannelType::System).await?;
        self.close_session(ChannelType::User).await?;
        Ok(())
    }

    /// Reconnect a session with automatic retry
    ///
    /// Attempts to reconnect up to `max_reconnect_attempts` times
    /// with `RECONNECT_INTERVAL_MS` between attempts.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel to reconnect
    ///
    /// # Returns
    ///
    /// `Ok(())` if reconnection succeeded, `Err(GeminiError)` if all attempts failed.
    pub async fn reconnect(&mut self, channel: ChannelType) -> Result<(), GeminiError> {
        let state = match channel {
            ChannelType::System => self.system_session.as_mut(),
            ChannelType::User => self.user_session.as_mut(),
        };

        let state = match state {
            Some(s) => s,
            None => return Err(GeminiError::ConnectionClosed),
        };

        // Get config for reconnection
        let config = state.config.clone();
        let max_attempts = self.max_reconnect_attempts;

        tracing::info!(
            target: "gemini",
            "Attempting to reconnect {} channel (max {} attempts)",
            channel,
            max_attempts
        );

        for attempt in 1..=max_attempts {
            tracing::debug!(target: "gemini", "Reconnect attempt {} of {}", attempt, max_attempts);

            // Create new client
            let mut client = GeminiLiveClient::new(config.clone());

            match client.connect().await {
                Ok(()) => {
                    state.client = client;
                    state.reconnect_failures = 0;

                    tracing::info!(
                        target: "gemini",
                        "Reconnected {} channel on attempt {}",
                        channel,
                        attempt
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        target: "gemini",
                        "Reconnect attempt {} failed: {}",
                        attempt,
                        e
                    );

                    state.reconnect_failures += 1;

                    // Wait before next attempt (except on last attempt)
                    if attempt < max_attempts {
                        sleep(Duration::from_millis(RECONNECT_INTERVAL_MS)).await;
                    }
                }
            }
        }

        // All attempts failed
        tracing::error!(
            target: "gemini",
            "Failed to reconnect {} channel after {} attempts",
            channel,
            max_attempts
        );

        Err(GeminiError::ConnectionFailed(format!(
            "Reconnection failed after {} attempts",
            max_attempts
        )))
    }

    /// Get a mutable reference to the system session client
    pub fn get_system_session(&mut self) -> Option<&mut GeminiLiveClient> {
        self.system_session.as_mut().map(|s| &mut s.client)
    }

    /// Get a mutable reference to the user session client
    pub fn get_user_session(&mut self) -> Option<&mut GeminiLiveClient> {
        self.user_session.as_mut().map(|s| &mut s.client)
    }

    /// Get a mutable reference to a session by channel type
    pub fn get_session(&mut self, channel: ChannelType) -> Option<&mut GeminiLiveClient> {
        match channel {
            ChannelType::System => self.get_system_session(),
            ChannelType::User => self.get_user_session(),
        }
    }

    /// Check if a channel has an active session
    pub fn has_session(&self, channel: ChannelType) -> bool {
        match channel {
            ChannelType::System => self.system_session.as_ref()
                .map(|s| s.client.is_connected())
                .unwrap_or(false),
            ChannelType::User => self.user_session.as_ref()
                .map(|s| s.client.is_connected())
                .unwrap_or(false),
        }
    }

    /// Check if both channels have active sessions
    pub fn both_connected(&self) -> bool {
        self.has_session(ChannelType::System) && self.has_session(ChannelType::User)
    }

    /// Check if any channel has an active session
    pub fn any_connected(&self) -> bool {
        self.has_session(ChannelType::System) || self.has_session(ChannelType::User)
    }

    /// Get the number of reconnection failures for a channel
    pub fn reconnect_failures(&self, channel: ChannelType) -> u8 {
        match channel {
            ChannelType::System => self.system_session.as_ref()
                .map(|s| s.reconnect_failures)
                .unwrap_or(0),
            ChannelType::User => self.user_session.as_ref()
                .map(|s| s.reconnect_failures)
                .unwrap_or(0),
        }
    }

    /// Send audio to a specific channel
    pub async fn send_audio(
        &mut self,
        channel: ChannelType,
        samples: &[i16],
    ) -> Result<(), GeminiError> {
        let client = self.get_session(channel)
            .ok_or(GeminiError::ConnectionClosed)?;
        
        client.send_audio(samples).await
    }

    /// Receive audio from a specific channel
    pub async fn receive_audio(
        &mut self,
        channel: ChannelType,
    ) -> Result<Option<Vec<i16>>, GeminiError> {
        let client = self.get_session(channel)
            .ok_or(GeminiError::ConnectionClosed)?;
        
        client.receive_audio().await
    }

    /// Receive audio from a channel with timeout
    pub async fn receive_audio_timeout(
        &mut self,
        channel: ChannelType,
        timeout_ms: u64,
    ) -> Result<Option<Vec<i16>>, GeminiError> {
        let client = self.get_session(channel)
            .ok_or(GeminiError::ConnectionClosed)?;
        
        client.receive_audio_timeout(timeout_ms).await
    }
}

impl Default for GeminiSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for GeminiSessionManager {
    fn drop(&mut self) {
        // Note: We can't do async cleanup in Drop
        // Sessions will be cleaned up when their clients are dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_display() {
        assert_eq!(format!("{}", ChannelType::System), "System");
        assert_eq!(format!("{}", ChannelType::User), "User");
    }

    #[test]
    fn test_session_manager_initial_state() {
        let manager = GeminiSessionManager::new();
        
        assert!(!manager.has_session(ChannelType::System));
        assert!(!manager.has_session(ChannelType::User));
        assert!(!manager.any_connected());
        assert!(!manager.both_connected());
    }

    #[test]
    fn test_session_manager_with_custom_reconnects() {
        let manager = GeminiSessionManager::with_max_reconnects(5);
        assert_eq!(manager.max_reconnect_attempts, 5);
    }

    #[test]
    fn test_reconnect_failures_initial() {
        let manager = GeminiSessionManager::new();
        
        assert_eq!(manager.reconnect_failures(ChannelType::System), 0);
        assert_eq!(manager.reconnect_failures(ChannelType::User), 0);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 3);
        assert_eq!(RECONNECT_INTERVAL_MS, 1000);
    }
}
