//! Local storage module
//! SQLite + encrypted local storage for configuration, usage tracking, and auth sessions
//! 
//! This module provides:
//! - `database`: Core SQLite database with migration support
//! - `config`: Application configuration persistence
//! - `usage`: Usage tracking and sync
//! - `usage_notifications`: Usage limit notifications (80% warning, 100% block)
//! - `session`: Session management integrating in-memory and persistent storage

pub mod database;
pub mod config;
pub mod session;
pub mod usage;
pub mod usage_notifications;

// Re-export commonly used types from database
pub use database::{
    Database, 
    DatabaseError, 
    AuthSession, 
    UsageRecord, 
    Invoice,
    get_default_db_path,
    derive_encryption_key,
    CURRENT_SCHEMA_VERSION,
};

// Re-export config types and functions
pub use config::{
    AppConfig, 
    LanguageConfig, 
    DeviceConfig, 
    PreferencesConfig,
    ConfigError,
    save_config,
    load_config,
    restore_config_on_startup,
    config_exists,
    delete_config,
    get_config_version,
    export_config_to_file,
    import_config_from_file,
    export_config_to_string,
    import_config_from_string,
    CURRENT_CONFIG_VERSION,
};

// Re-export usage types
pub use usage::{
    UsageStats, 
    MonthlyUsage, 
    DailyUsage,
    UsageTracker,
    UsageError,
    Channel,
    ActiveSession,
    Plan,
    SyncResult,
    SyncState,
    UsageSyncClient,
};

// Re-export session management types and functions
pub use session::{
    SessionManager,
    SessionError,
    SessionExpirationEvent,
    SessionRestoredEvent,
    session_event_names,
    emit_session_restored,
    emit_session_cleared,
    generate_session_expiration,
    is_session_expired,
    get_seconds_until_expiry,
};

// Re-export usage notification types and functions
pub use usage_notifications::{
    UsageLimitNotifier,
    UsageWarningEvent,
    UsageLimitReachedEvent,
    UsageBlockedEvent,
    UpgradeOption,
    usage_event_names,
    emit_usage_warning,
    emit_usage_limit_reached,
    emit_usage_blocked,
};
