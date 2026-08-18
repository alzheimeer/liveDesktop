//! BYOK API Key Validator
//!
//! This module provides validation for Bring Your Own Key (BYOK) API keys.
//! It validates both the format and the actual API key against Gemini's API.
//!
//! Requirements: 8.1, 8.2, 8.6

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::keyring::KeyringManager;

/// Gemini API endpoint for lightweight validation
/// Using the models.list endpoint as it's lightweight and requires authentication
const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Timeout for API validation requests
const VALIDATION_TIMEOUT_SECS: u64 = 10;

/// Error types for BYOK validation operations
#[derive(Debug, Clone)]
pub enum ByokValidationError {
    /// API key format is invalid (8001)
    InvalidFormat { reason: String },
    /// Gemini rejected the API key - 401 Unauthorized (8002)
    InvalidApiKey,
    /// Gemini rejected the API key - 403 Forbidden (8003)
    ApiKeyForbidden,
    /// Network error during validation (8004)
    NetworkError { reason: String },
    /// Validation request timed out (8005)
    Timeout,
    /// Unexpected error from Gemini API (8006)
    GeminiError { status: u16, message: String },
}

impl ByokValidationError {
    /// Get the error code
    pub fn code(&self) -> u32 {
        match self {
            ByokValidationError::InvalidFormat { .. } => 8001,
            ByokValidationError::InvalidApiKey => 8002,
            ByokValidationError::ApiKeyForbidden => 8003,
            ByokValidationError::NetworkError { .. } => 8004,
            ByokValidationError::Timeout => 8005,
            ByokValidationError::GeminiError { .. } => 8006,
        }
    }

    /// Get user-friendly message in Spanish
    pub fn message(&self) -> String {
        match self {
            ByokValidationError::InvalidFormat { reason } => {
                format!("Formato de API key inválido: {}", reason)
            }
            ByokValidationError::InvalidApiKey => {
                "La API key es inválida o ha sido revocada".to_string()
            }
            ByokValidationError::ApiKeyForbidden => {
                "La API key no tiene permisos para acceder a la API de Gemini".to_string()
            }
            ByokValidationError::NetworkError { reason } => {
                format!("Error de red al validar la API key: {}", reason)
            }
            ByokValidationError::Timeout => {
                "La validación de la API key excedió el tiempo límite".to_string()
            }
            ByokValidationError::GeminiError { status, message } => {
                format!("Error de Gemini ({}): {}", status, message)
            }
        }
    }

    /// Get recovery suggestion in Spanish
    pub fn suggestion(&self) -> &'static str {
        match self {
            ByokValidationError::InvalidFormat { .. } => {
                "La API key debe tener entre 1 y 256 caracteres alfanuméricos (incluyendo - y _)"
            }
            ByokValidationError::InvalidApiKey => {
                "Verifica que la API key sea correcta en tu consola de Google AI Studio"
            }
            ByokValidationError::ApiKeyForbidden => {
                "Verifica que la API key tenga habilitada la API de Gemini en Google Cloud Console"
            }
            ByokValidationError::NetworkError { .. } => {
                "Verifica tu conexión a internet e intenta nuevamente"
            }
            ByokValidationError::Timeout => {
                "Verifica tu conexión a internet e intenta nuevamente"
            }
            ByokValidationError::GeminiError { .. } => {
                "Intenta nuevamente. Si el problema persiste, verifica tu API key"
            }
        }
    }
}

impl std::fmt::Display for ByokValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for ByokValidationError {}

/// Gemini API error response structure
#[derive(Debug, Deserialize)]
struct GeminiErrorResponse {
    error: Option<GeminiErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorDetail {
    message: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

/// Result of API key validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the API key is valid
    pub valid: bool,
    /// Error message if invalid
    pub error_message: Option<String>,
    /// Suggestion for fixing the issue
    pub suggestion: Option<String>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            valid: true,
            error_message: None,
            suggestion: None,
        }
    }

    /// Create a failed validation result from an error
    pub fn from_error(error: &ByokValidationError) -> Self {
        Self {
            valid: false,
            error_message: Some(error.message()),
            suggestion: Some(error.suggestion().to_string()),
        }
    }
}

/// BYOK API Key Validator
///
/// Provides methods to validate API keys both for format and against the Gemini API.
pub struct ByokValidator {
    http_client: Client,
}

impl Default for ByokValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ByokValidator {
    /// Create a new BYOK validator
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(VALIDATION_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { http_client }
    }

    /// Create validator with custom HTTP client (for testing)
    pub fn with_client(http_client: Client) -> Self {
        Self { http_client }
    }

    /// Validate API key format
    ///
    /// A valid API key must:
    /// - Be between 1 and 256 characters
    /// - Contain only alphanumeric characters, hyphens (-), or underscores (_)
    ///
    /// # Arguments
    /// * `api_key` - The API key to validate
    ///
    /// # Returns
    /// * `Ok(())` - If the format is valid
    /// * `Err(ByokValidationError)` - If the format is invalid
    ///
    /// # Requirements
    /// Implements Requirements 8.1, 8.2
    pub fn validate_format(api_key: &str) -> Result<(), ByokValidationError> {
        if api_key.is_empty() {
            return Err(ByokValidationError::InvalidFormat {
                reason: "La API key no puede estar vacía".to_string(),
            });
        }

        if api_key.len() > 256 {
            return Err(ByokValidationError::InvalidFormat {
                reason: format!(
                    "La API key excede el límite de 256 caracteres (tiene {} caracteres)",
                    api_key.len()
                ),
            });
        }

        if !api_key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ByokValidationError::InvalidFormat {
                reason: "La API key contiene caracteres inválidos".to_string(),
            });
        }

        Ok(())
    }

    /// Test the API key against Gemini's API
    ///
    /// Makes a lightweight API call (list models) to verify the key works.
    ///
    /// # Arguments
    /// * `api_key` - The API key to test
    ///
    /// # Returns
    /// * `Ok(())` - If Gemini accepts the key
    /// * `Err(ByokValidationError)` - If Gemini rejects the key
    ///
    /// # Requirements
    /// Implements Requirement 8.6
    pub async fn test_api_key(&self, api_key: &str) -> Result<(), ByokValidationError> {
        // Use models.list as a lightweight endpoint to test the key
        let url = format!("{}/models?key={}", GEMINI_API_BASE, api_key);

        let response = self.http_client.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                ByokValidationError::Timeout
            } else {
                ByokValidationError::NetworkError {
                    reason: e.to_string(),
                }
            }
        })?;

        let status = response.status();

        match status.as_u16() {
            200..=299 => Ok(()),
            401 => Err(ByokValidationError::InvalidApiKey),
            403 => Err(ByokValidationError::ApiKeyForbidden),
            _ => {
                // Try to parse error message from response
                let error_msg = match response.json::<GeminiErrorResponse>().await {
                    Ok(err_resp) => err_resp
                        .error
                        .and_then(|e| e.message)
                        .unwrap_or_else(|| "Error desconocido".to_string()),
                    Err(_) => "No se pudo leer el error de Gemini".to_string(),
                };

                Err(ByokValidationError::GeminiError {
                    status: status.as_u16(),
                    message: error_msg,
                })
            }
        }
    }

    /// Validate API key completely (format + API test)
    ///
    /// This method:
    /// 1. Validates the API key format
    /// 2. Tests the key against Gemini's API
    ///
    /// # Arguments
    /// * `api_key` - The API key to validate
    ///
    /// # Returns
    /// * `Ok(ValidationResult)` - Result with validation status
    ///
    /// # Requirements
    /// Implements Requirements 8.1, 8.2, 8.6
    pub async fn validate_api_key(&self, api_key: &str) -> ValidationResult {
        // Step 1: Validate format
        if let Err(e) = Self::validate_format(api_key) {
            return ValidationResult::from_error(&e);
        }

        // Step 2: Test against Gemini API
        if let Err(e) = self.test_api_key(api_key).await {
            return ValidationResult::from_error(&e);
        }

        ValidationResult::success()
    }
}

// ============================================================================
// Convenience functions for use as Tauri commands
// These wrap ByokValidator methods and return serializable results
// ============================================================================

/// Validate API key format only (synchronous)
///
/// Returns true if the key format is valid, false otherwise.
pub fn validate_api_key_format(api_key: &str) -> bool {
    ByokValidator::validate_format(api_key).is_ok()
}

/// Validate API key completely (format + Gemini API test)
///
/// Returns a ValidationResult with details about the validation.
pub async fn validate_api_key(api_key: &str) -> ValidationResult {
    let validator = ByokValidator::new();
    validator.validate_api_key(api_key).await
}

/// Validate the stored BYOK API key
///
/// Retrieves the API key from the keyring and validates it.
///
/// # Returns
/// * `Ok(ValidationResult)` - If key was retrieved and validated
/// * `Err(String)` - If key retrieval failed
pub async fn validate_stored_byok_key() -> Result<ValidationResult, String> {
    // Get the stored API key
    let api_key = KeyringManager::get_byok_key().map_err(|e| e.to_string())?;

    match api_key {
        Some(key) => Ok(validate_api_key(&key).await),
        None => Ok(ValidationResult {
            valid: false,
            error_message: Some("No hay API key almacenada".to_string()),
            suggestion: Some("Ingresa tu API key de Gemini en la configuración".to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Format Validation Tests
    // ========================================================================

    #[test]
    fn test_validate_format_valid_keys() {
        // Valid keys
        assert!(ByokValidator::validate_format("abc123").is_ok());
        assert!(ByokValidator::validate_format("API-KEY-123").is_ok());
        assert!(ByokValidator::validate_format("api_key_test").is_ok());
        assert!(ByokValidator::validate_format("a").is_ok()); // Minimum length
        assert!(ByokValidator::validate_format(&"a".repeat(256)).is_ok()); // Maximum length
        assert!(ByokValidator::validate_format("AIzaSyB-test_KEY123").is_ok());
    }

    #[test]
    fn test_validate_format_invalid_empty() {
        let result = ByokValidator::validate_format("");
        assert!(result.is_err());
        match result {
            Err(ByokValidationError::InvalidFormat { reason }) => {
                assert!(reason.contains("vacía"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_validate_format_invalid_too_long() {
        let result = ByokValidator::validate_format(&"a".repeat(257));
        assert!(result.is_err());
        match result {
            Err(ByokValidationError::InvalidFormat { reason }) => {
                assert!(reason.contains("256"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_validate_format_invalid_characters() {
        // Keys with invalid characters
        assert!(ByokValidator::validate_format("key with spaces").is_err());
        assert!(ByokValidator::validate_format("key@special!chars").is_err());
        assert!(ByokValidator::validate_format("key.with.dots").is_err());
        assert!(ByokValidator::validate_format("key=value").is_err());
        assert!(ByokValidator::validate_format("key/path").is_err());
    }

    // ========================================================================
    // Convenience Function Tests
    // ========================================================================

    #[test]
    fn test_validate_api_key_format_function() {
        assert!(validate_api_key_format("valid-key_123"));
        assert!(!validate_api_key_format(""));
        assert!(!validate_api_key_format("key with spaces"));
        assert!(!validate_api_key_format(&"a".repeat(257)));
    }

    // ========================================================================
    // Error Code Tests
    // ========================================================================

    #[test]
    fn test_byok_validation_error_codes_in_8xxx_range() {
        let errors = vec![
            ByokValidationError::InvalidFormat {
                reason: "test".to_string(),
            },
            ByokValidationError::InvalidApiKey,
            ByokValidationError::ApiKeyForbidden,
            ByokValidationError::NetworkError {
                reason: "test".to_string(),
            },
            ByokValidationError::Timeout,
            ByokValidationError::GeminiError {
                status: 500,
                message: "test".to_string(),
            },
        ];

        for err in errors {
            let code = err.code();
            assert!(
                code >= 8000 && code < 9000,
                "BYOK validation error code {} not in 8xxx range",
                code
            );
        }
    }

    #[test]
    fn test_byok_validation_error_has_message_and_suggestion() {
        let err = ByokValidationError::InvalidApiKey;

        let message = err.message();
        assert!(message.contains("inválida") || message.contains("revocada"));

        let suggestion = err.suggestion();
        assert!(!suggestion.is_empty());
    }

    // ========================================================================
    // ValidationResult Tests
    // ========================================================================

    #[test]
    fn test_validation_result_success() {
        let result = ValidationResult::success();
        assert!(result.valid);
        assert!(result.error_message.is_none());
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_validation_result_from_error() {
        let error = ByokValidationError::InvalidApiKey;
        let result = ValidationResult::from_error(&error);

        assert!(!result.valid);
        assert!(result.error_message.is_some());
        assert!(result.suggestion.is_some());
    }

    // ========================================================================
    // Integration Tests (require network, marked as ignored)
    // ========================================================================

    #[tokio::test]
    #[ignore = "Requires network access and a valid/invalid API key"]
    async fn test_validate_api_key_with_invalid_key() {
        let result = validate_api_key("invalid-test-key-12345").await;
        assert!(!result.valid);
        // Should get InvalidApiKey or ApiKeyForbidden
        assert!(result.error_message.is_some());
    }
}
