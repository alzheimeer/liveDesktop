//! Application error types
//!
//! Provides structured error handling with error codes, user-friendly messages,
//! and recovery suggestions. Error codes are organized by category:
//! - Audio errors: 1xxx
//! - Network errors: 2xxx
//! - Auth errors: 3xxx
//! - Subscription errors: 4xxx
//! - Storage errors: 5xxx

use serde::Serialize;

/// Application error with code, message and recovery suggestion
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    /// Error code for categorization (1xxx = audio, 2xxx = network, etc.)
    pub code: u32,
    /// User-friendly error message in Spanish
    pub message: String,
    /// Suggested recovery action
    pub suggestion: String,
    /// Technical details (for logging, not shown to user)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

/// Audio-specific errors (1xxx codes)
#[derive(Debug, Clone)]
pub enum AudioError {
    /// Device not found (1001)
    DeviceNotFound { device_id: String, device_name: Option<String> },
    /// Device disconnected during capture (1002)
    DeviceDisconnected { device_id: String, device_name: String },
    /// WASAPI not available on system (1003)
    WasapiNotAvailable { reason: String },
    /// ScreenCaptureKit not available (1004)
    ScreenCaptureNotAvailable { reason: String },
    /// Screen recording permission denied (1005)
    ScreenRecordingPermissionDenied,
    /// VB-Cable not installed (1006)
    VBCableNotInstalled,
    /// VB-Cable output not available (1007)
    VBCableOutputNotAvailable,
    /// Audio buffer overrun (1008)
    BufferOverrun { dropped_frames: u32 },
    /// Capture timeout (1009)
    CaptureTimeout,
    /// No audio devices available (1010)
    NoDevicesAvailable,
    /// Capture initialization failed (1011)
    CaptureInitFailed { reason: String },
    /// Capture already active (1012)
    CaptureAlreadyActive,
    /// Capture not active (1013)
    CaptureNotActive,
    /// Invalid audio format (1014)
    InvalidAudioFormat { details: String },
    /// Device state changed (1015)
    DeviceStateChanged { device_id: String, new_state: DeviceState },
}

/// Device state for change notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DeviceState {
    Active,
    Disabled,
    NotPresent,
    Unplugged,
}

impl AudioError {
    /// Get the error code
    pub fn code(&self) -> u32 {
        match self {
            AudioError::DeviceNotFound { .. } => 1001,
            AudioError::DeviceDisconnected { .. } => 1002,
            AudioError::WasapiNotAvailable { .. } => 1003,
            AudioError::ScreenCaptureNotAvailable { .. } => 1004,
            AudioError::ScreenRecordingPermissionDenied => 1005,
            AudioError::VBCableNotInstalled => 1006,
            AudioError::VBCableOutputNotAvailable => 1007,
            AudioError::BufferOverrun { .. } => 1008,
            AudioError::CaptureTimeout => 1009,
            AudioError::NoDevicesAvailable => 1010,
            AudioError::CaptureInitFailed { .. } => 1011,
            AudioError::CaptureAlreadyActive => 1012,
            AudioError::CaptureNotActive => 1013,
            AudioError::InvalidAudioFormat { .. } => 1014,
            AudioError::DeviceStateChanged { .. } => 1015,
        }
    }

    /// Get user-friendly message in Spanish
    pub fn message(&self) -> String {
        match self {
            AudioError::DeviceNotFound { device_name, .. } => {
                match device_name {
                    Some(name) => format!("Dispositivo de audio '{}' no encontrado", name),
                    None => "Dispositivo de audio no encontrado".to_string(),
                }
            }
            AudioError::DeviceDisconnected { device_name, .. } => {
                format!("El dispositivo de audio '{}' se ha desconectado", device_name)
            }
            AudioError::WasapiNotAvailable { reason } => {
                format!("WASAPI no está disponible: {}", reason)
            }
            AudioError::ScreenCaptureNotAvailable { reason } => {
                format!("ScreenCaptureKit no disponible: {}", reason)
            }
            AudioError::ScreenRecordingPermissionDenied => {
                "Permiso de grabación de pantalla denegado".to_string()
            }
            AudioError::VBCableNotInstalled => {
                "VB-Cable no está instalado".to_string()
            }
            AudioError::VBCableOutputNotAvailable => {
                "El dispositivo VB-Cable Output no está disponible".to_string()
            }
            AudioError::BufferOverrun { dropped_frames } => {
                format!("Se perdieron {} frames de audio por sobrecarga del buffer", dropped_frames)
            }
            AudioError::CaptureTimeout => {
                "La operación de captura excedió el tiempo límite".to_string()
            }
            AudioError::NoDevicesAvailable => {
                "No hay dispositivos de audio de salida disponibles".to_string()
            }
            AudioError::CaptureInitFailed { reason } => {
                format!("Error al inicializar la captura de audio: {}", reason)
            }
            AudioError::CaptureAlreadyActive => {
                "La captura de audio ya está activa".to_string()
            }
            AudioError::CaptureNotActive => {
                "La captura de audio no está activa".to_string()
            }
            AudioError::InvalidAudioFormat { details } => {
                format!("Formato de audio inválido: {}", details)
            }
            AudioError::DeviceStateChanged { device_id: _, new_state } => {
                match new_state {
                    DeviceState::Active => "El dispositivo está ahora activo".to_string(),
                    DeviceState::Disabled => "El dispositivo ha sido deshabilitado".to_string(),
                    DeviceState::NotPresent => "El dispositivo ya no está presente".to_string(),
                    DeviceState::Unplugged => "El dispositivo ha sido desconectado".to_string(),
                }
            }
        }
    }

    /// Get recovery suggestion in Spanish
    pub fn suggestion(&self) -> &'static str {
        match self {
            AudioError::DeviceNotFound { .. } => {
                "Verifica que el dispositivo esté conectado y selecciona otro"
            }
            AudioError::DeviceDisconnected { .. } => {
                "Reconecta el dispositivo o selecciona uno alternativo"
            }
            AudioError::WasapiNotAvailable { .. } => {
                "Verifica que el servicio Windows Audio esté ejecutándose (services.msc)"
            }
            AudioError::ScreenCaptureNotAvailable { .. } => {
                "Esta función requiere macOS 14 (Sonoma) o superior"
            }
            AudioError::ScreenRecordingPermissionDenied => {
                "Abre Preferencias del Sistema > Privacidad > Grabación de Pantalla"
            }
            AudioError::VBCableNotInstalled => {
                "Instala VB-Cable desde el asistente de configuración inicial"
            }
            AudioError::VBCableOutputNotAvailable => {
                "Verifica que VB-Cable esté instalado correctamente y no esté en uso por otra aplicación"
            }
            AudioError::BufferOverrun { .. } => {
                "Cierra otras aplicaciones que usen audio o reduce la calidad de captura"
            }
            AudioError::CaptureTimeout => {
                "Intenta nuevamente. Si el problema persiste, reinicia el servicio de audio"
            }
            AudioError::NoDevicesAvailable => {
                "Conecta un dispositivo de audio (altavoces o auriculares) y asegúrate de que esté habilitado"
            }
            AudioError::CaptureInitFailed { .. } => {
                "Intenta reiniciar la aplicación. Si el problema persiste, verifica los controladores de audio"
            }
            AudioError::CaptureAlreadyActive => {
                "Detén la captura actual antes de iniciar una nueva"
            }
            AudioError::CaptureNotActive => {
                "Inicia la captura de audio primero"
            }
            AudioError::InvalidAudioFormat { .. } => {
                "Verifica la configuración del dispositivo de audio en el Panel de Control"
            }
            AudioError::DeviceStateChanged { .. } => {
                "Selecciona un dispositivo alternativo si el actual no está disponible"
            }
        }
    }
}

impl From<AudioError> for AppError {
    fn from(err: AudioError) -> Self {
        AppError {
            code: err.code(),
            message: err.message(),
            suggestion: err.suggestion().to_string(),
            details: Some(format!("{:?}", err)),
        }
    }
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for AudioError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_error_codes_in_1xxx_range() {
        let errors = vec![
            AudioError::DeviceNotFound { device_id: "test".to_string(), device_name: None },
            AudioError::DeviceDisconnected { device_id: "test".to_string(), device_name: "Test".to_string() },
            AudioError::WasapiNotAvailable { reason: "test".to_string() },
            AudioError::NoDevicesAvailable,
            AudioError::CaptureTimeout,
        ];

        for err in errors {
            let code = err.code();
            assert!(code >= 1000 && code < 2000, "Audio error code {} not in 1xxx range", code);
        }
    }

    #[test]
    fn test_audio_error_has_message_and_suggestion() {
        let err = AudioError::DeviceDisconnected {
            device_id: "test-id".to_string(),
            device_name: "Auriculares Bluetooth".to_string(),
        };

        let message = err.message();
        assert!(message.contains("Auriculares Bluetooth"));
        assert!(message.contains("desconectado"));

        let suggestion = err.suggestion();
        assert!(!suggestion.is_empty());
    }

    #[test]
    fn test_audio_error_to_app_error() {
        let audio_err = AudioError::WasapiNotAvailable {
            reason: "Service not running".to_string(),
        };
        
        let app_err: AppError = audio_err.into();
        
        assert_eq!(app_err.code, 1003);
        assert!(app_err.message.contains("WASAPI"));
        assert!(app_err.suggestion.contains("Windows Audio"));
    }

    #[test]
    fn test_no_devices_available_error() {
        let err = AudioError::NoDevicesAvailable;
        
        assert_eq!(err.code(), 1010);
        assert!(err.message().contains("No hay dispositivos"));
        assert!(err.suggestion().contains("Conecta"));
    }
}
