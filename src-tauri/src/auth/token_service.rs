//! Token Service client
//!
//! Handles ephemeral token generation for Gemini Live API.
//! Tokens are stored ONLY in memory and never persisted to disk (Requirement 7.3).
//!
//! # Token Renewal Manager
//!
//! The `TokenRenewalManager` provides automatic token renewal with:
//! - Background monitoring of token expiration (Requirement 7.2)
//! - Renewal when 10 minutes remaining before expiration
//! - Retry logic: 3 attempts with 30-second intervals (Requirement 7.4)
//! - Pause translation if renewal fails after all attempts
//! - Automatic retry when network connection is restored (Requirement 7.6)

use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Ephemeral token for Gemini Live API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralToken {
    /// The token string to use for Gemini authentication
    pub token: String,
    /// Expiration time in ISO 8601 format
    pub expires_at: String,
}

impl EphemeralToken {
    /// Create a new ephemeral token with 1 hour TTL
    pub fn new(token: String) -> Self {
        let expires_at = Utc::now() + Duration::hours(1);
        Self {
            token,
            expires_at: expires_at.to_rfc3339(),
        }
    }

    /// Create token with specific expiration time
    pub fn with_expiration(token: String, expires_at: DateTime<Utc>) -> Self {
        Self {
            token,
            expires_at: expires_at.to_rfc3339(),
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        match DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(expires) => Utc::now() >= expires.with_timezone(&Utc),
            Err(_) => true, // If we can't parse, treat as expired
        }
    }

    /// Check if token will expire within the given duration
    pub fn expires_within(&self, duration: Duration) -> bool {
        match DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(expires) => Utc::now() + duration >= expires.with_timezone(&Utc),
            Err(_) => true,
        }
    }

    /// Get time until expiration (returns None if already expired)
    pub fn time_until_expiry(&self) -> Option<Duration> {
        match DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(expires) => {
                let expires_utc = expires.with_timezone(&Utc);
                let now = Utc::now();
                if now >= expires_utc {
                    None
                } else {
                    Some(expires_utc - now)
                }
            }
            Err(_) => None,
        }
    }
}

/// Error types for Token Service operations (3xxx codes - Auth category)
#[derive(Debug, Clone)]
pub enum TokenServiceError {
    /// Network error connecting to token service (3001)
    NetworkError { reason: String },
    /// Invalid session token (3002)
    InvalidSession,
    /// Subscription required to get tokens (3003)
    SubscriptionRequired,
    /// Rate limited by token service (3004)
    RateLimited { retry_after_secs: u32 },
    /// Token service returned invalid response (3005)
    InvalidResponse { details: String },
    /// Token has expired (3006)
    TokenExpired,
    /// Request timeout (3007)
    RequestTimeout,
}

impl TokenServiceError {
    /// Get the error code
    pub fn code(&self) -> u32 {
        match self {
            TokenServiceError::NetworkError { .. } => 3001,
            TokenServiceError::InvalidSession => 3002,
            TokenServiceError::SubscriptionRequired => 3003,
            TokenServiceError::RateLimited { .. } => 3004,
            TokenServiceError::InvalidResponse { .. } => 3005,
            TokenServiceError::TokenExpired => 3006,
            TokenServiceError::RequestTimeout => 3007,
        }
    }

    /// Get user-friendly message in Spanish
    pub fn message(&self) -> String {
        match self {
            TokenServiceError::NetworkError { reason } => {
                format!("Error de red al conectar con el servicio de tokens: {}", reason)
            }
            TokenServiceError::InvalidSession => {
                "Tu sesión ha expirado o es inválida".to_string()
            }
            TokenServiceError::SubscriptionRequired => {
                "Se requiere una suscripción activa para usar esta función".to_string()
            }
            TokenServiceError::RateLimited { retry_after_secs } => {
                format!("Demasiadas solicitudes. Por favor espera {} segundos", retry_after_secs)
            }
            TokenServiceError::InvalidResponse { details } => {
                format!("El servidor de tokens devolvió una respuesta inválida: {}", details)
            }
            TokenServiceError::TokenExpired => {
                "Tu token de acceso ha expirado".to_string()
            }
            TokenServiceError::RequestTimeout => {
                "La solicitud al servicio de tokens excedió el tiempo límite".to_string()
            }
        }
    }

    /// Get recovery suggestion in Spanish
    pub fn suggestion(&self) -> &'static str {
        match self {
            TokenServiceError::NetworkError { .. } => {
                "Verifica tu conexión a internet e intenta nuevamente"
            }
            TokenServiceError::InvalidSession => {
                "Cierra sesión e inicia sesión nuevamente"
            }
            TokenServiceError::SubscriptionRequired => {
                "Actualiza a un plan de suscripción o configura tu propia API key (BYOK)"
            }
            TokenServiceError::RateLimited { .. } => {
                "Espera unos momentos antes de intentar nuevamente"
            }
            TokenServiceError::InvalidResponse { .. } => {
                "Intenta nuevamente. Si el problema persiste, contacta soporte"
            }
            TokenServiceError::TokenExpired => {
                "Se renovará automáticamente. Si persiste, inicia sesión nuevamente"
            }
            TokenServiceError::RequestTimeout => {
                "Verifica tu conexión e intenta nuevamente"
            }
        }
    }
}

impl std::fmt::Display for TokenServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for TokenServiceError {}

/// Response from POST /tokens/ephemeral endpoint
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EphemeralTokenResponse {
    Success {
        success: bool,
        token: String,
        #[serde(rename = "expiresAt")]
        expires_at: String,
    },
    Error {
        success: bool,
        error: String,
    },
}

/// Token Service client for managing ephemeral tokens
///
/// This client handles communication with the Token Service backend
/// to obtain ephemeral tokens for Gemini Live API access.
///
/// # Security
/// - Tokens are stored ONLY in memory, never persisted to disk (Requirement 7.3)
/// - Tokens have 1-hour TTL (Requirement 7.1)
///
/// # Example
/// ```ignore
/// let client = TokenServiceClient::new("https://api.traductor.app");
/// let token = client.get_ephemeral_token("session_token").await?;
/// ```
pub struct TokenServiceClient {
    /// Base URL of the Token Service (e.g., "https://api.traductor.app")
    base_url: String,
    /// HTTP client for making requests
    http_client: Client,
    /// Current ephemeral token stored in memory only (Requirement 7.3)
    current_token: Arc<RwLock<Option<EphemeralToken>>>,
}

impl TokenServiceClient {
    /// Create a new Token Service client
    ///
    /// # Arguments
    /// * `base_url` - The base URL of the Token Service API
    pub fn new(base_url: &str) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
            current_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create client with custom HTTP client (for testing)
    pub fn with_client(base_url: &str, http_client: Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
            current_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get ephemeral token for Gemini Live API
    ///
    /// Calls POST /tokens/ephemeral to obtain a new token with 1-hour validity.
    /// The token is stored only in memory, never persisted to disk.
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    ///
    /// # Returns
    /// * `Ok(EphemeralToken)` - Token valid for 1 hour
    /// * `Err(TokenServiceError)` - If request fails
    ///
    /// # Requirements
    /// - Validates: Requirements 7.1, 7.3
    pub async fn get_ephemeral_token(
        &self,
        session_token: &str,
    ) -> Result<EphemeralToken, TokenServiceError> {
        let url = format!("{}/tokens/ephemeral", self.base_url);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", session_token))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    TokenServiceError::RequestTimeout
                } else {
                    TokenServiceError::NetworkError {
                        reason: e.to_string(),
                    }
                }
            })?;

        let status = response.status();

        // Handle HTTP status codes
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(TokenServiceError::InvalidSession);
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(60);
            return Err(TokenServiceError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        let body = response.text().await.map_err(|e| {
            TokenServiceError::InvalidResponse {
                details: e.to_string(),
            }
        })?;

        let token_response: EphemeralTokenResponse =
            serde_json::from_str(&body).map_err(|e| TokenServiceError::InvalidResponse {
                details: format!("Failed to parse response: {}", e),
            })?;

        match token_response {
            EphemeralTokenResponse::Success {
                success: true,
                token,
                expires_at,
            } => {
                let ephemeral_token = match DateTime::parse_from_rfc3339(&expires_at) {
                    Ok(dt) => EphemeralToken::with_expiration(token, dt.with_timezone(&Utc)),
                    Err(_) => {
                        // Fallback: create token with 1-hour TTL if expiration parse fails
                        EphemeralToken::new(token)
                    }
                };

                // Store in memory only (Requirement 7.3)
                let mut current = self.current_token.write().await;
                *current = Some(ephemeral_token.clone());

                Ok(ephemeral_token)
            }
            EphemeralTokenResponse::Success { success: false, .. }
            | EphemeralTokenResponse::Error { success: false, .. } => {
                // Extract error message if available
                if let EphemeralTokenResponse::Error { error, .. } = token_response {
                    match error.as_str() {
                        "subscription_required" => Err(TokenServiceError::SubscriptionRequired),
                        "invalid_session" => Err(TokenServiceError::InvalidSession),
                        "rate_limited" => Err(TokenServiceError::RateLimited {
                            retry_after_secs: 60,
                        }),
                        _ => Err(TokenServiceError::InvalidResponse { details: error }),
                    }
                } else {
                    Err(TokenServiceError::InvalidResponse {
                        details: "Unknown error".to_string(),
                    })
                }
            }
            _ => Err(TokenServiceError::InvalidResponse {
                details: "Unexpected response format".to_string(),
            }),
        }
    }

    /// Get current token if valid, or None if expired/not available
    ///
    /// This method returns the in-memory token without making a network request.
    pub async fn get_current_token(&self) -> Option<EphemeralToken> {
        let token = self.current_token.read().await;
        token.as_ref().and_then(|t| {
            if t.is_expired() {
                None
            } else {
                Some(t.clone())
            }
        })
    }

    /// Check if token needs refresh (expires within 10 minutes)
    ///
    /// Used for proactive token renewal before expiration (Requirement 7.2).
    pub async fn needs_refresh(&self) -> bool {
        let token = self.current_token.read().await;
        match token.as_ref() {
            Some(t) => t.is_expired() || t.expires_within(Duration::minutes(10)),
            None => true,
        }
    }

    /// Clear the current token from memory
    ///
    /// Call this on logout or when session becomes invalid.
    pub async fn clear_token(&self) {
        let mut token = self.current_token.write().await;
        *token = None;
    }

    /// Get a valid token, fetching a new one if needed
    ///
    /// This is a convenience method that:
    /// 1. Returns current token if still valid
    /// 2. Fetches a new token if expired or not available
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    pub async fn get_valid_token(
        &self,
        session_token: &str,
    ) -> Result<EphemeralToken, TokenServiceError> {
        if let Some(token) = self.get_current_token().await {
            return Ok(token);
        }
        self.get_ephemeral_token(session_token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ephemeral_token_new_has_1_hour_ttl() {
        let token = EphemeralToken::new("test_token".to_string());

        let expires = DateTime::parse_from_rfc3339(&token.expires_at).unwrap();
        let now = Utc::now();
        let diff = expires.with_timezone(&Utc) - now;

        // Should be approximately 1 hour (allow 5 seconds tolerance)
        assert!(diff.num_seconds() >= 3595);
        assert!(diff.num_seconds() <= 3605);
    }

    #[test]
    fn test_ephemeral_token_is_expired() {
        // Token that expired an hour ago
        let past = Utc::now() - Duration::hours(1);
        let token = EphemeralToken::with_expiration("test".to_string(), past);
        assert!(token.is_expired());

        // Token that expires in an hour
        let future = Utc::now() + Duration::hours(1);
        let token = EphemeralToken::with_expiration("test".to_string(), future);
        assert!(!token.is_expired());
    }

    #[test]
    fn test_ephemeral_token_expires_within() {
        let expires_in_5_min = Utc::now() + Duration::minutes(5);
        let token = EphemeralToken::with_expiration("test".to_string(), expires_in_5_min);

        // Should expire within 10 minutes
        assert!(token.expires_within(Duration::minutes(10)));

        // Should not expire within 1 minute
        assert!(!token.expires_within(Duration::minutes(1)));
    }

    #[test]
    fn test_ephemeral_token_time_until_expiry() {
        let expires_in_30_min = Utc::now() + Duration::minutes(30);
        let token = EphemeralToken::with_expiration("test".to_string(), expires_in_30_min);

        let time = token.time_until_expiry().unwrap();
        assert!(time.num_minutes() >= 29);
        assert!(time.num_minutes() <= 30);

        // Expired token should return None
        let expired = EphemeralToken::with_expiration("test".to_string(), Utc::now() - Duration::hours(1));
        assert!(expired.time_until_expiry().is_none());
    }

    #[test]
    fn test_token_service_error_codes_in_3xxx_range() {
        let errors = vec![
            TokenServiceError::NetworkError { reason: "test".to_string() },
            TokenServiceError::InvalidSession,
            TokenServiceError::SubscriptionRequired,
            TokenServiceError::RateLimited { retry_after_secs: 60 },
            TokenServiceError::InvalidResponse { details: "test".to_string() },
            TokenServiceError::TokenExpired,
            TokenServiceError::RequestTimeout,
        ];

        for err in errors {
            let code = err.code();
            assert!(
                code >= 3000 && code < 4000,
                "Token service error code {} not in 3xxx range",
                code
            );
        }
    }

    #[test]
    fn test_token_service_error_has_message_and_suggestion() {
        let err = TokenServiceError::SubscriptionRequired;

        let message = err.message();
        assert!(message.contains("suscripción"));

        let suggestion = err.suggestion();
        assert!(!suggestion.is_empty());
    }

    #[tokio::test]
    async fn test_token_service_client_new() {
        let client = TokenServiceClient::new("https://api.example.com");
        assert!(client.get_current_token().await.is_none());
    }

    #[tokio::test]
    async fn test_token_service_client_clear_token() {
        let client = TokenServiceClient::new("https://api.example.com");

        // Manually set a token
        {
            let mut token = client.current_token.write().await;
            *token = Some(EphemeralToken::new("test".to_string()));
        }

        assert!(client.get_current_token().await.is_some());

        // Clear it
        client.clear_token().await;
        assert!(client.get_current_token().await.is_none());
    }

    #[tokio::test]
    async fn test_token_service_needs_refresh_with_no_token() {
        let client = TokenServiceClient::new("https://api.example.com");
        assert!(client.needs_refresh().await);
    }

    #[tokio::test]
    async fn test_token_service_needs_refresh_with_valid_token() {
        let client = TokenServiceClient::new("https://api.example.com");

        // Set a token that expires in 1 hour
        {
            let mut token = client.current_token.write().await;
            *token = Some(EphemeralToken::new("test".to_string()));
        }

        // Should not need refresh yet
        assert!(!client.needs_refresh().await);
    }

    #[tokio::test]
    async fn test_token_service_needs_refresh_with_expiring_token() {
        let client = TokenServiceClient::new("https://api.example.com");

        // Set a token that expires in 5 minutes
        {
            let mut token = client.current_token.write().await;
            let expires_soon = Utc::now() + Duration::minutes(5);
            *token = Some(EphemeralToken::with_expiration("test".to_string(), expires_soon));
        }

        // Should need refresh (< 10 minutes)
        assert!(client.needs_refresh().await);
    }
}


// ============================================================================
// Token Renewal Manager
// ============================================================================

/// Renewal retry configuration constants
const MAX_RENEWAL_ATTEMPTS: u8 = 3;
const RENEWAL_RETRY_INTERVAL_SECS: u64 = 30;
const RENEWAL_THRESHOLD_MINUTES: i64 = 10;
const MONITOR_CHECK_INTERVAL_SECS: u64 = 60;

/// Events emitted by the TokenRenewalManager
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TokenRenewalEvent {
    /// Token was successfully renewed
    TokenRenewed {
        /// New expiration time in ISO 8601 format
        expires_at: String,
    },
    /// Token is expiring soon (10 minutes or less)
    TokenExpiringSoon {
        /// Minutes remaining before expiration
        minutes_remaining: i64,
    },
    /// Token renewal failed after all retry attempts
    TokenRenewalFailed {
        /// Error message describing the failure
        error: String,
        /// Number of attempts made
        attempts: u8,
        /// User-friendly message in Spanish
        message: String,
        /// Suggested action
        suggestion: String,
    },
    /// Network connection lost during renewal
    NetworkLost {
        /// User-friendly message in Spanish
        message: String,
    },
    /// Network connection restored, will retry renewal
    NetworkRestored {
        /// User-friendly message in Spanish
        message: String,
    },
    /// Translation paused due to token issues
    TranslationPaused {
        /// Reason for pausing
        reason: String,
        /// User-friendly message in Spanish
        message: String,
    },
    /// Translation resumed after successful token renewal
    TranslationResumed,
}

/// State of the token renewal manager
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RenewalState {
    /// Manager is idle, not actively monitoring
    Idle,
    /// Actively monitoring token expiration
    Monitoring,
    /// Currently attempting to renew the token
    Renewing,
    /// Waiting to retry after a failed attempt
    WaitingRetry,
    /// Waiting for network to be restored
    WaitingNetwork,
    /// Renewal failed, translation paused
    Failed,
}

/// Configuration for the TokenRenewalManager
#[derive(Debug, Clone)]
pub struct RenewalConfig {
    /// Minutes before expiration to trigger renewal (default: 10)
    pub renewal_threshold_minutes: i64,
    /// Maximum number of retry attempts (default: 3)
    pub max_attempts: u8,
    /// Seconds between retry attempts (default: 30)
    pub retry_interval_secs: u64,
    /// Seconds between monitoring checks (default: 60)
    pub monitor_interval_secs: u64,
}

impl Default for RenewalConfig {
    fn default() -> Self {
        Self {
            renewal_threshold_minutes: RENEWAL_THRESHOLD_MINUTES,
            max_attempts: MAX_RENEWAL_ATTEMPTS,
            retry_interval_secs: RENEWAL_RETRY_INTERVAL_SECS,
            monitor_interval_secs: MONITOR_CHECK_INTERVAL_SECS,
        }
    }
}

/// Manages automatic token renewal in the background
///
/// The TokenRenewalManager monitors the current ephemeral token and automatically
/// renews it when approaching expiration. It handles network failures gracefully
/// and notifies the application of state changes via events.
///
/// # Requirements
/// - Requirement 7.2: Renew token when 10 minutes remaining
/// - Requirement 7.4: Retry up to 3 times with 30s intervals
/// - Requirement 7.6: Retry when network connection is restored
///
/// # Example
/// ```ignore
/// let client = Arc::new(TokenServiceClient::new("https://api.traductor.app"));
/// let (event_tx, mut event_rx) = mpsc::channel(32);
/// let manager = TokenRenewalManager::new(client, event_tx);
///
/// // Start monitoring with user's session token
/// manager.start_monitoring("session_token".to_string()).await;
///
/// // Listen for events
/// while let Some(event) = event_rx.recv().await {
///     match event {
///         TokenRenewalEvent::TokenRenewed { expires_at } => {
///             println!("Token renewed, expires at: {}", expires_at);
///         }
///         TokenRenewalEvent::TranslationPaused { message, .. } => {
///             println!("Translation paused: {}", message);
///         }
///         _ => {}
///     }
/// }
/// ```
pub struct TokenRenewalManager {
    /// Token service client for making renewal requests
    token_client: Arc<TokenServiceClient>,
    /// Channel for sending events to the application
    event_tx: mpsc::Sender<TokenRenewalEvent>,
    /// Current state of the manager
    state: Arc<RwLock<RenewalState>>,
    /// Whether the manager is running
    is_running: Arc<AtomicBool>,
    /// Current retry attempt counter
    retry_count: Arc<AtomicU8>,
    /// Session token for authentication
    session_token: Arc<RwLock<Option<String>>>,
    /// Flag indicating network is available
    network_available: Arc<AtomicBool>,
    /// Configuration
    config: RenewalConfig,
}

impl TokenRenewalManager {
    /// Create a new TokenRenewalManager
    ///
    /// # Arguments
    /// * `token_client` - The TokenServiceClient to use for renewal requests
    /// * `event_tx` - Channel sender for emitting events
    pub fn new(
        token_client: Arc<TokenServiceClient>,
        event_tx: mpsc::Sender<TokenRenewalEvent>,
    ) -> Self {
        Self::with_config(token_client, event_tx, RenewalConfig::default())
    }

    /// Create a new TokenRenewalManager with custom configuration
    pub fn with_config(
        token_client: Arc<TokenServiceClient>,
        event_tx: mpsc::Sender<TokenRenewalEvent>,
        config: RenewalConfig,
    ) -> Self {
        Self {
            token_client,
            event_tx,
            state: Arc::new(RwLock::new(RenewalState::Idle)),
            is_running: Arc::new(AtomicBool::new(false)),
            retry_count: Arc::new(AtomicU8::new(0)),
            session_token: Arc::new(RwLock::new(None)),
            network_available: Arc::new(AtomicBool::new(true)),
            config,
        }
    }

    /// Get the current state of the renewal manager
    pub async fn get_state(&self) -> RenewalState {
        *self.state.read().await
    }

    /// Check if the manager is currently running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Set the network availability status
    ///
    /// Call this when network connectivity changes. If network becomes available
    /// and renewal was waiting for network, it will automatically retry.
    pub async fn set_network_available(&self, available: bool) {
        let was_available = self.network_available.swap(available, Ordering::SeqCst);
        
        if !was_available && available {
            // Network was restored
            let _ = self.event_tx.send(TokenRenewalEvent::NetworkRestored {
                message: "Conexión de red restaurada. Reintentando renovación de token...".to_string(),
            }).await;
            
            // If we were waiting for network, trigger a renewal attempt
            let state = *self.state.read().await;
            if state == RenewalState::WaitingNetwork {
                self.retry_count.store(0, Ordering::SeqCst);
                // Renewal will be triggered by the monitoring loop
            }
        } else if was_available && !available {
            // Network lost
            let _ = self.event_tx.send(TokenRenewalEvent::NetworkLost {
                message: "Conexión de red perdida. Se reintentará cuando se restaure.".to_string(),
            }).await;
        }
    }

    /// Start monitoring token expiration in the background
    ///
    /// This spawns a background task that periodically checks if the token
    /// needs renewal and handles the renewal process automatically.
    ///
    /// # Arguments
    /// * `session_token` - The user's session token for authentication
    pub async fn start_monitoring(&self, session_token: String) {
        if self.is_running.swap(true, Ordering::SeqCst) {
            // Already running
            return;
        }

        {
            let mut token = self.session_token.write().await;
            *token = Some(session_token);
        }

        {
            let mut state = self.state.write().await;
            *state = RenewalState::Monitoring;
        }

        let manager = self.clone_for_task();
        tauri::async_runtime::spawn(async move {
            manager.monitoring_loop().await;
        });
    }

    /// Stop monitoring and clean up
    pub async fn stop_monitoring(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        
        {
            let mut state = self.state.write().await;
            *state = RenewalState::Idle;
        }
        
        {
            let mut token = self.session_token.write().await;
            *token = None;
        }
        
        self.retry_count.store(0, Ordering::SeqCst);
    }

    /// Create a clone of the manager for use in spawned tasks
    fn clone_for_task(&self) -> TokenRenewalManagerTask {
        TokenRenewalManagerTask {
            token_client: Arc::clone(&self.token_client),
            event_tx: self.event_tx.clone(),
            state: Arc::clone(&self.state),
            is_running: Arc::clone(&self.is_running),
            retry_count: Arc::clone(&self.retry_count),
            session_token: Arc::clone(&self.session_token),
            network_available: Arc::clone(&self.network_available),
            config: self.config.clone(),
        }
    }

    /// Manually trigger a token renewal
    ///
    /// This can be called to force a renewal attempt outside of the automatic
    /// monitoring cycle.
    pub async fn trigger_renewal(&self) -> Result<EphemeralToken, TokenServiceError> {
        let session_token = {
            let token = self.session_token.read().await;
            token.clone().ok_or(TokenServiceError::InvalidSession)?
        };

        self.token_client.get_ephemeral_token(&session_token).await
    }
}

/// Internal task structure for the monitoring loop
struct TokenRenewalManagerTask {
    token_client: Arc<TokenServiceClient>,
    event_tx: mpsc::Sender<TokenRenewalEvent>,
    state: Arc<RwLock<RenewalState>>,
    is_running: Arc<AtomicBool>,
    retry_count: Arc<AtomicU8>,
    session_token: Arc<RwLock<Option<String>>>,
    network_available: Arc<AtomicBool>,
    config: RenewalConfig,
}

impl TokenRenewalManagerTask {
    /// Main monitoring loop that runs in the background
    async fn monitoring_loop(&self) {
        while self.is_running.load(Ordering::SeqCst) {
            let current_state = *self.state.read().await;

            match current_state {
                RenewalState::Monitoring => {
                    self.check_and_renew_if_needed().await;
                }
                RenewalState::WaitingRetry => {
                    // Wait for retry interval then attempt renewal
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        self.config.retry_interval_secs,
                    ))
                    .await;
                    
                    if self.is_running.load(Ordering::SeqCst) {
                        self.attempt_renewal().await;
                    }
                }
                RenewalState::WaitingNetwork => {
                    // Check if network is available now
                    if self.network_available.load(Ordering::SeqCst) {
                        self.retry_count.store(0, Ordering::SeqCst);
                        {
                            let mut state = self.state.write().await;
                            *state = RenewalState::Monitoring;
                        }
                        // Trigger immediate renewal attempt
                        self.attempt_renewal().await;
                    } else {
                        // Wait a bit before checking again
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
                RenewalState::Failed => {
                    // Stay in failed state until manually reset or network restored
                    if self.network_available.load(Ordering::SeqCst) {
                        // Network is back, try to recover
                        self.retry_count.store(0, Ordering::SeqCst);
                        {
                            let mut state = self.state.write().await;
                            *state = RenewalState::Monitoring;
                        }
                        self.attempt_renewal().await;
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    }
                }
                _ => {
                    // Idle or Renewing - just wait
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }

            // Regular monitoring interval
            if *self.state.read().await == RenewalState::Monitoring {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    self.config.monitor_interval_secs,
                ))
                .await;
            }
        }
    }

    /// Check if token needs renewal and initiate if necessary
    async fn check_and_renew_if_needed(&self) {
        if self.token_client.needs_refresh().await {
            // Check if we should emit expiring soon event
            if let Some(token) = self.token_client.get_current_token().await {
                if let Some(time_remaining) = token.time_until_expiry() {
                    let minutes_remaining = time_remaining.num_minutes();
                    if minutes_remaining <= self.config.renewal_threshold_minutes && minutes_remaining > 0 {
                        let _ = self.event_tx.send(TokenRenewalEvent::TokenExpiringSoon {
                            minutes_remaining,
                        }).await;
                    }
                }
            }
            
            self.attempt_renewal().await;
        }
    }

    /// Attempt to renew the token with retry logic
    async fn attempt_renewal(&self) {
        // Check network availability first
        if !self.network_available.load(Ordering::SeqCst) {
            {
                let mut state = self.state.write().await;
                *state = RenewalState::WaitingNetwork;
            }
            return;
        }

        let session_token = {
            let token = self.session_token.read().await;
            match token.as_ref() {
                Some(t) => t.clone(),
                None => {
                    // No session token, can't renew
                    return;
                }
            }
        };

        {
            let mut state = self.state.write().await;
            *state = RenewalState::Renewing;
        }

        let result = self.token_client.get_ephemeral_token(&session_token).await;

        match result {
            Ok(token) => {
                // Success! Reset retry counter and emit event
                self.retry_count.store(0, Ordering::SeqCst);
                {
                    let mut state = self.state.write().await;
                    *state = RenewalState::Monitoring;
                }
                
                let _ = self.event_tx.send(TokenRenewalEvent::TokenRenewed {
                    expires_at: token.expires_at,
                }).await;
                
                // Also emit resume if we were paused
                let _ = self.event_tx.send(TokenRenewalEvent::TranslationResumed).await;
            }
            Err(err) => {
                self.handle_renewal_error(err).await;
            }
        }
    }

    /// Handle a renewal error with retry logic
    async fn handle_renewal_error(&self, err: TokenServiceError) {
        let current_attempt = self.retry_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Check if it's a network error
        let is_network_error = matches!(
            err,
            TokenServiceError::NetworkError { .. } | TokenServiceError::RequestTimeout
        );

        if is_network_error {
            // Mark network as unavailable
            self.network_available.store(false, Ordering::SeqCst);
            {
                let mut state = self.state.write().await;
                *state = RenewalState::WaitingNetwork;
            }
            
            let _ = self.event_tx.send(TokenRenewalEvent::NetworkLost {
                message: "Se perdió la conexión de red durante la renovación del token.".to_string(),
            }).await;
            
            return;
        }

        // Check if we've exhausted retry attempts
        if current_attempt >= self.config.max_attempts {
            {
                let mut state = self.state.write().await;
                *state = RenewalState::Failed;
            }

            // Emit failure event with pause
            let _ = self.event_tx.send(TokenRenewalEvent::TokenRenewalFailed {
                error: err.to_string(),
                attempts: current_attempt,
                message: format!(
                    "La renovación del token falló después de {} intentos. La traducción se ha pausado.",
                    current_attempt
                ),
                suggestion: "Verifica tu conexión a internet e intenta reconectar manualmente.".to_string(),
            }).await;

            let _ = self.event_tx.send(TokenRenewalEvent::TranslationPaused {
                reason: "token_renewal_failed".to_string(),
                message: "La sesión requiere reconexión. Por favor, verifica tu conexión e intenta nuevamente.".to_string(),
            }).await;
        } else {
            // Schedule retry
            {
                let mut state = self.state.write().await;
                *state = RenewalState::WaitingRetry;
            }
            
            tracing::warn!(
                "Token renewal attempt {} failed: {}. Retrying in {}s...",
                current_attempt,
                err,
                self.config.retry_interval_secs
            );
        }
    }
}

/// Tauri event names for token renewal
pub mod token_event_names {
    /// Token was renewed successfully
    pub const TOKEN_RENEWED: &str = "token-renewed";
    /// Token is expiring soon
    pub const TOKEN_EXPIRING_SOON: &str = "token-expiring-soon";
    /// Token renewal failed
    pub const TOKEN_RENEWAL_FAILED: &str = "token-renewal-failed";
    /// Translation paused due to token issues
    pub const TRANSLATION_PAUSED: &str = "translation-paused";
    /// Translation resumed
    pub const TRANSLATION_RESUMED: &str = "translation-resumed";
    /// Network connectivity lost
    pub const NETWORK_LOST: &str = "network-lost";
    /// Network connectivity restored
    pub const NETWORK_RESTORED: &str = "network-restored";
}

/// Helper function to emit token renewal events to Tauri
pub fn emit_token_event<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    event: TokenRenewalEvent,
) -> Result<(), tauri::Error> {
    use tauri::Emitter;
    
    let event_name = match &event {
        TokenRenewalEvent::TokenRenewed { .. } => token_event_names::TOKEN_RENEWED,
        TokenRenewalEvent::TokenExpiringSoon { .. } => token_event_names::TOKEN_EXPIRING_SOON,
        TokenRenewalEvent::TokenRenewalFailed { .. } => token_event_names::TOKEN_RENEWAL_FAILED,
        TokenRenewalEvent::TranslationPaused { .. } => token_event_names::TRANSLATION_PAUSED,
        TokenRenewalEvent::TranslationResumed => token_event_names::TRANSLATION_RESUMED,
        TokenRenewalEvent::NetworkLost { .. } => token_event_names::NETWORK_LOST,
        TokenRenewalEvent::NetworkRestored { .. } => token_event_names::NETWORK_RESTORED,
    };
    
    app.emit(event_name, event)
}

/// Spawn a task that forwards token renewal events to Tauri
///
/// This creates a bridge between the TokenRenewalManager's event channel
/// and Tauri's event system, allowing the frontend to receive notifications.
///
/// # Example
/// ```ignore
/// let (event_tx, event_rx) = mpsc::channel(32);
/// let manager = TokenRenewalManager::new(client, event_tx);
/// 
/// spawn_token_event_forwarder(app_handle, event_rx);
/// ```
pub fn spawn_token_event_forwarder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    mut event_rx: mpsc::Receiver<TokenRenewalEvent>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = emit_token_event(&app, event) {
                tracing::error!("Failed to emit token event: {}", e);
            }
        }
    });
}

// ============================================================================
// Additional Tests for Token Renewal Manager
// ============================================================================

#[cfg(test)]
mod renewal_tests {
    use super::*;

    #[test]
    fn test_renewal_config_default() {
        let config = RenewalConfig::default();
        assert_eq!(config.renewal_threshold_minutes, 10);
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.retry_interval_secs, 30);
        assert_eq!(config.monitor_interval_secs, 60);
    }

    #[test]
    fn test_renewal_event_serialization() {
        let event = TokenRenewalEvent::TokenRenewed {
            expires_at: "2024-01-01T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tokenRenewed"));
        // Note: Field names within enum variants are NOT affected by rename_all on the enum itself
        // They serialize as snake_case unless explicitly renamed
        assert!(json.contains("expires_at"));

        let event = TokenRenewalEvent::TokenRenewalFailed {
            error: "Network error".to_string(),
            attempts: 3,
            message: "Falló la renovación".to_string(),
            suggestion: "Verifica la conexión".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tokenRenewalFailed"));
        assert!(json.contains("attempts"));
    }

    #[test]
    fn test_renewal_state_serialization() {
        let state = RenewalState::Monitoring;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"monitoring\"");

        let state = RenewalState::WaitingNetwork;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"waitingNetwork\"");
    }

    #[tokio::test]
    async fn test_renewal_manager_creation() {
        let client = Arc::new(TokenServiceClient::new("https://api.example.com"));
        let (event_tx, _event_rx) = mpsc::channel(32);
        let manager = TokenRenewalManager::new(client, event_tx);
        
        assert_eq!(manager.get_state().await, RenewalState::Idle);
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_renewal_manager_with_custom_config() {
        let client = Arc::new(TokenServiceClient::new("https://api.example.com"));
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = RenewalConfig {
            renewal_threshold_minutes: 5,
            max_attempts: 5,
            retry_interval_secs: 15,
            monitor_interval_secs: 30,
        };
        let manager = TokenRenewalManager::with_config(client, event_tx, config);
        
        assert_eq!(manager.config.renewal_threshold_minutes, 5);
        assert_eq!(manager.config.max_attempts, 5);
    }

    #[tokio::test]
    async fn test_network_availability_flag() {
        let client = Arc::new(TokenServiceClient::new("https://api.example.com"));
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let manager = TokenRenewalManager::new(client, event_tx);
        
        // Initially network is available
        assert!(manager.network_available.load(Ordering::SeqCst));
        
        // Set network unavailable
        manager.set_network_available(false).await;
        assert!(!manager.network_available.load(Ordering::SeqCst));
        
        // Should receive NetworkLost event
        if let Some(event) = event_rx.recv().await {
            assert!(matches!(event, TokenRenewalEvent::NetworkLost { .. }));
        }
        
        // Set network available again
        manager.set_network_available(true).await;
        assert!(manager.network_available.load(Ordering::SeqCst));
        
        // Should receive NetworkRestored event
        if let Some(event) = event_rx.recv().await {
            assert!(matches!(event, TokenRenewalEvent::NetworkRestored { .. }));
        }
    }

    #[tokio::test]
    async fn test_stop_monitoring_resets_state() {
        let client = Arc::new(TokenServiceClient::new("https://api.example.com"));
        let (event_tx, _event_rx) = mpsc::channel(32);
        let manager = TokenRenewalManager::new(client, event_tx);
        
        // Manually set some state
        {
            let mut state = manager.state.write().await;
            *state = RenewalState::Monitoring;
        }
        manager.is_running.store(true, Ordering::SeqCst);
        manager.retry_count.store(2, Ordering::SeqCst);
        
        // Stop monitoring
        manager.stop_monitoring().await;
        
        assert_eq!(manager.get_state().await, RenewalState::Idle);
        assert!(!manager.is_running());
        assert_eq!(manager.retry_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_trigger_renewal_without_session_token() {
        let client = Arc::new(TokenServiceClient::new("https://api.example.com"));
        let (event_tx, _event_rx) = mpsc::channel(32);
        let manager = TokenRenewalManager::new(client, event_tx);
        
        // Try to trigger renewal without setting session token
        let result = manager.trigger_renewal().await;
        assert!(matches!(result, Err(TokenServiceError::InvalidSession)));
    }

    #[test]
    fn test_token_event_names() {
        assert_eq!(token_event_names::TOKEN_RENEWED, "token-renewed");
        assert_eq!(token_event_names::TOKEN_EXPIRING_SOON, "token-expiring-soon");
        assert_eq!(token_event_names::TOKEN_RENEWAL_FAILED, "token-renewal-failed");
        assert_eq!(token_event_names::TRANSLATION_PAUSED, "translation-paused");
        assert_eq!(token_event_names::TRANSLATION_RESUMED, "translation-resumed");
        assert_eq!(token_event_names::NETWORK_LOST, "network-lost");
        assert_eq!(token_event_names::NETWORK_RESTORED, "network-restored");
    }
}

// ============================================================================
// Property-Based Tests for Token Service
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // **Property 3: Ephemeral Tokens Expire in One Hour**
    // 
    // Verifies that `expiresAt = generationTime + 3600s (±1s)` for any generation time.
    // 
    // **Validates: Requirements 7.1**
    // 
    // This property ensures that:
    // 1. Every ephemeral token created has an expiration time exactly 1 hour from creation
    // 2. The tolerance of ±1 second accounts for execution time during token creation
    // 3. The property holds for any valid timestamp within a reasonable range
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn ephemeral_token_expires_in_one_hour(
            // Generate random token strings (1-256 alphanumeric chars)
            token_str in "[a-zA-Z0-9]{1,256}",
        ) {
            // Capture time immediately before token creation
            let before_creation = Utc::now();
            
            // Create the ephemeral token
            let token = EphemeralToken::new(token_str);
            
            // Capture time immediately after token creation
            let after_creation = Utc::now();
            
            // Parse the expiration time from the token
            let expires_at = DateTime::parse_from_rfc3339(&token.expires_at)
                .expect("Token should have valid RFC3339 expires_at")
                .with_timezone(&Utc);
            
            // Calculate expected expiration boundaries
            // Token should expire at generationTime + 3600s (±1s)
            let expected_min = before_creation + Duration::hours(1) - Duration::seconds(1);
            let expected_max = after_creation + Duration::hours(1) + Duration::seconds(1);
            
            // Verify the expiration time is within the expected range
            prop_assert!(
                expires_at >= expected_min && expires_at <= expected_max,
                "Token expiration {} should be within [{}, {}] (generationTime + 3600s ±1s)",
                expires_at,
                expected_min,
                expected_max
            );
            
            // Additional verification: check time_until_expiry is approximately 1 hour
            if let Some(time_remaining) = token.time_until_expiry() {
                let remaining_secs = time_remaining.num_seconds();
                // Should be approximately 3600 seconds (allowing ±2s for execution time)
                prop_assert!(
                    remaining_secs >= 3598 && remaining_secs <= 3602,
                    "Time until expiry ({} seconds) should be approximately 3600 seconds (1 hour)",
                    remaining_secs
                );
            }
        }

        /// Test that with_expiration respects the exact expiration time provided
        #[test]
        fn ephemeral_token_with_expiration_preserves_time(
            token_str in "[a-zA-Z0-9]{1,256}",
            // Generate random offset in seconds (simulating different future times)
            offset_secs in 0i64..86400, // 0 to 24 hours
        ) {
            let generation_time = Utc::now();
            let expected_expiration = generation_time + Duration::seconds(offset_secs);
            
            // Create token with specific expiration
            let token = EphemeralToken::with_expiration(token_str, expected_expiration);
            
            // Parse back the expiration
            let actual_expiration = DateTime::parse_from_rfc3339(&token.expires_at)
                .expect("Token should have valid RFC3339 expires_at")
                .with_timezone(&Utc);
            
            // Verify the expiration matches (within millisecond precision)
            let diff = (actual_expiration - expected_expiration).num_milliseconds().abs();
            prop_assert!(
                diff < 1000, // Less than 1 second difference (RFC3339 has second precision)
                "Token expiration {} should match expected {} (diff: {}ms)",
                actual_expiration,
                expected_expiration,
                diff
            );
        }

        /// Test that token correctly reports expiration status
        #[test]
        fn ephemeral_token_expiration_status_consistency(
            token_str in "[a-zA-Z0-9]{1,64}",
        ) {
            // Test with a newly created token (should not be expired)
            let fresh_token = EphemeralToken::new(token_str.clone());
            prop_assert!(
                !fresh_token.is_expired(),
                "Freshly created token should not be expired"
            );
            
            // Test with a token that expired 1 second ago
            let past_time = Utc::now() - Duration::seconds(1);
            let expired_token = EphemeralToken::with_expiration(token_str.clone(), past_time);
            prop_assert!(
                expired_token.is_expired(),
                "Token with past expiration should be expired"
            );
            
            // Test with a token that expires in exactly 1 hour
            let future_time = Utc::now() + Duration::hours(1);
            let future_token = EphemeralToken::with_expiration(token_str, future_time);
            prop_assert!(
                !future_token.is_expired(),
                "Token with future expiration should not be expired"
            );
        }

        /// Test that expires_within correctly predicts near-term expiration
        #[test]
        fn ephemeral_token_expires_within_prediction(
            token_str in "[a-zA-Z0-9]{1,64}",
            minutes_until_expiry in 1i64..120i64, // 1 to 120 minutes
            check_minutes in 1i64..60i64, // check window
        ) {
            let expiration_time = Utc::now() + Duration::minutes(minutes_until_expiry);
            let token = EphemeralToken::with_expiration(token_str, expiration_time);
            
            let should_expire_within = minutes_until_expiry <= check_minutes;
            let actual_result = token.expires_within(Duration::minutes(check_minutes));
            
            prop_assert_eq!(
                actual_result,
                should_expire_within,
                "Token expiring in {} minutes should {} expire within {} minutes",
                minutes_until_expiry,
                if should_expire_within { "" } else { "not" },
                check_minutes
            );
        }
    }
}
