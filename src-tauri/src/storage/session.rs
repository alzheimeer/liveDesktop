//! Session management module
//!
//! Integrates in-memory session management (BetterAuthClient) with SQLite persistence.
//! Handles session lifecycle: save, restore, expire, and logout.
//!
//! # Requirements
//! - 9.6: Session tokens with 7-day expiration
//! - 9.7: Generic error messages for invalid credentials
//! - 9.8: Store session tokens in encrypted local SQLite
//! - 9.10: Request re-authentication when token expires
//! - 23.6: Store auth tokens in encrypted SQLite

use crate::auth::{BetterAuthClient, BetterAuthError, SubscriptionPlan, UserSession};
use crate::storage::{AuthSession, Database, DatabaseError};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use thiserror::Error;
use tokio::sync::RwLock;

/// Session management error types
#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Auth error: {0}")]
    Auth(#[from] BetterAuthError),

    #[error("Session expired")]
    SessionExpired,

    #[error("No active session")]
    NoSession,

    #[error("Failed to parse expiration date: {0}")]
    InvalidExpiration(String),
}

/// Session expiration event names
pub mod session_event_names {
    pub const SESSION_EXPIRED: &str = "session:expired";
    pub const SESSION_EXPIRING_SOON: &str = "session:expiring_soon";
    pub const SESSION_RESTORED: &str = "session:restored";
    pub const SESSION_CLEARED: &str = "session:cleared";
}

/// Session expiration event payload
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionExpirationEvent {
    /// Whether the session has expired
    pub expired: bool,
    /// Time until expiration in seconds (negative if already expired)
    pub seconds_until_expiry: i64,
    /// Human-readable message in Spanish
    pub message: String,
}

/// Session restored event payload
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRestoredEvent {
    /// User email
    pub email: String,
    /// User display name
    pub name: String,
    /// Subscription plan
    pub plan: String,
    /// Expiration time in ISO 8601 format
    pub expires_at: String,
}

/// Session Manager that coordinates in-memory and persistent storage
///
/// Responsibilities:
/// - On login: save session to both memory (BetterAuthClient) AND SQLite
/// - On app start: restore session from SQLite to memory
/// - On logout: clear from both memory AND SQLite  
/// - Check expiration and emit Tauri events when session expires
pub struct SessionManager {
    /// Better Auth client for in-memory session management
    auth_client: Arc<BetterAuthClient>,
    /// Database for SQLite persistence
    database: Arc<Database>,
    /// Background task running flag
    expiration_checker_running: Arc<RwLock<bool>>,
}

impl SessionManager {
    /// Create a new SessionManager
    ///
    /// # Arguments
    /// * `auth_client` - BetterAuthClient for in-memory session management
    /// * `database` - Database for SQLite persistence
    pub fn new(auth_client: Arc<BetterAuthClient>, database: Arc<Database>) -> Self {
        Self {
            auth_client,
            database,
            expiration_checker_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Save session to both in-memory (BetterAuthClient) and SQLite
    ///
    /// This should be called after successful login to persist the session.
    ///
    /// # Arguments
    /// * `session` - The UserSession to save
    ///
    /// # Requirements
    /// - 9.6: Session token with 7-day expiration
    /// - 9.8: Store in SQLite local encrypted
    /// - 23.6: Store auth tokens in encrypted SQLite
    pub async fn save_session(&self, session: &UserSession) -> Result<(), SessionError> {
        // Save to in-memory (BetterAuthClient)
        self.auth_client.set_session(session.clone()).await;

        // Convert UserSession to AuthSession for database storage
        let auth_session = AuthSession {
            session_token: Some(session.session_token.clone()),
            user_id: Some(session.user_id.clone()),
            email: Some(session.email.clone()),
            name: Some(session.name.clone()),
            plan: Some(plan_to_string(&session.plan)),
            expires_at: Some(session.expires_at.clone()),
        };

        // Save to SQLite
        self.database.save_auth_session(&auth_session)?;

        tracing::info!(
            "Session saved for user {} (expires: {})",
            session.email,
            session.expires_at
        );

        Ok(())
    }

    /// Restore session from SQLite to in-memory (BetterAuthClient)
    ///
    /// This should be called on app start to restore the previous session.
    /// If the session is expired, it returns None and clears the stored session.
    ///
    /// # Returns
    /// - `Ok(Some(UserSession))` if a valid session was restored
    /// - `Ok(None)` if no session exists or it's expired
    /// - `Err(SessionError)` on database error
    ///
    /// # Requirements
    /// - 9.10: Request re-authentication when token expires
    pub async fn restore_session(&self) -> Result<Option<UserSession>, SessionError> {
        // Get session from SQLite
        let auth_session = match self.database.get_auth_session()? {
            Some(session) => session,
            None => {
                tracing::debug!("No stored session found");
                return Ok(None);
            }
        };

        // Convert AuthSession to UserSession
        let user_session = match auth_session_to_user_session(&auth_session) {
            Some(session) => session,
            None => {
                tracing::warn!("Stored session has missing fields, clearing");
                self.database.clear_auth_session()?;
                return Ok(None);
            }
        };

        // Check if expired
        if user_session.is_expired() {
            tracing::info!(
                "Stored session for {} has expired, clearing",
                user_session.email
            );
            self.database.clear_auth_session()?;
            return Ok(None);
        }

        // Restore to in-memory (BetterAuthClient)
        self.auth_client.set_session(user_session.clone()).await;

        tracing::info!(
            "Session restored for user {} (expires: {})",
            user_session.email,
            user_session.expires_at
        );

        Ok(Some(user_session))
    }

    /// Logout and clear session from both in-memory and SQLite
    ///
    /// # Requirements
    /// - Implements logout() that removes tokens from storage
    pub async fn logout(&self) -> Result<(), SessionError> {
        // Get current session email for logging
        let email = self
            .auth_client
            .get_session()
            .await
            .map(|s| s.email)
            .unwrap_or_else(|| "unknown".to_string());

        // Clear from in-memory (BetterAuthClient)
        self.auth_client.logout().await?;

        // Clear from SQLite
        self.database.clear_auth_session()?;

        tracing::info!("Session cleared for user {}", email);

        Ok(())
    }

    /// Get current session if available and not expired
    ///
    /// Checks in-memory session first, falls back to database if needed.
    pub async fn get_session(&self) -> Option<UserSession> {
        // First try in-memory
        if let Some(session) = self.auth_client.get_session().await {
            if !session.is_expired() {
                return Some(session);
            }
        }

        // Try to restore from database
        if let Ok(Some(session)) = self.restore_session().await {
            return Some(session);
        }

        None
    }

    /// Check if user is authenticated with a valid (non-expired) session
    pub async fn is_authenticated(&self) -> bool {
        self.get_session().await.is_some()
    }

    /// Check session expiration and return status
    ///
    /// # Returns
    /// - `Ok(seconds_until_expiry)` with positive value if session is valid
    /// - `Err(SessionError::SessionExpired)` if session has expired
    /// - `Err(SessionError::NoSession)` if no session exists
    pub async fn check_expiration(&self) -> Result<i64, SessionError> {
        let session = self.get_session().await.ok_or(SessionError::NoSession)?;

        let expires_at = DateTime::parse_from_rfc3339(&session.expires_at)
            .map_err(|e| SessionError::InvalidExpiration(e.to_string()))?;

        let now = Utc::now();
        let duration = expires_at.with_timezone(&Utc) - now;
        let seconds = duration.num_seconds();

        if seconds <= 0 {
            Err(SessionError::SessionExpired)
        } else {
            Ok(seconds)
        }
    }

    /// Start background task that checks session expiration periodically
    ///
    /// Emits Tauri events when:
    /// - Session is about to expire (24 hours before)
    /// - Session has expired
    ///
    /// # Arguments
    /// * `app_handle` - Tauri app handle for emitting events
    /// * `check_interval_secs` - How often to check (default: 60 seconds)
    ///
    /// # Requirements
    /// - 9.10: Request re-authentication when token expires
    pub async fn start_expiration_checker<R: Runtime>(
        &self,
        app_handle: AppHandle<R>,
        check_interval_secs: u64,
    ) {
        // Check if already running
        {
            let mut running = self.expiration_checker_running.write().await;
            if *running {
                tracing::warn!("Expiration checker already running");
                return;
            }
            *running = true;
        }

        let auth_client = Arc::clone(&self.auth_client);
        let database = Arc::clone(&self.database);
        let running_flag = Arc::clone(&self.expiration_checker_running);

        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(check_interval_secs)
            );

            // Flag to track if we've already notified about "expiring soon"
            let mut expiring_soon_notified = false;

            loop {
                interval.tick().await;

                // Check if we should stop
                {
                    let running = running_flag.read().await;
                    if !*running {
                        tracing::debug!("Expiration checker stopping");
                        break;
                    }
                }

                // Get current session
                let session = match auth_client.get_session().await {
                    Some(s) => s,
                    None => {
                        expiring_soon_notified = false;
                        continue;
                    }
                };

                // Calculate time until expiration
                let expires_at = match DateTime::parse_from_rfc3339(&session.expires_at) {
                    Ok(dt) => dt.with_timezone(&Utc),
                    Err(_) => continue,
                };

                let now = Utc::now();
                let seconds_until_expiry = (expires_at - now).num_seconds();

                // Check if expired
                if seconds_until_expiry <= 0 {
                    tracing::info!("Session expired for user {}", session.email);

                    // Clear session from both memory and database
                    auth_client.logout().await.ok();
                    database.clear_auth_session().ok();

                    // Emit session expired event
                    let event = SessionExpirationEvent {
                        expired: true,
                        seconds_until_expiry,
                        message: "Tu sesión ha expirado. Por favor inicia sesión nuevamente."
                            .to_string(),
                    };

                    if let Err(e) = app_handle.emit(session_event_names::SESSION_EXPIRED, &event) {
                        tracing::error!("Failed to emit session expired event: {}", e);
                    }

                    expiring_soon_notified = false;
                    continue;
                }

                // Check if expiring soon (within 24 hours = 86400 seconds)
                let hours_until_expiry = seconds_until_expiry / 3600;
                if hours_until_expiry <= 24 && !expiring_soon_notified {
                    tracing::info!(
                        "Session expiring soon for user {} ({} hours remaining)",
                        session.email,
                        hours_until_expiry
                    );

                    let event = SessionExpirationEvent {
                        expired: false,
                        seconds_until_expiry,
                        message: format!(
                            "Tu sesión expirará en {} horas. Considera iniciar sesión nuevamente.",
                            hours_until_expiry
                        ),
                    };

                    if let Err(e) =
                        app_handle.emit(session_event_names::SESSION_EXPIRING_SOON, &event)
                    {
                        tracing::error!("Failed to emit session expiring soon event: {}", e);
                    }

                    expiring_soon_notified = true;
                }
            }
        });

        tracing::info!(
            "Session expiration checker started (interval: {}s)",
            check_interval_secs
        );
    }

    /// Stop the background expiration checker
    pub async fn stop_expiration_checker(&self) {
        let mut running = self.expiration_checker_running.write().await;
        *running = false;
        tracing::info!("Session expiration checker stopped");
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert SubscriptionPlan to string for database storage
fn plan_to_string(plan: &SubscriptionPlan) -> String {
    match plan {
        SubscriptionPlan::ByokFree => "byok_free".to_string(),
        SubscriptionPlan::Starter => "starter".to_string(),
        SubscriptionPlan::Pro => "pro".to_string(),
    }
}

/// Convert string from database to SubscriptionPlan
fn string_to_plan(plan_str: &str) -> SubscriptionPlan {
    match plan_str.to_lowercase().as_str() {
        "starter" => SubscriptionPlan::Starter,
        "pro" => SubscriptionPlan::Pro,
        _ => SubscriptionPlan::ByokFree,
    }
}

/// Convert AuthSession (from database) to UserSession
fn auth_session_to_user_session(auth_session: &AuthSession) -> Option<UserSession> {
    Some(UserSession {
        user_id: auth_session.user_id.clone()?,
        email: auth_session.email.clone()?,
        name: auth_session.name.clone().unwrap_or_else(|| "Usuario".to_string()),
        avatar_url: None, // Not stored in database
        plan: string_to_plan(auth_session.plan.as_deref().unwrap_or("byok_free")),
        session_token: auth_session.session_token.clone()?,
        expires_at: auth_session.expires_at.clone()?,
    })
}

/// Emit session restored event to frontend
pub fn emit_session_restored<R: Runtime>(
    app_handle: &AppHandle<R>,
    session: &UserSession,
) -> Result<(), tauri::Error> {
    let event = SessionRestoredEvent {
        email: session.email.clone(),
        name: session.name.clone(),
        plan: plan_to_string(&session.plan),
        expires_at: session.expires_at.clone(),
    };

    app_handle.emit(session_event_names::SESSION_RESTORED, &event)
}

/// Emit session cleared event to frontend
pub fn emit_session_cleared<R: Runtime>(app_handle: &AppHandle<R>) -> Result<(), tauri::Error> {
    app_handle.emit(session_event_names::SESSION_CLEARED, ())
}

/// Generate a session token with 7-day expiration
///
/// Note: The actual session token comes from Better Auth server.
/// This function is used to ensure the expiration time is set correctly
/// if the server doesn't provide one.
///
/// # Requirements
/// - 9.6: Session tokens with 7-day expiration
pub fn generate_session_expiration() -> String {
    let expiry = Utc::now() + Duration::days(7);
    expiry.to_rfc3339()
}

/// Check if a session token expiration string represents an expired session
pub fn is_session_expired(expires_at: &str) -> bool {
    match DateTime::parse_from_rfc3339(expires_at) {
        Ok(dt) => Utc::now() >= dt.with_timezone(&Utc),
        Err(_) => true, // Invalid date format is treated as expired
    }
}

/// Get remaining time until session expires in seconds
pub fn get_seconds_until_expiry(expires_at: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(expires_at)
        .ok()
        .map(|dt| (dt.with_timezone(&Utc) - Utc::now()).num_seconds())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_conversion_roundtrip() {
        let plans = vec![
            SubscriptionPlan::ByokFree,
            SubscriptionPlan::Starter,
            SubscriptionPlan::Pro,
        ];

        for plan in plans {
            let plan_str = plan_to_string(&plan);
            let converted = string_to_plan(&plan_str);
            assert_eq!(plan, converted);
        }
    }

    #[test]
    fn test_string_to_plan_case_insensitive() {
        assert_eq!(string_to_plan("STARTER"), SubscriptionPlan::Starter);
        assert_eq!(string_to_plan("Starter"), SubscriptionPlan::Starter);
        assert_eq!(string_to_plan("starter"), SubscriptionPlan::Starter);
        assert_eq!(string_to_plan("PRO"), SubscriptionPlan::Pro);
        assert_eq!(string_to_plan("unknown"), SubscriptionPlan::ByokFree);
    }

    #[test]
    fn test_generate_session_expiration() {
        let expires_at = generate_session_expiration();
        let dt = DateTime::parse_from_rfc3339(&expires_at).expect("Valid RFC3339 format");

        // Should be approximately 7 days from now (within 1 second tolerance)
        let expected = Utc::now() + Duration::days(7);
        let diff = (dt.with_timezone(&Utc) - expected).num_seconds().abs();
        assert!(diff <= 1, "Expiration should be 7 days from now");
    }

    #[test]
    fn test_is_session_expired() {
        // Expired session (1 day ago)
        let expired = (Utc::now() - Duration::days(1)).to_rfc3339();
        assert!(is_session_expired(&expired));

        // Valid session (7 days in future)
        let valid = (Utc::now() + Duration::days(7)).to_rfc3339();
        assert!(!is_session_expired(&valid));

        // Invalid format
        assert!(is_session_expired("invalid-date"));
    }

    #[test]
    fn test_get_seconds_until_expiry() {
        // Future expiration
        let future = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let seconds = get_seconds_until_expiry(&future).unwrap();
        assert!(seconds > 3500 && seconds <= 3600); // ~1 hour

        // Past expiration
        let past = (Utc::now() - Duration::hours(1)).to_rfc3339();
        let seconds = get_seconds_until_expiry(&past).unwrap();
        assert!(seconds < 0);

        // Invalid format
        assert!(get_seconds_until_expiry("invalid").is_none());
    }

    #[test]
    fn test_auth_session_to_user_session() {
        let auth_session = AuthSession {
            session_token: Some("test_token".to_string()),
            user_id: Some("user_123".to_string()),
            email: Some("test@example.com".to_string()),
            name: Some("Test User".to_string()),
            plan: Some("starter".to_string()),
            expires_at: Some("2025-12-31T23:59:59Z".to_string()),
        };

        let user_session = auth_session_to_user_session(&auth_session).unwrap();
        assert_eq!(user_session.user_id, "user_123");
        assert_eq!(user_session.email, "test@example.com");
        assert_eq!(user_session.name, "Test User");
        assert_eq!(user_session.plan, SubscriptionPlan::Starter);
        assert_eq!(user_session.session_token, "test_token");
    }

    #[test]
    fn test_auth_session_to_user_session_missing_fields() {
        let auth_session = AuthSession {
            session_token: None, // Missing required field
            user_id: Some("user_123".to_string()),
            email: Some("test@example.com".to_string()),
            name: None,
            plan: None,
            expires_at: Some("2025-12-31T23:59:59Z".to_string()),
        };

        let result = auth_session_to_user_session(&auth_session);
        assert!(result.is_none());
    }

    #[test]
    fn test_auth_session_to_user_session_default_name() {
        let auth_session = AuthSession {
            session_token: Some("test_token".to_string()),
            user_id: Some("user_123".to_string()),
            email: Some("test@example.com".to_string()),
            name: None, // Will use default
            plan: None, // Will use default (byok_free)
            expires_at: Some("2025-12-31T23:59:59Z".to_string()),
        };

        let user_session = auth_session_to_user_session(&auth_session).unwrap();
        assert_eq!(user_session.name, "Usuario");
        assert_eq!(user_session.plan, SubscriptionPlan::ByokFree);
    }
}
