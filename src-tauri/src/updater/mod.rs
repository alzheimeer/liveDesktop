//! Auto-updater module
//!
//! Handles automatic update checking and installation using Tauri's updater plugin.
//!
//! # Requirements
//!
//! - Requirement 17.1: Check for updates on app startup and every 24 hours
//! - Requirement 17.2: Show notification with summarized changelog when update available
//!
//! # Architecture
//!
//! The updater runs two check cycles:
//! 1. Immediate check on app startup
//! 2. Periodic check every 24 hours in the background
//!
//! When an update is available, it emits an `update-available` event to the frontend
//! with the version and changelog information.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::interval;

use crate::events::emit_update_available;

/// Interval between update checks (24 hours)
const UPDATE_CHECK_INTERVAL_HOURS: u64 = 24;

/// Information about an available update
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// New version string (e.g., "1.2.0")
    pub version: String,
    /// Changelog/release notes (summarized)
    pub changelog: String,
    /// Download URL for the update (if available)
    pub download_url: Option<String>,
    /// Whether the update is mandatory
    pub mandatory: bool,
    /// Size of the update in bytes (if known)
    pub size_bytes: Option<u64>,
}

/// Result of an update check
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    /// Whether an update is available
    pub update_available: bool,
    /// Update information if available
    pub update_info: Option<UpdateInfo>,
    /// Current app version
    pub current_version: String,
    /// Error message if check failed
    pub error: Option<String>,
}

/// Updater state shared across the application
pub struct UpdaterState {
    /// Last update info received
    pub last_update_info: Arc<RwLock<Option<UpdateInfo>>>,
    /// Whether the periodic checker is running
    pub checker_running: Arc<AtomicBool>,
    /// Whether an update is currently downloading
    pub downloading: Arc<AtomicBool>,
    /// Handle to cancel the periodic checker task
    pub checker_handle: Arc<RwLock<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

impl UpdaterState {
    pub fn new() -> Self {
        Self {
            last_update_info: Arc::new(RwLock::new(None)),
            checker_running: Arc::new(AtomicBool::new(false)),
            downloading: Arc::new(AtomicBool::new(false)),
            checker_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the last known update info
    pub async fn get_last_update_info(&self) -> Option<UpdateInfo> {
        self.last_update_info.read().await.clone()
    }

    /// Set the last update info
    pub async fn set_last_update_info(&self, info: Option<UpdateInfo>) {
        *self.last_update_info.write().await = info;
    }

    /// Check if the periodic checker is running
    pub fn is_checker_running(&self) -> bool {
        self.checker_running.load(Ordering::SeqCst)
    }

    /// Check if an update is currently downloading
    pub fn is_downloading(&self) -> bool {
        self.downloading.load(Ordering::SeqCst)
    }

    /// Set the downloading state
    pub fn set_downloading(&self, downloading: bool) {
        self.downloading.store(downloading, Ordering::SeqCst);
    }
}

impl Default for UpdaterState {
    fn default() -> Self {
        Self::new()
    }
}

/// Summarize changelog to a shorter version for notification display
///
/// Takes the full changelog and creates a summarized version suitable for
/// notification display. Limits to first 3-5 bullet points.
fn summarize_changelog(changelog: &str) -> String {
    let lines: Vec<&str> = changelog
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && (trimmed.starts_with('-')
                    || trimmed.starts_with('*')
                    || trimmed.starts_with('•')
                    || trimmed.starts_with("- ")
                    || trimmed.starts_with("* "))
        })
        .take(5)
        .collect();

    if lines.is_empty() {
        // If no bullet points, take first 3 non-empty lines
        changelog
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut result = lines.join("\n");
        
        // Count total bullet points to see if we truncated
        let total_points = changelog
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('-')
                    || trimmed.starts_with('*')
                    || trimmed.starts_with('•')
            })
            .count();

        if total_points > 5 {
            result.push_str(&format!("\n... y {} cambios más", total_points - 5));
        }

        result
    }
}

/// Check for updates using Tauri's updater plugin
///
/// # Requirements
/// - Requirement 17.1: Check for updates
/// - Requirement 17.2: Return changelog for notification
pub async fn check_for_updates<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    
    let current_version = app.config().version.clone().unwrap_or_else(|| "1.0.0".to_string());

    tracing::info!("Verificando actualizaciones... (versión actual: {})", current_version);

    // Use the updater plugin - get the updater from the app handle
    match app.updater() {
        Ok(updater) => {
            match updater.check().await {
                Ok(update_response) => {
                    if let Some(update) = update_response {
                        let version = update.version.clone();
                        let body = update.body.clone().unwrap_or_default();
                        let changelog = summarize_changelog(&body);

                        let update_info = UpdateInfo {
                            version: version.clone(),
                            changelog,
                            download_url: Some(update.download_url.as_str().to_string()),
                            mandatory: false, // Tauri updater doesn't have mandatory field by default
                            size_bytes: None,
                        };

                        tracing::info!(
                            "Actualización disponible: {} -> {}",
                            current_version,
                            version
                        );

                        Ok(UpdateCheckResult {
                            update_available: true,
                            update_info: Some(update_info),
                            current_version,
                            error: None,
                        })
                    } else {
                        tracing::info!("No hay actualizaciones disponibles");
                        Ok(UpdateCheckResult {
                            update_available: false,
                            update_info: None,
                            current_version,
                            error: None,
                        })
                    }
                }
                Err(e) => {
                    let error_msg = format!("Error al verificar actualizaciones: {}", e);
                    tracing::warn!("{}", error_msg);

                    // Don't fail completely - return result with error
                    // Requirement 1.6: Continue functioning if update server unavailable
                    Ok(UpdateCheckResult {
                        update_available: false,
                        update_info: None,
                        current_version,
                        error: Some(error_msg),
                    })
                }
            }
        }
        Err(e) => {
            // Updater plugin not available - this shouldn't happen in production
            // but allows the app to work in development
            tracing::warn!(
                "Plugin de actualizaciones no disponible: {}. \
                 Esto es normal durante desarrollo.",
                e
            );

            Ok(UpdateCheckResult {
                update_available: false,
                update_info: None,
                current_version,
                error: Some(format!("Plugin de actualizaciones no disponible: {}", e)),
            })
        }
    }
}

/// Start the periodic update checker
///
/// # Requirements
/// - Requirement 17.1: Check every 24 hours
///
/// Spawns a background task that checks for updates every 24 hours.
/// When an update is found, emits an event to the frontend.
pub async fn start_periodic_checker<R: tauri::Runtime + 'static>(
    app: tauri::AppHandle<R>,
    state: Arc<UpdaterState>,
) {
    // Don't start if already running
    if state.checker_running.swap(true, Ordering::SeqCst) {
        tracing::warn!("El verificador periódico de actualizaciones ya está en ejecución");
        return;
    }

    let state_clone = state.clone();
    let app_clone = app.clone();

    let handle = tauri::async_runtime::spawn(async move {
        let mut check_interval = interval(Duration::from_secs(UPDATE_CHECK_INTERVAL_HOURS * 3600));

        // Skip the first tick (we check on startup separately)
        check_interval.tick().await;

        loop {
            check_interval.tick().await;

            if !state_clone.checker_running.load(Ordering::SeqCst) {
                tracing::info!("Deteniendo verificador periódico de actualizaciones");
                break;
            }

            tracing::info!("Verificación periódica de actualizaciones (cada 24 horas)");

            match check_for_updates(&app_clone).await {
                Ok(result) => {
                    if result.update_available {
                        if let Some(ref info) = result.update_info {
                            // Store the update info
                            state_clone.set_last_update_info(Some(info.clone())).await;

                            // Emit event to frontend
                            // Requirement 17.2: Show notification with changelog
                            if let Err(e) = emit_update_available(
                                &app_clone,
                                &info.version,
                                &info.changelog,
                                info.download_url.as_deref(),
                                info.mandatory,
                            ) {
                                tracing::error!(
                                    "Error al emitir evento de actualización: {}",
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error en verificación periódica: {}", e);
                }
            }
        }
    });

    // Store the handle so we can cancel it later
    *state.checker_handle.write().await = Some(handle);
}

/// Stop the periodic update checker
pub async fn stop_periodic_checker(state: Arc<UpdaterState>) {
    state.checker_running.store(false, Ordering::SeqCst);

    if let Some(handle) = state.checker_handle.write().await.take() {
        handle.abort();
        tracing::info!("Verificador periódico de actualizaciones detenido");
    }
}

/// Check for updates on app startup
///
/// # Requirements
/// - Requirement 17.1: Check on startup
/// - Requirement 1.6: Don't block if server unavailable
///
/// This is called once during app initialization. It checks for updates
/// and if one is available, emits an event to the frontend.
pub async fn check_on_startup<R: tauri::Runtime + 'static>(
    app: tauri::AppHandle<R>,
    state: Arc<UpdaterState>,
) {
    tracing::info!("Verificando actualizaciones al iniciar...");

    match check_for_updates(&app).await {
        Ok(result) => {
            if result.update_available {
                if let Some(ref info) = result.update_info {
                    // Store the update info
                    state.set_last_update_info(Some(info.clone())).await;

                    // Emit event to frontend
                    // Requirement 17.2: Show notification with changelog
                    if let Err(e) = emit_update_available(
                        &app,
                        &info.version,
                        &info.changelog,
                        info.download_url.as_deref(),
                        info.mandatory,
                    ) {
                        tracing::error!("Error al emitir evento de actualización: {}", e);
                    }

                    tracing::info!(
                        "Actualización disponible: {} - notificación enviada",
                        info.version
                    );
                }
            } else if let Some(error) = result.error {
                // Requirement 1.6: Continue functioning if server unavailable
                tracing::warn!(
                    "No se pudo verificar actualizaciones: {}. \
                     La aplicación continuará funcionando normalmente.",
                    error
                );
            } else {
                tracing::info!("La aplicación está actualizada");
            }
        }
        Err(e) => {
            // Requirement 1.6: Continue functioning if server unavailable
            tracing::warn!(
                "Error al verificar actualizaciones: {}. \
                 La aplicación continuará funcionando normalmente.",
                e
            );
        }
    }
}

// ==================== Tauri Commands ====================

/// IPC command to manually check for updates
///
/// Called from the frontend when user requests a manual update check.
#[tauri::command]
pub async fn check_for_updates_command<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    updater_state: tauri::State<'_, UpdaterState>,
) -> Result<UpdateCheckResult, String> {
    let result = check_for_updates(&app).await?;

    // Store update info if available
    if let Some(ref info) = result.update_info {
        updater_state.set_last_update_info(Some(info.clone())).await;
    }

    Ok(result)
}

/// IPC command to get the last known update info
#[tauri::command]
pub async fn get_update_info(
    updater_state: tauri::State<'_, UpdaterState>,
) -> Result<Option<UpdateInfo>, String> {
    Ok(updater_state.get_last_update_info().await)
}

/// IPC command to check if the app is currently downloading an update
#[tauri::command]
pub fn is_update_downloading(
    updater_state: tauri::State<'_, UpdaterState>,
) -> bool {
    updater_state.is_downloading()
}

/// IPC command to get the current app version
#[tauri::command]
pub fn get_current_version<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> String {
    app.config().version.clone().unwrap_or_else(|| "1.0.0".to_string())
}

/// IPC command to start the periodic update checker
///
/// This is typically called during app initialization.
#[tauri::command]
pub async fn start_update_checker<R: tauri::Runtime + 'static>(
    app: tauri::AppHandle<R>,
    updater_state: tauri::State<'_, UpdaterState>,
) -> Result<(), String> {
    // Create an Arc from the state inner value for the background task
    let state = Arc::new(UpdaterState {
        last_update_info: updater_state.last_update_info.clone(),
        checker_running: updater_state.checker_running.clone(),
        downloading: updater_state.downloading.clone(),
        checker_handle: updater_state.checker_handle.clone(),
    });

    // Check on startup first
    check_on_startup(app.clone(), state.clone()).await;

    // Then start periodic checker
    start_periodic_checker(app, state).await;

    Ok(())
}

/// IPC command to stop the periodic update checker
#[tauri::command]
pub async fn stop_update_checker(
    updater_state: tauri::State<'_, UpdaterState>,
) -> Result<(), String> {
    let state = Arc::new(UpdaterState {
        last_update_info: updater_state.last_update_info.clone(),
        checker_running: updater_state.checker_running.clone(),
        downloading: updater_state.downloading.clone(),
        checker_handle: updater_state.checker_handle.clone(),
    });

    stop_periodic_checker(state).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_changelog_with_bullets() {
        let changelog = r#"
## Changes

- Nueva función de traducción
- Mejoras de rendimiento
- Corrección de errores de audio
- Actualización de dependencias
- Mejora en la interfaz de usuario
- Nueva opción de configuración
- Soporte para más idiomas
"#;
        let summary = summarize_changelog(changelog);
        
        // Should have 5 items plus "... y X cambios más"
        assert!(summary.contains("Nueva función"));
        assert!(summary.contains("Mejoras de rendimiento"));
        assert!(summary.contains("... y 2 cambios más"));
    }

    #[test]
    fn test_summarize_changelog_few_items() {
        let changelog = r#"
- Corrección menor
- Actualización de seguridad
"#;
        let summary = summarize_changelog(changelog);
        
        assert!(summary.contains("Corrección menor"));
        assert!(summary.contains("Actualización de seguridad"));
        assert!(!summary.contains("cambios más"));
    }

    #[test]
    fn test_summarize_changelog_no_bullets() {
        let changelog = r#"
Version 1.2.0

This release includes important fixes.
Performance improvements across the board.
Better error handling.
More features coming soon.
"#;
        let summary = summarize_changelog(changelog);
        
        // Should take first 3 non-empty lines
        assert!(summary.contains("Version 1.2.0"));
        let line_count = summary.lines().count();
        assert!(line_count <= 3);
    }

    #[test]
    fn test_summarize_changelog_empty() {
        let changelog = "";
        let summary = summarize_changelog(changelog);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_summarize_changelog_asterisks() {
        let changelog = r#"
* Feature A
* Feature B
* Feature C
"#;
        let summary = summarize_changelog(changelog);
        
        assert!(summary.contains("Feature A"));
        assert!(summary.contains("Feature B"));
        assert!(summary.contains("Feature C"));
    }

    #[test]
    fn test_update_info_serialization() {
        let info = UpdateInfo {
            version: "1.2.0".to_string(),
            changelog: "- Nueva función".to_string(),
            download_url: Some("https://example.com/update".to_string()),
            mandatory: false,
            size_bytes: Some(1024 * 1024),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\":\"1.2.0\""));
        assert!(json.contains("\"changelog\""));
        assert!(json.contains("\"downloadUrl\""));
        assert!(json.contains("\"mandatory\":false"));
        assert!(json.contains("\"sizeBytes\""));
    }

    #[test]
    fn test_update_check_result_serialization() {
        let result = UpdateCheckResult {
            update_available: true,
            update_info: Some(UpdateInfo {
                version: "2.0.0".to_string(),
                changelog: "Major update".to_string(),
                download_url: None,
                mandatory: true,
                size_bytes: None,
            }),
            current_version: "1.0.0".to_string(),
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"updateAvailable\":true"));
        assert!(json.contains("\"currentVersion\":\"1.0.0\""));
    }

    #[test]
    fn test_updater_state_default() {
        let state = UpdaterState::default();
        assert!(!state.is_checker_running());
        assert!(!state.is_downloading());
    }

    #[tokio::test]
    async fn test_updater_state_last_update_info() {
        let state = UpdaterState::new();
        
        // Initially no update info
        assert!(state.get_last_update_info().await.is_none());

        // Set update info
        let info = UpdateInfo {
            version: "1.1.0".to_string(),
            changelog: "Test".to_string(),
            download_url: None,
            mandatory: false,
            size_bytes: None,
        };
        state.set_last_update_info(Some(info.clone())).await;

        // Should be available now
        let retrieved = state.get_last_update_info().await.unwrap();
        assert_eq!(retrieved.version, "1.1.0");

        // Clear it
        state.set_last_update_info(None).await;
        assert!(state.get_last_update_info().await.is_none());
    }

    #[test]
    fn test_downloading_state() {
        let state = UpdaterState::new();
        
        assert!(!state.is_downloading());
        
        state.set_downloading(true);
        assert!(state.is_downloading());
        
        state.set_downloading(false);
        assert!(!state.is_downloading());
    }
}
