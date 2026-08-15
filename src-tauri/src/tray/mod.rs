//! System Tray Module
//!
//! This module provides System Tray functionality for Traductor Desktop.
//! The tray icon shows the current state of the application:
//! - Gray: Inactive (no translation in progress)
//! - Green: Active (translation in progress)
//! - Red: Error state
//!
//! # Requirements
//!
//! - Requirement 16.1: Show icon in System Tray (Windows) / Menu Bar (macOS)
//! - Requirement 16.2: Change icon color/shape to indicate state:
//!   - Inactive: gray
//!   - Translating: green
//!   - Error: red
//! - Requirement 16.3: Menu with: Show window, Pause/Resume, Settings, Close
//! - Requirement 16.4: Minimize to tray when closing window (instead of closing app)
//! - Requirement 16.5: Option to start minimized

pub mod menu;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::MenuEvent,
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tokio::sync::RwLock;

pub use menu::*;

/// Application state as shown in the tray icon
///
/// # Requirement 16.2
/// The tray icon changes color/shape to indicate:
/// - Inactive (gray): No translation in progress
/// - Active (green): Translation is running
/// - Error (red): An error has occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrayState {
    /// No translation in progress - gray icon
    #[default]
    Inactive,
    /// Translation is active - green icon
    Active,
    /// Error state - red icon
    Error,
}

impl TrayState {
    /// Get the icon path for this state
    pub fn icon_path(&self) -> &'static str {
        match self {
            TrayState::Inactive => "icons/tray-inactive.png",
            TrayState::Active => "icons/tray-active.png",
            TrayState::Error => "icons/tray-error.png",
        }
    }

    /// Get a user-friendly description for this state
    pub fn description(&self) -> &'static str {
        match self {
            TrayState::Inactive => "Inactivo",
            TrayState::Active => "Traduciendo",
            TrayState::Error => "Error",
        }
    }

    /// Get the tooltip text for this state
    pub fn tooltip(&self) -> &'static str {
        match self {
            TrayState::Inactive => "Traductor Desktop - Inactivo",
            TrayState::Active => "Traductor Desktop - Traduciendo",
            TrayState::Error => "Traductor Desktop - Error",
        }
    }
}

/// Event names for tray-related events
pub mod tray_events {
    /// Tray state changed
    pub const TRAY_STATE_CHANGED: &str = "tray-state-changed";
    /// Tray icon clicked
    pub const TRAY_ICON_CLICKED: &str = "tray-icon-clicked";
    /// Tray menu action
    pub const TRAY_MENU_ACTION: &str = "tray-menu-action";
}

/// Payload for tray state change event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStateChangedPayload {
    pub state: TrayState,
    pub description: String,
}

/// Tray manager that handles the system tray icon and state
pub struct TrayManager {
    state: Arc<RwLock<TrayState>>,
    is_paused: Arc<RwLock<bool>>,
    config: Arc<RwLock<TrayConfig>>,
}

impl TrayManager {
    /// Create a new TrayManager instance
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(TrayState::Inactive)),
            is_paused: Arc::new(RwLock::new(false)),
            config: Arc::new(RwLock::new(TrayConfig::default())),
        }
    }

    /// Get the current tray state
    pub async fn get_state(&self) -> TrayState {
        *self.state.read().await
    }

    /// Get the current tray config
    pub async fn get_config(&self) -> TrayConfig {
        self.config.read().await.clone()
    }

    /// Set the tray configuration
    pub async fn set_config(&self, config: TrayConfig) {
        let mut current_config = self.config.write().await;
        *current_config = config;
    }

    /// Set the tray state and update the icon
    ///
    /// # Arguments
    /// * `state` - The new state to set
    /// * `app` - The Tauri app handle to update the tray icon
    ///
    /// # Requirement 16.2
    /// Changes icon color/shape according to state:
    /// - Inactive: gray
    /// - Active: green
    /// - Error: red
    pub async fn set_state<R: Runtime>(&self, state: TrayState, app: &AppHandle<R>) -> Result<(), TrayError> {
        let mut current_state = self.state.write().await;
        
        // Only update if state actually changed
        if *current_state == state {
            return Ok(());
        }

        *current_state = state;
        
        // Update the tray icon
        self.update_tray_icon(state, app)?;

        // Emit state change event
        let _ = app.emit(
            tray_events::TRAY_STATE_CHANGED,
            TrayStateChangedPayload {
                state,
                description: state.description().to_string(),
            },
        );

        tracing::info!(
            state = ?state,
            "Tray state updated"
        );

        Ok(())
    }

    /// Update the tray icon based on current state
    fn update_tray_icon<R: Runtime>(&self, state: TrayState, app: &AppHandle<R>) -> Result<(), TrayError> {
        if let Some(tray) = app.tray_by_id("main-tray") {
            // Load the appropriate icon for the state
            let icon = load_tray_icon(state)?;
            
            // Update the icon
            tray.set_icon(Some(icon))
                .map_err(|e| TrayError::IconUpdate(e.to_string()))?;
            
            // Update tooltip
            tray.set_tooltip(Some(state.tooltip()))
                .map_err(|e| TrayError::TooltipUpdate(e.to_string()))?;
        }
        
        Ok(())
    }

    /// Check if translation is paused
    pub async fn is_paused(&self) -> bool {
        *self.is_paused.read().await
    }

    /// Toggle pause state and update the menu
    ///
    /// # Arguments
    /// * `app` - The Tauri app handle to update the menu
    ///
    /// # Returns
    /// The new pause state (true = paused, false = running)
    pub async fn toggle_pause<R: Runtime>(&self, app: Option<&AppHandle<R>>) -> bool {
        let mut is_paused = self.is_paused.write().await;
        *is_paused = !*is_paused;
        let new_state = *is_paused;
        drop(is_paused); // Release lock before updating menu
        
        // Update the menu to reflect the new pause state
        if let Some(app) = app {
            if let Err(e) = update_pause_menu_item(app, new_state) {
                tracing::error!(error = %e, "Failed to update pause menu item");
            }
        }
        
        new_state
    }

    /// Set pause state explicitly
    ///
    /// # Arguments
    /// * `paused` - The new pause state
    /// * `app` - Optional app handle to update the menu
    pub async fn set_paused<R: Runtime>(&self, paused: bool, app: Option<&AppHandle<R>>) {
        let mut is_paused = self.is_paused.write().await;
        *is_paused = paused;
        drop(is_paused); // Release lock before updating menu
        
        // Update the menu to reflect the new pause state
        if let Some(app) = app {
            if let Err(e) = update_pause_menu_item(app, paused) {
                tracing::error!(error = %e, "Failed to update pause menu item");
            }
        }
    }

    /// Check if the app should minimize to tray when closing
    ///
    /// # Requirement 16.4
    pub async fn should_minimize_to_tray(&self) -> bool {
        self.config.read().await.minimize_to_tray
    }

    /// Check if the app should start minimized
    ///
    /// # Requirement 16.5
    pub async fn should_start_minimized(&self) -> bool {
        self.config.read().await.start_minimized
    }

    /// Handle window close request
    ///
    /// # Requirement 16.4
    /// Minimize to tray when closing window (instead of closing app)
    ///
    /// # Returns
    /// `true` if the close should be prevented (window hidden to tray)
    pub async fn handle_close_request<R: Runtime>(&self, app: &AppHandle<R>) -> bool {
        let config = self.config.read().await;
        handle_window_close_request(app, &config)
    }

    /// Apply start minimized setting
    ///
    /// # Requirement 16.5
    pub async fn apply_start_minimized<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), TrayError> {
        let config = self.config.read().await;
        apply_start_minimized(app, &config)
            .map_err(|e| TrayError::Creation(e.to_string()))
    }
}

impl Default for TrayManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during tray operations
#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("Failed to create tray icon: {0}")]
    Creation(String),
    #[error("Failed to update tray icon: {0}")]
    IconUpdate(String),
    #[error("Failed to update tooltip: {0}")]
    TooltipUpdate(String),
    #[error("Failed to load icon: {0}")]
    IconLoad(String),
    #[error("Tray icon not found")]
    NotFound,
}

/// Load a tray icon for the given state
///
/// This function generates icons programmatically based on state color.
/// The icons are:
/// - Gray (inactive): A gray microphone icon
/// - Green (active): A green microphone icon
/// - Red (error): A red microphone icon
fn load_tray_icon(state: TrayState) -> Result<Image<'static>, TrayError> {
    // Generate icon programmatically based on state
    // Using a simple 32x32 PNG icon with the appropriate color
    let icon_data = generate_tray_icon_data(state);
    
    // Image::new_owned doesn't return Result in Tauri 2.x, it returns Image directly
    Ok(Image::new_owned(icon_data, 32, 32))
}

/// Generate tray icon RGBA data for the given state
///
/// Creates a simple circular icon with the appropriate color:
/// - Inactive: #808080 (gray)
/// - Active: #00C853 (green)
/// - Error: #FF5252 (red)
fn generate_tray_icon_data(state: TrayState) -> Vec<u8> {
    let (r, g, b) = match state {
        TrayState::Inactive => (128, 128, 128), // Gray
        TrayState::Active => (0, 200, 83),       // Green (#00C853)
        TrayState::Error => (255, 82, 82),       // Red (#FF5252)
    };

    let size = 32;
    let center = size as f32 / 2.0;
    let radius = 12.0;
    let inner_radius = 6.0;
    
    let mut data = Vec::with_capacity(size * size * 4);
    
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            
            // Create a ring-like shape (microphone/translation icon)
            let alpha = if dist <= radius && dist >= inner_radius {
                // Full opacity for the ring
                255u8
            } else if dist < inner_radius && dist >= inner_radius - 2.0 {
                // Inner edge anti-aliasing
                ((inner_radius - dist) * 127.0).min(255.0) as u8
            } else if dist > radius && dist <= radius + 1.0 {
                // Outer edge anti-aliasing
                ((radius + 1.0 - dist) * 255.0).max(0.0) as u8
            } else if dist < inner_radius - 2.0 {
                // Center filled area (smaller circle for mic icon)
                if dist <= 4.0 {
                    255u8
                } else {
                    0u8
                }
            } else {
                0u8
            };
            
            // Add a small stem at the bottom for microphone-like appearance
            let stem_alpha = if y as f32 > center + radius - 2.0 
                && y as f32 <= center + radius + 4.0
                && (x as f32 - center).abs() <= 2.0 {
                255u8
            } else {
                0u8
            };
            
            let final_alpha = alpha.max(stem_alpha);
            
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(final_alpha);
        }
    }
    
    data
}

/// Setup the system tray with initial state and menu
///
/// # Requirements
/// - Requirement 16.1: Show icon in System Tray (Windows) / Menu Bar (macOS)
/// - Requirement 16.2: Initial state is gray (inactive)
/// - Requirement 16.3: Menu with: Show window, Pause/Resume, Settings, Close
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<TrayIcon<R>, TrayError> {
    let initial_state = TrayState::Inactive;
    let icon = load_tray_icon(initial_state)?;

    // Build the initial menu
    let menu = build_tray_menu(app, false)
        .map_err(|e| TrayError::Creation(format!("Failed to build menu: {}", e)))?;

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip(initial_state.tooltip())
        .icon_as_template(true) // For macOS Menu Bar
        .menu(&menu)
        .show_menu_on_left_click(false) // Right-click shows menu, left-click shows window
        .on_tray_icon_event(|tray, event| {
            handle_tray_icon_event(tray, event);
        })
        .on_menu_event(|app, event| {
            handle_tray_menu_event(app, event);
        })
        .build(app)
        .map_err(|e| TrayError::Creation(e.to_string()))?;

    tracing::info!("System tray initialized with inactive state and menu");

    Ok(tray)
}

/// Handle tray icon events (clicks)
///
/// # Requirement 16.3
/// Left-click on tray icon shows the main window
fn handle_tray_icon_event<R: Runtime>(tray: &TrayIcon<R>, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => {
            // Left click - show window
            if let Some(app) = tray.app_handle().get_webview_window("main") {
                let _ = app.show();
                let _ = app.unminimize();
                let _ = app.set_focus();
                tracing::debug!("Tray left-click: showing main window");
            }
        }
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            // Double click - also show window
            if let Some(app) = tray.app_handle().get_webview_window("main") {
                let _ = app.show();
                let _ = app.unminimize();
                let _ = app.set_focus();
                tracing::debug!("Tray double-click: showing main window");
            }
        }
        _ => {}
    }
}

/// Handle tray menu events
///
/// # Requirement 16.3
/// Menu actions: Show window, Pause/Resume, Settings, Close
fn handle_tray_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let menu_id = event.id().as_ref();
    
    if let Some(action) = handle_menu_click(app, menu_id) {
        // Execute the action asynchronously
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = execute_menu_action(&app_clone, action).await {
                tracing::error!(error = %e, action = ?action, "Failed to execute menu action");
            }
        });
    }
}

/// Update the tray icon based on application state
///
/// This is a convenience function to update the tray from external modules.
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `state` - The new tray state
pub fn update_tray_state<R: Runtime>(app: &AppHandle<R>, state: TrayState) -> Result<(), TrayError> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let icon = load_tray_icon(state)?;
        
        tray.set_icon(Some(icon))
            .map_err(|e| TrayError::IconUpdate(e.to_string()))?;
        
        tray.set_tooltip(Some(state.tooltip()))
            .map_err(|e| TrayError::TooltipUpdate(e.to_string()))?;
        
        tracing::debug!(state = ?state, "Tray icon updated");
        
        Ok(())
    } else {
        Err(TrayError::NotFound)
    }
}

/// Convenience function to set tray to active state
pub fn set_tray_active<R: Runtime>(app: &AppHandle<R>) -> Result<(), TrayError> {
    update_tray_state(app, TrayState::Active)
}

/// Convenience function to set tray to inactive state
pub fn set_tray_inactive<R: Runtime>(app: &AppHandle<R>) -> Result<(), TrayError> {
    update_tray_state(app, TrayState::Inactive)
}

/// Convenience function to set tray to error state
pub fn set_tray_error<R: Runtime>(app: &AppHandle<R>) -> Result<(), TrayError> {
    update_tray_state(app, TrayState::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_state_default() {
        let state = TrayState::default();
        assert_eq!(state, TrayState::Inactive);
    }

    #[test]
    fn test_tray_state_descriptions() {
        assert_eq!(TrayState::Inactive.description(), "Inactivo");
        assert_eq!(TrayState::Active.description(), "Traduciendo");
        assert_eq!(TrayState::Error.description(), "Error");
    }

    #[test]
    fn test_tray_state_tooltips() {
        assert!(TrayState::Inactive.tooltip().contains("Inactivo"));
        assert!(TrayState::Active.tooltip().contains("Traduciendo"));
        assert!(TrayState::Error.tooltip().contains("Error"));
    }

    #[test]
    fn test_tray_state_icon_paths() {
        assert!(TrayState::Inactive.icon_path().contains("inactive"));
        assert!(TrayState::Active.icon_path().contains("active"));
        assert!(TrayState::Error.icon_path().contains("error"));
    }

    #[test]
    fn test_generate_tray_icon_data_size() {
        let data = generate_tray_icon_data(TrayState::Inactive);
        // 32x32 pixels, 4 bytes per pixel (RGBA)
        assert_eq!(data.len(), 32 * 32 * 4);
    }

    #[test]
    fn test_generate_tray_icon_data_colors() {
        // Test that different states produce different color data
        let inactive_data = generate_tray_icon_data(TrayState::Inactive);
        let active_data = generate_tray_icon_data(TrayState::Active);
        let error_data = generate_tray_icon_data(TrayState::Error);
        
        // They should be different
        assert_ne!(inactive_data, active_data);
        assert_ne!(inactive_data, error_data);
        assert_ne!(active_data, error_data);
    }

    #[test]
    fn test_tray_manager_creation() {
        let _manager = TrayManager::new();
        // Manager should be created successfully
        assert!(true);
    }

    #[test]
    fn test_tray_state_serialization() {
        let state = TrayState::Active;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"active\"");

        let state: TrayState = serde_json::from_str("\"inactive\"").unwrap();
        assert_eq!(state, TrayState::Inactive);
    }

    #[test]
    fn test_tray_state_changed_payload() {
        let payload = TrayStateChangedPayload {
            state: TrayState::Active,
            description: "Traduciendo".to_string(),
        };
        
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("active"));
        assert!(json.contains("Traduciendo"));
    }

    #[tokio::test]
    async fn test_tray_manager_state_changes() {
        let manager = TrayManager::new();
        
        // Initial state should be Inactive
        assert_eq!(manager.get_state().await, TrayState::Inactive);
    }

    #[tokio::test]
    async fn test_tray_manager_pause_toggle() {
        let manager = TrayManager::new();
        
        // Initially not paused
        assert!(!manager.is_paused().await);
        
        // Toggle pause (without app handle for menu update)
        let new_state = manager.toggle_pause::<tauri::Wry>(None).await;
        assert!(new_state);
        assert!(manager.is_paused().await);
        
        // Toggle again
        let new_state = manager.toggle_pause::<tauri::Wry>(None).await;
        assert!(!new_state);
        assert!(!manager.is_paused().await);
    }

    #[tokio::test]
    async fn test_tray_manager_set_paused() {
        let manager = TrayManager::new();
        
        manager.set_paused::<tauri::Wry>(true, None).await;
        assert!(manager.is_paused().await);
        
        manager.set_paused::<tauri::Wry>(false, None).await;
        assert!(!manager.is_paused().await);
    }

    #[tokio::test]
    async fn test_tray_manager_config() {
        let manager = TrayManager::new();
        
        // Default config
        let config = manager.get_config().await;
        assert!(config.minimize_to_tray);
        assert!(!config.start_minimized);
        
        // Update config
        let new_config = TrayConfig {
            minimize_to_tray: false,
            start_minimized: true,
        };
        manager.set_config(new_config.clone()).await;
        
        let config = manager.get_config().await;
        assert!(!config.minimize_to_tray);
        assert!(config.start_minimized);
    }

    #[tokio::test]
    async fn test_tray_manager_should_minimize_to_tray() {
        let manager = TrayManager::new();
        
        // Default should be true
        assert!(manager.should_minimize_to_tray().await);
        
        // Set to false
        manager.set_config(TrayConfig {
            minimize_to_tray: false,
            start_minimized: false,
        }).await;
        
        assert!(!manager.should_minimize_to_tray().await);
    }

    #[tokio::test]
    async fn test_tray_manager_should_start_minimized() {
        let manager = TrayManager::new();
        
        // Default should be false
        assert!(!manager.should_start_minimized().await);
        
        // Set to true
        manager.set_config(TrayConfig {
            minimize_to_tray: true,
            start_minimized: true,
        }).await;
        
        assert!(manager.should_start_minimized().await);
    }
}
