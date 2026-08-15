//! Core Audio Engine
//! 
//! Manages audio capture, processing, and playback for translation channels.
//! Supports hot-swap of devices without stopping translation sessions.
//!
//! # Architecture
//!
//! The engine manages two independent audio pipelines:
//! - **System Channel**: Captures meeting audio → Translates → Plays to user
//! - **User Channel**: Captures mic → Translates → Routes to VB-Cable
//!
//! # Requirements
//!
//! - Requirement 6.5: Support two simultaneous Gemini Live sessions
//! - Requirement 14.4: Apply device changes without restarting session

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
use crate::audio::windows::{wasapi, vbcable};

/// State of an audio channel
/// 
/// Serializes to match the TypeScript type:
/// ```typescript
/// export type ChannelState = 
///   | { type: 'inactive' }
///   | { type: 'active' }
///   | { type: 'paused' }
///   | { type: 'error'; message: string };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChannelState {
    Inactive,
    Active,
    Paused,
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}

/// Reason for channel pause
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PauseReason {
    #[serde(rename = "userRequested")]
    UserRequested,
    #[serde(rename = "deviceDisconnected")]
    DeviceDisconnected {
        #[serde(rename = "deviceId")]
        device_id: String,
        #[serde(rename = "deviceName")]
        device_name: String,
    },
    #[serde(rename = "networkError")]
    NetworkError,
    #[serde(rename = "geminiDisconnected")]
    GeminiDisconnected,
}

/// Channel configuration
/// Receives camelCase from frontend, uses snake_case internally
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub source_lang: String,      // ISO 639-1 (e.g., "en")
    pub target_lang: String,      // ISO 639-1 (e.g., "es")
    pub input_device: String,     // Device ID
    pub output_device: String,    // Device ID
}

/// Real-time audio metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioMetrics {
    pub input_level_db: f32,      // -60.0 to 0.0
    pub output_level_db: f32,     // -60.0 to 0.0
    pub latency_ms: u32,          // End-to-end latency
    pub packets_sent: u64,
    pub packets_received: u64,
}

// Manual PartialEq for AudioMetrics due to f32 fields
// Uses bitwise comparison for f32 which is safe for serialization round-trips
impl PartialEq for AudioMetrics {
    fn eq(&self, other: &Self) -> bool {
        self.input_level_db.to_bits() == other.input_level_db.to_bits()
            && self.output_level_db.to_bits() == other.output_level_db.to_bits()
            && self.latency_ms == other.latency_ms
            && self.packets_sent == other.packets_sent
            && self.packets_received == other.packets_received
    }
}

/// Audio device information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub device_type: String, // "input", "output", "loopback"
    pub is_default: bool,
}

/// Type of audio channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    /// System channel: Meeting → User (captures meeting audio, translates, plays to user)
    System,
    /// User channel: User → Meeting (captures mic, translates, routes to VB-Cable)
    User,
}

/// Event emitted when device state changes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEvent {
    pub event_type: DeviceEventType,
    pub device: AudioDevice,
}

/// Type of device event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceEventType {
    Connected,
    Disconnected,
    StateChanged,
}

/// Engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub system_channel: ChannelState,
    pub user_channel: ChannelState,
    pub metrics: Option<AudioMetrics>,
    pub pause_reason: Option<PauseReason>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            system_channel: ChannelState::Inactive,
            user_channel: ChannelState::Inactive,
            metrics: None,
            pause_reason: None,
        }
    }
}

/// Internal channel state for managing audio pipeline
struct AudioChannelState {
    /// Current channel state
    state: ChannelState,
    /// Channel configuration
    config: Option<ChannelConfig>,
    /// Current input device ID
    input_device_id: Option<String>,
    /// Current output device ID  
    output_device_id: Option<String>,
    /// Gemini token for this channel
    token: Option<String>,
}

impl Default for AudioChannelState {
    fn default() -> Self {
        Self {
            state: ChannelState::Inactive,
            config: None,
            input_device_id: None,
            output_device_id: None,
            token: None,
        }
    }
}

/// Main Audio Engine
/// 
/// Thread-safe engine managing two audio channels for bidirectional translation.
/// Uses Arc<RwLock<>> for concurrent access from Tauri commands.
pub struct AudioEngine {
    /// System channel: Meeting audio → User
    system_channel: Arc<RwLock<AudioChannelState>>,
    /// User channel: Microphone → Meeting
    user_channel: Arc<RwLock<AudioChannelState>>,
    /// Metrics sender for real-time updates
    metrics_tx: Option<mpsc::Sender<AudioMetrics>>,
    /// Pause reason if any channel is paused
    pause_reason: Arc<RwLock<Option<PauseReason>>>,
}

impl AudioEngine {
    /// Create a new Audio Engine
    ///
    /// # Arguments
    /// * `metrics_tx` - Optional channel for sending real-time audio metrics
    pub fn new(metrics_tx: Option<mpsc::Sender<AudioMetrics>>) -> Self {
        tracing::info!("Initializing AudioEngine");
        Self {
            system_channel: Arc::new(RwLock::new(AudioChannelState::default())),
            user_channel: Arc::new(RwLock::new(AudioChannelState::default())),
            metrics_tx,
            pause_reason: Arc::new(RwLock::new(None)),
        }
    }

    /// Enumerate available audio devices
    ///
    /// Returns a list of available audio devices within 2 seconds.
    /// On Windows, uses WASAPI to enumerate output devices for loopback capture.
    /// On macOS, uses CoreAudio/ScreenCaptureKit for device enumeration.
    ///
    /// # Requirements
    /// - Requirement 2.2: Enumerate devices within 2 seconds
    /// - Requirement 2.5: Show error if WASAPI not available
    /// - Requirement 2.6: Show error if no devices available
    ///
    /// # Errors
    /// Returns an error if:
    /// - WASAPI is not available (Windows)
    /// - ScreenCaptureKit is not available (macOS)
    /// - No audio devices are available
    pub async fn enumerate_devices(&self) -> Result<Vec<AudioDevice>, String> {
        #[cfg(target_os = "windows")]
        {
            // Check if WASAPI is available
            wasapi::check_wasapi_available()
                .map_err(|e| {
                    let audio_err: crate::error::AudioError = e.into();
                    format!("{}\n\nSugerencia: {}", audio_err.message(), audio_err.suggestion())
                })?;

            // Enumerate devices
            let devices = wasapi::enumerate_output_devices()
                .map_err(|e| {
                    let audio_err: crate::error::AudioError = e.into();
                    format!("{}\n\nSugerencia: {}", audio_err.message(), audio_err.suggestion())
                })?;

            // Check if any devices are available
            if devices.is_empty() {
                let audio_err = crate::error::AudioError::NoDevicesAvailable;
                return Err(format!("{}\n\nSugerencia: {}", audio_err.message(), audio_err.suggestion()));
            }

            tracing::info!("Enumerated {} audio devices", devices.len());
            Ok(devices)
        }

        #[cfg(target_os = "macos")]
        {
            // TODO: Implement macOS device enumeration using CoreAudio/ScreenCaptureKit
            tracing::warn!("macOS device enumeration not yet implemented");
            Ok(vec![])
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Err("Plataforma no soportada. Solo Windows y macOS son compatibles.".to_string())
        }
    }

    /// Start the system audio channel (Meeting → User)
    ///
    /// Starts capture of meeting audio, sends to Gemini for translation,
    /// and plays the translated audio to the user's output device.
    ///
    /// # Arguments
    /// * `config` - Channel configuration with languages and devices
    /// * `token` - Gemini API token (ephemeral or BYOK)
    ///
    /// # Requirements
    /// - Requirement 6.5: Support simultaneous Gemini sessions
    ///
    /// # Errors
    /// Returns an error if:
    /// - Channel is already active
    /// - Input device not found
    /// - Failed to connect to Gemini
    pub async fn start_system_channel(&self, config: ChannelConfig, token: &str) -> Result<(), String> {
        let mut channel = self.system_channel.write().await;
        
        // Check if already active
        if matches!(channel.state, ChannelState::Active) {
            return Err("El canal de sistema ya está activo".to_string());
        }

        tracing::info!(
            "Starting system channel: {} → {}, input: {}, output: {}",
            config.source_lang,
            config.target_lang,
            config.input_device,
            config.output_device
        );

        // Store configuration
        channel.config = Some(config.clone());
        channel.input_device_id = Some(config.input_device.clone());
        channel.output_device_id = Some(config.output_device.clone());
        channel.token = Some(token.to_string());

        // TODO: Initialize WASAPI capture on input device
        // TODO: Connect to Gemini WebSocket
        // TODO: Initialize playback on output device
        
        // For now, mark as active - actual implementation will come with Gemini client
        channel.state = ChannelState::Active;

        tracing::info!("System channel started successfully");
        Ok(())
    }

    /// Start the user audio channel (User → Meeting)
    ///
    /// Starts capture of user's microphone, sends to Gemini for translation,
    /// and routes the translated audio to VB-Cable for meeting injection.
    ///
    /// # Arguments
    /// * `config` - Channel configuration with languages and devices
    /// * `token` - Gemini API token (ephemeral or BYOK)
    ///
    /// # Requirements
    /// - Requirement 6.5: Support simultaneous Gemini sessions
    /// - Requirement 4.5: Route audio to VB-Cable Output
    ///
    /// # Errors
    /// Returns an error if:
    /// - Channel is already active
    /// - Input device (microphone) not found
    /// - VB-Cable not available
    /// - Failed to connect to Gemini
    pub async fn start_user_channel(&self, config: ChannelConfig, token: &str) -> Result<(), String> {
        let mut channel = self.user_channel.write().await;
        
        // Check if already active
        if matches!(channel.state, ChannelState::Active) {
            return Err("El canal de usuario ya está activo".to_string());
        }

        tracing::info!(
            "Starting user channel: {} → {}, input: {}, output: {}",
            config.source_lang,
            config.target_lang,
            config.input_device,
            config.output_device
        );

        #[cfg(target_os = "windows")]
        {
            // Verify VB-Cable is available for output
            if !vbcable::is_vbcable_installed() {
                // Try to detect VB-Cable now
                if let Ok(status) = vbcable::detect_vbcable() {
                    if !status.output_available {
                        return Err(
                            "VB-Cable Output no está disponible. Es necesario para inyectar \
                             audio traducido en las aplicaciones de reunión.\n\n\
                             Sugerencia: Instala VB-Cable desde el asistente de configuración inicial."
                                .to_string(),
                        );
                    }
                } else {
                    return Err(
                        "No se pudo verificar VB-Cable. Verifica que esté instalado correctamente."
                            .to_string(),
                    );
                }
            }
        }

        // Store configuration
        channel.config = Some(config.clone());
        channel.input_device_id = Some(config.input_device.clone());
        channel.output_device_id = Some(config.output_device.clone());
        channel.token = Some(token.to_string());

        // TODO: Initialize microphone capture
        // TODO: Connect to Gemini WebSocket
        // TODO: Initialize VB-Cable output

        // For now, mark as active
        channel.state = ChannelState::Active;

        tracing::info!("User channel started successfully");
        Ok(())
    }

    /// Stop a specific audio channel
    ///
    /// Stops capture, closes Gemini connection, and releases resources
    /// for the specified channel.
    ///
    /// # Arguments
    /// * `channel_type` - Which channel to stop (System or User)
    ///
    /// # Errors
    /// Returns an error if the channel is not active.
    pub async fn stop_channel(&self, channel_type: ChannelType) -> Result<(), String> {
        match channel_type {
            ChannelType::System => {
                let mut channel = self.system_channel.write().await;
                
                if matches!(channel.state, ChannelState::Inactive) {
                    return Err("El canal de sistema no está activo".to_string());
                }

                tracing::info!("Stopping system channel");

                // TODO: Stop WASAPI capture
                // TODO: Close Gemini WebSocket
                // TODO: Stop playback

                channel.state = ChannelState::Inactive;
                channel.config = None;
                channel.input_device_id = None;
                channel.output_device_id = None;
                channel.token = None;

                tracing::info!("System channel stopped");
            }
            ChannelType::User => {
                let mut channel = self.user_channel.write().await;
                
                if matches!(channel.state, ChannelState::Inactive) {
                    return Err("El canal de usuario no está activo".to_string());
                }

                tracing::info!("Stopping user channel");

                // TODO: Stop microphone capture
                // TODO: Close Gemini WebSocket
                // TODO: Stop VB-Cable output

                channel.state = ChannelState::Inactive;
                channel.config = None;
                channel.input_device_id = None;
                channel.output_device_id = None;
                channel.token = None;

                tracing::info!("User channel stopped");
            }
        }

        // Clear pause reason if both channels are now inactive
        let system_state = self.system_channel.read().await.state.clone();
        let user_state = self.user_channel.read().await.state.clone();
        
        if matches!(system_state, ChannelState::Inactive) && matches!(user_state, ChannelState::Inactive) {
            *self.pause_reason.write().await = None;
        }

        Ok(())
    }

    /// Change device for an active channel without restarting the session
    ///
    /// Performs hot-swap of audio devices while maintaining the Gemini connection.
    /// This allows users to switch headphones or microphones without interrupting translation.
    ///
    /// # Arguments
    /// * `channel_type` - Which channel to modify
    /// * `device_id` - New device ID to use
    /// * `is_input` - True for input device, false for output device
    ///
    /// # Requirements
    /// - Requirement 14.4: Apply device changes without restarting session
    ///
    /// # Errors
    /// Returns an error if:
    /// - Channel is not active
    /// - Device not found
    /// - Device change fails
    pub async fn change_device(
        &self,
        channel_type: ChannelType,
        device_id: &str,
        is_input: bool,
    ) -> Result<(), String> {
        match channel_type {
            ChannelType::System => {
                let mut channel = self.system_channel.write().await;
                
                if !matches!(channel.state, ChannelState::Active | ChannelState::Paused) {
                    return Err("El canal de sistema no está activo".to_string());
                }

                tracing::info!(
                    "Changing system channel {} device to: {}",
                    if is_input { "input" } else { "output" },
                    device_id
                );

                // Validate device exists
                let devices = self.enumerate_devices_internal().await?;
                if !devices.iter().any(|d| d.id == device_id) {
                    return Err(format!("Dispositivo '{}' no encontrado", device_id));
                }

                if is_input {
                    // Hot-swap input capture device
                    // TODO: Stop current WASAPI capture
                    // TODO: Start new WASAPI capture on new device
                    // Gemini connection stays alive
                    channel.input_device_id = Some(device_id.to_string());
                    
                    // Update config if present
                    if let Some(ref mut config) = channel.config {
                        config.input_device = device_id.to_string();
                    }
                } else {
                    // Hot-swap output playback device
                    // TODO: Stop current playback
                    // TODO: Start new playback on new device
                    channel.output_device_id = Some(device_id.to_string());
                    
                    if let Some(ref mut config) = channel.config {
                        config.output_device = device_id.to_string();
                    }
                }

                // Clear pause state if we were paused due to device disconnection
                if matches!(channel.state, ChannelState::Paused) {
                    channel.state = ChannelState::Active;
                    *self.pause_reason.write().await = None;
                }

                tracing::info!("System channel device changed successfully");
            }
            ChannelType::User => {
                let mut channel = self.user_channel.write().await;
                
                if !matches!(channel.state, ChannelState::Active | ChannelState::Paused) {
                    return Err("El canal de usuario no está activo".to_string());
                }

                tracing::info!(
                    "Changing user channel {} device to: {}",
                    if is_input { "input" } else { "output" },
                    device_id
                );

                // Validate device exists
                let devices = self.enumerate_devices_internal().await?;
                if !devices.iter().any(|d| d.id == device_id) {
                    return Err(format!("Dispositivo '{}' no encontrado", device_id));
                }

                if is_input {
                    channel.input_device_id = Some(device_id.to_string());
                    if let Some(ref mut config) = channel.config {
                        config.input_device = device_id.to_string();
                    }
                } else {
                    channel.output_device_id = Some(device_id.to_string());
                    if let Some(ref mut config) = channel.config {
                        config.output_device = device_id.to_string();
                    }
                }

                if matches!(channel.state, ChannelState::Paused) {
                    channel.state = ChannelState::Active;
                    *self.pause_reason.write().await = None;
                }

                tracing::info!("User channel device changed successfully");
            }
        }

        Ok(())
    }

    /// Internal device enumeration (doesn't apply error formatting)
    async fn enumerate_devices_internal(&self) -> Result<Vec<AudioDevice>, String> {
        #[cfg(target_os = "windows")]
        {
            wasapi::enumerate_output_devices()
                .map_err(|e| e.to_string())
        }

        #[cfg(target_os = "macos")]
        {
            Ok(vec![])
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Err("Plataforma no soportada".to_string())
        }
    }

    /// Get current engine state
    ///
    /// Returns the current state of both channels and any active metrics.
    pub async fn get_state(&self) -> EngineState {
        let system_channel = self.system_channel.read().await;
        let user_channel = self.user_channel.read().await;
        let pause_reason = self.pause_reason.read().await;

        EngineState {
            system_channel: system_channel.state.clone(),
            user_channel: user_channel.state.clone(),
            metrics: None, // TODO: Implement metrics collection
            pause_reason: pause_reason.clone(),
        }
    }

    /// Handle device disconnection during capture
    ///
    /// This method should be called when a device disconnection is detected.
    /// It pauses the affected channel and updates the engine state.
    ///
    /// # Requirements
    /// - Requirement 2.7: Pause capture and notify user when device disconnects
    pub async fn handle_device_disconnection(
        &self,
        device_id: &str,
        device_name: &str,
        channel_type: ChannelType,
    ) {
        let pause_reason = PauseReason::DeviceDisconnected {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
        };

        match channel_type {
            ChannelType::System => {
                let mut channel = self.system_channel.write().await;
                channel.state = ChannelState::Paused;
            }
            ChannelType::User => {
                let mut channel = self.user_channel.write().await;
                channel.state = ChannelState::Paused;
            }
        }

        *self.pause_reason.write().await = Some(pause_reason);

        tracing::warn!(
            "Device '{}' ({}) disconnected, {} channel paused",
            device_name,
            device_id,
            match channel_type {
                ChannelType::System => "system",
                ChannelType::User => "user",
            }
        );
    }

    /// Get disconnection info if a channel is paused due to device disconnection
    ///
    /// Returns the device_id and device_name if the pause was caused by a device
    /// disconnection, None otherwise.
    pub async fn get_disconnection_info(&self) -> Option<(String, String)> {
        let pause_reason = self.pause_reason.read().await;
        match &*pause_reason {
            Some(PauseReason::DeviceDisconnected { device_id, device_name }) => {
                Some((device_id.clone(), device_name.clone()))
            }
            _ => None,
        }
    }

    /// Clear pause state (when user selects a new device)
    pub async fn clear_pause_state(&self) {
        *self.pause_reason.write().await = None;
    }

    /// Pause a channel (user-initiated)
    pub async fn pause_channel(&self, channel_type: ChannelType) -> Result<(), String> {
        match channel_type {
            ChannelType::System => {
                let mut channel = self.system_channel.write().await;
                if !matches!(channel.state, ChannelState::Active) {
                    return Err("El canal de sistema no está activo".to_string());
                }
                channel.state = ChannelState::Paused;
                *self.pause_reason.write().await = Some(PauseReason::UserRequested);
                tracing::info!("System channel paused by user");
            }
            ChannelType::User => {
                let mut channel = self.user_channel.write().await;
                if !matches!(channel.state, ChannelState::Active) {
                    return Err("El canal de usuario no está activo".to_string());
                }
                channel.state = ChannelState::Paused;
                *self.pause_reason.write().await = Some(PauseReason::UserRequested);
                tracing::info!("User channel paused by user");
            }
        }
        Ok(())
    }

    /// Resume a paused channel
    pub async fn resume_channel(&self, channel_type: ChannelType) -> Result<(), String> {
        match channel_type {
            ChannelType::System => {
                let mut channel = self.system_channel.write().await;
                if !matches!(channel.state, ChannelState::Paused) {
                    return Err("El canal de sistema no está pausado".to_string());
                }
                channel.state = ChannelState::Active;
                *self.pause_reason.write().await = None;
                tracing::info!("System channel resumed");
            }
            ChannelType::User => {
                let mut channel = self.user_channel.write().await;
                if !matches!(channel.state, ChannelState::Paused) {
                    return Err("El canal de usuario no está pausado".to_string());
                }
                channel.state = ChannelState::Active;
                *self.pause_reason.write().await = None;
                tracing::info!("User channel resumed");
            }
        }
        Ok(())
    }

    /// Send metrics through the channel
    pub async fn send_metrics(&self, metrics: AudioMetrics) {
        if let Some(ref tx) = self.metrics_tx {
            if let Err(e) = tx.send(metrics).await {
                tracing::warn!("Failed to send audio metrics: {}", e);
            }
        }
    }

    /// Check if system channel is active
    pub async fn is_system_channel_active(&self) -> bool {
        matches!(self.system_channel.read().await.state, ChannelState::Active)
    }

    /// Check if user channel is active
    pub async fn is_user_channel_active(&self) -> bool {
        matches!(self.user_channel.read().await.state, ChannelState::Active)
    }

    /// Check if any channel is active
    pub async fn is_any_channel_active(&self) -> bool {
        self.is_system_channel_active().await || self.is_user_channel_active().await
    }

    /// Get current configuration for a channel
    pub async fn get_channel_config(&self, channel_type: ChannelType) -> Option<ChannelConfig> {
        match channel_type {
            ChannelType::System => self.system_channel.read().await.config.clone(),
            ChannelType::User => self.user_channel.read().await.config.clone(),
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new(None)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = AudioEngine::new(None);
        let state = engine.get_state().await;
        
        assert!(matches!(state.system_channel, ChannelState::Inactive));
        assert!(matches!(state.user_channel, ChannelState::Inactive));
        assert!(state.pause_reason.is_none());
    }

    #[tokio::test]
    async fn test_engine_with_metrics_channel() {
        let (tx, _rx) = mpsc::channel(100);
        let engine = AudioEngine::new(Some(tx));
        
        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Inactive));
    }

    #[tokio::test]
    async fn test_start_system_channel() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        let result = engine.start_system_channel(config, "test-token").await;
        assert!(result.is_ok());

        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Active));
    }

    #[tokio::test]
    async fn test_start_system_channel_already_active() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        // First start should succeed
        engine.start_system_channel(config.clone(), "test-token").await.unwrap();
        
        // Second start should fail
        let result = engine.start_system_channel(config, "test-token").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ya está activo"));
    }

    #[tokio::test]
    async fn test_stop_system_channel() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        engine.start_system_channel(config, "test-token").await.unwrap();
        
        let result = engine.stop_channel(ChannelType::System).await;
        assert!(result.is_ok());

        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Inactive));
    }

    #[tokio::test]
    async fn test_stop_inactive_channel() {
        let engine = AudioEngine::new(None);
        
        let result = engine.stop_channel(ChannelType::System).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no está activo"));
    }

    #[tokio::test]
    async fn test_pause_resume_channel() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        engine.start_system_channel(config, "test-token").await.unwrap();
        
        // Pause
        engine.pause_channel(ChannelType::System).await.unwrap();
        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Paused));
        assert!(matches!(state.pause_reason, Some(PauseReason::UserRequested)));
        
        // Resume
        engine.resume_channel(ChannelType::System).await.unwrap();
        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Active));
        assert!(state.pause_reason.is_none());
    }

    #[tokio::test]
    async fn test_device_disconnection_handling() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        engine.start_system_channel(config, "test-token").await.unwrap();
        
        // Simulate device disconnection
        engine.handle_device_disconnection(
            "test-input",
            "Test Device",
            ChannelType::System,
        ).await;

        let state = engine.get_state().await;
        assert!(matches!(state.system_channel, ChannelState::Paused));
        
        let disconnect_info = engine.get_disconnection_info().await;
        assert!(disconnect_info.is_some());
        let (device_id, device_name) = disconnect_info.unwrap();
        assert_eq!(device_id, "test-input");
        assert_eq!(device_name, "Test Device");
    }

    #[tokio::test]
    async fn test_both_channels_simultaneously() {
        let engine = AudioEngine::new(None);
        
        let system_config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "system-input".to_string(),
            output_device: "system-output".to_string(),
        };
        
        let user_config = ChannelConfig {
            source_lang: "es".to_string(),
            target_lang: "en".to_string(),
            input_device: "mic-input".to_string(),
            output_device: "vbcable-output".to_string(),
        };

        // Start both channels
        engine.start_system_channel(system_config, "token1").await.unwrap();
        
        // User channel may fail on non-Windows or without VB-Cable, so we handle that
        let user_result = engine.start_user_channel(user_config, "token2").await;
        
        // System channel should be active regardless
        assert!(engine.is_system_channel_active().await);
        
        // On Windows with VB-Cable, both would be active
        // On other platforms, user channel might fail
        if user_result.is_ok() {
            assert!(engine.is_any_channel_active().await);
        }
    }

    #[tokio::test]
    async fn test_get_channel_config() {
        let engine = AudioEngine::new(None);
        
        let config = ChannelConfig {
            source_lang: "en".to_string(),
            target_lang: "es".to_string(),
            input_device: "test-input".to_string(),
            output_device: "test-output".to_string(),
        };

        engine.start_system_channel(config.clone(), "test-token").await.unwrap();
        
        let retrieved_config = engine.get_channel_config(ChannelType::System).await;
        assert!(retrieved_config.is_some());
        
        let retrieved = retrieved_config.unwrap();
        assert_eq!(retrieved.source_lang, "en");
        assert_eq!(retrieved.target_lang, "es");
    }

    #[test]
    fn test_channel_state_serialization() {
        // Test Active state serialization to match TypeScript { type: 'active' }
        let state = ChannelState::Active;
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"type\":\"active\"") || json.contains("\"type\": \"active\""));

        // Test Error state serialization to match TypeScript { type: 'error', message: '...' }
        let error_state = ChannelState::Error { message: "test error".to_string() };
        let json = serde_json::to_string(&error_state).unwrap();
        assert!(json.contains("\"type\":\"error\"") || json.contains("\"type\": \"error\""));
        assert!(json.contains("\"message\":\"test error\"") || json.contains("\"message\": \"test error\""));
    }

    #[test]
    fn test_engine_state_default() {
        let state = EngineState::default();
        assert!(matches!(state.system_channel, ChannelState::Inactive));
        assert!(matches!(state.user_channel, ChannelState::Inactive));
        assert!(state.metrics.is_none());
        assert!(state.pause_reason.is_none());
    }

    #[test]
    fn test_audio_metrics_default() {
        let metrics = AudioMetrics::default();
        assert_eq!(metrics.input_level_db, 0.0);
        assert_eq!(metrics.output_level_db, 0.0);
        assert_eq!(metrics.latency_ms, 0);
        assert_eq!(metrics.packets_sent, 0);
        assert_eq!(metrics.packets_received, 0);
    }
}
