//! Configuration persistence module
//! 
//! Handles storing and retrieving application configuration including:
//! - Language settings for translation channels
//! - Audio device selections
//! - UI preferences (theme, auto-start, etc.)
//! 
//! Supports automatic migrations between config schema versions.
//! Supports export/import configuration as JSON for backup and device migration.
//! 
//! Requirements: 25.1, 25.2, 25.3, 25.4, 25.5

use serde::{Deserialize, Serialize};
use super::database::{Database, DatabaseError};

/// Current configuration schema version for migrations
pub const CURRENT_CONFIG_VERSION: i32 = 1;

/// Configuration storage key in the database
const CONFIG_KEY: &str = "app_config";
const CONFIG_VERSION_KEY: &str = "config_version";

/// Error types for configuration operations
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("File I/O error: {0}")]
    FileIo(#[from] std::io::Error),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Main application configuration
/// 
/// Contains all user-configurable settings that should persist between sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub languages: LanguageConfig,
    pub devices: DeviceConfig,
    pub preferences: PreferencesConfig,
}

/// Language configuration for translation channels
/// 
/// Each channel (system and user) has independent source/target language settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageConfig {
    /// Source language for system audio channel (e.g., "en" for English)
    pub system_source_lang: String,
    /// Target language for system audio channel (what the user hears)
    pub system_target_lang: String,
    /// Source language for user microphone channel
    pub user_source_lang: String,
    /// Target language for user microphone channel (what meeting participants hear)
    pub user_target_lang: String,
}

/// Audio device configuration
/// 
/// Stores device IDs for audio input, output, and system capture devices.
/// Device IDs are platform-specific strings that may change between sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceConfig {
    /// User's microphone device ID
    pub input_device: Option<String>,
    /// System audio capture device ID (for WASAPI loopback / ScreenCaptureKit)
    pub system_capture_device: Option<String>,
    /// Audio output device ID for translated audio playback
    pub output_device: Option<String>,
}

/// UI and behavior preferences
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreferencesConfig {
    /// Start the app minimized to system tray
    pub start_minimized: bool,
    /// Auto-start app when the operating system boots
    pub auto_start: bool,
    /// UI theme: "dark", "light", or "system"
    pub theme: String,
    /// Enable Sentry error reporting (requires user consent)
    pub enable_sentry: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            languages: LanguageConfig::default(),
            devices: DeviceConfig::default(),
            preferences: PreferencesConfig::default(),
        }
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            system_source_lang: "en".to_string(),
            system_target_lang: "es".to_string(),
            user_source_lang: "es".to_string(),
            user_target_lang: "en".to_string(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            system_capture_device: None,
            output_device: None,
        }
    }
}

impl Default for PreferencesConfig {
    fn default() -> Self {
        Self {
            start_minimized: false,
            auto_start: false,
            theme: "dark".to_string(),
            enable_sentry: false,
        }
    }
}

// ============================================================================
// Configuration Persistence Functions
// ============================================================================

/// Save application configuration to the database
/// 
/// Serializes the configuration to JSON and stores it in the config table.
/// Also stores the current config schema version for future migrations.
/// 
/// # Arguments
/// * `db` - Database connection
/// * `config` - Configuration to save
/// 
/// # Returns
/// * `Ok(())` on success
/// * `Err(ConfigError)` on failure
/// 
/// # Example
/// ```no_run
/// # use traductor_desktop_lib::storage::config::{save_config, AppConfig};
/// # use traductor_desktop_lib::storage::database::Database;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let db = Database::new("test.db", "key")?;
/// let config = AppConfig::default();
/// save_config(&db, &config)?;
/// # Ok(())
/// # }
/// ```
pub fn save_config(db: &Database, config: &AppConfig) -> Result<(), ConfigError> {
    // Serialize config to JSON
    let config_json = serde_json::to_string(config)?;
    
    // Save config and version in a transaction
    db.save_config(CONFIG_KEY, &config_json)?;
    db.save_config(CONFIG_VERSION_KEY, &CURRENT_CONFIG_VERSION.to_string())?;
    
    tracing::debug!("Configuration saved successfully");
    Ok(())
}

/// Load application configuration from the database
/// 
/// Retrieves the stored configuration, running any necessary migrations.
/// If no configuration exists or it's invalid, returns default configuration.
/// 
/// # Arguments
/// * `db` - Database connection
/// 
/// # Returns
/// * `Ok(AppConfig)` - The loaded (or default) configuration
/// * `Err(ConfigError)` - Only on critical database errors
/// 
/// # Example
/// ```no_run
/// # use traductor_desktop_lib::storage::config::load_config;
/// # use traductor_desktop_lib::storage::database::Database;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let db = Database::new("test.db", "key")?;
/// let config = load_config(&db)?;
/// println!("Theme: {}", config.preferences.theme);
/// # Ok(())
/// # }
/// ```
pub fn load_config(db: &Database) -> Result<AppConfig, ConfigError> {
    // Try to load existing config
    let config_json = match db.get_config(CONFIG_KEY)? {
        Some(json) => json,
        None => {
            tracing::info!("No configuration found, using defaults");
            return Ok(AppConfig::default());
        }
    };
    
    // Get stored version
    let stored_version = match db.get_config(CONFIG_VERSION_KEY)? {
        Some(v) => v.parse::<i32>().unwrap_or(0),
        None => 0,
    };
    
    // Run migrations if needed, falling back to defaults on migration error
    let config_json = if stored_version < CURRENT_CONFIG_VERSION {
        tracing::info!("Migrating config from version {} to {}", stored_version, CURRENT_CONFIG_VERSION);
        match migrate_config(&config_json, stored_version) {
            Ok(migrated) => migrated,
            Err(e) => {
                tracing::warn!("Config migration failed ({}), using defaults", e);
                return Ok(AppConfig::default());
            }
        }
    } else {
        config_json
    };
    
    // Deserialize config, falling back to defaults on error
    match serde_json::from_str::<AppConfig>(&config_json) {
        Ok(config) => {
            tracing::debug!("Configuration loaded successfully");
            Ok(config)
        }
        Err(e) => {
            tracing::warn!("Failed to parse configuration ({}), using defaults", e);
            Ok(AppConfig::default())
        }
    }
}

/// Load configuration with automatic restore on app startup
/// 
/// This is the main entry point for loading configuration when the app starts.
/// It ensures the database has the latest schema and loads/creates configuration.
/// 
/// # Arguments
/// * `db` - Database connection
/// 
/// # Returns
/// * `Ok(AppConfig)` - The loaded (or default) configuration
pub fn restore_config_on_startup(db: &Database) -> Result<AppConfig, ConfigError> {
    // Load existing config or create default
    let config = load_config(db)?;
    
    // Save to ensure migrations are persisted
    save_config(db, &config)?;
    
    Ok(config)
}

// ============================================================================
// Configuration Migrations
// ============================================================================

/// Migrate configuration from an older schema version to the current version
/// 
/// Migrations are applied sequentially, transforming the JSON at each step.
/// This allows smooth upgrades between any two versions.
/// 
/// # Arguments
/// * `json` - The configuration JSON string
/// * `from_version` - The version the config was stored as
/// 
/// # Returns
/// * `Ok(String)` - The migrated JSON string
/// * `Err(ConfigError)` - If migration fails
fn migrate_config(json: &str, from_version: i32) -> Result<String, ConfigError> {
    let mut current_json = json.to_string();
    let mut version = from_version;
    
    // Apply migrations sequentially
    while version < CURRENT_CONFIG_VERSION {
        current_json = match version {
            0 => migrate_v0_to_v1(&current_json)?,
            // Add future migrations here:
            // 1 => migrate_v1_to_v2(&current_json)?,
            // 2 => migrate_v2_to_v3(&current_json)?,
            _ => {
                // Unknown version - try to use as-is
                tracing::warn!("Unknown config version {}, attempting to use as-is", version);
                break;
            }
        };
        version += 1;
    }
    
    Ok(current_json)
}

/// Migrate from version 0 (no version) to version 1
/// 
/// Version 0 represents legacy configs that didn't have version tracking.
/// This migration ensures all required fields exist with sensible defaults.
fn migrate_v0_to_v1(json: &str) -> Result<String, ConfigError> {
    // Parse as generic JSON value to preserve any existing fields
    let mut value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ConfigError::Migration(format!("Failed to parse v0 config: {}", e)))?;
    
    // Ensure top-level structure exists
    let obj = value.as_object_mut()
        .ok_or_else(|| ConfigError::Migration("Config is not an object".to_string()))?;
    
    // Ensure languages section exists with defaults
    if !obj.contains_key("languages") {
        obj.insert("languages".to_string(), serde_json::json!({
            "system_source_lang": "en",
            "system_target_lang": "es",
            "user_source_lang": "es",
            "user_target_lang": "en"
        }));
    } else if let Some(langs) = obj.get_mut("languages").and_then(|v| v.as_object_mut()) {
        // Ensure all language fields exist
        if !langs.contains_key("system_source_lang") {
            langs.insert("system_source_lang".to_string(), serde_json::json!("en"));
        }
        if !langs.contains_key("system_target_lang") {
            langs.insert("system_target_lang".to_string(), serde_json::json!("es"));
        }
        if !langs.contains_key("user_source_lang") {
            langs.insert("user_source_lang".to_string(), serde_json::json!("es"));
        }
        if !langs.contains_key("user_target_lang") {
            langs.insert("user_target_lang".to_string(), serde_json::json!("en"));
        }
    }
    
    // Ensure devices section exists with defaults
    if !obj.contains_key("devices") {
        obj.insert("devices".to_string(), serde_json::json!({
            "input_device": null,
            "system_capture_device": null,
            "output_device": null
        }));
    } else if let Some(devices) = obj.get_mut("devices").and_then(|v| v.as_object_mut()) {
        // Ensure all device fields exist
        if !devices.contains_key("input_device") {
            devices.insert("input_device".to_string(), serde_json::Value::Null);
        }
        if !devices.contains_key("system_capture_device") {
            devices.insert("system_capture_device".to_string(), serde_json::Value::Null);
        }
        if !devices.contains_key("output_device") {
            devices.insert("output_device".to_string(), serde_json::Value::Null);
        }
    }
    
    // Ensure preferences section exists with defaults
    if !obj.contains_key("preferences") {
        obj.insert("preferences".to_string(), serde_json::json!({
            "start_minimized": false,
            "auto_start": false,
            "theme": "dark",
            "enable_sentry": false
        }));
    } else if let Some(prefs) = obj.get_mut("preferences").and_then(|v| v.as_object_mut()) {
        // Ensure all preference fields exist
        if !prefs.contains_key("start_minimized") {
            prefs.insert("start_minimized".to_string(), serde_json::json!(false));
        }
        if !prefs.contains_key("auto_start") {
            prefs.insert("auto_start".to_string(), serde_json::json!(false));
        }
        if !prefs.contains_key("theme") {
            prefs.insert("theme".to_string(), serde_json::json!("dark"));
        }
        if !prefs.contains_key("enable_sentry") {
            prefs.insert("enable_sentry".to_string(), serde_json::json!(false));
        }
    }
    
    serde_json::to_string(&value)
        .map_err(|e| ConfigError::Migration(format!("Failed to serialize v1 config: {}", e)))
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Check if a configuration exists in the database
pub fn config_exists(db: &Database) -> Result<bool, ConfigError> {
    Ok(db.get_config(CONFIG_KEY)?.is_some())
}

/// Delete all configuration (useful for reset/testing)
pub fn delete_config(db: &Database) -> Result<(), ConfigError> {
    db.delete_config(CONFIG_KEY)?;
    db.delete_config(CONFIG_VERSION_KEY)?;
    tracing::info!("Configuration deleted");
    Ok(())
}

/// Get the stored configuration version
pub fn get_config_version(db: &Database) -> Result<i32, ConfigError> {
    match db.get_config(CONFIG_VERSION_KEY)? {
        Some(v) => Ok(v.parse::<i32>().unwrap_or(0)),
        None => Ok(0),
    }
}

// ============================================================================
// Export/Import Functions
// Requirements: 25.5
// ============================================================================

/// Export current configuration from database to a JSON file
/// 
/// Loads the configuration from the database and writes it to the specified path
/// as a formatted JSON file for backup or migration between devices.
/// 
/// # Arguments
/// * `db` - Database connection to load config from
/// * `path` - File path where JSON config will be written
/// 
/// # Returns
/// * `Ok(())` on success
/// * `Err(ConfigError)` if database read or file write fails
/// 
/// # Example
/// ```no_run
/// # use traductor_desktop_lib::storage::config::export_config_to_file;
/// # use traductor_desktop_lib::storage::database::Database;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let db = Database::new("test.db", "key")?;
/// export_config_to_file(&db, "backup/config.json")?;
/// # Ok(())
/// # }
/// ```
pub fn export_config_to_file(db: &Database, path: &str) -> Result<(), ConfigError> {
    // Load current config from database
    let config = load_config(db)?;
    
    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(&config)?;
    
    // Write to file
    std::fs::write(path, json)?;
    
    tracing::info!("Configuration exported to: {}", path);
    Ok(())
}

/// Import configuration from a JSON file and save to database
/// 
/// Reads a JSON configuration file, validates it, and saves it to the database.
/// The imported configuration replaces any existing configuration.
/// 
/// # Arguments
/// * `db` - Database connection to save config to
/// * `path` - File path to read JSON config from
/// 
/// # Returns
/// * `Ok(AppConfig)` - The imported configuration on success
/// * `Err(ConfigError::FileIo)` if file cannot be read
/// * `Err(ConfigError::Serialization)` if JSON is invalid
/// * `Err(ConfigError::Database)` if save fails
/// 
/// # Example
/// ```no_run
/// # use traductor_desktop_lib::storage::config::import_config_from_file;
/// # use traductor_desktop_lib::storage::database::Database;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let db = Database::new("test.db", "key")?;
/// let config = import_config_from_file(&db, "backup/config.json")?;
/// println!("Imported theme: {}", config.preferences.theme);
/// # Ok(())
/// # }
/// ```
pub fn import_config_from_file(db: &Database, path: &str) -> Result<AppConfig, ConfigError> {
    // Check if file exists
    if !std::path::Path::new(path).exists() {
        return Err(ConfigError::FileIo(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Configuration file not found: {}", path),
        )));
    }
    
    // Read file contents
    let json = std::fs::read_to_string(path)?;
    
    // Parse and validate configuration
    let config = import_config_from_string(&json)?;
    
    // Save to database
    save_config(db, &config)?;
    
    tracing::info!("Configuration imported from: {}", path);
    Ok(config)
}

/// Export configuration to a JSON string
/// 
/// Serializes the provided configuration to a JSON string.
/// Useful for clipboard operations or API responses.
/// 
/// # Arguments
/// * `config` - Configuration to serialize
/// 
/// # Returns
/// * `Ok(String)` - JSON string representation of the config
/// * `Err(ConfigError::Serialization)` if serialization fails
/// 
/// # Example
/// ```
/// # use traductor_desktop_lib::storage::config::{export_config_to_string, AppConfig};
/// let config = AppConfig::default();
/// let json = export_config_to_string(&config).unwrap();
/// assert!(json.contains("languages"));
/// ```
pub fn export_config_to_string(config: &AppConfig) -> Result<String, ConfigError> {
    let json = serde_json::to_string_pretty(config)?;
    Ok(json)
}

/// Import configuration from a JSON string
/// 
/// Parses a JSON string into an AppConfig struct.
/// Useful for clipboard paste operations or API requests.
/// 
/// # Arguments
/// * `json` - JSON string containing configuration data
/// 
/// # Returns
/// * `Ok(AppConfig)` - Parsed configuration
/// * `Err(ConfigError::Serialization)` if JSON is invalid or doesn't match schema
/// * `Err(ConfigError::InvalidConfig)` if config fails validation
/// 
/// # Example
/// ```
/// # use traductor_desktop_lib::storage::config::{import_config_from_string, AppConfig};
/// let json = r#"{
///     "languages": {"system_source_lang": "en", "system_target_lang": "es", "user_source_lang": "es", "user_target_lang": "en"},
///     "devices": {"input_device": null, "system_capture_device": null, "output_device": null},
///     "preferences": {"start_minimized": false, "auto_start": false, "theme": "dark", "enable_sentry": false}
/// }"#;
/// let config = import_config_from_string(json).unwrap();
/// assert_eq!(config.languages.system_source_lang, "en");
/// ```
pub fn import_config_from_string(json: &str) -> Result<AppConfig, ConfigError> {
    // Check for empty input
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidConfig("Empty JSON string".to_string()));
    }
    
    // Parse JSON into AppConfig
    let config: AppConfig = serde_json::from_str(trimmed).map_err(|e| {
        ConfigError::Serialization(e)
    })?;
    
    // Validate the configuration
    validate_config(&config)?;
    
    Ok(config)
}

/// Validate a configuration for correctness
/// 
/// Checks that the configuration values are valid and sensible.
/// 
/// # Arguments
/// * `config` - Configuration to validate
/// 
/// # Returns
/// * `Ok(())` if valid
/// * `Err(ConfigError::InvalidConfig)` if validation fails
fn validate_config(config: &AppConfig) -> Result<(), ConfigError> {
    // Validate language codes are not empty
    if config.languages.system_source_lang.trim().is_empty() {
        return Err(ConfigError::InvalidConfig(
            "system_source_lang cannot be empty".to_string(),
        ));
    }
    if config.languages.system_target_lang.trim().is_empty() {
        return Err(ConfigError::InvalidConfig(
            "system_target_lang cannot be empty".to_string(),
        ));
    }
    if config.languages.user_source_lang.trim().is_empty() {
        return Err(ConfigError::InvalidConfig(
            "user_source_lang cannot be empty".to_string(),
        ));
    }
    if config.languages.user_target_lang.trim().is_empty() {
        return Err(ConfigError::InvalidConfig(
            "user_target_lang cannot be empty".to_string(),
        ));
    }
    
    // Validate theme is a known value
    let valid_themes = ["dark", "light", "system"];
    if !valid_themes.contains(&config.preferences.theme.as_str()) {
        return Err(ConfigError::InvalidConfig(format!(
            "Invalid theme '{}'. Must be one of: {}",
            config.preferences.theme,
            valid_themes.join(", ")
        )));
    }
    
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_db() -> Database {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Database::new(db_path.to_str().unwrap(), "test_key").unwrap()
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        
        assert_eq!(config.languages.system_source_lang, "en");
        assert_eq!(config.languages.system_target_lang, "es");
        assert_eq!(config.languages.user_source_lang, "es");
        assert_eq!(config.languages.user_target_lang, "en");
        
        assert!(config.devices.input_device.is_none());
        assert!(config.devices.system_capture_device.is_none());
        assert!(config.devices.output_device.is_none());
        
        assert!(!config.preferences.start_minimized);
        assert!(!config.preferences.auto_start);
        assert_eq!(config.preferences.theme, "dark");
        assert!(!config.preferences.enable_sentry);
    }

    #[test]
    fn test_save_and_load_config() {
        let db = create_test_db();
        
        let config = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "fr".to_string(),
                system_target_lang: "de".to_string(),
                user_source_lang: "de".to_string(),
                user_target_lang: "fr".to_string(),
            },
            devices: DeviceConfig {
                input_device: Some("mic_123".to_string()),
                system_capture_device: Some("loopback_456".to_string()),
                output_device: Some("speaker_789".to_string()),
            },
            preferences: PreferencesConfig {
                start_minimized: true,
                auto_start: true,
                theme: "light".to_string(),
                enable_sentry: true,
            },
        };
        
        // Save config
        save_config(&db, &config).unwrap();
        
        // Load config
        let loaded = load_config(&db).unwrap();
        
        // Verify round-trip
        assert_eq!(config, loaded);
    }

    #[test]
    fn test_load_missing_config_returns_default() {
        let db = create_test_db();
        
        // Don't save anything - should return defaults
        let config = load_config(&db).unwrap();
        
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn test_load_invalid_config_returns_default() {
        let db = create_test_db();
        
        // Save invalid JSON
        db.save_config(CONFIG_KEY, "not valid json at all").unwrap();
        
        // Should return defaults instead of erroring
        let config = load_config(&db).unwrap();
        
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn test_config_exists() {
        let db = create_test_db();
        
        // Initially doesn't exist
        assert!(!config_exists(&db).unwrap());
        
        // After saving, it exists
        save_config(&db, &AppConfig::default()).unwrap();
        assert!(config_exists(&db).unwrap());
    }

    #[test]
    fn test_delete_config() {
        let db = create_test_db();
        
        // Save and verify exists
        save_config(&db, &AppConfig::default()).unwrap();
        assert!(config_exists(&db).unwrap());
        
        // Delete and verify gone
        delete_config(&db).unwrap();
        assert!(!config_exists(&db).unwrap());
    }

    #[test]
    fn test_config_version_tracking() {
        let db = create_test_db();
        
        // Initially no version
        assert_eq!(get_config_version(&db).unwrap(), 0);
        
        // After save, version is current
        save_config(&db, &AppConfig::default()).unwrap();
        assert_eq!(get_config_version(&db).unwrap(), CURRENT_CONFIG_VERSION);
    }

    #[test]
    fn test_migrate_v0_to_v1_empty_object() {
        let json = "{}";
        let migrated = migrate_v0_to_v1(json).unwrap();
        
        // Should be able to parse as valid AppConfig
        let config: AppConfig = serde_json::from_str(&migrated).unwrap();
        
        // Should have defaults
        assert_eq!(config.languages.system_source_lang, "en");
        assert_eq!(config.preferences.theme, "dark");
    }

    #[test]
    fn test_migrate_v0_to_v1_partial_config() {
        let json = r#"{
            "languages": {
                "system_source_lang": "pt"
            },
            "preferences": {
                "theme": "light"
            }
        }"#;
        
        let migrated = migrate_v0_to_v1(json).unwrap();
        let config: AppConfig = serde_json::from_str(&migrated).unwrap();
        
        // Preserved values should remain
        assert_eq!(config.languages.system_source_lang, "pt");
        assert_eq!(config.preferences.theme, "light");
        
        // Missing values should be filled with defaults
        assert_eq!(config.languages.system_target_lang, "es");
        assert!(!config.preferences.start_minimized);
    }

    #[test]
    fn test_restore_config_on_startup() {
        let db = create_test_db();
        
        // First startup - should create defaults
        let config1 = restore_config_on_startup(&db).unwrap();
        assert_eq!(config1, AppConfig::default());
        
        // Verify it was saved
        assert!(config_exists(&db).unwrap());
        
        // Modify and save
        let mut config2 = config1.clone();
        config2.preferences.theme = "light".to_string();
        save_config(&db, &config2).unwrap();
        
        // Second startup - should load saved config
        let config3 = restore_config_on_startup(&db).unwrap();
        assert_eq!(config3.preferences.theme, "light");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "ja".to_string(),
                system_target_lang: "ko".to_string(),
                user_source_lang: "zh".to_string(),
                user_target_lang: "ru".to_string(),
            },
            devices: DeviceConfig {
                input_device: Some("device with spaces".to_string()),
                system_capture_device: Some("device-with-dashes".to_string()),
                output_device: None,
            },
            preferences: PreferencesConfig {
                start_minimized: true,
                auto_start: false,
                theme: "system".to_string(),
                enable_sentry: true,
            },
        };
        
        // Serialize to JSON
        let json = serde_json::to_string(&config).unwrap();
        
        // Deserialize back
        let restored: AppConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(config, restored);
    }

    // ==================== Export/Import Tests ====================

    #[test]
    fn test_export_config_to_string() {
        let config = AppConfig::default();
        
        let json = export_config_to_string(&config).unwrap();
        
        // Should be valid JSON that can be parsed back
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
        
        // Should be pretty-printed (contains newlines)
        assert!(json.contains('\n'));
    }

    #[test]
    fn test_import_config_from_string_valid() {
        let json = r#"{
            "languages": {
                "system_source_lang": "en",
                "system_target_lang": "es",
                "user_source_lang": "es",
                "user_target_lang": "en"
            },
            "devices": {
                "input_device": null,
                "system_capture_device": null,
                "output_device": null
            },
            "preferences": {
                "start_minimized": false,
                "auto_start": false,
                "theme": "dark",
                "enable_sentry": false
            }
        }"#;
        
        let config = import_config_from_string(json).unwrap();
        
        assert_eq!(config.languages.system_source_lang, "en");
        assert_eq!(config.preferences.theme, "dark");
    }

    #[test]
    fn test_import_config_from_string_with_custom_values() {
        let json = r#"{
            "languages": {
                "system_source_lang": "fr",
                "system_target_lang": "de",
                "user_source_lang": "de",
                "user_target_lang": "fr"
            },
            "devices": {
                "input_device": "mic_abc",
                "system_capture_device": "loopback_xyz",
                "output_device": "speaker_123"
            },
            "preferences": {
                "start_minimized": true,
                "auto_start": true,
                "theme": "light",
                "enable_sentry": true
            }
        }"#;
        
        let config = import_config_from_string(json).unwrap();
        
        assert_eq!(config.languages.system_source_lang, "fr");
        assert_eq!(config.devices.input_device, Some("mic_abc".to_string()));
        assert!(config.preferences.start_minimized);
        assert_eq!(config.preferences.theme, "light");
    }

    #[test]
    fn test_import_config_from_string_invalid_json() {
        let json = "not valid json at all";
        
        let result = import_config_from_string(json);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Serialization(_)));
    }

    #[test]
    fn test_import_config_from_string_empty() {
        let json = "";
        
        let result = import_config_from_string(json);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidConfig(_)));
    }

    #[test]
    fn test_import_config_from_string_whitespace_only() {
        let json = "   \n\t  ";
        
        let result = import_config_from_string(json);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidConfig(_)));
    }

    #[test]
    fn test_import_config_from_string_invalid_theme() {
        let json = r#"{
            "languages": {
                "system_source_lang": "en",
                "system_target_lang": "es",
                "user_source_lang": "es",
                "user_target_lang": "en"
            },
            "devices": {
                "input_device": null,
                "system_capture_device": null,
                "output_device": null
            },
            "preferences": {
                "start_minimized": false,
                "auto_start": false,
                "theme": "invalid_theme",
                "enable_sentry": false
            }
        }"#;
        
        let result = import_config_from_string(json);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidConfig(msg) => {
                assert!(msg.contains("Invalid theme"));
            }
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[test]
    fn test_import_config_from_string_empty_language() {
        let json = r#"{
            "languages": {
                "system_source_lang": "",
                "system_target_lang": "es",
                "user_source_lang": "es",
                "user_target_lang": "en"
            },
            "devices": {
                "input_device": null,
                "system_capture_device": null,
                "output_device": null
            },
            "preferences": {
                "start_minimized": false,
                "auto_start": false,
                "theme": "dark",
                "enable_sentry": false
            }
        }"#;
        
        let result = import_config_from_string(json);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidConfig(msg) => {
                assert!(msg.contains("system_source_lang cannot be empty"));
            }
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[test]
    fn test_export_import_roundtrip() {
        let original = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "pt".to_string(),
                system_target_lang: "it".to_string(),
                user_source_lang: "it".to_string(),
                user_target_lang: "pt".to_string(),
            },
            devices: DeviceConfig {
                input_device: Some("test_mic".to_string()),
                system_capture_device: None,
                output_device: Some("test_speaker".to_string()),
            },
            preferences: PreferencesConfig {
                start_minimized: true,
                auto_start: false,
                theme: "system".to_string(),
                enable_sentry: true,
            },
        };
        
        // Export to string
        let json = export_config_to_string(&original).unwrap();
        
        // Import back
        let imported = import_config_from_string(&json).unwrap();
        
        // Should match
        assert_eq!(original, imported);
    }

    #[test]
    fn test_export_config_to_file() {
        let dir = tempdir().unwrap();
        let db = create_test_db();
        let file_path = dir.path().join("config_export.json");
        
        // Create custom config
        let config = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "zh".to_string(),
                system_target_lang: "ja".to_string(),
                user_source_lang: "ja".to_string(),
                user_target_lang: "zh".to_string(),
            },
            devices: DeviceConfig::default(),
            preferences: PreferencesConfig {
                theme: "light".to_string(),
                ..PreferencesConfig::default()
            },
        };
        
        // Save to database
        save_config(&db, &config).unwrap();
        
        // Export to file
        export_config_to_file(&db, file_path.to_str().unwrap()).unwrap();
        
        // Verify file exists and contains valid JSON
        assert!(file_path.exists());
        let content = std::fs::read_to_string(&file_path).unwrap();
        let parsed: AppConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_import_config_from_file() {
        let dir = tempdir().unwrap();
        let db = create_test_db();
        let file_path = dir.path().join("config_import.json");
        
        // Create a JSON file
        let config = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "ru".to_string(),
                system_target_lang: "uk".to_string(),
                user_source_lang: "uk".to_string(),
                user_target_lang: "ru".to_string(),
            },
            devices: DeviceConfig {
                input_device: Some("imported_mic".to_string()),
                system_capture_device: None,
                output_device: None,
            },
            preferences: PreferencesConfig::default(),
        };
        
        let json = serde_json::to_string_pretty(&config).unwrap();
        std::fs::write(&file_path, json).unwrap();
        
        // Import from file
        let imported = import_config_from_file(&db, file_path.to_str().unwrap()).unwrap();
        
        // Verify it matches
        assert_eq!(config, imported);
        
        // Verify it was saved to database
        let loaded = load_config(&db).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn test_import_config_from_file_not_found() {
        let db = create_test_db();
        
        let result = import_config_from_file(&db, "/nonexistent/path/config.json");
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::FileIo(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
            }
            _ => panic!("Expected FileIo error"),
        }
    }

    #[test]
    fn test_import_config_from_file_invalid_json() {
        let dir = tempdir().unwrap();
        let db = create_test_db();
        let file_path = dir.path().join("invalid.json");
        
        // Write invalid JSON
        std::fs::write(&file_path, "{ invalid json }").unwrap();
        
        let result = import_config_from_file(&db, file_path.to_str().unwrap());
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Serialization(_)));
    }

    #[test]
    fn test_export_import_file_roundtrip() {
        let dir = tempdir().unwrap();
        let db1 = create_test_db();
        let db2 = {
            let db_path = dir.path().join("test2.db");
            Database::new(db_path.to_str().unwrap(), "test_key").unwrap()
        };
        let file_path = dir.path().join("transfer.json");
        
        // Save config to first database
        let original = AppConfig {
            languages: LanguageConfig {
                system_source_lang: "ar".to_string(),
                system_target_lang: "he".to_string(),
                user_source_lang: "he".to_string(),
                user_target_lang: "ar".to_string(),
            },
            devices: DeviceConfig::default(),
            preferences: PreferencesConfig {
                start_minimized: true,
                auto_start: true,
                theme: "system".to_string(),
                enable_sentry: false,
            },
        };
        save_config(&db1, &original).unwrap();
        
        // Export from db1
        export_config_to_file(&db1, file_path.to_str().unwrap()).unwrap();
        
        // Import to db2
        let imported = import_config_from_file(&db2, file_path.to_str().unwrap()).unwrap();
        
        // Should match
        assert_eq!(original, imported);
        
        // Verify db2 has the config
        let loaded = load_config(&db2).unwrap();
        assert_eq!(original, loaded);
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    /// Create a test database in a temporary directory
    fn create_prop_test_db() -> Database {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("prop_test.db");
        Database::new(db_path.to_str().unwrap(), "prop_test_key").unwrap()
    }

    // ========================================================================
    // Strategy Generators for AppConfig
    // ========================================================================

    /// Generate valid ISO 639-1 language codes (2 lowercase letters)
    fn valid_lang_code() -> impl Strategy<Value = String> {
        // Generate 2 lowercase ASCII letters as language code
        "[a-z]{2}".prop_map(|s| s.to_string())
    }

    /// Generate valid theme values
    fn valid_theme() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("dark".to_string()),
            Just("light".to_string()),
            Just("system".to_string()),
        ]
    }

    /// Generate optional device IDs (alphanumeric with dashes, spaces, underscores)
    fn optional_device_id() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            // Device IDs can contain letters, numbers, dashes, underscores, spaces
            "[a-zA-Z0-9_\\- ]{1,64}".prop_map(|s| Some(s.to_string())),
        ]
    }

    /// Generate valid LanguageConfig
    fn valid_language_config() -> impl Strategy<Value = LanguageConfig> {
        (
            valid_lang_code(),
            valid_lang_code(),
            valid_lang_code(),
            valid_lang_code(),
        )
            .prop_map(|(sys_src, sys_tgt, usr_src, usr_tgt)| LanguageConfig {
                system_source_lang: sys_src,
                system_target_lang: sys_tgt,
                user_source_lang: usr_src,
                user_target_lang: usr_tgt,
            })
    }

    /// Generate valid DeviceConfig
    fn valid_device_config() -> impl Strategy<Value = DeviceConfig> {
        (
            optional_device_id(),
            optional_device_id(),
            optional_device_id(),
        )
            .prop_map(|(input, capture, output)| DeviceConfig {
                input_device: input,
                system_capture_device: capture,
                output_device: output,
            })
    }

    /// Generate valid PreferencesConfig
    fn valid_preferences_config() -> impl Strategy<Value = PreferencesConfig> {
        (any::<bool>(), any::<bool>(), valid_theme(), any::<bool>()).prop_map(
            |(start_min, auto_start, theme, sentry)| PreferencesConfig {
                start_minimized: start_min,
                auto_start,
                theme,
                enable_sentry: sentry,
            },
        )
    }

    /// Generate valid AppConfig
    fn valid_app_config() -> impl Strategy<Value = AppConfig> {
        (
            valid_language_config(),
            valid_device_config(),
            valid_preferences_config(),
        )
            .prop_map(|(languages, devices, preferences)| AppConfig {
                languages,
                devices,
                preferences,
            })
    }

    // ========================================================================
    // Property 11: Configuration Persistence Round-Trip
    // **Validates: Requirements 25.3, 25.4**
    //
    // For any valid AppConfig C:
    // - load(save(C)) = C
    // - All fields are preserved exactly as saved
    // - Schema migrations do not lose data
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// **Property 11: Configuration Persistence Round-Trip**
        ///
        /// Verifies that `load(save(config)) == config` for any valid configuration.
        /// This ensures that the serialization/deserialization process preserves
        /// all configuration values exactly.
        ///
        /// **Validates: Requirements 25.3, 25.4**
        #[test]
        fn prop_config_persistence_round_trip(config in valid_app_config()) {
            // Create a fresh test database for each test case
            let db = create_prop_test_db();

            // Save the config to the database
            let save_result = save_config(&db, &config);
            prop_assert!(save_result.is_ok(), "Failed to save config: {:?}", save_result.err());

            // Load the config back from the database
            let load_result = load_config(&db);
            prop_assert!(load_result.is_ok(), "Failed to load config: {:?}", load_result.err());

            let loaded_config = load_result.unwrap();

            // Verify round-trip equality: load(save(config)) == config
            prop_assert_eq!(
                &config.languages.system_source_lang,
                &loaded_config.languages.system_source_lang,
                "system_source_lang mismatch"
            );
            prop_assert_eq!(
                &config.languages.system_target_lang,
                &loaded_config.languages.system_target_lang,
                "system_target_lang mismatch"
            );
            prop_assert_eq!(
                &config.languages.user_source_lang,
                &loaded_config.languages.user_source_lang,
                "user_source_lang mismatch"
            );
            prop_assert_eq!(
                &config.languages.user_target_lang,
                &loaded_config.languages.user_target_lang,
                "user_target_lang mismatch"
            );
            prop_assert_eq!(
                &config.devices.input_device,
                &loaded_config.devices.input_device,
                "input_device mismatch"
            );
            prop_assert_eq!(
                &config.devices.system_capture_device,
                &loaded_config.devices.system_capture_device,
                "system_capture_device mismatch"
            );
            prop_assert_eq!(
                &config.devices.output_device,
                &loaded_config.devices.output_device,
                "output_device mismatch"
            );
            prop_assert_eq!(
                config.preferences.start_minimized,
                loaded_config.preferences.start_minimized,
                "start_minimized mismatch"
            );
            prop_assert_eq!(
                config.preferences.auto_start,
                loaded_config.preferences.auto_start,
                "auto_start mismatch"
            );
            prop_assert_eq!(
                &config.preferences.theme,
                &loaded_config.preferences.theme,
                "theme mismatch"
            );
            prop_assert_eq!(
                config.preferences.enable_sentry,
                loaded_config.preferences.enable_sentry,
                "enable_sentry mismatch"
            );

            // Also verify full struct equality
            prop_assert_eq!(config, loaded_config, "Full config mismatch");
        }

        /// **Property 11 (Extension): JSON Export/Import Round-Trip**
        ///
        /// Verifies that `import(export(config)) == config` for string serialization.
        /// This tests the JSON serialization path used for config backup/restore.
        ///
        /// **Validates: Requirements 25.3, 25.4, 25.5**
        #[test]
        fn prop_config_json_round_trip(config in valid_app_config()) {
            // Export config to JSON string
            let export_result = export_config_to_string(&config);
            prop_assert!(export_result.is_ok(), "Failed to export config: {:?}", export_result.err());

            let json = export_result.unwrap();

            // Import config from JSON string
            let import_result = import_config_from_string(&json);
            prop_assert!(import_result.is_ok(), "Failed to import config: {:?}", import_result.err());

            let imported_config = import_result.unwrap();

            // Verify round-trip equality
            prop_assert_eq!(config, imported_config, "JSON round-trip failed");
        }

        /// **Property 11 (Extension): Multiple Save/Load Cycles**
        ///
        /// Verifies that repeated save/load cycles preserve config integrity.
        /// This ensures no data corruption over multiple persistence operations.
        ///
        /// **Validates: Requirements 25.3, 25.4**
        #[test]
        fn prop_config_multiple_cycles(config in valid_app_config()) {
            let db = create_prop_test_db();

            // Perform 3 save/load cycles
            let mut current_config = config.clone();
            for cycle in 0..3 {
                save_config(&db, &current_config)
                    .expect(&format!("Save failed at cycle {}", cycle));
                
                current_config = load_config(&db)
                    .expect(&format!("Load failed at cycle {}", cycle));
            }

            // After 3 cycles, config should still match original
            prop_assert_eq!(config, current_config, "Config corrupted after multiple cycles");
        }

        /// **Property 11 (Extension): Config Overwrite Preserves Latest Values**
        ///
        /// Verifies that saving a new config completely overwrites the previous one.
        /// This ensures no "ghost" values from previous configs leak through.
        ///
        /// **Validates: Requirements 25.3**
        #[test]
        fn prop_config_overwrite(
            config1 in valid_app_config(),
            config2 in valid_app_config()
        ) {
            let db = create_prop_test_db();

            // Save first config
            save_config(&db, &config1).expect("Failed to save config1");
            
            // Save second config (should completely overwrite)
            save_config(&db, &config2).expect("Failed to save config2");
            
            // Load should return second config, not first or a mix
            let loaded = load_config(&db).expect("Failed to load config");
            
            prop_assert_eq!(config2, loaded, "Loaded config should match second save, not first");
        }
    }
}
