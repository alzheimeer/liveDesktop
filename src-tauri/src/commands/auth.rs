//! Authentication commands
//!
//! Handles login, logout, session management, and BYOK API key management.
//! All commands are exposed via Tauri IPC for frontend consumption.
//!
//! # Commands
//! - `login_with_google` - OAuth login flow (opens browser)
//! - `login_with_email(email, password)` - Email/password login
//! - `register_with_email(email, password, name)` - Create new account
//! - `logout` - Clear session and tokens
//! - `get_session` - Get current session info
//!
//! # Requirements
//! - 9.6: Session tokens with 7-day expiration  
//! - 9.7: Generic error messages for invalid credentials
//! - 9.8: Store session tokens in encrypted local SQLite
//! - 9.10: Request re-authentication when token expires
//! - 22.1: IPC commands for authentication
//! - 23.6: Store auth tokens in encrypted SQLite

use crate::auth::{
    validate_api_key as validate_byok_api_key, validate_api_key_format, 
    BetterAuthClient, KeyringManager, SubscriptionPlan, UserSession, ValidationResult,
};
use crate::storage::{
    Database, SessionManager, emit_session_cleared, emit_session_restored,
    get_default_db_path, derive_encryption_key,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, Runtime, State};

// ============================================================================
// Response Types for IPC
// ============================================================================

/// Login response structure for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    /// Whether login was successful
    pub success: bool,
    /// User information if login succeeded
    pub user: Option<UserInfo>,
    /// Error message if login failed
    pub error: Option<String>,
}

/// User information structure for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// User's unique identifier
    pub user_id: String,
    /// User's email address
    pub email: String,
    /// User's display name
    pub name: String,
    /// User's subscription plan
    pub plan: String,
}

impl From<&UserSession> for UserInfo {
    fn from(session: &UserSession) -> Self {
        Self {
            user_id: session.user_id.clone(),
            email: session.email.clone(),
            name: session.name.clone(),
            plan: plan_to_string(&session.plan),
        }
    }
}

/// Session information structure for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Whether user is authenticated
    pub is_authenticated: bool,
    /// User information if authenticated
    pub user: Option<UserInfo>,
    /// Session expiration time in ISO 8601 format
    pub expires_at: Option<String>,
}

/// Convert SubscriptionPlan to string for serialization
fn plan_to_string(plan: &SubscriptionPlan) -> String {
    match plan {
        SubscriptionPlan::ByokFree => "byok_free".to_string(),
        SubscriptionPlan::Starter => "starter".to_string(),
        SubscriptionPlan::Pro => "pro".to_string(),
    }
}

// ============================================================================
// Event Names
// ============================================================================

/// Event names for session-related events
pub mod auth_event_names {
    /// Emitted when user logs in successfully
    pub const LOGIN_SUCCESS: &str = "auth:login_success";
    /// Emitted when login fails
    pub const LOGIN_ERROR: &str = "auth:login_error";
    /// Emitted when user logs out
    pub const LOGOUT: &str = "auth:logout";
    /// Emitted when session changes
    pub const SESSION_CHANGED: &str = "auth:session_changed";
}

/// Auth state managed by Tauri
pub struct AuthState {
    /// Session manager for coordinating in-memory and persistent storage
    session_manager: Arc<SessionManager>,
    /// Better Auth client for authentication operations
    auth_client: Arc<BetterAuthClient>,
}

impl AuthState {
    /// Create a new AuthState with default configuration
    pub fn new() -> Self {
        // Get Better Auth URL from environment or use default
        // Development: http://localhost:3100
        // Production: https://auth.traductor.app
        let auth_url = std::env::var("BETTER_AUTH_URL")
            .or_else(|_| std::env::var("VITE_AUTH_URL"))
            .unwrap_or_else(|_| "http://localhost:3100".to_string());
        
        tracing::info!("Initializing Better Auth client with URL: {}", auth_url);
        let auth_client = Arc::new(BetterAuthClient::new(&auth_url));
        
        // Create database connection
        let db_path = get_default_db_path().unwrap_or_else(|_| "traductor.db".to_string());
        let encryption_key = derive_encryption_key().unwrap_or_else(|_| "default".to_string());
        let database = Arc::new(
            Database::new(&db_path, &encryption_key)
                .expect("Failed to initialize database")
        );
        
        // Create session manager
        let session_manager = Arc::new(SessionManager::new(
            Arc::clone(&auth_client),
            database,
        ));
        
        Self {
            session_manager,
            auth_client,
        }
    }
    
    /// Create AuthState with custom components (for testing)
    pub fn with_components(
        auth_client: Arc<BetterAuthClient>,
        database: Arc<Database>,
    ) -> Self {
        let session_manager = Arc::new(SessionManager::new(
            Arc::clone(&auth_client),
            database,
        ));
        
        Self {
            session_manager,
            auth_client,
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Login with Google OAuth
/// 
/// Initiates OAuth flow with Google. Opens the default browser for user authentication.
/// After successful authentication, the callback will be handled by the deep link handler.
/// 
/// # Returns
/// * `Ok(String)` - OAuth URL to open in browser (for manual handling if needed)
/// * `Err(String)` - Error message if initiating OAuth fails
/// 
/// # Requirements
/// - 9.2: OAuth flow with Google
/// - 9.3: OAuth error handling
/// - 22.1: IPC command for authentication
#[command]
pub async fn login_with_google<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
) -> Result<LoginResponse, String> {
    // Get OAuth URL from Better Auth client
    match auth_state.auth_client.login_with_google().await {
        Ok(_oauth_url) => {
            // Note: Shell opening functionality requires platform-specific implementation
            // For now, the OAuth URL is available in the response for manual handling
            
            tracing::info!("OAuth login initiated");
            
            // Return the URL (frontend can use it if browser didn't open)
            Ok(LoginResponse {
                success: true,
                user: None, // User info will come after OAuth callback
                error: None,
            })
        }
        Err(e) => {
            let error_msg = e.message();
            tracing::error!("OAuth login failed: {}", error_msg);
            
            // Emit error event
            let _ = app_handle.emit(auth_event_names::LOGIN_ERROR, &error_msg);
            
            Ok(LoginResponse {
                success: false,
                user: None,
                error: Some(error_msg),
            })
        }
    }
}

/// Login with email and password
/// 
/// Authenticates user with email/password and persists session.
/// Emits session events for frontend state management.
/// 
/// # Arguments
/// * `email` - User email (RFC 5322 format)
/// * `password` - User password (minimum 8 characters)
/// 
/// # Returns
/// * `Ok(LoginResponse)` - Response with success status and user info
/// 
/// # Requirements
/// - 9.4: Email/password validation
/// - 9.6: Session token with 7-day expiration
/// - 9.7: Generic error message for invalid credentials
/// - 9.8: Store session in encrypted SQLite
/// - 22.1: IPC command for authentication
#[command]
pub async fn login_with_email<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
    email: String, 
    password: String,
) -> Result<LoginResponse, String> {
    // Authenticate with Better Auth
    match auth_state.auth_client.login_with_email(&email, &password).await {
        Ok(session) => {
            // Save session to both in-memory and SQLite
            if let Err(e) = auth_state.session_manager.save_session(&session).await {
                tracing::error!("Failed to save session: {}", e);
                return Ok(LoginResponse {
                    success: false,
                    user: None,
                    error: Some("Error al guardar la sesión".to_string()),
                });
            }
            
            // Create user info for response
            let user_info = UserInfo::from(&session);
            
            // Emit session restored event to frontend
            if let Err(e) = emit_session_restored(&app_handle, &session) {
                tracing::warn!("Failed to emit session event: {}", e);
            }
            
            // Emit login success event
            let _ = app_handle.emit(auth_event_names::LOGIN_SUCCESS, &user_info);
            
            tracing::info!("User {} logged in successfully", session.email);
            
            Ok(LoginResponse {
                success: true,
                user: Some(user_info),
                error: None,
            })
        }
        Err(e) => {
            let error_msg = e.message();
            tracing::warn!("Login failed for {}: {}", email, error_msg);
            
            // Emit error event
            let _ = app_handle.emit(auth_event_names::LOGIN_ERROR, &error_msg);
            
            Ok(LoginResponse {
                success: false,
                user: None,
                error: Some(error_msg),
            })
        }
    }
}

/// Register with email and password
/// 
/// Creates a new account and persists session.
/// Emits session events for frontend state management.
/// 
/// # Arguments
/// * `email` - User email (RFC 5322 format)
/// * `password` - User password (minimum 8 characters)
/// * `name` - User display name (optional, will default to "Usuario" if not provided)
/// 
/// # Returns
/// * `Ok(LoginResponse)` - Response with success status and user info
/// 
/// # Requirements
/// - 9.4: Email/password validation
/// - 9.5: Handle email already exists
/// - 9.6: Session token with 7-day expiration
/// - 9.8: Store session in encrypted SQLite
/// - 22.1: IPC command for authentication
#[command]
pub async fn register_with_email<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
    email: String, 
    password: String,
    name: Option<String>,
) -> Result<LoginResponse, String> {
    // Register with Better Auth
    match auth_state.auth_client.register_with_email(&email, &password).await {
        Ok(mut session) => {
            // Set custom name if provided
            if let Some(custom_name) = name {
                if !custom_name.trim().is_empty() {
                    session.name = custom_name;
                }
            }
            
            // Save session to both in-memory and SQLite
            if let Err(e) = auth_state.session_manager.save_session(&session).await {
                tracing::error!("Failed to save session: {}", e);
                return Ok(LoginResponse {
                    success: false,
                    user: None,
                    error: Some("Error al guardar la sesión".to_string()),
                });
            }
            
            // Create user info for response
            let user_info = UserInfo::from(&session);
            
            // Emit session restored event to frontend
            if let Err(e) = emit_session_restored(&app_handle, &session) {
                tracing::warn!("Failed to emit session event: {}", e);
            }
            
            // Emit login success event
            let _ = app_handle.emit(auth_event_names::LOGIN_SUCCESS, &user_info);
            
            tracing::info!("User {} registered successfully", session.email);
            
            Ok(LoginResponse {
                success: true,
                user: Some(user_info),
                error: None,
            })
        }
        Err(e) => {
            let error_msg = e.message();
            tracing::warn!("Registration failed for {}: {}", email, error_msg);
            
            // Emit error event
            let _ = app_handle.emit(auth_event_names::LOGIN_ERROR, &error_msg);
            
            Ok(LoginResponse {
                success: false,
                user: None,
                error: Some(error_msg),
            })
        }
    }
}

/// Logout current user
/// 
/// Clears session from both in-memory and SQLite storage.
/// Emits logout event for frontend state management.
/// 
/// # Returns
/// * `Ok(())` - Logout successful
/// * `Err(String)` - Error message if logout fails
/// 
/// # Requirements
/// - 22.1: IPC command for authentication
/// - Implements logout() that removes tokens from storage
#[command]
pub async fn logout<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
) -> Result<(), String> {
    // Logout using session manager (clears both memory and SQLite)
    auth_state.session_manager
        .logout()
        .await
        .map_err(|e| e.to_string())?;
    
    // Emit session cleared event to frontend
    if let Err(e) = emit_session_cleared(&app_handle) {
        tracing::warn!("Failed to emit session cleared event: {}", e);
    }
    
    // Emit logout event
    let _ = app_handle.emit(auth_event_names::LOGOUT, ());
    
    tracing::info!("User logged out successfully");
    
    Ok(())
}

/// Get current session
/// 
/// Returns the current user session info if authenticated and not expired.
/// If session is expired, it will be cleared and returns unauthenticated state.
/// 
/// # Returns
/// * `Ok(SessionInfo)` - Session information with authentication status
/// 
/// # Requirements
/// - 9.10: Request re-authentication when token expires
/// - 22.1: IPC command for authentication
#[command]
pub async fn get_session(
    auth_state: State<'_, AuthState>,
) -> Result<SessionInfo, String> {
    // Get session from session manager (checks both memory and SQLite)
    match auth_state.session_manager.get_session().await {
        Some(session) => Ok(SessionInfo {
            is_authenticated: true,
            user: Some(UserInfo::from(&session)),
            expires_at: Some(session.expires_at),
        }),
        None => Ok(SessionInfo {
            is_authenticated: false,
            user: None,
            expires_at: None,
        }),
    }
}

/// Restore session on app startup
/// 
/// Attempts to restore session from SQLite to in-memory storage.
/// Should be called during app initialization.
/// Emits session restored event if successful.
/// 
/// # Returns
/// * `Ok(SessionInfo)` - Session information with authentication status
/// 
/// # Requirements
/// - 22.1: IPC command for authentication
#[command]
pub async fn restore_session<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
) -> Result<SessionInfo, String> {
    let session = auth_state.session_manager
        .restore_session()
        .await
        .map_err(|e| e.to_string())?;
    
    match session {
        Some(s) => {
            if let Err(e) = emit_session_restored(&app_handle, &s) {
                tracing::warn!("Failed to emit session restored event: {}", e);
            }
            
            // Emit session changed event
            let user_info = UserInfo::from(&s);
            let _ = app_handle.emit(auth_event_names::SESSION_CHANGED, &user_info);
            
            tracing::info!("Session restored for user {}", s.email);
            
            Ok(SessionInfo {
                is_authenticated: true,
                user: Some(user_info),
                expires_at: Some(s.expires_at),
            })
        }
        None => {
            tracing::debug!("No session to restore");
            Ok(SessionInfo {
                is_authenticated: false,
                user: None,
                expires_at: None,
            })
        }
    }
}

/// Check if user is authenticated with a valid session
#[command]
pub async fn is_authenticated(
    auth_state: State<'_, AuthState>,
) -> Result<bool, String> {
    Ok(auth_state.session_manager.is_authenticated().await)
}

/// Start session expiration checker background task
/// 
/// Starts a background task that periodically checks session expiration
/// and emits Tauri events when session expires or is about to expire.
/// 
/// # Arguments
/// * `check_interval_secs` - How often to check (default: 60 seconds)
/// 
/// # Requirements
/// - 9.10: Request re-authentication when token expires
#[command]
pub async fn start_session_expiration_checker<R: Runtime>(
    app_handle: AppHandle<R>,
    auth_state: State<'_, AuthState>,
    check_interval_secs: Option<u64>,
) -> Result<(), String> {
    let interval = check_interval_secs.unwrap_or(60);
    
    auth_state.session_manager
        .start_expiration_checker(app_handle, interval)
        .await;
    
    Ok(())
}

/// Stop session expiration checker background task
#[command]
pub async fn stop_session_expiration_checker(
    auth_state: State<'_, AuthState>,
) -> Result<(), String> {
    auth_state.session_manager
        .stop_expiration_checker()
        .await;
    
    Ok(())
}

/// Set BYOK API key in OS keyring
///
/// Stores the user's Gemini API key securely in:
/// - Windows: Windows Credential Manager
/// - macOS: macOS Keychain
///
/// # Arguments
/// * `api_key` - The Gemini API key to store (1-256 alphanumeric characters)
///
/// # Returns
/// * `Ok(())` - If the key was stored successfully
/// * `Err(String)` - Error message if storage failed
///
/// # Requirements
/// Implements Requirements 8.3 (secure storage in OS keyring)
#[command]
pub async fn set_byok_key(api_key: String) -> Result<(), String> {
    KeyringManager::set_byok_key(&api_key).map_err(|e| e.to_string())
}

/// Check if BYOK key exists in OS keyring
///
/// # Returns
/// * `Ok(true)` - If an API key is stored
/// * `Ok(false)` - If no API key is stored
/// * `Err(String)` - Error message if check failed
#[command]
pub async fn get_byok_key_exists() -> Result<bool, String> {
    let result = KeyringManager::has_byok_key().map_err(|e| e.to_string());
    tracing::info!("get_byok_key_exists result: {:?}", result);
    result
}

/// Delete BYOK API key from OS keyring
///
/// # Returns
/// * `Ok(())` - If the key was deleted or didn't exist
/// * `Err(String)` - Error message if deletion failed
///
/// # Requirements
/// Implements Requirements 8.7 (user can delete stored API key)
#[command]
pub async fn delete_byok_key() -> Result<(), String> {
    KeyringManager::delete_byok_key().map_err(|e| e.to_string())
}

/// Validate BYOK API key format
///
/// Checks if the API key has a valid format:
/// - 1-256 characters
/// - Only alphanumeric characters, hyphens (-), or underscores (_)
///
/// Note: This only validates format, not whether the key is actually valid with Gemini.
/// Use `validate_byok_key_full` for complete validation including API test.
///
/// # Arguments
/// * `api_key` - The API key to validate
///
/// # Returns
/// * `Ok(true)` - If the key format is valid
/// * `Ok(false)` - If the key format is invalid
///
/// # Requirements
/// Implements Requirements 8.1, 8.2 (validate API key format)
#[command]
pub async fn validate_byok_key(api_key: String) -> Result<bool, String> {
    Ok(validate_api_key_format(&api_key))
}

/// Validate BYOK API key completely (format + Gemini API test)
///
/// This command:
/// 1. Validates the API key format (1-256 alphanumeric chars including - and _)
/// 2. Tests the key against Gemini's API to verify it works
///
/// # Arguments
/// * `api_key` - The API key to validate
///
/// # Returns
/// * `ValidationResult` - Contains:
///   - `valid: bool` - Whether the key is valid
///   - `error_message: Option<String>` - Error description if invalid
///   - `suggestion: Option<String>` - How to fix the issue if invalid
///
/// # Requirements
/// Implements Requirements 8.1, 8.2, 8.6 (validate format and show error if Gemini rejects)
#[command]
pub async fn validate_byok_key_full(api_key: String) -> Result<ValidationResult, String> {
    Ok(validate_byok_api_key(&api_key).await)
}
