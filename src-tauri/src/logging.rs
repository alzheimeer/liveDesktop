//! Logging module with sensitive data sanitization
//!
//! Provides log sanitization to ensure API keys and other sensitive data
//! are never included in logs, error messages, or Sentry reports.
//!
//! # Requirements
//! - Requirement 20.2: Rotative logs (max 10MB, 5 files)
//! - Requirement 23.3: NEVER include API keys in logs, error messages, or Sentry reports
//!
//! # Sanitization Rules
//! - API keys are replaced with [REDACTED]
//! - No substring of 4+ characters from the original key should appear in logs
//! - Supports various key formats: alphanumeric, with special chars, etc.

use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Placeholder used to replace sensitive data in logs
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Minimum length of a substring to be considered a potential leak
pub const MIN_SUBSTRING_LENGTH: usize = 4;

/// Common patterns that look like API keys
static API_KEY_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

/// Get compiled regex patterns for API key detection
fn get_api_key_patterns() -> &'static Vec<Regex> {
    API_KEY_PATTERNS.get_or_init(|| {
        vec![
            // Google API keys (AIza...)
            Regex::new(r"AIza[0-9A-Za-z\-_]{35}").unwrap(),
            // Generic API key patterns (sk-..., pk-..., etc.)
            Regex::new(r"(?:sk|pk|api|key)[-_][0-9A-Za-z\-_]{20,}").unwrap(),
            // Bearer tokens
            Regex::new(r"Bearer\s+[0-9A-Za-z\-_\.]+").unwrap(),
            // Long alphanumeric strings that could be keys (32+ chars)
            Regex::new(r"\b[0-9A-Za-z\-_]{32,}\b").unwrap(),
            // Base64-encoded data that might be keys
            Regex::new(r"[A-Za-z0-9+/]{40,}={0,2}").unwrap(),
        ]
    })
}

/// Sanitizes a log message by replacing known API keys with [REDACTED].
///
/// This function detects common API key patterns and replaces them with
/// a safe placeholder to prevent credential leakage in logs.
///
/// # Arguments
/// * `message` - The log message to sanitize
///
/// # Returns
/// The sanitized message with API keys replaced by [REDACTED]
///
/// # Requirements
/// - Requirement 23.3: NEVER include API keys in logs
pub fn sanitize_log_message(message: &str) -> String {
    let mut result = message.to_string();
    
    for pattern in get_api_key_patterns() {
        result = pattern.replace_all(&result, REDACTED_PLACEHOLDER).to_string();
    }
    
    result
}

/// Sanitizes a log message by removing any occurrence of a known API key.
///
/// This function takes a list of known API keys and ensures none of them
/// (or any substring of 4+ characters) appear in the log message.
///
/// # Arguments
/// * `message` - The log message to sanitize
/// * `known_keys` - Slice of known API keys to look for and redact
///
/// # Returns
/// The sanitized message with all API keys and their substrings removed
///
/// # Requirements
/// - Requirement 23.3: NEVER include API keys in logs
/// - Property 10: API keys never appear in logs (no substring of 4+ chars)
pub fn sanitize_with_known_keys(message: &str, known_keys: &[&str]) -> String {
    let mut result = message.to_string();
    
    for key in known_keys {
        if key.len() < MIN_SUBSTRING_LENGTH {
            // For very short keys, just replace the exact match
            result = result.replace(*key, REDACTED_PLACEHOLDER);
            continue;
        }
        
        // Replace the full key first
        result = result.replace(*key, REDACTED_PLACEHOLDER);
        
        // Generate all substrings of length >= MIN_SUBSTRING_LENGTH
        // and replace them as well to prevent partial leaks
        let substrings = generate_substrings(key, MIN_SUBSTRING_LENGTH);
        
        // Sort by length descending to replace longer matches first
        let mut sorted_substrings: Vec<_> = substrings.into_iter().collect();
        sorted_substrings.sort_by(|a, b| b.len().cmp(&a.len()));
        
        for substring in sorted_substrings {
            result = result.replace(&substring, REDACTED_PLACEHOLDER);
        }
    }
    
    result
}

/// Generates all unique substrings of a string with minimum length.
///
/// # Arguments
/// * `s` - The string to generate substrings from
/// * `min_len` - Minimum length of substrings to generate
///
/// # Returns
/// A HashSet of all unique substrings with length >= min_len
fn generate_substrings(s: &str, min_len: usize) -> HashSet<String> {
    let mut substrings = HashSet::new();
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    
    if n < min_len {
        return substrings;
    }
    
    // Generate all substrings of length >= min_len
    for len in min_len..=n {
        for start in 0..=(n - len) {
            let substring: String = chars[start..start + len].iter().collect();
            substrings.insert(substring);
        }
    }
    
    substrings
}

/// Checks if a message contains any substring (4+ chars) from a given key.
///
/// This is used to verify that sanitization was successful and no part
/// of the API key leaks into logs.
///
/// # Arguments
/// * `message` - The message to check
/// * `key` - The API key to check for
///
/// # Returns
/// `true` if the message contains any substring of 4+ chars from the key
///
/// # Requirements
/// - Property 10: API keys never appear in logs (no substring of 4+ chars)
pub fn contains_key_substring(message: &str, key: &str) -> bool {
    if key.len() < MIN_SUBSTRING_LENGTH {
        // For very short keys, check exact match only
        return message.contains(key);
    }
    
    let substrings = generate_substrings(key, MIN_SUBSTRING_LENGTH);
    
    for substring in &substrings {
        if message.contains(substring) {
            return true;
        }
    }
    
    false
}

/// A log sanitizer that maintains a list of known API keys to redact.
///
/// This can be used throughout the application to ensure consistent
/// sanitization of all log messages.
#[derive(Default)]
pub struct LogSanitizer {
    known_keys: Vec<String>,
}

impl LogSanitizer {
    /// Creates a new empty LogSanitizer
    pub fn new() -> Self {
        Self {
            known_keys: Vec::new(),
        }
    }
    
    /// Registers an API key to be redacted from all future log messages.
    ///
    /// # Arguments
    /// * `key` - The API key to register
    pub fn register_key(&mut self, key: impl Into<String>) {
        self.known_keys.push(key.into());
    }
    
    /// Removes a registered API key.
    ///
    /// # Arguments
    /// * `key` - The API key to remove
    pub fn unregister_key(&mut self, key: &str) {
        self.known_keys.retain(|k| k != key);
    }
    
    /// Clears all registered API keys.
    pub fn clear_keys(&mut self) {
        self.known_keys.clear();
    }
    
    /// Sanitizes a message using all registered keys plus pattern matching.
    ///
    /// # Arguments
    /// * `message` - The message to sanitize
    ///
    /// # Returns
    /// The sanitized message
    pub fn sanitize(&self, message: &str) -> String {
        // First apply pattern-based sanitization
        let mut result = sanitize_log_message(message);
        
        // Then sanitize with known keys
        let key_refs: Vec<&str> = self.known_keys.iter().map(|s| s.as_str()).collect();
        result = sanitize_with_known_keys(&result, &key_refs);
        
        result
    }
    
    /// Checks if a message would leak any registered API keys.
    ///
    /// # Arguments
    /// * `message` - The message to check
    ///
    /// # Returns
    /// `true` if any registered key (or substring) appears in the message
    pub fn would_leak(&self, message: &str) -> bool {
        for key in &self.known_keys {
            if contains_key_substring(message, key) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ==================== Unit Tests ====================

    #[test]
    fn test_sanitize_google_api_key() {
        let message = "Using API key: AIzaSyDaGmWKa4JsXZ-HjGw7ISLn_3namBGewQe";
        let sanitized = sanitize_log_message(message);
        assert!(!sanitized.contains("AIzaSyDaGmWKa4JsXZ-HjGw7ISLn_3namBGewQe"));
        assert!(sanitized.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_sanitize_bearer_token() {
        let message = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test";
        let sanitized = sanitize_log_message(message);
        assert!(sanitized.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_sanitize_with_known_key() {
        let key = "my-secret-api-key-12345";
        let message = format!("Connecting with key: {}", key);
        let sanitized = sanitize_with_known_keys(&message, &[key]);
        
        assert!(!sanitized.contains(key));
        assert!(sanitized.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_sanitize_removes_key_substrings() {
        let key = "ABCDEFGHIJKLMNOP";
        let message = "Found fragment: DEFGHIJK in the log";
        let sanitized = sanitize_with_known_keys(&message, &[key]);
        
        // Should not contain the substring DEFGHIJK
        assert!(!sanitized.contains("DEFGHIJK"));
    }

    #[test]
    fn test_contains_key_substring() {
        let key = "mysupersecretkey123";
        
        // Should detect the full key
        assert!(contains_key_substring("Using key: mysupersecretkey123", key));
        
        // Should detect substrings of 4+ chars
        assert!(contains_key_substring("Found: super", key));
        assert!(contains_key_substring("The secret is...", key));
        assert!(contains_key_substring("key123 found", key));
        
        // Should not detect substrings of 3 chars or less
        assert!(!contains_key_substring("my stuff", key));
        assert!(!contains_key_substring("The key is 123", key));
    }

    #[test]
    fn test_log_sanitizer() {
        let mut sanitizer = LogSanitizer::new();
        sanitizer.register_key("test-api-key-abc123");
        
        let message = "Authenticating with test-api-key-abc123";
        let sanitized = sanitizer.sanitize(message);
        
        assert!(!sanitized.contains("test-api-key-abc123"));
        assert!(!sanitizer.would_leak(&sanitized));
    }

    #[test]
    fn test_generate_substrings() {
        let substrings = generate_substrings("ABCDEF", 4);
        
        // Should contain substrings of length 4, 5, and 6
        assert!(substrings.contains("ABCD"));
        assert!(substrings.contains("BCDE"));
        assert!(substrings.contains("CDEF"));
        assert!(substrings.contains("ABCDE"));
        assert!(substrings.contains("BCDEF"));
        assert!(substrings.contains("ABCDEF"));
        
        // Should not contain shorter substrings
        assert!(!substrings.contains("ABC"));
        assert!(!substrings.contains("AB"));
    }

    #[test]
    fn test_short_key_handling() {
        let key = "abc"; // Less than MIN_SUBSTRING_LENGTH
        let message = "Key is abc here";
        let sanitized = sanitize_with_known_keys(&message, &[key]);
        
        assert!(!sanitized.contains("abc"));
        assert!(sanitized.contains(REDACTED_PLACEHOLDER));
    }

    // ==================== Property-Based Tests ====================

    /// Strategy to generate valid API key strings
    fn api_key_strategy() -> impl Strategy<Value = String> {
        // Generate alphanumeric strings of length 8-64
        proptest::string::string_regex("[a-zA-Z0-9_-]{8,64}")
            .unwrap()
    }

    /// Strategy to generate log messages that might contain API keys
    fn log_message_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                Just("Connecting to API with key: ".to_string()),
                Just("Authentication token: ".to_string()),
                Just("Using credentials: ".to_string()),
                Just("API key = ".to_string()),
                Just("Bearer ".to_string()),
                Just("Authorization: ".to_string()),
                Just("Error with key ".to_string()),
                Just("Successfully authenticated with ".to_string()),
            ],
            1..3
        ).prop_map(|v| v.join(""))
    }

    proptest! {
        /// **Property 10: API Keys Never Appear in Logs**
        ///
        /// Verifies that after sanitization, no API key or substring of 4+ characters
        /// from the original key appears in the sanitized output.
        ///
        /// **Validates: Requirements 23.3**
        #[test]
        fn prop_api_keys_never_appear_in_logs(
            api_key in api_key_strategy(),
            prefix in log_message_strategy(),
            suffix in proptest::string::string_regex("[a-zA-Z0-9 ,.!?]{0,50}").unwrap(),
        ) {
            // Create a log message that includes the API key
            let log_message = format!("{}{}{}", prefix, api_key, suffix);
            
            // Sanitize the message
            let sanitized = sanitize_with_known_keys(&log_message, &[&api_key]);
            
            // Property 1: The full API key should never appear in sanitized output
            prop_assert!(
                !sanitized.contains(&api_key),
                "Full API key found in sanitized output! Key: {}, Output: {}",
                api_key, sanitized
            );
            
            // Property 2: No substring of 4+ characters from the key should appear
            // (only if key is long enough)
            if api_key.len() >= MIN_SUBSTRING_LENGTH {
                prop_assert!(
                    !contains_key_substring(&sanitized, &api_key),
                    "API key substring (4+ chars) found in sanitized output! Key: {}, Output: {}",
                    api_key, sanitized
                );
            }
            
            // Property 3: The sanitized output should contain the redacted placeholder
            // (since we know the message contained the key)
            prop_assert!(
                sanitized.contains(REDACTED_PLACEHOLDER),
                "Sanitized output should contain [REDACTED] placeholder. Output: {}",
                sanitized
            );
        }

        /// Property: Sanitizer is idempotent - sanitizing twice gives same result
        #[test]
        fn prop_sanitization_is_idempotent(
            api_key in api_key_strategy(),
            message in log_message_strategy(),
        ) {
            let full_message = format!("{}{}", message, api_key);
            
            let once = sanitize_with_known_keys(&full_message, &[&api_key]);
            let twice = sanitize_with_known_keys(&once, &[&api_key]);
            
            prop_assert_eq!(once, twice, "Sanitization should be idempotent");
        }

        /// Property: Sanitization preserves non-sensitive parts
        #[test]
        fn prop_sanitization_preserves_safe_content(
            api_key in api_key_strategy(),
            safe_prefix in proptest::string::string_regex("[A-Z]{0,10}").unwrap(),
            safe_suffix in proptest::string::string_regex("[0-9]{0,10}").unwrap(),
        ) {
            // Use prefixes/suffixes that definitely won't be substrings of the key
            // by using patterns that don't overlap with the key charset
            let safe_pre = format!("<<<{}>>>", safe_prefix);
            let safe_suf = format!("[[{}]]", safe_suffix);
            
            let message = format!("{} API_KEY:{} {}", safe_pre, api_key, safe_suf);
            let sanitized = sanitize_with_known_keys(&message, &[&api_key]);
            
            // Safe markers should be preserved
            prop_assert!(
                sanitized.contains(&safe_pre),
                "Safe prefix should be preserved. Looking for '{}' in '{}'",
                safe_pre, sanitized
            );
            prop_assert!(
                sanitized.contains(&safe_suf),
                "Safe suffix should be preserved. Looking for '{}' in '{}'",
                safe_suf, sanitized
            );
        }

        /// Property: Keys with special characters are sanitized correctly
        #[test]
        fn prop_special_char_keys_sanitized(
            base in proptest::string::string_regex("[a-zA-Z0-9]{8,20}").unwrap(),
            separator in prop_oneof![Just("-"), Just("_"), Just(".")],
        ) {
            let api_key = format!("sk{}{}{}{}", separator, base, separator, "key");
            let message = format!("Token: {}", api_key);
            
            let sanitized = sanitize_with_known_keys(&message, &[&api_key]);
            
            // Full key should not appear
            prop_assert!(
                !sanitized.contains(&api_key),
                "API key with special chars found in output. Key: {}, Output: {}",
                api_key, sanitized
            );
        }
    }
}
