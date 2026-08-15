// Traductor Desktop - Tauri 2.x Backend
// Real-time voice-to-voice translation desktop application

// Module declarations
pub mod commands;
#[cfg(feature = "audio")]
pub mod audio;
pub mod gemini;
pub mod auth;
pub mod billing;
pub mod storage;
pub mod tray;
pub mod updater;
pub mod error;
pub mod events;
pub mod logging;

use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::Manager;

#[cfg(feature = "audio")]
use commands::{
    // Audio commands
    enumerate_audio_devices, start_system_channel, start_user_channel,
    stop_channel, change_audio_device, get_audio_state, get_vbcable_status,
    get_virtual_audio_status,
    // Audio test commands
    play_audio_test, stop_audio_test, is_audio_test_playing,
    AudioEngineState, AudioTestStateWrapper,
};

use commands::{
    // Auth commands
    login_with_google, login_with_email, register_with_email, logout, get_session,
    restore_session, is_authenticated, start_session_expiration_checker, stop_session_expiration_checker,
    set_byok_key, get_byok_key_exists, delete_byok_key, validate_byok_key, validate_byok_key_full,
    AuthState,
    // Config commands
    get_config, save_config, export_config, import_config,
    export_config_string, import_config_string, config_exists, reset_config,
    ConfigState,
    // Tray commands
    get_tray_state, set_tray_state, is_tray_paused, toggle_tray_pause,
    get_tray_config, set_tray_config, show_window_from_tray, hide_window_to_tray,
    // Usage commands
    get_usage_stats, get_usage_history, can_start_translation, get_upgrade_options,
    check_usage_limits, emit_usage_blocked, set_user_plan, reset_usage_notifications,
    UsageState,
};

use tray::{TrayManager, hide_to_tray};

use updater::{
    check_for_updates_command, get_update_info, is_update_downloading,
    get_current_version, start_update_checker, stop_update_checker,
    UpdaterState,
};

/// Wrapper state for the TrayManager
pub struct TrayManagerState(pub Arc<RwLock<TrayManager>>);

impl TrayManagerState {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(TrayManager::new())))
    }
}

impl Default for TrayManagerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize platform-specific components at app startup.
///
/// On Windows: Detects VB-Cable installation and registers the result.
///
/// # Requirements
///
/// - Requirement 4.1: Detect if VB-Cable is installed at app startup
///   and register the result in internal system state
#[cfg(feature = "audio")]
fn init_platform_components() {
    #[cfg(target_os = "windows")]
    {
        // Detect VB-Cable and register result in internal system state
        // This fulfills Requirement 4.1: Detect if VB-Cable is installed at app startup
        match audio::windows::vbcable::detect_and_register() {
            Ok(status) => {
                tracing::info!(
                    is_installed = status.is_installed,
                    input_available = status.input_available,
                    output_available = status.output_available,
                    "VB-Cable detection completed at startup"
                );
                
                if !status.is_installed {
                    tracing::warn!(
                        "VB-Cable no está instalado. La inyección de audio traducido \
                         al micrófono virtual no estará disponible."
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Error durante la detección de VB-Cable al iniciar"
                );
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS uses Virtual Audio Endpoint instead of VB-Cable
        tracing::info!("macOS detected - Virtual Audio Endpoint will be used for audio injection");
    }
}

#[cfg(not(feature = "audio"))]
fn init_platform_components() {
    tracing::info!("Audio feature disabled - skipping platform audio initialization");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize platform-specific components (VB-Cable detection on Windows)
    init_platform_components();
    
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Setup system tray on app ready
        .setup(|app| {
            // Initialize the system tray
            // Requirement 16.1: Show icon in System Tray (Windows) / Menu Bar (macOS)
            // Requirement 16.3: Menu with: Show window, Pause/Resume, Settings, Close
            match tray::setup_tray(&app.handle()) {
                Ok(_tray) => {
                    tracing::info!("System tray initialized successfully with menu");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to initialize system tray");
                }
            }

            // Setup window close handler for minimize to tray behavior
            // Requirement 16.4: Minimize to tray when closing window
            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Get the tray manager state to check config
                        if let Some(tray_state) = app_handle.try_state::<TrayManagerState>() {
                            let tray_state_clone = tray_state.0.clone();
                            let app_handle_clone = app_handle.clone();
                            
                            // Check if we should minimize to tray
                            let should_prevent = tauri::async_runtime::block_on(async {
                                let manager = tray_state_clone.read().await;
                                manager.should_minimize_to_tray().await
                            });
                            
                            if should_prevent {
                                // Prevent the close and hide to tray instead
                                api.prevent_close();
                                if let Err(e) = hide_to_tray(&app_handle_clone) {
                                    tracing::error!(error = %e, "Failed to hide window to tray on close");
                                } else {
                                    tracing::debug!("Window hidden to tray on close request");
                                }
                            }
                        }
                    }
                });
            }

            // Apply start minimized setting
            // Requirement 16.5: Option to start minimized
            if let Some(tray_state) = app.try_state::<TrayManagerState>() {
                let tray_state_clone = tray_state.0.clone();
                let app_handle = app.handle().clone();
                
                tauri::async_runtime::spawn(async move {
                    let manager = tray_state_clone.read().await;
                    if let Err(e) = manager.apply_start_minimized(&app_handle).await {
                        tracing::error!(error = %e, "Failed to apply start minimized setting");
                    }
                });
            }

            // Initialize the auto-updater
            // Requirement 17.1: Check for updates on startup and every 24 hours
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Get the updater state
                if let Some(updater_state) = app_handle.try_state::<UpdaterState>() {
                    let state = Arc::new(UpdaterState {
                        last_update_info: updater_state.last_update_info.clone(),
                        checker_running: updater_state.checker_running.clone(),
                        downloading: updater_state.downloading.clone(),
                        checker_handle: updater_state.checker_handle.clone(),
                    });

                    // Check for updates on startup
                    updater::check_on_startup(app_handle.clone(), state.clone()).await;

                    // Start periodic checker (every 24 hours)
                    updater::start_periodic_checker(app_handle, state).await;
                } else {
                    tracing::warn!("UpdaterState not available during setup");
                }
            });

            Ok(())
        });

    // Audio state management (only when audio feature enabled)
    #[cfg(feature = "audio")]
    {
        builder = builder
            // Manage the AudioEngine state as a singleton
            .manage(AudioEngineState::new())
            // Manage the AudioTestState as a singleton
            .manage(AudioTestStateWrapper::new());
    }

    builder = builder
        // Manage the Auth state as a singleton
        .manage(AuthState::new())
        // Manage the Config state as a singleton
        .manage(ConfigState::new())
        // Manage the Usage state as a singleton
        .manage(UsageState::default())
        // Manage the Tray state as a singleton
        .manage(TrayManagerState::new())
        // Manage the Updater state as a singleton
        .manage(UpdaterState::default());

    // Register handlers based on features
    #[cfg(feature = "audio")]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            // Audio commands
            enumerate_audio_devices,
            start_system_channel,
            start_user_channel,
            stop_channel,
            change_audio_device,
            get_audio_state,
            get_vbcable_status,
            get_virtual_audio_status,
            // Audio test commands
            play_audio_test,
            stop_audio_test,
            is_audio_test_playing,
            // Auth commands
            login_with_google,
            login_with_email,
            register_with_email,
            logout,
            get_session,
            restore_session,
            is_authenticated,
            start_session_expiration_checker,
            stop_session_expiration_checker,
            set_byok_key,
            get_byok_key_exists,
            delete_byok_key,
            validate_byok_key,
            validate_byok_key_full,
            // Config commands
            get_config,
            save_config,
            export_config,
            import_config,
            export_config_string,
            import_config_string,
            config_exists,
            reset_config,
            // Usage commands
            get_usage_stats,
            get_usage_history,
            can_start_translation,
            get_upgrade_options,
            check_usage_limits,
            emit_usage_blocked,
            set_user_plan,
            reset_usage_notifications,
            // Tray commands
            get_tray_state,
            set_tray_state,
            is_tray_paused,
            toggle_tray_pause,
            get_tray_config,
            set_tray_config,
            show_window_from_tray,
            hide_window_to_tray,
            // Updater commands
            check_for_updates_command,
            get_update_info,
            is_update_downloading,
            get_current_version,
            start_update_checker,
            stop_update_checker,
        ]);
    }

    #[cfg(not(feature = "audio"))]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            // Auth commands (no audio commands)
            login_with_google,
            login_with_email,
            register_with_email,
            logout,
            get_session,
            restore_session,
            is_authenticated,
            start_session_expiration_checker,
            stop_session_expiration_checker,
            set_byok_key,
            get_byok_key_exists,
            delete_byok_key,
            validate_byok_key,
            validate_byok_key_full,
            // Config commands
            get_config,
            save_config,
            export_config,
            import_config,
            export_config_string,
            import_config_string,
            config_exists,
            reset_config,
            // Usage commands
            get_usage_stats,
            get_usage_history,
            can_start_translation,
            get_upgrade_options,
            check_usage_limits,
            emit_usage_blocked,
            set_user_plan,
            reset_usage_notifications,
            // Tray commands
            get_tray_state,
            set_tray_state,
            is_tray_paused,
            toggle_tray_pause,
            get_tray_config,
            set_tray_config,
            show_window_from_tray,
            hide_window_to_tray,
            // Updater commands
            check_for_updates_command,
            get_update_info,
            is_update_downloading,
            get_current_version,
            start_update_checker,
            stop_update_checker,
        ]);
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
