// Commands module - Tauri IPC commands
// This module contains all IPC commands that the frontend can call

#[cfg(feature = "audio")]
pub mod audio;
pub mod auth;
pub mod config;
pub mod tray;
pub mod usage;

#[cfg(test)]
mod ipc_tests;

#[cfg(feature = "audio")]
pub use audio::*;
pub use auth::*;
pub use config::{
    get_config,
    save_config,
    export_config,
    import_config,
    export_config_string,
    import_config_string,
    config_exists,
    reset_config,
    ConfigState,
    ConfigResponse,
    ConfigInput,
};
pub use tray::*;
pub use usage::*;
