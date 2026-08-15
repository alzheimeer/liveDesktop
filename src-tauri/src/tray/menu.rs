//! Tray Menu Module
//!
//! This module defines the context menu shown when right-clicking the tray icon.
//! The menu provides quick access to common actions without opening the main window.
//!
//! # Requirements
//!
//! - Requirement 16.3: Menu with: Show window, Pause/Resume, Settings, Close
//! - Requirement 16.4: Minimize to tray when closing window (instead of closing app)
//! - Requirement 16.5: Option to start minimized

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    AppHandle, Emitter, Manager, Runtime,
};

/// Actions that can be triggered from the tray menu
///
/// # Requirement 16.3
/// Menu actions: Show window, Pause/Resume, Settings, Close
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayMenuAction {
    /// Show the main window
    ShowWindow,
    /// Toggle pause/resume translation
    TogglePause,
    /// Open settings
    OpenSettings,
    /// Quit the application
    Quit,
}

impl TrayMenuAction {
    /// Get a user-friendly label for this action
    pub fn label(&self) -> &'static str {
        match self {
            TrayMenuAction::ShowWindow => "Mostrar Ventana",
            TrayMenuAction::TogglePause => "Pausar/Reanudar",
            TrayMenuAction::OpenSettings => "Configuración",
            TrayMenuAction::Quit => "Cerrar",
        }
    }

    /// Get the menu item ID for this action
    pub fn id(&self) -> &'static str {
        match self {
            TrayMenuAction::ShowWindow => "show-window",
            TrayMenuAction::TogglePause => "toggle-pause",
            TrayMenuAction::OpenSettings => "settings",
            TrayMenuAction::Quit => "quit",
        }
    }

    /// Parse action from menu item ID
    pub fn from_id(id: &str) -> Option<TrayMenuAction> {
        match id {
            "show-window" => Some(TrayMenuAction::ShowWindow),
            "toggle-pause" => Some(TrayMenuAction::TogglePause),
            "settings" => Some(TrayMenuAction::OpenSettings),
            "quit" => Some(TrayMenuAction::Quit),
            _ => None,
        }
    }
}

/// Event payload for tray menu action events
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuActionPayload {
    pub action: TrayMenuAction,
}

/// Configuration for tray behavior
///
/// # Requirements
/// - Requirement 16.4: minimize_to_tray - Minimize to tray when closing window
/// - Requirement 16.5: start_minimized - Option to start minimized
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayConfig {
    /// Whether to minimize to tray when closing window (Requirement 16.4)
    pub minimize_to_tray: bool,
    /// Whether to start minimized (Requirement 16.5)
    pub start_minimized: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            minimize_to_tray: true,
            start_minimized: false,
        }
    }
}

/// Build the tray context menu
///
/// # Requirement 16.3
/// Creates a menu with: Mostrar Ventana, Pausar/Reanudar, Configuración, Cerrar
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `is_paused` - Whether translation is currently paused (affects label)
///
/// # Returns
/// The built Menu ready to be attached to the tray icon
pub fn build_tray_menu<R: Runtime>(app: &AppHandle<R>, is_paused: bool) -> Result<Menu<R>, TrayMenuError> {
    // Determine the pause/resume label based on current state
    let pause_label = if is_paused {
        "Reanudar Traducción"
    } else {
        "Pausar Traducción"
    };

    // Build menu items
    let show_window = MenuItemBuilder::new("Mostrar Ventana")
        .id(TrayMenuAction::ShowWindow.id())
        .build(app)
        .map_err(|e| TrayMenuError::MenuItemCreation(e.to_string()))?;

    let toggle_pause = MenuItemBuilder::new(pause_label)
        .id(TrayMenuAction::TogglePause.id())
        .build(app)
        .map_err(|e| TrayMenuError::MenuItemCreation(e.to_string()))?;

    let settings = MenuItemBuilder::new("Configuración")
        .id(TrayMenuAction::OpenSettings.id())
        .build(app)
        .map_err(|e| TrayMenuError::MenuItemCreation(e.to_string()))?;

    let quit = MenuItemBuilder::new("Cerrar")
        .id(TrayMenuAction::Quit.id())
        .build(app)
        .map_err(|e| TrayMenuError::MenuItemCreation(e.to_string()))?;

    // Build the menu with separator before quit
    let menu = MenuBuilder::new(app)
        .item(&show_window)
        .separator()
        .item(&toggle_pause)
        .item(&settings)
        .separator()
        .item(&quit)
        .build()
        .map_err(|e| TrayMenuError::MenuBuild(e.to_string()))?;

    Ok(menu)
}

/// Update the pause menu item text
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `is_paused` - Whether translation is currently paused
pub fn update_pause_menu_item<R: Runtime>(app: &AppHandle<R>, is_paused: bool) -> Result<(), TrayMenuError> {
    // Get the tray by ID
    if let Some(tray) = app.tray_by_id("main-tray") {
        // Rebuild the menu with the new pause state
        let menu = build_tray_menu(app, is_paused)?;
        tray.set_menu(Some(menu))
            .map_err(|e| TrayMenuError::MenuUpdate(e.to_string()))?;
    }
    Ok(())
}

/// Handle tray menu item click
///
/// # Requirement 16.3
/// Handles: Show window, Pause/Resume, Settings, Close
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `menu_id` - The ID of the clicked menu item
///
/// # Returns
/// The action that was executed, or None if the ID was not recognized
pub fn handle_menu_click<R: Runtime>(
    app: &AppHandle<R>,
    menu_id: &str,
) -> Option<TrayMenuAction> {
    let action = TrayMenuAction::from_id(menu_id)?;
    
    tracing::debug!(action = ?action, "Tray menu action triggered");
    
    // Emit the menu action event for the frontend
    let _ = app.emit(
        super::tray_events::TRAY_MENU_ACTION,
        TrayMenuActionPayload { action },
    );

    Some(action)
}

/// Execute a tray menu action
///
/// # Requirement 16.3
/// - ShowWindow: Show and focus the main window
/// - TogglePause: Toggle translation pause state
/// - OpenSettings: Emit event to open settings in frontend
/// - Quit: Close the application
///
/// # Returns
/// Result indicating success or failure of the action
pub async fn execute_menu_action<R: Runtime>(
    app: &AppHandle<R>,
    action: TrayMenuAction,
) -> Result<(), TrayMenuError> {
    match action {
        TrayMenuAction::ShowWindow => {
            show_main_window(app)?;
        }
        TrayMenuAction::TogglePause => {
            // The actual pause toggle is handled by the TrayManager
            // This just emits the event for the frontend to respond to
            tracing::debug!("Toggle pause action - event emitted to frontend");
        }
        TrayMenuAction::OpenSettings => {
            // First show the window, then emit settings event
            show_main_window(app)?;
            let _ = app.emit("open-settings", ());
            tracing::debug!("Settings action - window shown and event emitted");
        }
        TrayMenuAction::Quit => {
            tracing::info!("Quit action - closing application");
            app.exit(0);
        }
    }
    
    Ok(())
}

/// Show the main window and bring it to focus
///
/// # Arguments
/// * `app` - The Tauri app handle
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), TrayMenuError> {
    if let Some(window) = app.get_webview_window("main") {
        // Show the window if hidden
        window.show().map_err(|e| TrayMenuError::WindowOperation(e.to_string()))?;
        
        // Unminimize if minimized
        window.unminimize().map_err(|e| TrayMenuError::WindowOperation(e.to_string()))?;
        
        // Bring to front and focus
        window.set_focus().map_err(|e| TrayMenuError::WindowOperation(e.to_string()))?;
        
        tracing::debug!("Main window shown and focused");
    } else {
        tracing::warn!("Main window not found");
        return Err(TrayMenuError::WindowNotFound);
    }
    
    Ok(())
}

/// Hide the main window to tray
///
/// # Requirement 16.4
/// Minimize to tray when closing window (instead of closing app)
///
/// # Arguments
/// * `app` - The Tauri app handle
pub fn hide_to_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), TrayMenuError> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| TrayMenuError::WindowOperation(e.to_string()))?;
        tracing::debug!("Main window hidden to tray");
    } else {
        return Err(TrayMenuError::WindowNotFound);
    }
    
    Ok(())
}

/// Check if the main window is visible
pub fn is_window_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    if let Some(window) = app.get_webview_window("main") {
        window.is_visible().unwrap_or(false)
    } else {
        false
    }
}

/// Handle the window close request
///
/// # Requirement 16.4
/// Minimize to tray when closing window (instead of closing app)
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `config` - The tray configuration
///
/// # Returns
/// `true` if the close should be prevented (window will be hidden to tray)
/// `false` if the window should actually close
pub fn handle_window_close_request<R: Runtime>(
    app: &AppHandle<R>,
    config: &TrayConfig,
) -> bool {
    if config.minimize_to_tray {
        // Hide to tray instead of closing
        if let Err(e) = hide_to_tray(app) {
            tracing::error!(error = %e, "Failed to hide window to tray");
            return false; // Allow close if hide fails
        }
        true // Prevent the actual close
    } else {
        false // Allow the window to close
    }
}

/// Apply start minimized setting
///
/// # Requirement 16.5
/// Option to start minimized
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `config` - The tray configuration
pub fn apply_start_minimized<R: Runtime>(app: &AppHandle<R>, config: &TrayConfig) -> Result<(), TrayMenuError> {
    if config.start_minimized {
        hide_to_tray(app)?;
        tracing::info!("Application started minimized to tray");
    }
    Ok(())
}

/// Errors that can occur during tray menu operations
#[derive(Debug, thiserror::Error)]
pub enum TrayMenuError {
    #[error("Failed to create menu item: {0}")]
    MenuItemCreation(String),
    #[error("Failed to build menu: {0}")]
    MenuBuild(String),
    #[error("Failed to update menu: {0}")]
    MenuUpdate(String),
    #[error("Failed to perform window operation: {0}")]
    WindowOperation(String),
    #[error("Main window not found")]
    WindowNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_menu_action_labels() {
        assert_eq!(TrayMenuAction::ShowWindow.label(), "Mostrar Ventana");
        assert_eq!(TrayMenuAction::TogglePause.label(), "Pausar/Reanudar");
        assert_eq!(TrayMenuAction::OpenSettings.label(), "Configuración");
        assert_eq!(TrayMenuAction::Quit.label(), "Cerrar");
    }

    #[test]
    fn test_tray_menu_action_ids() {
        assert_eq!(TrayMenuAction::ShowWindow.id(), "show-window");
        assert_eq!(TrayMenuAction::TogglePause.id(), "toggle-pause");
        assert_eq!(TrayMenuAction::OpenSettings.id(), "settings");
        assert_eq!(TrayMenuAction::Quit.id(), "quit");
    }

    #[test]
    fn test_tray_menu_action_from_id() {
        assert_eq!(TrayMenuAction::from_id("show-window"), Some(TrayMenuAction::ShowWindow));
        assert_eq!(TrayMenuAction::from_id("toggle-pause"), Some(TrayMenuAction::TogglePause));
        assert_eq!(TrayMenuAction::from_id("settings"), Some(TrayMenuAction::OpenSettings));
        assert_eq!(TrayMenuAction::from_id("quit"), Some(TrayMenuAction::Quit));
        assert_eq!(TrayMenuAction::from_id("unknown"), None);
    }

    #[test]
    fn test_tray_menu_action_serialization() {
        let action = TrayMenuAction::ShowWindow;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"showWindow\"");

        let action: TrayMenuAction = serde_json::from_str("\"quit\"").unwrap();
        assert_eq!(action, TrayMenuAction::Quit);
    }

    #[test]
    fn test_tray_config_default() {
        let config = TrayConfig::default();
        assert!(config.minimize_to_tray);
        assert!(!config.start_minimized);
    }

    #[test]
    fn test_tray_config_serialization() {
        let config = TrayConfig {
            minimize_to_tray: true,
            start_minimized: true,
        };
        
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("minimizeToTray"));
        assert!(json.contains("startMinimized"));
        
        let deserialized: TrayConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.minimize_to_tray);
        assert!(deserialized.start_minimized);
    }

    #[test]
    fn test_tray_menu_action_payload() {
        let payload = TrayMenuActionPayload {
            action: TrayMenuAction::TogglePause,
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("togglePause"));
    }

    #[test]
    fn test_tray_config_minimize_to_tray_behavior() {
        // Test that default config enables minimize to tray
        let config = TrayConfig::default();
        assert!(config.minimize_to_tray, "By default, minimize_to_tray should be true");
        
        // Test custom config
        let config = TrayConfig {
            minimize_to_tray: false,
            start_minimized: false,
        };
        assert!(!config.minimize_to_tray);
    }

    #[test]
    fn test_tray_config_start_minimized_behavior() {
        // Test that default config does NOT start minimized
        let config = TrayConfig::default();
        assert!(!config.start_minimized, "By default, start_minimized should be false");
        
        // Test custom config
        let config = TrayConfig {
            minimize_to_tray: true,
            start_minimized: true,
        };
        assert!(config.start_minimized);
    }
}
