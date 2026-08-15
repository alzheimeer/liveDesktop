//! Billing Service client
//!
//! Handles communication with the backend for subscription management.
//! The desktop app does NOT interact with Stripe directly - all billing
//! operations go through the Token Service backend.
//!
//! # Requirements
//! - Requirement 10.1: Three plans (BYOK Free $0, Starter $14.99/600min, Pro $39.99/2000min)
//! - Requirement 10.3: Connection timeout of 30 seconds
//! - Requirement 10.4: Retry logic (2 additional retries, 5s interval)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Backend API base URL
const DEFAULT_API_URL: &str = "https://api.traductor.app";

/// Request timeout in seconds (Requirement 10.3)
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Maximum retry attempts (2 additional = 3 total) (Requirement 10.4)
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Retry interval in seconds (Requirement 10.4)
const RETRY_INTERVAL_SECS: u64 = 5;

/// Subscription plans available in the application
/// 
/// # Requirement 10.1
/// - BYOK Free: $0, unlimited minutes (user's own API key)
/// - Starter: $14.99/month, 600 minutes
/// - Pro: $39.99/month, 2000 minutes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// BYOK Free plan - $0, requires user's own Gemini API key
    ByokFree,
    /// Starter plan - $14.99/month, 600 minutes
    Starter,
    /// Pro plan - $39.99/month, 2000 minutes  
    Pro,
}


impl Plan {
    /// Get the plan's display name
    pub fn name(&self) -> &'static str {
        match self {
            Plan::ByokFree => "BYOK Free",
            Plan::Starter => "Starter",
            Plan::Pro => "Pro",
        }
    }

    /// Get the monthly price in USD
    pub fn price(&self) -> f64 {
        match self {
            Plan::ByokFree => 0.0,
            Plan::Starter => 14.99,
            Plan::Pro => 39.99,
        }
    }

    /// Get the monthly minutes limit (0 = unlimited)
    pub fn minutes_limit(&self) -> u32 {
        match self {
            Plan::ByokFree => 0, // Unlimited with own API key
            Plan::Starter => 600,
            Plan::Pro => 2000,
        }
    }

    /// Check if this plan requires BYOK (user's own API key)
    pub fn requires_byok(&self) -> bool {
        matches!(self, Plan::ByokFree)
    }

    /// Get all available plans
    pub fn all() -> Vec<Plan> {
        vec![Plan::ByokFree, Plan::Starter, Plan::Pro]
    }
}

impl Default for Plan {
    fn default() -> Self {
        Plan::ByokFree
    }
}


/// Detailed information about a subscription plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDetails {
    /// Plan identifier
    pub plan: Plan,
    /// Display name
    pub name: String,
    /// Monthly price in USD
    pub price: f64,
    /// Minutes limit per month (0 = unlimited)
    pub minutes_limit: u32,
    /// List of features included
    pub features: Vec<String>,
    /// Whether this plan requires BYOK
    pub requires_byok: bool,
}

impl PlanDetails {
    /// Create PlanDetails for a given Plan
    pub fn for_plan(plan: Plan) -> Self {
        let features = match plan {
            Plan::ByokFree => vec![
                "Usa tu propia API key de Gemini".to_string(),
                "Sin límite de minutos".to_string(),
                "Traducción en tiempo real".to_string(),
                "Soporte de múltiples idiomas".to_string(),
            ],
            Plan::Starter => vec![
                "600 minutos de traducción/mes".to_string(),
                "Sin necesidad de API key propia".to_string(),
                "Traducción en tiempo real".to_string(),
                "Soporte de múltiples idiomas".to_string(),
                "Soporte por email".to_string(),
            ],
            Plan::Pro => vec![
                "2000 minutos de traducción/mes".to_string(),
                "Sin necesidad de API key propia".to_string(),
                "Traducción en tiempo real".to_string(),
                "Soporte de múltiples idiomas".to_string(),
                "Soporte prioritario".to_string(),
                "Estadísticas avanzadas de uso".to_string(),
            ],
        };

        Self {
            plan,
            name: plan.name().to_string(),
            price: plan.price(),
            minutes_limit: plan.minutes_limit(),
            features,
            requires_byok: plan.requires_byok(),
        }
    }

    /// Get details for all available plans
    pub fn all_plans() -> Vec<PlanDetails> {
        Plan::all().into_iter().map(PlanDetails::for_plan).collect()
    }
}

/// Status of a subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// Subscription is active and valid
    Active,
    /// Subscription was canceled but still valid until period end
    Canceled,
    /// Payment is past due
    PastDue,
    /// Subscription is paused
    Paused,
    /// No subscription (using BYOK Free)
    None,
}

/// Information about a scheduled plan change
///
/// When a user requests a plan change, it takes effect at the start of
/// the next billing cycle. The current plan remains active until then.
///
/// # Requirement 10.8
/// - Apply plan change at the start of the next billing cycle
/// - Maintain current plan until the paid period ends
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChangeInfo {
    /// Current active plan
    pub current_plan: Plan,
    /// Plan that will become active at effective_date
    pub new_plan: Plan,
    /// When the change takes effect (ISO 8601 UTC)
    pub effective_date: String,
    /// Whether the change will be prorated
    pub will_prorate: bool,
}

impl PlanChangeInfo {
    /// Create a new PlanChangeInfo
    pub fn new(current_plan: Plan, new_plan: Plan, effective_date: String, will_prorate: bool) -> Self {
        Self {
            current_plan,
            new_plan,
            effective_date,
            will_prorate,
        }
    }

    /// Check if this is an upgrade (moving to a higher-priced plan)
    pub fn is_upgrade(&self) -> bool {
        self.new_plan.price() > self.current_plan.price()
    }

    /// Check if this is a downgrade (moving to a lower-priced plan)
    pub fn is_downgrade(&self) -> bool {
        self.new_plan.price() < self.current_plan.price()
    }
}

/// Information about the user's current subscription
///
/// # Requirement 10.8
/// Includes `scheduled_change` field to track pending plan changes
/// that will take effect at the next billing cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    /// Current plan
    pub plan: Plan,
    /// Subscription status
    pub status: SubscriptionStatus,
    /// Current billing period start (ISO 8601)
    pub current_period_start: Option<String>,
    /// Current billing period end (ISO 8601)
    pub current_period_end: Option<String>,
    /// Whether subscription will cancel at period end
    pub cancel_at_period_end: bool,
    /// Pending plan change if any (Requirement 10.8)
    /// When present, indicates a scheduled plan change that will take
    /// effect at the start of the next billing cycle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_change: Option<PlanChangeInfo>,
}


impl Default for SubscriptionInfo {
    fn default() -> Self {
        Self {
            plan: Plan::ByokFree,
            status: SubscriptionStatus::None,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            scheduled_change: None,
        }
    }
}

impl SubscriptionInfo {
    /// Check if there's a pending plan change
    pub fn has_scheduled_change(&self) -> bool {
        self.scheduled_change.is_some()
    }

    /// Get the effective plan (considering scheduled changes)
    /// Returns the current plan since changes only take effect at the next cycle
    pub fn effective_plan(&self) -> Plan {
        self.plan
    }

    /// Get the future plan if a change is scheduled
    pub fn future_plan(&self) -> Option<Plan> {
        self.scheduled_change.as_ref().map(|c| c.new_plan)
    }
}

/// Response for checkout session creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutSession {
    /// URL to redirect user to for payment
    pub checkout_url: String,
    /// Session ID for tracking
    pub session_id: String,
}

/// Response for customer portal session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPortalSession {
    /// URL to redirect user to manage subscription
    pub portal_url: String,
}

/// Error types for Billing Service operations (4xxx codes)
#[derive(Debug, Clone)]
pub enum BillingError {
    /// Network error connecting to billing service (4001)
    NetworkError { reason: String },
    /// Invalid or expired session token (4002)
    InvalidSession,
    /// Payment processing failed (4003)
    PaymentFailed { reason: String },
    /// Request timed out after 30s (4004)
    RequestTimeout,
    /// Invalid API response (4005)
    InvalidResponse { details: String },
    /// Rate limited (4006)
    RateLimited { retry_after_secs: u32 },
    /// Plan not found (4007)
    PlanNotFound { plan: String },
    /// Subscription already exists (4008)
    SubscriptionExists,
    /// Cannot change to the same plan (4009)
    SamePlanChange,
    /// No active subscription to cancel or modify (4010)
    NoActiveSubscription,
}


impl BillingError {
    /// Get the error code
    pub fn code(&self) -> u32 {
        match self {
            BillingError::NetworkError { .. } => 4001,
            BillingError::InvalidSession => 4002,
            BillingError::PaymentFailed { .. } => 4003,
            BillingError::RequestTimeout => 4004,
            BillingError::InvalidResponse { .. } => 4005,
            BillingError::RateLimited { .. } => 4006,
            BillingError::PlanNotFound { .. } => 4007,
            BillingError::SubscriptionExists => 4008,
            BillingError::SamePlanChange => 4009,
            BillingError::NoActiveSubscription => 4010,
        }
    }

    /// Get user-friendly message in Spanish
    pub fn message(&self) -> String {
        match self {
            BillingError::NetworkError { reason } => {
                format!("Error de conexión con el servicio de pagos: {}", reason)
            }
            BillingError::InvalidSession => {
                "Tu sesión ha expirado. Por favor inicia sesión nuevamente.".to_string()
            }
            BillingError::PaymentFailed { reason } => {
                format!("El pago no pudo procesarse: {}", reason)
            }
            BillingError::RequestTimeout => {
                "La solicitud excedió el tiempo límite de 30 segundos.".to_string()
            }
            BillingError::InvalidResponse { details } => {
                format!("Respuesta inválida del servidor: {}", details)
            }
            BillingError::RateLimited { retry_after_secs } => {
                format!("Demasiadas solicitudes. Espera {} segundos.", retry_after_secs)
            }
            BillingError::PlanNotFound { plan } => {
                format!("El plan '{}' no existe.", plan)
            }
            BillingError::SubscriptionExists => {
                "Ya tienes una suscripción activa.".to_string()
            }
            BillingError::SamePlanChange => {
                "No puedes cambiar al mismo plan que ya tienes.".to_string()
            }
            BillingError::NoActiveSubscription => {
                "No tienes una suscripción activa para modificar.".to_string()
            }
        }
    }


    /// Get recovery suggestion in Spanish
    pub fn suggestion(&self) -> &'static str {
        match self {
            BillingError::NetworkError { .. } => {
                "Verifica tu conexión a internet e intenta nuevamente."
            }
            BillingError::InvalidSession => {
                "Cierra sesión e inicia sesión nuevamente."
            }
            BillingError::PaymentFailed { .. } => {
                "Verifica los datos de tu tarjeta o intenta con otro método de pago."
            }
            BillingError::RequestTimeout => {
                "Intenta nuevamente. Si el problema persiste, contacta soporte."
            }
            BillingError::InvalidResponse { .. } => {
                "Intenta nuevamente. Si el problema persiste, contacta soporte."
            }
            BillingError::RateLimited { .. } => {
                "Espera unos momentos antes de intentar nuevamente."
            }
            BillingError::PlanNotFound { .. } => {
                "Selecciona un plan válido de la lista disponible."
            }
            BillingError::SubscriptionExists => {
                "Cancela tu suscripción actual antes de cambiar de plan."
            }
            BillingError::SamePlanChange => {
                "Selecciona un plan diferente al actual."
            }
            BillingError::NoActiveSubscription => {
                "Suscríbete a un plan antes de intentar hacer cambios."
            }
        }
    }
}

impl std::fmt::Display for BillingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for BillingError {}


// ============================================================================
// API Response Types (internal)
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PlansResponse {
    Success {
        success: bool,
        plans: Vec<PlanDetails>,
    },
    Error {
        success: bool,
        error: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SubscriptionResponse {
    Success {
        success: bool,
        subscription: SubscriptionInfo,
    },
    Error {
        success: bool,
        error: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CheckoutResponse {
    Success {
        success: bool,
        #[serde(rename = "checkoutUrl")]
        checkout_url: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Error {
        success: bool,
        error: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PortalResponse {
    Success {
        success: bool,
        #[serde(rename = "portalUrl")]
        portal_url: String,
    },
    Error {
        success: bool,
        error: String,
    },
}

/// Response for plan change request
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PlanChangeResponse {
    Success {
        success: bool,
        #[serde(rename = "planChange")]
        plan_change: PlanChangeInfo,
    },
    Error {
        success: bool,
        error: String,
    },
}

/// Response for subscription cancellation
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CancelSubscriptionResponse {
    Success {
        success: bool,
        message: String,
        /// Date when access ends (ISO 8601)
        #[serde(rename = "accessEndsAt")]
        access_ends_at: String,
    },
    Error {
        success: bool,
        error: String,
    },
}

/// Result of a successful subscription cancellation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancellationInfo {
    /// Confirmation message
    pub message: String,
    /// Date when access to the service ends (ISO 8601 UTC)
    /// This is typically the end of the current billing period
    pub access_ends_at: String,
}


// ============================================================================
// Billing Client
// ============================================================================

/// Billing Service client for subscription management
///
/// This client handles communication with the Token Service backend for all
/// billing operations. The desktop app does NOT interact with Stripe directly.
///
/// # Requirements
/// - Requirement 10.1: Three plans (BYOK Free, Starter, Pro)
/// - Requirement 10.3: Connection timeout of 30 seconds
/// - Requirement 10.4: Retry logic (2 additional retries, 5s interval)
///
/// # Example
/// ```ignore
/// let client = BillingClient::new("https://api.traductor.app");
/// let plans = client.get_plans().await?;
/// let subscription = client.get_current_plan("session_token").await?;
/// ```
pub struct BillingClient {
    /// Base URL of the billing API
    base_url: String,
    /// HTTP client with configured timeout
    http_client: Client,
}

impl BillingClient {
    /// Create a new Billing client with default API URL
    pub fn new() -> Self {
        Self::with_url(DEFAULT_API_URL)
    }

    /// Create a new Billing client with custom API URL
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the billing API
    pub fn with_url(base_url: &str) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
        }
    }


    /// Create a new Billing client with custom HTTP client (for testing)
    pub fn with_client(base_url: &str, http_client: Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
        }
    }

    /// Execute a request with retry logic
    ///
    /// Implements Requirement 10.4: 2 additional retries with 5s interval
    async fn execute_with_retry<F, Fut, T>(
        &self,
        operation: F,
    ) -> Result<T, BillingError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, BillingError>>,
    {
        let mut last_error = BillingError::NetworkError {
            reason: "No se pudo conectar".to_string(),
        };

        for attempt in 0..MAX_RETRY_ATTEMPTS {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e.clone();
                    
                    // Don't retry for certain errors
                    match &e {
                        BillingError::InvalidSession
                        | BillingError::PlanNotFound { .. }
                        | BillingError::SubscriptionExists => {
                            return Err(e);
                        }
                        _ => {}
                    }

                    // Log retry attempt
                    if attempt < MAX_RETRY_ATTEMPTS - 1 {
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_attempts = MAX_RETRY_ATTEMPTS,
                            error = %e,
                            "Billing request failed, retrying in {}s",
                            RETRY_INTERVAL_SECS
                        );
                        tokio::time::sleep(Duration::from_secs(RETRY_INTERVAL_SECS)).await;
                    }
                }
            }
        }

        Err(last_error)
    }


    /// Parse HTTP error response to BillingError
    fn parse_error_response(&self, status: reqwest::StatusCode, body: &str) -> BillingError {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return BillingError::InvalidSession;
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return BillingError::RateLimited { retry_after_secs: 60 };
        }

        // Try to parse error from body
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(error) = json.get("error").and_then(|e| e.as_str()) {
                match error {
                    "invalid_session" | "unauthorized" => return BillingError::InvalidSession,
                    "rate_limited" => return BillingError::RateLimited { retry_after_secs: 60 },
                    "subscription_exists" => return BillingError::SubscriptionExists,
                    "payment_failed" => return BillingError::PaymentFailed {
                        reason: json.get("details")
                            .and_then(|d| d.as_str())
                            .unwrap_or("Error desconocido")
                            .to_string(),
                    },
                    _ => {}
                }
            }
        }

        BillingError::InvalidResponse {
            details: format!("HTTP {}: {}", status.as_u16(), body),
        }
    }

    /// Get list of available plans
    ///
    /// Returns static plan definitions. Does not require authentication.
    ///
    /// # Returns
    /// * `Ok(Vec<PlanDetails>)` - List of all available plans
    /// * `Err(BillingError)` - If request fails
    pub async fn get_plans(&self) -> Result<Vec<PlanDetails>, BillingError> {
        // Plans can be returned statically without API call for offline support
        // But we also fetch from backend to ensure pricing is current
        let url = format!("{}/billing/plans", self.base_url);

        
        self.execute_with_retry(|| async {
            let response = self.http_client
                .get(&url)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        BillingError::RequestTimeout
                    } else {
                        BillingError::NetworkError { reason: e.to_string() }
                    }
                })?;

            let status = response.status();
            let body = response.text().await.map_err(|e| {
                BillingError::InvalidResponse { details: e.to_string() }
            })?;

            if !status.is_success() {
                return Err(self.parse_error_response(status, &body));
            }

            let plans_response: PlansResponse = serde_json::from_str(&body)
                .map_err(|e| BillingError::InvalidResponse { 
                    details: format!("JSON parse error: {}", e) 
                })?;

            match plans_response {
                PlansResponse::Success { success: true, plans } => Ok(plans),
                PlansResponse::Success { success: false, .. } => {
                    Err(BillingError::InvalidResponse { 
                        details: "Unexpected success=false".to_string() 
                    })
                }
                PlansResponse::Error { error, .. } => {
                    Err(BillingError::InvalidResponse { details: error })
                }
            }
        }).await.or_else(|_| {
            // Fallback to static plans if API fails
            tracing::warn!("Failed to fetch plans from API, using static definitions");
            Ok(PlanDetails::all_plans())
        })
    }


    /// Get user's current subscription plan
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    ///
    /// # Returns
    /// * `Ok(SubscriptionInfo)` - Current subscription information
    /// * `Err(BillingError)` - If request fails
    pub async fn get_current_plan(
        &self,
        session_token: &str,
    ) -> Result<SubscriptionInfo, BillingError> {
        let url = format!("{}/billing/subscription", self.base_url);
        let token = session_token.to_string();

        self.execute_with_retry(|| {
            let url = url.clone();
            let token = token.clone();
            async move {
                let response = self.http_client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            BillingError::RequestTimeout
                        } else {
                            BillingError::NetworkError { reason: e.to_string() }
                        }
                    })?;

                let status = response.status();
                let body = response.text().await.map_err(|e| {
                    BillingError::InvalidResponse { details: e.to_string() }
                })?;

                if !status.is_success() {
                    return Err(self.parse_error_response(status, &body));
                }

                let sub_response: SubscriptionResponse = serde_json::from_str(&body)
                    .map_err(|e| BillingError::InvalidResponse {
                        details: format!("JSON parse error: {}", e)
                    })?;


                match sub_response {
                    SubscriptionResponse::Success { success: true, subscription } => {
                        Ok(subscription)
                    }
                    SubscriptionResponse::Success { success: false, .. } => {
                        Err(BillingError::InvalidResponse {
                            details: "Unexpected success=false".to_string()
                        })
                    }
                    SubscriptionResponse::Error { error, .. } => {
                        Err(BillingError::InvalidResponse { details: error })
                    }
                }
            }
        }).await
    }

    /// Create a Stripe checkout session for subscription
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    /// * `plan` - The plan to subscribe to
    ///
    /// # Returns
    /// * `Ok(CheckoutSession)` - Contains URL to redirect user for payment
    /// * `Err(BillingError)` - If request fails
    pub async fn create_checkout_session(
        &self,
        session_token: &str,
        plan: Plan,
    ) -> Result<CheckoutSession, BillingError> {
        let url = format!("{}/billing/checkout", self.base_url);
        let token = session_token.to_string();

        self.execute_with_retry(|| {
            let url = url.clone();
            let token = token.clone();
            async move {
                let response = self.http_client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "plan": plan
                    }))
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            BillingError::RequestTimeout
                        } else {
                            BillingError::NetworkError { reason: e.to_string() }
                        }
                    })?;


                let status = response.status();
                let body = response.text().await.map_err(|e| {
                    BillingError::InvalidResponse { details: e.to_string() }
                })?;

                if !status.is_success() {
                    return Err(self.parse_error_response(status, &body));
                }

                let checkout_response: CheckoutResponse = serde_json::from_str(&body)
                    .map_err(|e| BillingError::InvalidResponse {
                        details: format!("JSON parse error: {}", e)
                    })?;

                match checkout_response {
                    CheckoutResponse::Success { 
                        success: true, 
                        checkout_url, 
                        session_id 
                    } => {
                        Ok(CheckoutSession { checkout_url, session_id })
                    }
                    CheckoutResponse::Success { success: false, .. } => {
                        Err(BillingError::InvalidResponse {
                            details: "Unexpected success=false".to_string()
                        })
                    }
                    CheckoutResponse::Error { error, .. } => {
                        if error.contains("payment") {
                            Err(BillingError::PaymentFailed { reason: error })
                        } else {
                            Err(BillingError::InvalidResponse { details: error })
                        }
                    }
                }
            }
        }).await
    }


    /// Get Stripe customer portal URL to manage subscription
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    ///
    /// # Returns
    /// * `Ok(CustomerPortalSession)` - Contains URL to redirect user to portal
    /// * `Err(BillingError)` - If request fails
    pub async fn manage_subscription(
        &self,
        session_token: &str,
    ) -> Result<CustomerPortalSession, BillingError> {
        let url = format!("{}/billing/portal", self.base_url);
        let token = session_token.to_string();

        self.execute_with_retry(|| {
            let url = url.clone();
            let token = token.clone();
            async move {
                let response = self.http_client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            BillingError::RequestTimeout
                        } else {
                            BillingError::NetworkError { reason: e.to_string() }
                        }
                    })?;

                let status = response.status();
                let body = response.text().await.map_err(|e| {
                    BillingError::InvalidResponse { details: e.to_string() }
                })?;

                if !status.is_success() {
                    return Err(self.parse_error_response(status, &body));
                }


                let portal_response: PortalResponse = serde_json::from_str(&body)
                    .map_err(|e| BillingError::InvalidResponse {
                        details: format!("JSON parse error: {}", e)
                    })?;

                match portal_response {
                    PortalResponse::Success { success: true, portal_url } => {
                        Ok(CustomerPortalSession { portal_url })
                    }
                    PortalResponse::Success { success: false, .. } => {
                        Err(BillingError::InvalidResponse {
                            details: "Unexpected success=false".to_string()
                        })
                    }
                    PortalResponse::Error { error, .. } => {
                        Err(BillingError::InvalidResponse { details: error })
                    }
                }
            }
        }).await
    }

    /// Get static plan details (offline fallback)
    ///
    /// Returns plan details without making an API call.
    /// Useful when network is unavailable.
    pub fn get_static_plans(&self) -> Vec<PlanDetails> {
        PlanDetails::all_plans()
    }

    /// Get static plan details for a specific plan
    pub fn get_static_plan(&self, plan: Plan) -> PlanDetails {
        PlanDetails::for_plan(plan)
    }

    /// Request a plan change for the subscription
    ///
    /// The plan change takes effect at the start of the next billing cycle.
    /// The current plan remains active until the end of the paid period.
    ///
    /// # Requirement 10.8
    /// - Apply plan change at the start of the next billing cycle
    /// - Maintain current plan until the paid period ends
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    /// * `new_plan` - The plan to change to
    ///
    /// # Returns
    /// * `Ok(PlanChangeInfo)` - Information about when the change takes effect
    /// * `Err(BillingError)` - If request fails
    ///
    /// # Example
    /// ```ignore
    /// let client = BillingClient::new();
    /// let change_info = client.request_plan_change("token", Plan::Pro).await?;
    /// println!("Change takes effect on: {}", change_info.effective_date);
    /// ```
    pub async fn request_plan_change(
        &self,
        session_token: &str,
        new_plan: Plan,
    ) -> Result<PlanChangeInfo, BillingError> {
        let url = format!("{}/billing/change-plan", self.base_url);
        let token = session_token.to_string();

        self.execute_with_retry(|| {
            let url = url.clone();
            let token = token.clone();
            async move {
                let response = self.http_client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "newPlan": new_plan
                    }))
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            BillingError::RequestTimeout
                        } else {
                            BillingError::NetworkError { reason: e.to_string() }
                        }
                    })?;

                let status = response.status();
                let body = response.text().await.map_err(|e| {
                    BillingError::InvalidResponse { details: e.to_string() }
                })?;

                if !status.is_success() {
                    return Err(self.parse_plan_change_error(status, &body));
                }

                let change_response: PlanChangeResponse = serde_json::from_str(&body)
                    .map_err(|e| BillingError::InvalidResponse {
                        details: format!("JSON parse error: {}", e)
                    })?;

                match change_response {
                    PlanChangeResponse::Success { success: true, plan_change } => {
                        tracing::info!(
                            current = ?plan_change.current_plan,
                            new = ?plan_change.new_plan,
                            effective_date = %plan_change.effective_date,
                            "Plan change scheduled successfully"
                        );
                        Ok(plan_change)
                    }
                    PlanChangeResponse::Success { success: false, .. } => {
                        Err(BillingError::InvalidResponse {
                            details: "Unexpected success=false".to_string()
                        })
                    }
                    PlanChangeResponse::Error { error, .. } => {
                        Err(self.parse_plan_change_error_string(&error))
                    }
                }
            }
        }).await
    }

    /// Cancel the current subscription
    ///
    /// Cancellation takes effect at the end of the current billing period.
    /// The user maintains access until then.
    ///
    /// # Requirement 10.8
    /// - Maintains access until the end of the paid period
    ///
    /// # Arguments
    /// * `session_token` - User's authentication session token
    ///
    /// # Returns
    /// * `Ok(CancellationInfo)` - Information about when access ends
    /// * `Err(BillingError)` - If request fails
    ///
    /// # Example
    /// ```ignore
    /// let client = BillingClient::new();
    /// let cancellation = client.cancel_subscription("token").await?;
    /// println!("Access ends on: {}", cancellation.access_ends_at);
    /// ```
    pub async fn cancel_subscription(
        &self,
        session_token: &str,
    ) -> Result<CancellationInfo, BillingError> {
        let url = format!("{}/billing/cancel", self.base_url);
        let token = session_token.to_string();

        self.execute_with_retry(|| {
            let url = url.clone();
            let token = token.clone();
            async move {
                let response = self.http_client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            BillingError::RequestTimeout
                        } else {
                            BillingError::NetworkError { reason: e.to_string() }
                        }
                    })?;

                let status = response.status();
                let body = response.text().await.map_err(|e| {
                    BillingError::InvalidResponse { details: e.to_string() }
                })?;

                if !status.is_success() {
                    return Err(self.parse_cancel_error(status, &body));
                }

                let cancel_response: CancelSubscriptionResponse = serde_json::from_str(&body)
                    .map_err(|e| BillingError::InvalidResponse {
                        details: format!("JSON parse error: {}", e)
                    })?;

                match cancel_response {
                    CancelSubscriptionResponse::Success { 
                        success: true, 
                        message, 
                        access_ends_at 
                    } => {
                        tracing::info!(
                            access_ends_at = %access_ends_at,
                            "Subscription cancellation scheduled successfully"
                        );
                        Ok(CancellationInfo {
                            message,
                            access_ends_at,
                        })
                    }
                    CancelSubscriptionResponse::Success { success: false, .. } => {
                        Err(BillingError::InvalidResponse {
                            details: "Unexpected success=false".to_string()
                        })
                    }
                    CancelSubscriptionResponse::Error { error, .. } => {
                        Err(self.parse_cancel_error_string(&error))
                    }
                }
            }
        }).await
    }

    /// Parse HTTP error response for plan change
    fn parse_plan_change_error(&self, status: reqwest::StatusCode, body: &str) -> BillingError {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return BillingError::InvalidSession;
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return BillingError::RateLimited { retry_after_secs: 60 };
        }

        // Try to parse error from body
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(error) = json.get("error").and_then(|e| e.as_str()) {
                return self.parse_plan_change_error_string(error);
            }
        }

        BillingError::InvalidResponse {
            details: format!("HTTP {}: {}", status.as_u16(), body),
        }
    }

    /// Parse error string for plan change
    fn parse_plan_change_error_string(&self, error: &str) -> BillingError {
        match error {
            "invalid_session" | "unauthorized" => BillingError::InvalidSession,
            "rate_limited" => BillingError::RateLimited { retry_after_secs: 60 },
            "same_plan" => BillingError::SamePlanChange,
            "no_subscription" | "no_active_subscription" => BillingError::NoActiveSubscription,
            "plan_not_found" => BillingError::PlanNotFound { plan: "unknown".to_string() },
            _ => BillingError::InvalidResponse { details: error.to_string() },
        }
    }

    /// Parse HTTP error response for subscription cancellation
    fn parse_cancel_error(&self, status: reqwest::StatusCode, body: &str) -> BillingError {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return BillingError::InvalidSession;
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return BillingError::RateLimited { retry_after_secs: 60 };
        }

        // Try to parse error from body
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(error) = json.get("error").and_then(|e| e.as_str()) {
                return self.parse_cancel_error_string(error);
            }
        }

        BillingError::InvalidResponse {
            details: format!("HTTP {}: {}", status.as_u16(), body),
        }
    }

    /// Parse error string for cancellation
    fn parse_cancel_error_string(&self, error: &str) -> BillingError {
        match error {
            "invalid_session" | "unauthorized" => BillingError::InvalidSession,
            "rate_limited" => BillingError::RateLimited { retry_after_secs: 60 },
            "no_subscription" | "no_active_subscription" => BillingError::NoActiveSubscription,
            "already_canceled" => BillingError::InvalidResponse { 
                details: "La suscripción ya está cancelada".to_string() 
            },
            _ => BillingError::InvalidResponse { details: error.to_string() },
        }
    }
}

impl Default for BillingClient {
    fn default() -> Self {
        Self::new()
    }
}


// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_prices() {
        assert_eq!(Plan::ByokFree.price(), 0.0);
        assert_eq!(Plan::Starter.price(), 14.99);
        assert_eq!(Plan::Pro.price(), 39.99);
    }

    #[test]
    fn test_plan_minutes_limits() {
        assert_eq!(Plan::ByokFree.minutes_limit(), 0); // Unlimited
        assert_eq!(Plan::Starter.minutes_limit(), 600);
        assert_eq!(Plan::Pro.minutes_limit(), 2000);
    }

    #[test]
    fn test_plan_names() {
        assert_eq!(Plan::ByokFree.name(), "BYOK Free");
        assert_eq!(Plan::Starter.name(), "Starter");
        assert_eq!(Plan::Pro.name(), "Pro");
    }

    #[test]
    fn test_plan_byok_requirement() {
        assert!(Plan::ByokFree.requires_byok());
        assert!(!Plan::Starter.requires_byok());
        assert!(!Plan::Pro.requires_byok());
    }

    #[test]
    fn test_plan_all() {
        let plans = Plan::all();
        assert_eq!(plans.len(), 3);
        assert!(plans.contains(&Plan::ByokFree));
        assert!(plans.contains(&Plan::Starter));
        assert!(plans.contains(&Plan::Pro));
    }


    #[test]
    fn test_plan_details_for_plan() {
        let starter = PlanDetails::for_plan(Plan::Starter);
        assert_eq!(starter.plan, Plan::Starter);
        assert_eq!(starter.name, "Starter");
        assert_eq!(starter.price, 14.99);
        assert_eq!(starter.minutes_limit, 600);
        assert!(!starter.requires_byok);
        assert!(!starter.features.is_empty());
    }

    #[test]
    fn test_plan_details_all_plans() {
        let all = PlanDetails::all_plans();
        assert_eq!(all.len(), 3);
        
        // Verify order and content
        assert_eq!(all[0].plan, Plan::ByokFree);
        assert_eq!(all[1].plan, Plan::Starter);
        assert_eq!(all[2].plan, Plan::Pro);
    }

    #[test]
    fn test_subscription_info_default() {
        let info = SubscriptionInfo::default();
        assert_eq!(info.plan, Plan::ByokFree);
        assert_eq!(info.status, SubscriptionStatus::None);
        assert!(info.current_period_start.is_none());
        assert!(info.current_period_end.is_none());
        assert!(!info.cancel_at_period_end);
        assert!(info.scheduled_change.is_none());
    }

    #[test]
    fn test_subscription_info_with_scheduled_change() {
        let change = PlanChangeInfo::new(
            Plan::Starter,
            Plan::Pro,
            "2025-02-01T00:00:00Z".to_string(),
            false,
        );
        
        let info = SubscriptionInfo {
            plan: Plan::Starter,
            status: SubscriptionStatus::Active,
            current_period_start: Some("2025-01-01T00:00:00Z".to_string()),
            current_period_end: Some("2025-02-01T00:00:00Z".to_string()),
            cancel_at_period_end: false,
            scheduled_change: Some(change),
        };
        
        assert!(info.has_scheduled_change());
        assert_eq!(info.effective_plan(), Plan::Starter);
        assert_eq!(info.future_plan(), Some(Plan::Pro));
    }

    #[test]
    fn test_plan_change_info_upgrade() {
        let change = PlanChangeInfo::new(
            Plan::Starter,
            Plan::Pro,
            "2025-02-01T00:00:00Z".to_string(),
            false,
        );
        
        assert!(change.is_upgrade());
        assert!(!change.is_downgrade());
    }

    #[test]
    fn test_plan_change_info_downgrade() {
        let change = PlanChangeInfo::new(
            Plan::Pro,
            Plan::Starter,
            "2025-02-01T00:00:00Z".to_string(),
            false,
        );
        
        assert!(!change.is_upgrade());
        assert!(change.is_downgrade());
    }

    #[test]
    fn test_plan_change_info_serialization() {
        let change = PlanChangeInfo::new(
            Plan::Starter,
            Plan::Pro,
            "2025-02-01T00:00:00Z".to_string(),
            true,
        );
        
        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"currentPlan\":\"starter\""));
        assert!(json.contains("\"newPlan\":\"pro\""));
        assert!(json.contains("\"effectiveDate\":"));
        assert!(json.contains("\"willProrate\":true"));
        
        let deserialized: PlanChangeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.current_plan, Plan::Starter);
        assert_eq!(deserialized.new_plan, Plan::Pro);
        assert!(deserialized.will_prorate);
    }

    #[test]
    fn test_cancellation_info_serialization() {
        let info = CancellationInfo {
            message: "Suscripción cancelada".to_string(),
            access_ends_at: "2025-02-01T00:00:00Z".to_string(),
        };
        
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"accessEndsAt\""));
        
        let deserialized: CancellationInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.access_ends_at, "2025-02-01T00:00:00Z");
    }

    #[test]
    fn test_billing_error_codes_in_4xxx_range() {
        let errors = vec![
            BillingError::NetworkError { reason: "test".to_string() },
            BillingError::InvalidSession,
            BillingError::PaymentFailed { reason: "test".to_string() },
            BillingError::RequestTimeout,
            BillingError::InvalidResponse { details: "test".to_string() },
            BillingError::RateLimited { retry_after_secs: 60 },
            BillingError::PlanNotFound { plan: "test".to_string() },
            BillingError::SubscriptionExists,
            BillingError::SamePlanChange,
            BillingError::NoActiveSubscription,
        ];

        for err in errors {
            let code = err.code();
            assert!(
                code >= 4000 && code < 5000,
                "Billing error code {} not in 4xxx range",
                code
            );
        }
    }


    #[test]
    fn test_billing_error_has_message_and_suggestion() {
        let err = BillingError::PaymentFailed { 
            reason: "Card declined".to_string() 
        };

        let message = err.message();
        assert!(message.contains("pago") || message.contains("Card"));

        let suggestion = err.suggestion();
        assert!(!suggestion.is_empty());
    }

    #[test]
    fn test_new_billing_errors_have_message_and_suggestion() {
        let same_plan_err = BillingError::SamePlanChange;
        let same_plan_msg = same_plan_err.message();
        assert!(same_plan_msg.contains("mismo plan"));
        assert!(!same_plan_err.suggestion().is_empty());
        
        let no_sub_err = BillingError::NoActiveSubscription;
        let no_sub_msg = no_sub_err.message();
        assert!(no_sub_msg.contains("suscripción activa"));
        assert!(!no_sub_err.suggestion().is_empty());
    }

    #[test]
    fn test_billing_error_display() {
        let err = BillingError::RequestTimeout;
        let display = format!("{}", err);
        
        // Should contain error code
        assert!(display.contains("4004"));
        // Should contain message
        assert!(display.contains("30 segundos"));
    }

    #[test]
    fn test_billing_client_new() {
        let client = BillingClient::new();
        assert_eq!(client.base_url, DEFAULT_API_URL);
    }

    #[test]
    fn test_billing_client_with_url() {
        let client = BillingClient::with_url("https://custom.api.com/");
        // Should trim trailing slash
        assert_eq!(client.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_billing_client_static_plans() {
        let client = BillingClient::new();
        let plans = client.get_static_plans();
        
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].plan, Plan::ByokFree);
        assert_eq!(plans[1].plan, Plan::Starter);
        assert_eq!(plans[2].plan, Plan::Pro);
    }

    #[test]
    fn test_billing_client_static_plan() {
        let client = BillingClient::new();
        let pro = client.get_static_plan(Plan::Pro);
        
        assert_eq!(pro.plan, Plan::Pro);
        assert_eq!(pro.price, 39.99);
        assert_eq!(pro.minutes_limit, 2000);
    }

    #[test]
    fn test_plan_serialization() {
        let plan = Plan::Starter;
        let json = serde_json::to_string(&plan).unwrap();
        assert_eq!(json, "\"starter\"");
        
        let deserialized: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Plan::Starter);
    }


    #[test]
    fn test_subscription_status_serialization() {
        let status = SubscriptionStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");
        
        let deserialized: SubscriptionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SubscriptionStatus::Active);
    }

    #[test]
    fn test_subscription_info_serialization() {
        let info = SubscriptionInfo {
            plan: Plan::Pro,
            status: SubscriptionStatus::Active,
            current_period_start: Some("2025-01-01T00:00:00Z".to_string()),
            current_period_end: Some("2025-02-01T00:00:00Z".to_string()),
            cancel_at_period_end: false,
            scheduled_change: None,
        };
        
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"plan\":\"pro\""));
        assert!(json.contains("\"status\":\"active\""));
        // scheduled_change should be skipped when None
        assert!(!json.contains("scheduledChange"));
        
        let deserialized: SubscriptionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.plan, Plan::Pro);
        assert_eq!(deserialized.status, SubscriptionStatus::Active);
        assert!(deserialized.scheduled_change.is_none());
    }

    #[test]
    fn test_subscription_info_serialization_with_scheduled_change() {
        let change = PlanChangeInfo::new(
            Plan::Pro,
            Plan::Starter,
            "2025-02-01T00:00:00Z".to_string(),
            false,
        );
        
        let info = SubscriptionInfo {
            plan: Plan::Pro,
            status: SubscriptionStatus::Active,
            current_period_start: Some("2025-01-01T00:00:00Z".to_string()),
            current_period_end: Some("2025-02-01T00:00:00Z".to_string()),
            cancel_at_period_end: false,
            scheduled_change: Some(change),
        };
        
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"scheduledChange\""));
        assert!(json.contains("\"newPlan\":\"starter\""));
        
        let deserialized: SubscriptionInfo = serde_json::from_str(&json).unwrap();
        assert!(deserialized.scheduled_change.is_some());
        let scheduled = deserialized.scheduled_change.unwrap();
        assert_eq!(scheduled.new_plan, Plan::Starter);
    }

    #[test]
    fn test_checkout_session_serialization() {
        let session = CheckoutSession {
            checkout_url: "https://checkout.stripe.com/test".to_string(),
            session_id: "cs_test_123".to_string(),
        };
        
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("checkoutUrl"));
        assert!(json.contains("sessionId"));
    }

    #[test]
    fn test_customer_portal_session_serialization() {
        let session = CustomerPortalSession {
            portal_url: "https://billing.stripe.com/test".to_string(),
        };
        
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("portalUrl"));
    }
}
