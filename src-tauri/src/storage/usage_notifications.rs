//! Usage limit notifications
//!
//! This module provides functionality to notify users about usage limits:
//! - Warning notification at 80% usage (Requirement 10.6)
//! - Block translation at 100% and show upgrade options (Requirement 10.7, 11.8)
//!
//! Requirements:
//! - 10.6: Show notification when user reaches 80% of plan minutes
//! - 10.7: Block translation at 100% and show upgrade options
//! - 11.8: Show notification at 100% limit and block new sessions

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

use super::usage::{Plan, UsageStats};

// ============================================================================
// Event Names
// ============================================================================

/// Event names for usage limit notifications
pub mod usage_event_names {
    /// Emitted when usage reaches 80% of the plan limit
    pub const USAGE_WARNING: &str = "usage-warning";
    /// Emitted when usage reaches 100% of the plan limit
    pub const USAGE_LIMIT_REACHED: &str = "usage-limit-reached";
    /// Emitted when translation is blocked due to limit
    pub const USAGE_BLOCKED: &str = "usage-blocked";
}

// ============================================================================
// Event Payloads
// ============================================================================

/// Payload for the usage warning event (80% threshold)
///
/// Sent when the user has consumed 80% or more of their monthly minutes.
/// 
/// Requirement: 10.6
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWarningEvent {
    /// Percentage of plan limit used (e.g., 80.5)
    pub percentage_used: f32,
    /// Total minutes used in current billing period
    pub minutes_used: u32,
    /// Minutes remaining until limit
    pub minutes_remaining: u32,
    /// Monthly minute limit from plan
    pub minutes_limit: u32,
    /// User-friendly message in Spanish
    pub message: String,
}

impl UsageWarningEvent {
    /// Create a new warning event from usage stats
    pub fn from_stats(stats: &UsageStats) -> Self {
        Self {
            percentage_used: stats.percentage_used,
            minutes_used: stats.total_minutes_used,
            minutes_remaining: stats.minutes_remaining,
            minutes_limit: stats.minutes_limit,
            message: format!(
                "Has consumido el {:.0}% de tus minutos mensuales ({} de {} minutos). Te quedan {} minutos.",
                stats.percentage_used,
                stats.total_minutes_used,
                stats.minutes_limit,
                stats.minutes_remaining
            ),
        }
    }
}

/// Available upgrade option
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeOption {
    /// Plan identifier
    pub plan_id: String,
    /// Plan display name
    pub plan_name: String,
    /// Monthly price in USD
    pub price_usd: f32,
    /// Monthly minute limit (0 = unlimited)
    pub minutes_limit: u32,
    /// Whether this is the recommended upgrade
    pub is_recommended: bool,
    /// Additional features of this plan
    pub features: Vec<String>,
}

/// Payload for the usage limit reached event (100% threshold)
///
/// Sent when the user has reached their monthly limit.
///
/// Requirements: 10.7, 11.8
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitReachedEvent {
    /// Total minutes used (equals or exceeds limit)
    pub minutes_used: u32,
    /// Monthly minute limit from plan
    pub minutes_limit: u32,
    /// Current plan name
    pub current_plan: String,
    /// Available upgrade options
    pub upgrade_options: Vec<UpgradeOption>,
    /// URL to subscription/upgrade page
    pub upgrade_url: String,
    /// User-friendly message in Spanish
    pub message: String,
}

impl UsageLimitReachedEvent {
    /// Create a new limit reached event from usage stats and plan
    pub fn new(stats: &UsageStats, current_plan: Plan, upgrade_options: Vec<UpgradeOption>) -> Self {
        Self {
            minutes_used: stats.total_minutes_used,
            minutes_limit: stats.minutes_limit,
            current_plan: current_plan.name().to_string(),
            upgrade_options,
            upgrade_url: "https://traductor.app/subscription".to_string(),
            message: format!(
                "Has alcanzado el límite de {} minutos de tu plan {}. Actualiza tu plan para continuar traduciendo.",
                stats.minutes_limit,
                current_plan.name()
            ),
        }
    }
}

/// Payload for the usage blocked event
///
/// Sent when a translation attempt is blocked due to exceeding the limit.
///
/// Requirement: 10.7
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBlockedEvent {
    /// Reason for blocking
    pub reason: String,
    /// URL to subscription/upgrade page
    pub upgrade_url: String,
    /// Current plan name
    pub current_plan: String,
    /// User-friendly message in Spanish
    pub message: String,
    /// Suggested action
    pub suggestion: String,
}

impl UsageBlockedEvent {
    /// Create a new blocked event
    pub fn new(current_plan: Plan) -> Self {
        Self {
            reason: "monthly_limit_exceeded".to_string(),
            upgrade_url: "https://traductor.app/subscription".to_string(),
            current_plan: current_plan.name().to_string(),
            message: format!(
                "La traducción está bloqueada porque has alcanzado el límite mensual de tu plan {}.",
                current_plan.name()
            ),
            suggestion: "Actualiza tu plan o espera al inicio del próximo ciclo de facturación.".to_string(),
        }
    }
}

// ============================================================================
// Usage Limit Notifier
// ============================================================================

/// Notifier for usage limit events
///
/// Tracks whether warnings have been sent to avoid spamming the user,
/// and provides methods to check limits and emit appropriate events.
///
/// Requirements: 10.6, 10.7, 11.8
pub struct UsageLimitNotifier {
    /// Flag to track if the 80% warning was already sent this month
    warning_sent_this_month: Arc<AtomicBool>,
    /// Flag to track if the 100% limit notification was sent this month
    limit_sent_this_month: Arc<AtomicBool>,
    /// Current billing month (format: "YYYY-MM")
    current_month: Arc<std::sync::Mutex<String>>,
}

impl UsageLimitNotifier {
    /// Create a new usage limit notifier
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        let month = format!("{:04}-{:02}", now.year(), now.month());
        
        Self {
            warning_sent_this_month: Arc::new(AtomicBool::new(false)),
            limit_sent_this_month: Arc::new(AtomicBool::new(false)),
            current_month: Arc::new(std::sync::Mutex::new(month)),
        }
    }

    /// Reset notification flags for a new month
    fn reset_for_new_month(&self) {
        let now = chrono::Utc::now();
        let current = format!("{:04}-{:02}", now.year(), now.month());
        
        if let Ok(mut month) = self.current_month.lock() {
            if *month != current {
                *month = current;
                self.warning_sent_this_month.store(false, Ordering::SeqCst);
                self.limit_sent_this_month.store(false, Ordering::SeqCst);
                tracing::info!("Reset usage notification flags for new month");
            }
        }
    }

    /// Check if the warning threshold (80%) has been reached
    pub fn should_send_warning(&self, stats: &UsageStats) -> bool {
        self.reset_for_new_month();
        
        // Only send warning once per month and if threshold is reached
        stats.is_warning_threshold() 
            && !stats.is_limit_reached()
            && !self.warning_sent_this_month.load(Ordering::SeqCst)
    }

    /// Check if the limit (100%) has been reached
    pub fn should_send_limit_reached(&self, stats: &UsageStats) -> bool {
        self.reset_for_new_month();
        
        // Only send limit notification once per month
        stats.is_limit_reached() 
            && !self.limit_sent_this_month.load(Ordering::SeqCst)
    }

    /// Mark that the warning has been sent
    pub fn mark_warning_sent(&self) {
        self.warning_sent_this_month.store(true, Ordering::SeqCst);
        tracing::info!("Marked 80% usage warning as sent for this month");
    }

    /// Mark that the limit reached notification has been sent
    pub fn mark_limit_sent(&self) {
        self.limit_sent_this_month.store(true, Ordering::SeqCst);
        tracing::info!("Marked 100% usage limit notification as sent for this month");
    }

    /// Check if translation can start based on current usage
    ///
    /// Returns true if translation is allowed, false if blocked due to limit.
    ///
    /// Requirement: 10.7 - Block translation at 100%
    pub fn can_start_translation(&self, stats: &UsageStats) -> bool {
        // BYOK has no limit (minutes_limit == 0)
        if stats.minutes_limit == 0 {
            return true;
        }
        
        !stats.is_limit_reached()
    }

    /// Get available upgrade options for the current plan
    ///
    /// Returns a list of plans with higher limits than the current plan.
    pub fn get_upgrade_options(current_plan: Plan) -> Vec<UpgradeOption> {
        let mut options = Vec::new();
        
        match current_plan {
            Plan::ByokFree => {
                // BYOK users might want subscription for convenience
                options.push(UpgradeOption {
                    plan_id: "starter".to_string(),
                    plan_name: "Starter".to_string(),
                    price_usd: 14.99,
                    minutes_limit: 600,
                    is_recommended: false,
                    features: vec![
                        "600 minutos/mes".to_string(),
                        "Sin necesidad de API key propia".to_string(),
                        "Soporte prioritario".to_string(),
                    ],
                });
                options.push(UpgradeOption {
                    plan_id: "pro".to_string(),
                    plan_name: "Pro".to_string(),
                    price_usd: 39.99,
                    minutes_limit: 2000,
                    is_recommended: true,
                    features: vec![
                        "2000 minutos/mes".to_string(),
                        "Sin necesidad de API key propia".to_string(),
                        "Soporte prioritario".to_string(),
                        "Acceso a funciones beta".to_string(),
                    ],
                });
            }
            Plan::Starter => {
                options.push(UpgradeOption {
                    plan_id: "pro".to_string(),
                    plan_name: "Pro".to_string(),
                    price_usd: 39.99,
                    minutes_limit: 2000,
                    is_recommended: true,
                    features: vec![
                        "2000 minutos/mes".to_string(),
                        "Más del triple de minutos".to_string(),
                        "Acceso a funciones beta".to_string(),
                    ],
                });
                // Also offer BYOK as alternative
                options.push(UpgradeOption {
                    plan_id: "byok_free".to_string(),
                    plan_name: "BYOK Free".to_string(),
                    price_usd: 0.0,
                    minutes_limit: 0, // Unlimited
                    is_recommended: false,
                    features: vec![
                        "Minutos ilimitados".to_string(),
                        "Usa tu propia API key de Gemini".to_string(),
                        "Pagas directamente a Google".to_string(),
                    ],
                });
            }
            Plan::Pro => {
                // Pro users can only upgrade to BYOK for unlimited
                options.push(UpgradeOption {
                    plan_id: "byok_free".to_string(),
                    plan_name: "BYOK Free".to_string(),
                    price_usd: 0.0,
                    minutes_limit: 0, // Unlimited
                    is_recommended: true,
                    features: vec![
                        "Minutos ilimitados".to_string(),
                        "Usa tu propia API key de Gemini".to_string(),
                        "Pagas directamente a Google".to_string(),
                        "Sin límites mensuales".to_string(),
                    ],
                });
            }
        }
        
        options
    }

    /// Check usage limits and emit appropriate notifications
    ///
    /// This should be called after stopping a session to notify the user
    /// if they've crossed important thresholds.
    ///
    /// Requirements: 10.6, 10.7, 11.8
    pub fn check_usage_limits<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        stats: &UsageStats,
        plan: Plan,
    ) -> Result<(), tauri::Error> {
        self.reset_for_new_month();
        
        // Check 100% limit first (higher priority)
        if self.should_send_limit_reached(stats) {
            let upgrade_options = Self::get_upgrade_options(plan);
            let event = UsageLimitReachedEvent::new(stats, plan, upgrade_options);
            
            emit_usage_limit_reached(app, event)?;
            self.mark_limit_sent();
            
            tracing::warn!(
                minutes_used = stats.total_minutes_used,
                minutes_limit = stats.minutes_limit,
                "User reached 100% usage limit"
            );
        }
        // Check 80% warning
        else if self.should_send_warning(stats) {
            let event = UsageWarningEvent::from_stats(stats);
            
            emit_usage_warning(app, event)?;
            self.mark_warning_sent();
            
            tracing::info!(
                percentage = stats.percentage_used,
                minutes_used = stats.total_minutes_used,
                minutes_remaining = stats.minutes_remaining,
                "User reached 80% usage threshold"
            );
        }
        
        Ok(())
    }

    /// Emit blocked event when translation is attempted but limit is reached
    ///
    /// Requirement: 10.7
    pub fn emit_translation_blocked<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        plan: Plan,
    ) -> Result<(), tauri::Error> {
        let event = UsageBlockedEvent::new(plan);
        emit_usage_blocked(app, event)?;
        
        tracing::warn!(
            plan = plan.name(),
            "Translation blocked due to usage limit"
        );
        
        Ok(())
    }

    /// Check if warning has been sent this month
    pub fn was_warning_sent(&self) -> bool {
        self.warning_sent_this_month.load(Ordering::SeqCst)
    }

    /// Check if limit notification has been sent this month
    pub fn was_limit_sent(&self) -> bool {
        self.limit_sent_this_month.load(Ordering::SeqCst)
    }

    /// Manually reset all notification flags (for testing or admin purposes)
    pub fn reset_notifications(&self) {
        self.warning_sent_this_month.store(false, Ordering::SeqCst);
        self.limit_sent_this_month.store(false, Ordering::SeqCst);
        tracing::debug!("Usage notification flags manually reset");
    }
}

impl Default for UsageLimitNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for UsageLimitNotifier {
    fn clone(&self) -> Self {
        Self {
            warning_sent_this_month: Arc::clone(&self.warning_sent_this_month),
            limit_sent_this_month: Arc::clone(&self.limit_sent_this_month),
            current_month: Arc::clone(&self.current_month),
        }
    }
}

// Import chrono traits for year/month
use chrono::Datelike;

// ============================================================================
// Event Emission Functions
// ============================================================================

/// Emit usage warning event (80% threshold)
///
/// Requirement: 10.6
pub fn emit_usage_warning<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    event: UsageWarningEvent,
) -> Result<(), tauri::Error> {
    app.emit(usage_event_names::USAGE_WARNING, event)
}

/// Emit usage limit reached event (100% threshold)
///
/// Requirements: 10.7, 11.8
pub fn emit_usage_limit_reached<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    event: UsageLimitReachedEvent,
) -> Result<(), tauri::Error> {
    app.emit(usage_event_names::USAGE_LIMIT_REACHED, event)
}

/// Emit usage blocked event (translation blocked)
///
/// Requirement: 10.7
pub fn emit_usage_blocked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    event: UsageBlockedEvent,
) -> Result<(), tauri::Error> {
    app.emit(usage_event_names::USAGE_BLOCKED, event)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_warning_event_creation() {
        let stats = UsageStats::new(480, 600); // 80%
        let event = UsageWarningEvent::from_stats(&stats);
        
        assert!((event.percentage_used - 80.0).abs() < 0.01);
        assert_eq!(event.minutes_used, 480);
        assert_eq!(event.minutes_remaining, 120);
        assert_eq!(event.minutes_limit, 600);
        assert!(event.message.contains("80%"));
    }

    #[test]
    fn test_usage_limit_reached_event_creation() {
        let stats = UsageStats::new(600, 600); // 100%
        let options = UsageLimitNotifier::get_upgrade_options(Plan::Starter);
        let event = UsageLimitReachedEvent::new(&stats, Plan::Starter, options);
        
        assert_eq!(event.minutes_used, 600);
        assert_eq!(event.minutes_limit, 600);
        assert_eq!(event.current_plan, "Starter");
        assert!(!event.upgrade_options.is_empty());
        assert!(event.message.contains("600 minutos"));
    }

    #[test]
    fn test_usage_blocked_event_creation() {
        let event = UsageBlockedEvent::new(Plan::Starter);
        
        assert_eq!(event.reason, "monthly_limit_exceeded");
        assert_eq!(event.current_plan, "Starter");
        assert!(event.message.contains("bloqueada"));
        assert!(event.upgrade_url.contains("subscription"));
    }

    #[test]
    fn test_notifier_warning_threshold() {
        let notifier = UsageLimitNotifier::new();
        
        // Below 80%
        let stats = UsageStats::new(400, 600);
        assert!(!notifier.should_send_warning(&stats));
        
        // At 80%
        let stats = UsageStats::new(480, 600);
        assert!(notifier.should_send_warning(&stats));
        
        // Mark as sent
        notifier.mark_warning_sent();
        assert!(!notifier.should_send_warning(&stats));
    }

    #[test]
    fn test_notifier_limit_reached() {
        let notifier = UsageLimitNotifier::new();
        
        // Below 100%
        let stats = UsageStats::new(500, 600);
        assert!(!notifier.should_send_limit_reached(&stats));
        
        // At 100%
        let stats = UsageStats::new(600, 600);
        assert!(notifier.should_send_limit_reached(&stats));
        
        // Mark as sent
        notifier.mark_limit_sent();
        assert!(!notifier.should_send_limit_reached(&stats));
    }

    #[test]
    fn test_can_start_translation() {
        let notifier = UsageLimitNotifier::new();
        
        // Below limit
        let stats = UsageStats::new(500, 600);
        assert!(notifier.can_start_translation(&stats));
        
        // At limit
        let stats = UsageStats::new(600, 600);
        assert!(!notifier.can_start_translation(&stats));
        
        // Over limit
        let stats = UsageStats::new(700, 600);
        assert!(!notifier.can_start_translation(&stats));
        
        // BYOK (unlimited)
        let stats = UsageStats::new(10000, 0);
        assert!(notifier.can_start_translation(&stats));
    }

    #[test]
    fn test_upgrade_options_for_starter() {
        let options = UsageLimitNotifier::get_upgrade_options(Plan::Starter);
        
        assert_eq!(options.len(), 2);
        
        // Should have Pro as recommended
        let pro = options.iter().find(|o| o.plan_id == "pro").unwrap();
        assert!(pro.is_recommended);
        assert_eq!(pro.minutes_limit, 2000);
        
        // Should have BYOK as alternative
        let byok = options.iter().find(|o| o.plan_id == "byok_free").unwrap();
        assert!(!byok.is_recommended);
        assert_eq!(byok.minutes_limit, 0); // Unlimited
    }

    #[test]
    fn test_upgrade_options_for_pro() {
        let options = UsageLimitNotifier::get_upgrade_options(Plan::Pro);
        
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].plan_id, "byok_free");
        assert!(options[0].is_recommended);
    }

    #[test]
    fn test_upgrade_options_for_byok() {
        let options = UsageLimitNotifier::get_upgrade_options(Plan::ByokFree);
        
        assert_eq!(options.len(), 2);
        
        // Pro should be recommended for BYOK users who want convenience
        let pro = options.iter().find(|o| o.plan_id == "pro").unwrap();
        assert!(pro.is_recommended);
    }

    #[test]
    fn test_notifier_clone() {
        let notifier = UsageLimitNotifier::new();
        notifier.mark_warning_sent();
        
        let cloned = notifier.clone();
        assert!(cloned.was_warning_sent());
    }

    #[test]
    fn test_reset_notifications() {
        let notifier = UsageLimitNotifier::new();
        
        notifier.mark_warning_sent();
        notifier.mark_limit_sent();
        
        assert!(notifier.was_warning_sent());
        assert!(notifier.was_limit_sent());
        
        notifier.reset_notifications();
        
        assert!(!notifier.was_warning_sent());
        assert!(!notifier.was_limit_sent());
    }

    #[test]
    fn test_event_serialization() {
        let stats = UsageStats::new(480, 600);
        let event = UsageWarningEvent::from_stats(&stats);
        
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("percentageUsed")); // camelCase
        assert!(json.contains("minutesUsed"));
        assert!(json.contains("minutesRemaining"));
    }

    #[test]
    fn test_upgrade_option_serialization() {
        let option = UpgradeOption {
            plan_id: "pro".to_string(),
            plan_name: "Pro".to_string(),
            price_usd: 39.99,
            minutes_limit: 2000,
            is_recommended: true,
            features: vec!["Feature 1".to_string()],
        };
        
        let json = serde_json::to_string(&option).unwrap();
        assert!(json.contains("planId")); // camelCase
        assert!(json.contains("priceUsd"));
        assert!(json.contains("isRecommended"));
    }
}
