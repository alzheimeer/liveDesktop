//! Configuration commands
//! 
//! Handles app configuration persistence, export, and import.
//! 
//! Requirements:
//! - 22.1: IPC commands for configuration management
//! - 25.1, 25.2, 25.3, 25.4: Configuration persistence
//! - 25.5: Export/import configuration as JSON

use tauri::{command, State};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::storage::{
    AppConfig,
    Database,
    load_config as storage_load_config,
    save_config as storage_save_config,
    get_default_db_path,
    derive_encryption_key,
};
use crate::storage::config::{
    export_config_to_file,
    import_config_from_file,
    export_config_to_string,
    import_config_from_string,
};

// ============================================================================
// State Types
// ============================================================================

/// Shared state for configuration management
/// 
/// Holds the database connection used for storing and retrieving
/// application configuration.
pub struct ConfigState {
    /// Database connection (lazy initialized)
    pub db: Arc<RwLock<Option<Database>>>,
    /// Cached configuration for quick access
    pub cached_config: Arc<RwLock<Option<AppConfig>>>,
}

impl ConfigState {
    /// Create a new ConfigState
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(None)),
            cached_config: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Initialize the database connection
    pub async fn init_db(&self) -> Result<(), String> {
        let mut db_guard = self.db.write().await;
        
        if db_guard.is_some() {
            return Ok(()); // Already initialized
        }
        
        let db_path = get_default_db_path()
            .map_err(|e| format!("Error obteniendo ruta de base de datos: {}", e))?;
        
        let encryption_key = derive_encryption_key()
            .map_err(|e| format!("Error derivando clave de encriptación: {}", e))?;
        
        let db = Database::new(&db_path, &encryption_key)
            .map_err(|e| format!("Error abriendo base de datos: {}", e))?;
        
        *db_guard = Some(db);
        
        tracing::info!("Database initialized at: {}", db_path);
        Ok(())
    }
    
    /// Get a reference to the database, initializing if needed
    async fn get_db(&self) -> Result<Database, String> {
        // Try to read first
        {
            let db_guard = self.db.read().await;
            if let Some(ref db) = *db_guard {
                return Ok(db.clone());
            }
        }
        
        // Need to initialize
        self.init_db().await?;
        
        let db_guard = self.db.read().await;
        db_guard.clone().ok_or_else(|| "Database not initialized".to_string())
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// Configuration response for frontend (matches IPC types.ts)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub languages: LanguagesResponse,
    pub devices: DevicesResponse,
    pub preferences: PreferencesResponse,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagesResponse {
    pub system_source_lang: String,
    pub system_target_lang: String,
    pub user_source_lang: String,
    pub user_target_lang: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesResponse {
    pub input_device: Option<String>,
    pub system_capture_device: Option<String>,
    pub output_device: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesResponse {
    pub start_minimized: bool,
    pub auto_start: bool,
    pub theme: String,
    pub enable_sentry: bool,
}

/// Configuration input from frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInput {
    pub languages: LanguagesInput,
    pub devices: DevicesInput,
    pub preferences: PreferencesInput,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagesInput {
    pub system_source_lang: String,
    pub system_target_lang: String,
    pub user_source_lang: String,
    pub user_target_lang: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesInput {
    pub input_device: Option<String>,
    pub system_capture_device: Option<String>,
    pub output_device: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesInput {
    pub start_minimized: bool,
    pub auto_start: bool,
    pub theme: String,
    pub enable_sentry: bool,
}

// ============================================================================
// Conversion Functions
// ============================================================================

impl From<AppConfig> for ConfigResponse {
    fn from(config: AppConfig) -> Self {
        ConfigResponse {
            languages: LanguagesResponse {
                system_source_lang: config.languages.system_source_lang,
                system_target_lang: config.languages.system_target_lang,
                user_source_lang: config.languages.user_source_lang,
                user_target_lang: config.languages.user_target_lang,
            },
            devices: DevicesResponse {
                input_device: config.devices.input_device,
                system_capture_device: config.devices.system_capture_device,
                output_device: config.devices.output_device,
            },
            preferences: PreferencesResponse {
                start_minimized: config.preferences.start_minimized,
                auto_start: config.preferences.auto_start,
                theme: config.preferences.theme,
                enable_sentry: config.preferences.enable_sentry,
            },
        }
    }
}

impl From<ConfigInput> for AppConfig {
    fn from(input: ConfigInput) -> Self {
        use crate::storage::config::{LanguageConfig, DeviceConfig, PreferencesConfig};
        
        AppConfig {
            languages: LanguageConfig {
                system_source_lang: input.languages.system_source_lang,
                system_target_lang: input.languages.system_target_lang,
                user_source_lang: input.languages.user_source_lang,
                user_target_lang: input.languages.user_target_lang,
            },
            devices: DeviceConfig {
                input_device: input.devices.input_device,
                system_capture_device: input.devices.system_capture_device,
                output_device: input.devices.output_device,
            },
            preferences: PreferencesConfig {
                start_minimized: input.preferences.start_minimized,
                auto_start: input.preferences.auto_start,
                theme: input.preferences.theme,
                enable_sentry: input.preferences.enable_sentry,
            },
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Get application configuration
/// 
/// Retrieves the current configuration from the local database.
/// If no configuration exists, returns default values.
/// 
/// # Returns
/// ConfigResponse with current settings
/// 
/// # Requirements
/// - 22.1: IPC command for get_config
/// - 25.3: Restore configuration at startup
#[command]
pub async fn get_config(
    state: State<'_, ConfigState>,
) -> Result<ConfigResponse, String> {
    // Check cache first
    {
        let cache_guard = state.cached_config.read().await;
        if let Some(ref config) = *cache_guard {
            return Ok(ConfigResponse::from(config.clone()));
        }
    }
    
    // Load from database
    let db = state.get_db().await?;
    
    let config = storage_load_config(&db)
        .map_err(|e| format!("Error cargando configuración: {}", e))?;
    
    // Update cache
    {
        let mut cache_guard = state.cached_config.write().await;
        *cache_guard = Some(config.clone());
    }
    
    tracing::debug!("Configuration loaded from database");
    Ok(ConfigResponse::from(config))
}

/// Save application configuration
/// 
/// Persists the provided configuration to the local database.
/// 
/// # Arguments
/// * `config` - The configuration to save
/// 
/// # Returns
/// Ok(()) on success
/// 
/// # Requirements
/// - 22.1: IPC command for save_config
/// - 25.1, 25.2: Save configuration to local storage
#[command]
pub async fn save_config(
    state: State<'_, ConfigState>,
    config: ConfigInput,
) -> Result<(), String> {
    let db = state.get_db().await?;
    
    let app_config: AppConfig = config.into();
    
    storage_save_config(&db, &app_config)
        .map_err(|e| format!("Error guardando configuración: {}", e))?;
    
    // Update cache
    {
        let mut cache_guard = state.cached_config.write().await;
        *cache_guard = Some(app_config);
    }
    
    tracing::info!("Configuration saved successfully");
    Ok(())
}

/// Export configuration to a JSON file
/// 
/// Exports the current configuration to a JSON file at the specified path.
/// This allows users to backup their settings or migrate to another device.
/// 
/// # Arguments
/// * `path` - Absolute file path where the JSON file will be written
/// 
/// # Returns
/// Ok(()) on success
/// 
/// # Requirements
/// - 22.1: IPC command for export_config
/// - 25.5: Export configuration as JSON file
#[command]
pub async fn export_config(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<(), String> {
    // Validate path
    if path.trim().is_empty() {
        return Err("La ruta del archivo no puede estar vacía".to_string());
    }
    
    let db = state.get_db().await?;
    
    export_config_to_file(&db, &path)
        .map_err(|e| format!("Error exportando configuración: {}", e))?;
    
    tracing::info!("Configuration exported to: {}", path);
    Ok(())
}

/// Import configuration from a JSON file
/// 
/// Reads configuration from a JSON file and saves it to the local database.
/// The imported configuration replaces the existing one.
/// 
/// # Arguments
/// * `path` - Absolute file path to the JSON configuration file
/// 
/// # Returns
/// ConfigResponse with the imported configuration
/// 
/// # Requirements
/// - 22.1: IPC command for import_config
/// - 25.5: Import configuration from JSON file
#[command]
pub async fn import_config(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<ConfigResponse, String> {
    // Validate path
    if path.trim().is_empty() {
        return Err("La ruta del archivo no puede estar vacía".to_string());
    }
    
    let db = state.get_db().await?;
    
    let config = import_config_from_file(&db, &path)
        .map_err(|e| format!("Error importando configuración: {}", e))?;
    
    // Update cache
    {
        let mut cache_guard = state.cached_config.write().await;
        *cache_guard = Some(config.clone());
    }
    
    tracing::info!("Configuration imported from: {}", path);
    Ok(ConfigResponse::from(config))
}

/// Export configuration to a JSON string
/// 
/// Useful for clipboard operations or API responses.
/// 
/// # Returns
/// JSON string representation of the current configuration
#[command]
pub async fn export_config_string(
    state: State<'_, ConfigState>,
) -> Result<String, String> {
    let db = state.get_db().await?;
    
    let config = storage_load_config(&db)
        .map_err(|e| format!("Error cargando configuración: {}", e))?;
    
    export_config_to_string(&config)
        .map_err(|e| format!("Error serializando configuración: {}", e))
}

/// Import configuration from a JSON string
/// 
/// Useful for clipboard paste operations or API requests.
/// 
/// # Arguments
/// * `json` - JSON string containing configuration data
/// 
/// # Returns
/// ConfigResponse with the imported configuration
#[command]
pub async fn import_config_string(
    state: State<'_, ConfigState>,
    json: String,
) -> Result<ConfigResponse, String> {
    // Parse and validate
    let config = import_config_from_string(&json)
        .map_err(|e| format!("Error parseando configuración: {}", e))?;
    
    // Save to database
    let db = state.get_db().await?;
    
    storage_save_config(&db, &config)
        .map_err(|e| format!("Error guardando configuración: {}", e))?;
    
    // Update cache
    {
        let mut cache_guard = state.cached_config.write().await;
        *cache_guard = Some(config.clone());
    }
    
    tracing::info!("Configuration imported from string");
    Ok(ConfigResponse::from(config))
}

/// Check if configuration exists
/// 
/// Returns true if there is saved configuration in the database.
#[command]
pub async fn config_exists(
    state: State<'_, ConfigState>,
) -> Result<bool, String> {
    let db = state.get_db().await?;
    
    crate::storage::config_exists(&db)
        .map_err(|e| format!("Error verificando configuración: {}", e))
}

/// Reset configuration to defaults
/// 
/// Deletes the current configuration and resets to default values.
#[command]
pub async fn reset_config(
    state: State<'_, ConfigState>,
) -> Result<ConfigResponse, String> {
    let db = state.get_db().await?;
    
    // Delete existing config
    crate::storage::delete_config(&db)
        .map_err(|e| format!("Error eliminando configuración: {}", e))?;
    
    // Create and save defaults
    let default_config = AppConfig::default();
    storage_save_config(&db, &default_config)
        .map_err(|e| format!("Error guardando configuración por defecto: {}", e))?;
    
    // Update cache
    {
        let mut cache_guard = state.cached_config.write().await;
        *cache_guard = Some(default_config.clone());
    }
    
    tracing::info!("Configuration reset to defaults");
    Ok(ConfigResponse::from(default_config))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_state_default() {
        let _state = ConfigState::default();
        // Just verify it creates without panic
        assert!(true);
    }

    #[test]
    fn test_config_response_from_app_config() {
        let config = AppConfig::default();
        let response = ConfigResponse::from(config.clone());
        
        assert_eq!(response.languages.system_source_lang, config.languages.system_source_lang);
        assert_eq!(response.preferences.theme, config.preferences.theme);
    }

    #[test]
    fn test_config_input_to_app_config() {
        let input = ConfigInput {
            languages: LanguagesInput {
                system_source_lang: "fr".to_string(),
                system_target_lang: "de".to_string(),
                user_source_lang: "de".to_string(),
                user_target_lang: "fr".to_string(),
            },
            devices: DevicesInput {
                input_device: Some("mic123".to_string()),
                system_capture_device: None,
                output_device: Some("speaker456".to_string()),
            },
            preferences: PreferencesInput {
                start_minimized: true,
                auto_start: false,
                theme: "light".to_string(),
                enable_sentry: true,
            },
        };
        
        let config: AppConfig = input.into();
        
        assert_eq!(config.languages.system_source_lang, "fr");
        assert_eq!(config.devices.input_device, Some("mic123".to_string()));
        assert!(config.preferences.start_minimized);
        assert_eq!(config.preferences.theme, "light");
    }
}
