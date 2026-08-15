//! Application events for frontend communication
//!
//! This module defines events emitted from the Rust backend to the React frontend
//! via Tauri's event system. Events are used for real-time notifications about
//! audio state changes, device connections, usage limits, and errors.
//!
//! # Requirements
//!
//! - Requirement 2.5: Notify when WASAPI is not available
//! - Requirement 2.6: Notify when no audio devices available
//! - Requirement 2.7: Notify when device disconnects during capture
//! - Requirement 10.6: Notify when usage reaches 80%
//! - Requirement 10.7: Block translation at 100% and show upgrade options
//! - Requirement 11.8: Notify when usage limit is reached
//! - Requirement 22.2: Emit backend events to frontend (audio-metrics, channel-state,
//!   device-changed, token-expiring, gemini-error, usage-limit, update-available)

use serde::Serialize;
use tauri::Emitter;

#[cfg(feature = "audio")]
use crate::audio::engine::{AudioDevice, AudioMetrics, ChannelState, ChannelType};

#[cfg(feature = "audio")]
use crate::error::DeviceState;

// Stub types for when audio feature is disabled
#[cfg(not(feature = "audio"))]
mod audio_stub {
    use serde::{Deserialize, Serialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AudioDevice {
        pub id: String,
        pub name: String,
    }
    
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct AudioMetrics {
        pub dummy: bool,
    }
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum ChannelState {
        Idle,
        Active,
    }
    
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum ChannelType {
        System,
        User,
    }
    
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum DeviceState {
        Active,
        Disabled,
        Unplugged,
    }
}

#[cfg(not(feature = "audio"))]
use audio_stub::{AudioDevice, AudioMetrics, ChannelState, ChannelType, DeviceState};

/// Event names used for Tauri event emission
pub mod event_names {
    /// Audio metrics updated (emitted every 100ms during active capture)
    pub const AUDIO_METRICS: &str = "audio-metrics";
    /// Audio channel state changed
    pub const CHANNEL_STATE_CHANGED: &str = "channel-state";
    /// Audio device connected or disconnected
    pub const DEVICE_CHANGED: &str = "device-changed";
    /// Error occurred in audio subsystem
    pub const AUDIO_ERROR: &str = "audio-error";
    /// Device disconnected during capture (Requirement 2.7)
    pub const DEVICE_DISCONNECTED: &str = "device-disconnected";
    /// WASAPI not available (Requirement 2.5)
    pub const WASAPI_NOT_AVAILABLE: &str = "wasapi-not-available";
    /// No audio devices available (Requirement 2.6)
    pub const NO_DEVICES_AVAILABLE: &str = "no-devices-available";
    /// Token expiring soon (10 minutes before expiration)
    pub const TOKEN_EXPIRING: &str = "token-expiring";
    /// Error from Gemini Live connection
    pub const GEMINI_ERROR: &str = "gemini-error";
    /// Usage warning at 80% threshold (Requirement 10.6)
    pub const USAGE_WARNING: &str = "usage-warning";
    /// Usage limit reached at 100% (Requirements 10.7, 11.8)
    pub const USAGE_LIMIT_REACHED: &str = "usage-limit";
    /// Translation blocked due to usage limit (Requirement 10.7)
    pub const USAGE_BLOCKED: &str = "usage-blocked";
    /// Application update available
    pub const UPDATE_AVAILABLE: &str = "update-available";
}

/// Device action for device change events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceAction {
    /// Device was connected
    Connected,
    /// Device was disconnected
    Disconnected,
    /// Device state changed (e.g., disabled, unplugged)
    StateChanged {
        #[serde(rename = "newState")]
        new_state: DeviceState,
    },
}

/// Payload for device change events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceChangedPayload {
    /// Action that occurred
    pub action: DeviceAction,
    /// Device that triggered the event
    pub device: AudioDevice,
}

/// Payload for device disconnection during capture (Requirement 2.7)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDisconnectedPayload {
    /// ID of the disconnected device
    pub device_id: String,
    /// Friendly name of the disconnected device
    pub device_name: String,
    /// Which channel was affected (system or user)
    pub channel: ChannelType,
    /// User-friendly message in Spanish
    pub message: String,
    /// Suggested recovery action
    pub suggestion: String,
}

/// Payload for channel state change events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStateChangedPayload {
    /// Which channel changed
    pub channel: ChannelType,
    /// New state of the channel
    pub state: ChannelState,
}

/// Payload for audio error events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioErrorPayload {
    /// Error code (1xxx for audio errors)
    pub code: u32,
    /// User-friendly error message in Spanish
    pub message: String,
    /// Suggested recovery action
    pub suggestion: String,
    /// Which channel is affected (if applicable)
    pub channel: Option<ChannelType>,
}

/// Payload for WASAPI not available error (Requirement 2.5)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasapiNotAvailablePayload {
    /// Specific reason why WASAPI is not available
    pub reason: String,
    /// User-friendly message in Spanish
    pub message: String,
    /// Suggested recovery step
    pub suggestion: String,
}

/// Payload for no devices available error (Requirement 2.6)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoDevicesAvailablePayload {
    /// User-friendly message in Spanish
    pub message: String,
    /// Suggested recovery step
    pub suggestion: String,
}

/// Payload for token expiring soon event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenExpiringPayload {
    /// Minutes until expiration
    pub minutes_remaining: u32,
    /// User-friendly message in Spanish
    pub message: String,
}

/// Payload for Gemini error events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiErrorPayload {
    /// Which channel encountered the error
    pub channel: ChannelType,
    /// Error message
    pub error: String,
    /// Error code if available
    pub code: Option<String>,
    /// User-friendly message in Spanish
    pub message: String,
    /// Suggested recovery action
    pub suggestion: String,
}

/// Payload for usage limit reached event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitPayload {
    /// Minutes used this month
    pub used: u32,
    /// Monthly limit in minutes
    pub limit: u32,
    /// Percentage of limit used (0-100)
    pub percentage: u32,
    /// User-friendly message in Spanish
    pub message: String,
}

/// Payload for update available event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvailablePayload {
    /// New version available
    pub version: String,
    /// Changelog/release notes
    pub changelog: String,
    /// Download URL (optional)
    pub download_url: Option<String>,
    /// Whether update is mandatory
    pub mandatory: bool,
}

/// Application events emitted by the backend
///
/// This enum provides a type-safe way to emit events from the Rust backend
/// to the React frontend. Use the `emit_app_event` function to emit these events.
///
/// # Requirement 22.2
/// Events: audio-metrics, channel-state, device-changed, token-expiring,
/// gemini-error, usage-limit, update-available
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Audio metrics updated (emitted every 100ms during active capture)
    AudioMetrics(AudioMetrics),
    
    /// Channel state changed
    ChannelStateChanged {
        channel: ChannelType,
        state: ChannelState,
    },
    
    /// Device connected/disconnected
    DeviceChanged {
        action: DeviceAction,
        device: AudioDevice,
    },
    
    /// Token expiring soon (10 minutes before expiration)
    TokenExpiringSoon {
        minutes_remaining: u32,
    },
    
    /// Error from Gemini Live connection
    GeminiError {
        channel: ChannelType,
        error: String,
        code: Option<String>,
    },
    
    /// Usage limit reached
    UsageLimitReached {
        used: u32,
        limit: u32,
    },
    
    /// Application update available
    UpdateAvailable {
        version: String,
        changelog: String,
        download_url: Option<String>,
        mandatory: bool,
    },
}

/// Emit an event to all windows
pub fn emit_event<R: tauri::Runtime, S: Serialize + Clone>(
    app: &tauri::AppHandle<R>,
    event_name: &str,
    payload: S,
) -> Result<(), tauri::Error> {
    app.emit(event_name, payload)
}

/// Emit audio metrics update
pub fn emit_audio_metrics<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    metrics: AudioMetrics,
) -> Result<(), tauri::Error> {
    emit_event(app, event_names::AUDIO_METRICS, metrics)
}

/// Emit channel state change
pub fn emit_channel_state_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    channel: ChannelType,
    state: ChannelState,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::CHANNEL_STATE_CHANGED,
        ChannelStateChangedPayload { channel, state },
    )
}

/// Emit device change event (connected/disconnected)
pub fn emit_device_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    action: DeviceAction,
    device: AudioDevice,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::DEVICE_CHANGED,
        DeviceChangedPayload { action, device },
    )
}

/// Emit device disconnected during capture event (Requirement 2.7)
///
/// This event is emitted when a device is disconnected while capture is active.
/// The frontend should:
/// 1. Pause the affected channel's UI
/// 2. Show a notification to the user
/// 3. Allow the user to select an alternative device
pub fn emit_device_disconnected<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    device_id: &str,
    device_name: &str,
    channel: ChannelType,
) -> Result<(), tauri::Error> {
    let channel_name = match channel {
        ChannelType::System => "sistema",
        ChannelType::User => "usuario",
    };
    
    emit_event(
        app,
        event_names::DEVICE_DISCONNECTED,
        DeviceDisconnectedPayload {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            channel,
            message: format!(
                "El dispositivo de audio '{}' se ha desconectado durante la captura del canal {}.",
                device_name, channel_name
            ),
            suggestion: "Reconecta el dispositivo o selecciona uno alternativo para continuar.".to_string(),
        },
    )
}

/// Emit WASAPI not available error (Requirement 2.5)
///
/// This event is emitted when WASAPI is not available on the system.
/// Common causes:
/// - Windows Audio service is not running
/// - Audio drivers are not installed
/// - Audio subsystem initialization failed
pub fn emit_wasapi_not_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    reason: &str,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::WASAPI_NOT_AVAILABLE,
        WasapiNotAvailablePayload {
            reason: reason.to_string(),
            message: format!(
                "WASAPI no está disponible en este sistema: {}",
                reason
            ),
            suggestion: "Verifica que el servicio Windows Audio esté ejecutándose. \
                         Abre services.msc y busca 'Windows Audio', asegúrate de que esté \
                         iniciado y configurado como automático.".to_string(),
        },
    )
}

/// Emit no devices available error (Requirement 2.6)
///
/// This event is emitted when no audio output devices are found.
/// The frontend should disable the system channel activation and
/// show appropriate guidance.
pub fn emit_no_devices_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::NO_DEVICES_AVAILABLE,
        NoDevicesAvailablePayload {
            message: "No hay dispositivos de audio de salida disponibles.".to_string(),
            suggestion: "Conecta altavoces o auriculares y asegúrate de que estén \
                         habilitados en la configuración de sonido de Windows.".to_string(),
        },
    )
}

/// Emit generic audio error
pub fn emit_audio_error<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    code: u32,
    message: &str,
    suggestion: &str,
    channel: Option<ChannelType>,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::AUDIO_ERROR,
        AudioErrorPayload {
            code,
            message: message.to_string(),
            suggestion: suggestion.to_string(),
            channel,
        },
    )
}

/// Emit token expiring soon event
///
/// This event should be emitted 10 minutes before the token expires
/// to allow the frontend to refresh the token or notify the user.
pub fn emit_token_expiring<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    minutes_remaining: u32,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::TOKEN_EXPIRING,
        TokenExpiringPayload {
            minutes_remaining,
            message: format!(
                "Tu sesión expirará en {} minutos. Se renovará automáticamente.",
                minutes_remaining
            ),
        },
    )
}

/// Emit Gemini error event
///
/// This event is emitted when there's an error with the Gemini Live connection.
/// The frontend should handle this by showing an error message and potentially
/// attempting to reconnect.
pub fn emit_gemini_error<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    channel: ChannelType,
    error: &str,
    code: Option<&str>,
) -> Result<(), tauri::Error> {
    let channel_name = match channel {
        ChannelType::System => "sistema",
        ChannelType::User => "usuario",
    };
    
    emit_event(
        app,
        event_names::GEMINI_ERROR,
        GeminiErrorPayload {
            channel,
            error: error.to_string(),
            code: code.map(|c| c.to_string()),
            message: format!(
                "Error de conexión con Gemini en el canal {}: {}",
                channel_name, error
            ),
            suggestion: "Se intentará reconectar automáticamente. Si el problema persiste, \
                         verifica tu conexión a internet.".to_string(),
        },
    )
}

/// Emit usage limit reached event
///
/// This event is emitted when the user reaches their monthly usage limit.
/// The frontend should block translation and show upgrade options.
///
/// # Requirements
/// - Requirement 10.7: Block translation at 100% and show upgrade options
/// - Requirement 11.8: Notify when usage limit is reached
pub fn emit_usage_limit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    used: u32,
    limit: u32,
) -> Result<(), tauri::Error> {
    let percentage = if limit > 0 {
        ((used as f64 / limit as f64) * 100.0).min(100.0) as u32
    } else {
        0
    };
    
    emit_event(
        app,
        event_names::USAGE_LIMIT_REACHED,
        UsageLimitPayload {
            used,
            limit,
            percentage,
            message: format!(
                "Has alcanzado el límite de {} minutos de tu plan. \
                 Actualiza tu suscripción para continuar.",
                limit
            ),
        },
    )
}

/// Emit update available event
///
/// This event is emitted when a new version of the application is available.
/// The frontend should show a notification and allow the user to download/install.
pub fn emit_update_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    version: &str,
    changelog: &str,
    download_url: Option<&str>,
    mandatory: bool,
) -> Result<(), tauri::Error> {
    emit_event(
        app,
        event_names::UPDATE_AVAILABLE,
        UpdateAvailablePayload {
            version: version.to_string(),
            changelog: changelog.to_string(),
            download_url: download_url.map(|u| u.to_string()),
            mandatory,
        },
    )
}

/// Emit an AppEvent to all windows
///
/// This is a convenience function that takes an `AppEvent` enum value
/// and emits the appropriate event with the correct payload.
///
/// # Example
///
/// ```ignore
/// use traductor_desktop_lib::events::{emit_app_event, AppEvent};
/// // Requires a valid Tauri AppHandle, typically obtained in a Tauri command
/// // let metrics = AudioMetrics::default();
/// // emit_app_event(&app_handle, AppEvent::AudioMetrics(metrics));
/// ```
pub fn emit_app_event<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    event: AppEvent,
) -> Result<(), tauri::Error> {
    match event {
        AppEvent::AudioMetrics(metrics) => {
            app.emit(event_names::AUDIO_METRICS, metrics)
        }
        AppEvent::ChannelStateChanged { channel, state } => {
            app.emit(
                event_names::CHANNEL_STATE_CHANGED,
                ChannelStateChangedPayload { channel, state },
            )
        }
        AppEvent::DeviceChanged { action, device } => {
            app.emit(
                event_names::DEVICE_CHANGED,
                DeviceChangedPayload { action, device },
            )
        }
        AppEvent::TokenExpiringSoon { minutes_remaining } => {
            app.emit(
                event_names::TOKEN_EXPIRING,
                TokenExpiringPayload {
                    minutes_remaining,
                    message: format!(
                        "Tu sesión expirará en {} minutos. Se renovará automáticamente.",
                        minutes_remaining
                    ),
                },
            )
        }
        AppEvent::GeminiError { channel, error, code } => {
            let channel_name = match channel {
                ChannelType::System => "sistema",
                ChannelType::User => "usuario",
            };
            app.emit(
                event_names::GEMINI_ERROR,
                GeminiErrorPayload {
                    channel,
                    error: error.clone(),
                    code,
                    message: format!(
                        "Error de conexión con Gemini en el canal {}: {}",
                        channel_name, error
                    ),
                    suggestion: "Se intentará reconectar automáticamente. Si el problema persiste, \
                                 verifica tu conexión a internet.".to_string(),
                },
            )
        }
        AppEvent::UsageLimitReached { used, limit } => {
            let percentage = if limit > 0 {
                ((used as f64 / limit as f64) * 100.0).min(100.0) as u32
            } else {
                0
            };
            app.emit(
                event_names::USAGE_LIMIT_REACHED,
                UsageLimitPayload {
                    used,
                    limit,
                    percentage,
                    message: format!(
                        "Has alcanzado el límite de {} minutos de tu plan. \
                         Actualiza tu suscripción para continuar.",
                        limit
                    ),
                },
            )
        }
        AppEvent::UpdateAvailable { version, changelog, download_url, mandatory } => {
            app.emit(
                event_names::UPDATE_AVAILABLE,
                UpdateAvailablePayload {
                    version,
                    changelog,
                    download_url,
                    mandatory,
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_disconnected_payload_serialization() {
        let payload = DeviceDisconnectedPayload {
            device_id: "test-id".to_string(),
            device_name: "Auriculares Bluetooth".to_string(),
            channel: ChannelType::System,
            message: "Test message".to_string(),
            suggestion: "Test suggestion".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("deviceId"));
        assert!(json.contains("deviceName"));
        assert!(json.contains("Auriculares Bluetooth"));
    }

    #[test]
    fn test_wasapi_not_available_payload() {
        let payload = WasapiNotAvailablePayload {
            reason: "Service not running".to_string(),
            message: "WASAPI no disponible".to_string(),
            suggestion: "Verifica Windows Audio".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("reason"));
        assert!(json.contains("message"));
        assert!(json.contains("suggestion"));
    }

    #[test]
    fn test_device_action_serialization() {
        let action = DeviceAction::Disconnected;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"disconnected\"");

        let action = DeviceAction::StateChanged { 
            new_state: DeviceState::Unplugged 
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("stateChanged"));
        assert!(json.contains("newState"));
    }

    #[test]
    fn test_token_expiring_payload_serialization() {
        let payload = TokenExpiringPayload {
            minutes_remaining: 10,
            message: "Tu sesión expirará en 10 minutos".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("minutesRemaining"));
        assert!(json.contains("10"));
        assert!(json.contains("message"));
    }

    #[test]
    fn test_gemini_error_payload_serialization() {
        let payload = GeminiErrorPayload {
            channel: ChannelType::System,
            error: "Connection timeout".to_string(),
            code: Some("TIMEOUT".to_string()),
            message: "Error de conexión".to_string(),
            suggestion: "Verifica tu conexión".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("channel"));
        assert!(json.contains("system"));
        assert!(json.contains("error"));
        assert!(json.contains("Connection timeout"));
        assert!(json.contains("TIMEOUT"));
    }

    #[test]
    fn test_gemini_error_payload_without_code() {
        let payload = GeminiErrorPayload {
            channel: ChannelType::User,
            error: "Network error".to_string(),
            code: None,
            message: "Error de red".to_string(),
            suggestion: "Intenta de nuevo".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"code\":null"));
    }

    #[test]
    fn test_usage_limit_payload_serialization() {
        let payload = UsageLimitPayload {
            used: 60,
            limit: 60,
            percentage: 100,
            message: "Has alcanzado el límite".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("used"));
        assert!(json.contains("limit"));
        assert!(json.contains("percentage"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_update_available_payload_serialization() {
        let payload = UpdateAvailablePayload {
            version: "1.2.0".to_string(),
            changelog: "- Nueva función\n- Corrección de errores".to_string(),
            download_url: Some("https://example.com/update".to_string()),
            mandatory: false,
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("version"));
        assert!(json.contains("1.2.0"));
        assert!(json.contains("changelog"));
        assert!(json.contains("downloadUrl"));
        assert!(json.contains("mandatory"));
    }

    #[test]
    fn test_update_available_payload_without_url() {
        let payload = UpdateAvailablePayload {
            version: "2.0.0".to_string(),
            changelog: "Major update".to_string(),
            download_url: None,
            mandatory: true,
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"downloadUrl\":null"));
        assert!(json.contains("\"mandatory\":true"));
    }

    #[test]
    fn test_event_names_are_kebab_case() {
        // All event names should follow kebab-case convention for frontend consistency
        assert_eq!(event_names::AUDIO_METRICS, "audio-metrics");
        assert_eq!(event_names::CHANNEL_STATE_CHANGED, "channel-state");
        assert_eq!(event_names::DEVICE_CHANGED, "device-changed");
        assert_eq!(event_names::TOKEN_EXPIRING, "token-expiring");
        assert_eq!(event_names::GEMINI_ERROR, "gemini-error");
        assert_eq!(event_names::USAGE_LIMIT_REACHED, "usage-limit");
        assert_eq!(event_names::UPDATE_AVAILABLE, "update-available");
    }
}
