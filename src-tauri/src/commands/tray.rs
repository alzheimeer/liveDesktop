//! Tray Commands for IPC
//!
//! Commands to control the system tray from the frontend

use tauri::command;

use crate::tray::{TrayState, TrayConfig, show_main_window, hide_to_tray};
use crate::TrayManagerState;

/// Get the current tray state
///
/// # Requirements
/// - Requirement 16.2: Get current state (inactive/active/error)
#[command]
pub async fn get_tray_state(
    tray_state: tauri::State<'_, TrayManagerState>,
) -> Result<TrayState, String> {
    let manager = tray_state.0.read().await;
    Ok(manager.get_state().await)
}

/// Set the tray state and update the icon
///
/// # Requirements
/// - Requirement 16.2: Change icon color/shape based on state
#[command]
pub async fn set_tray_state(
    app: tauri::AppHandle,
    tray_state: tauri::State<'_, TrayManagerState>,
    state: TrayState,
) -> Result<(), String> {
    let manager = tray_state.0.read().await;
    manager.set_state(state, &app).await.map_err(|e| e.to_string())
}

/// Check if translation is paused
#[command]
pub async fn is_tray_paused(
    tray_state: tauri::State<'_, TrayManagerState>,
) -> Result<bool, String> {
    let manager = tray_state.0.read().await;
    Ok(manager.is_paused().await)
}

/// Toggle the pause state
///
/// # Requirements
/// - Requirement 16.3: Toggle pause/resume from tray menu
#[command]
pub async fn toggle_tray_pause(
    app: tauri::AppHandle,
    tray_state: tauri::State<'_, TrayManagerState>,
) -> Result<bool, String> {
    let manager = tray_state.0.read().await;
    Ok(manager.toggle_pause(Some(&app)).await)
}

/// Get the tray configuration
///
/// # Requirements
/// - Requirement 16.4, 16.5: Get minimize_to_tray and start_minimized settings
#[command]
pub async fn get_tray_config(
    tray_state: tauri::State<'_, TrayManagerState>,
) -> Result<TrayConfig, String> {
    let manager = tray_state.0.read().await;
    Ok(manager.get_config().await)
}

/// Set the tray configuration
///
/// # Requirements
/// - Requirement 16.4: Configure minimize to tray on close
/// - Requirement 16.5: Configure start minimized
#[command]
pub async fn set_tray_config(
    tray_state: tauri::State<'_, TrayManagerState>,
    config: TrayConfig,
) -> Result<(), String> {
    let manager = tray_state.0.read().await;
    manager.set_config(config).await;
    Ok(())
}

/// Show the main window from tray
///
/// # Requirements
/// - Requirement 16.3: Show window action from tray menu
#[command]
pub async fn show_window_from_tray(
    app: tauri::AppHandle,
) -> Result<(), String> {
    show_main_window(&app).map_err(|e| e.to_string())
}

/// Hide the main window to tray
///
/// # Requirements
/// - Requirement 16.4: Minimize to tray
#[command]
pub async fn hide_window_to_tray(
    app: tauri::AppHandle,
) -> Result<(), String> {
    hide_to_tray(&app).map_err(|e| e.to_string())
}
