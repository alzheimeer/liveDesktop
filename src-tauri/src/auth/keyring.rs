//! OS Keyring wrapper for secure API key storage
//!
//! This module provides secure storage for BYOK (Bring Your Own Key) API keys
//! using the operating system's native credential manager:
//! - Windows: Windows Credential Manager
//! - macOS: macOS Keychain
//!
//! Requirements: 8.3, 8.7

use keyring::Entry;
use thiserror::Error;

/// Service name used to identify credentials in the OS keyring
const SERVICE_NAME: &str = "traductor-desktop";

/// Username/account name for the BYOK API key entry
const BYOK_USERNAME: &str = "byok-api-key";

/// Errors that can occur during keyring operations
#[derive(Error, Debug)]
pub enum KeyringError {
    /// Failed to access the OS keyring
    #[error("Failed to access OS keyring: {0}")]
    AccessError(String),

    /// Failed to store the API key
    #[error("Failed to store API key: {0}")]
    StoreError(String),

    /// Failed to retrieve the API key
    #[error("Failed to retrieve API key: {0}")]
    RetrieveError(String),

    /// Failed to delete the API key
    #[error("Failed to delete API key: {0}")]
    DeleteError(String),

    /// The keyring entry was not found
    #[error("API key not found in keyring")]
    NotFound,

    /// Invalid API key format
    #[error("Invalid API key format: {0}")]
    InvalidFormat(String),
}

/// Manages secure storage of API keys using the OS keyring
pub struct KeyringManager;

impl KeyringManager {
    /// Creates a keyring entry for the BYOK API key
    fn get_entry() -> Result<Entry, KeyringError> {
        Entry::new(SERVICE_NAME, BYOK_USERNAME)
            .map_err(|e| KeyringError::AccessError(e.to_string()))
    }

    /// Store BYOK API key securely in OS keyring
    ///
    /// The API key is stored in:
    /// - Windows: Windows Credential Manager
    /// - macOS: macOS Keychain
    ///
    /// # Arguments
    /// * `api_key` - The Gemini API key to store (1-256 characters)
    ///
    /// # Returns
    /// * `Ok(())` - If the key was stored successfully
    /// * `Err(KeyringError)` - If storage failed
    ///
    /// # Requirements
    /// Implements Requirements 8.3 (secure storage in OS keyring)
    pub fn set_byok_key(api_key: &str) -> Result<(), KeyringError> {
        // Validate the key format before storing
        if !Self::validate_key_format(api_key) {
            return Err(KeyringError::InvalidFormat(
                "API key must be 1-256 alphanumeric characters (including - and _)".to_string(),
            ));
        }

        let entry = Self::get_entry()?;
        entry
            .set_password(api_key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Retrieve BYOK API key from OS keyring
    ///
    /// # Returns
    /// * `Ok(Some(key))` - If the key exists and was retrieved successfully
    /// * `Ok(None)` - If no key is stored
    /// * `Err(KeyringError)` - If retrieval failed for other reasons
    ///
    /// # Requirements
    /// Implements Requirements 8.3 (retrieve from OS keyring)
    pub fn get_byok_key() -> Result<Option<String>, KeyringError> {
        let entry = Self::get_entry()?;

        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(KeyringError::RetrieveError(e.to_string())),
        }
    }

    /// Check if BYOK API key exists in OS keyring
    ///
    /// # Returns
    /// * `Ok(true)` - If an API key is stored
    /// * `Ok(false)` - If no API key is stored
    /// * `Err(KeyringError)` - If the check failed
    pub fn has_byok_key() -> Result<bool, KeyringError> {
        let entry = Self::get_entry()?;

        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(KeyringError::RetrieveError(e.to_string())),
        }
    }

    /// Delete BYOK API key from OS keyring
    ///
    /// # Returns
    /// * `Ok(())` - If the key was deleted or didn't exist
    /// * `Err(KeyringError)` - If deletion failed
    ///
    /// # Requirements
    /// Implements Requirements 8.7 (user can delete stored API key)
    pub fn delete_byok_key() -> Result<(), KeyringError> {
        let entry = Self::get_entry()?;

        match entry.delete_password() {
            Ok(()) => Ok(()),
            // Treat "not found" as success - key is already gone
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyringError::DeleteError(e.to_string())),
        }
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
    /// * `true` - If the key format is valid
    /// * `false` - If the key format is invalid
    ///
    /// # Requirements
    /// Implements Requirements 8.1, 8.2 (validate API key format)
    pub fn validate_key_format(api_key: &str) -> bool {
        !api_key.is_empty()
            && api_key.len() <= 256
            && api_key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }
}

// ============================================================================
// Convenience functions for use as Tauri commands
// These wrap KeyringManager methods and convert errors to String for IPC
// ============================================================================

/// Store BYOK API key (convenience function for IPC)
pub fn set_byok_key(api_key: &str) -> Result<(), String> {
    KeyringManager::set_byok_key(api_key).map_err(|e| e.to_string())
}

/// Get BYOK API key (convenience function for IPC)
pub fn get_byok_key() -> Result<Option<String>, String> {
    KeyringManager::get_byok_key().map_err(|e| e.to_string())
}

/// Check if BYOK key exists (convenience function for IPC)
pub fn has_byok_key() -> Result<bool, String> {
    KeyringManager::has_byok_key().map_err(|e| e.to_string())
}

/// Delete BYOK API key (convenience function for IPC)
pub fn delete_byok_key() -> Result<(), String> {
    KeyringManager::delete_byok_key().map_err(|e| e.to_string())
}

/// Validate API key format (convenience function for IPC)
pub fn validate_key_format(api_key: &str) -> bool {
    KeyringManager::validate_key_format(api_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_key_format_valid() {
        // Valid keys
        assert!(KeyringManager::validate_key_format("abc123"));
        assert!(KeyringManager::validate_key_format("API-KEY-123"));
        assert!(KeyringManager::validate_key_format("api_key_test"));
        assert!(KeyringManager::validate_key_format("a")); // Minimum length
        assert!(KeyringManager::validate_key_format(&"a".repeat(256))); // Maximum length
        assert!(KeyringManager::validate_key_format("AIzaSyB-test_KEY123"));
    }

    #[test]
    fn test_validate_key_format_invalid() {
        // Invalid keys
        assert!(!KeyringManager::validate_key_format("")); // Empty
        assert!(!KeyringManager::validate_key_format(&"a".repeat(257))); // Too long
        assert!(!KeyringManager::validate_key_format("key with spaces"));
        assert!(!KeyringManager::validate_key_format("key@special!chars"));
        assert!(!KeyringManager::validate_key_format("key.with.dots"));
    }

    // Note: Integration tests for actual keyring operations would require
    // a clean test environment and should be run separately to avoid
    // affecting real stored credentials.
}

// ============================================================================
// Property-Based Tests using proptest
// ============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// **Property 5: BYOK API Key Validation**
    ///
    /// Validates: Requirements 8.1, 8.2
    ///
    /// This property test verifies the API key validation logic:
    /// - Valid keys (1-256 alphanumeric chars, hyphens, underscores) → true
    /// - Invalid keys (empty, >256 chars, invalid characters) → false
    ///
    /// The property is defined as:
    /// ```
    /// ∀ key: String.
    ///   (1 ≤ len(key) ≤ 256 ∧ ∀c ∈ key. isAlphanumeric(c) ∨ c = '-' ∨ c = '_')
    ///   ⟺ validate_key_format(key) = true
    /// ```

    // Strategy to generate valid API keys: 1-256 alphanumeric chars with - and _
    fn valid_api_key_strategy() -> impl Strategy<Value = String> {
        // Define valid characters: alphanumeric + hyphen + underscore
        let valid_chars = prop::sample::select(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
                .chars()
                .collect::<Vec<_>>(),
        );
        
        // Generate strings of length 1-256 using valid characters
        prop::collection::vec(valid_chars, 1..=256)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    // Strategy to generate invalid API keys: empty strings
    fn empty_key_strategy() -> impl Strategy<Value = String> {
        Just(String::new())
    }

    // Strategy to generate invalid API keys: too long (>256 chars)
    fn too_long_key_strategy() -> impl Strategy<Value = String> {
        // Generate strings of length 257-512 with valid characters
        let valid_chars = prop::sample::select(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
                .chars()
                .collect::<Vec<_>>(),
        );
        
        prop::collection::vec(valid_chars, 257..=512)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    // Strategy to generate invalid API keys: containing invalid characters
    fn invalid_chars_key_strategy() -> impl Strategy<Value = String> {
        // Generate strings that contain at least one invalid character
        // Invalid chars include: spaces, punctuation (except - and _), special chars
        let invalid_chars = prop::sample::select(
            " !@#$%^&*()+=[]{}|\\:;\"'<>,.?/`~"
                .chars()
                .collect::<Vec<_>>(),
        );
        
        // Generate a mix of valid and invalid characters ensuring at least one invalid
        (
            // Valid prefix (0-100 chars)
            prop::collection::vec(
                prop::sample::select("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".chars().collect::<Vec<_>>()),
                0..=100
            ),
            // At least one invalid char
            invalid_chars,
            // Valid suffix (0-100 chars)
            prop::collection::vec(
                prop::sample::select("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".chars().collect::<Vec<_>>()),
                0..=100
            ),
        )
            .prop_map(|(prefix, invalid_char, suffix)| {
                let mut result: String = prefix.into_iter().collect();
                result.push(invalid_char);
                result.extend(suffix);
                result
            })
            // Filter out keys that are too long (>256) to focus on the invalid char property
            .prop_filter("key should not be too long", |k| k.len() <= 256)
    }

    proptest! {
        /// Property: Valid API keys (1-256 alphanumeric/hyphen/underscore chars) return true
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_valid_keys_accepted(key in valid_api_key_strategy()) {
            // Pre-condition: key is 1-256 chars and contains only valid chars
            prop_assert!(key.len() >= 1 && key.len() <= 256);
            prop_assert!(key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
            
            // Property: validate_key_format should return true
            prop_assert!(
                KeyringManager::validate_key_format(&key),
                "Valid key '{}' (len={}) should be accepted",
                if key.len() > 50 { format!("{}...", &key[..50]) } else { key.clone() },
                key.len()
            );
        }

        /// Property: Empty keys return false
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_empty_keys_rejected(key in empty_key_strategy()) {
            // Property: empty keys should be rejected
            prop_assert!(
                !KeyringManager::validate_key_format(&key),
                "Empty key should be rejected"
            );
        }

        /// Property: Keys longer than 256 chars return false
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_too_long_keys_rejected(key in too_long_key_strategy()) {
            // Pre-condition: key is > 256 chars
            prop_assert!(key.len() > 256);
            
            // Property: too long keys should be rejected
            prop_assert!(
                !KeyringManager::validate_key_format(&key),
                "Key with length {} should be rejected (max 256)",
                key.len()
            );
        }

        /// Property: Keys with invalid characters return false
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_invalid_chars_rejected(key in invalid_chars_key_strategy()) {
            // Pre-condition: key contains at least one invalid character
            let has_invalid = key.chars().any(|c| !c.is_alphanumeric() && c != '-' && c != '_');
            prop_assert!(has_invalid, "Test key should contain invalid chars");
            
            // Property: keys with invalid chars should be rejected
            prop_assert!(
                !KeyringManager::validate_key_format(&key),
                "Key '{}' with invalid characters should be rejected",
                if key.len() > 50 { format!("{}...", &key[..50]) } else { key.clone() }
            );
        }

        /// Property: Validation is consistent (idempotent)
        /// Calling validate twice should give the same result
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_validation_idempotent(key in ".*") {
            let result1 = KeyringManager::validate_key_format(&key);
            let result2 = KeyringManager::validate_key_format(&key);
            
            prop_assert_eq!(
                result1, result2,
                "Validation should be idempotent for key '{}'",
                if key.len() > 50 { format!("{}...", &key[..50]) } else { key.clone() }
            );
        }

        /// Property: Boundary test - exactly 256 chars should be valid
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_boundary_256_valid(suffix in "[a-zA-Z0-9_-]{1,256}") {
            // Ensure exactly 256 characters by padding or truncating
            let key: String = if suffix.len() >= 256 {
                suffix.chars().take(256).collect()
            } else {
                let padding = "a".repeat(256 - suffix.len());
                format!("{}{}", suffix, padding)
            };
            
            prop_assert_eq!(key.len(), 256);
            prop_assert!(
                KeyringManager::validate_key_format(&key),
                "Key with exactly 256 valid chars should be accepted"
            );
        }

        /// Property: Boundary test - exactly 257 chars should be invalid
        ///
        /// **Validates: Requirements 8.1, 8.2**
        #[test]
        fn prop_boundary_257_invalid(suffix in "[a-zA-Z0-9_-]{1,257}") {
            // Ensure exactly 257 characters by padding or truncating
            let key: String = if suffix.len() >= 257 {
                suffix.chars().take(257).collect()
            } else {
                let padding = "a".repeat(257 - suffix.len());
                format!("{}{}", suffix, padding)
            };
            
            prop_assert_eq!(key.len(), 257);
            prop_assert!(
                !KeyringManager::validate_key_format(&key),
                "Key with exactly 257 chars should be rejected"
            );
        }
    }

    // Additional edge case tests as unit tests within proptest module
    #[test]
    fn test_single_char_keys() {
        // Single alphanumeric characters should be valid
        assert!(KeyringManager::validate_key_format("a"));
        assert!(KeyringManager::validate_key_format("Z"));
        assert!(KeyringManager::validate_key_format("0"));
        assert!(KeyringManager::validate_key_format("9"));
        assert!(KeyringManager::validate_key_format("-"));
        assert!(KeyringManager::validate_key_format("_"));
    }

    #[test]
    fn test_unicode_and_special_chars() {
        // Unicode and special characters should be invalid
        assert!(!KeyringManager::validate_key_format("key🔑"));
        assert!(!KeyringManager::validate_key_format("klúč"));
        assert!(!KeyringManager::validate_key_format("キー"));
        assert!(!KeyringManager::validate_key_format("\n"));
        assert!(!KeyringManager::validate_key_format("\t"));
        assert!(!KeyringManager::validate_key_format("\0"));
    }

    #[test]
    fn test_real_gemini_key_format() {
        // Real Gemini API keys follow a specific format (39 chars, starts with AIza)
        assert!(KeyringManager::validate_key_format("AIzaSyB1234567890abcdefghijklmnopqrstuv"));
        assert!(KeyringManager::validate_key_format("AIzaSyB-test_KEY_123"));
    }
}
