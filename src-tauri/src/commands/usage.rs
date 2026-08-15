//! Usage tracking commands
//!
//! Handles usage statistics, history, and limit notifications.
//!
//! Requirements:
//! - 10.6: Notify at 80% usage
//! - 10.7: Block translation at 100% and show upgrade options
//! - 11.4: Show usage dashboard
//! - 11.8: Notify when limit is reached

use tauri::{command, AppHandle, State};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::storage::{
    Plan,
    UsageStats as StorageUsageStats,
    UsageTracker,
    UsageLimitNotifier,
    UpgradeOption,
};

// ============================================================================
// State Types
// ============================================================================

/// Shared state for usage tracking
pub struct UsageState {
    /// Usage tracker instance
    pub tracker: Arc<RwLock<Option<UsageTracker>>>,
    /// Usage limit notifier
    pub notifier: Arc<UsageLimitNotifier>,
    /// Current user plan
    pub current_plan: Arc<RwLock<Plan>>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            tracker: Arc::new(RwLock::new(None)),
            notifier: Arc::new(UsageLimitNotifier::new()),
            current_plan: Arc::new(RwLock::new(Plan::ByokFree)),
        }
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// Usage stats response for frontend
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatsResponse {
    pub current_month: CurrentMonthUsage,
    pub plan: String,
    pub renewal_date: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentMonthUsage {
    pub used: u32,
    pub limit: u32,
    pub percentage: f32,
}

/// Daily usage response for frontend
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageResponse {
    pub date: String,
    pub system_minutes: u32,
    pub user_minutes: u32,
    pub total_minutes: u32,
}

// ============================================================================
// Commands
// ============================================================================

/// Get current usage statistics
///
/// Returns minutes used, limit, percentage, and plan information.
///
/// Requirement: 11.4 - Show dashboard with minutos used vs limit
#[command]
pub async fn get_usage_stats(
    state: State<'_, UsageState>,
) -> Result<UsageStatsResponse, String> {
    let tracker_guard = state.tracker.read().await;
    let plan = *state.current_plan.read().await;
    
    let stats = if let Some(ref tracker) = *tracker_guard {
        tracker.get_current_usage(plan)
            .map_err(|e| format!("Error obteniendo estadísticas de uso: {}", e))?
    } else {
        // Return empty stats if tracker not initialized
        StorageUsageStats::new(0, plan.minutes_limit())
    };
    
    // Calculate renewal date (first day of next month)
    let now = chrono::Utc::now();
    let next_month = if now.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
    };
    
    let renewal_date = next_month
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    
    Ok(UsageStatsResponse {
        current_month: CurrentMonthUsage {
            used: stats.total_minutes_used,
            limit: stats.minutes_limit,
            percentage: stats.percentage_used,
        },
        plan: plan_to_string(plan),
        renewal_date,
    })
}

/// Get usage history for the specified number of days
///
/// Returns daily breakdown of system and user channel usage.
///
/// Requirement: 11.4 - Show gráfica de uso diario de últimos 30 días
#[command]
pub async fn get_usage_history(
    state: State<'_, UsageState>,
    days: u32,
) -> Result<Vec<DailyUsageResponse>, String> {
    let tracker_guard = state.tracker.read().await;
    
    let daily_usage = if let Some(ref tracker) = *tracker_guard {
        tracker.get_daily_usage(days)
            .map_err(|e| format!("Error obteniendo historial de uso: {}", e))?
    } else {
        Vec::new()
    };
    
    let response: Vec<DailyUsageResponse> = daily_usage
        .into_iter()
        .map(|d| DailyUsageResponse {
            date: d.date,
            system_minutes: d.system_minutes,
            user_minutes: d.user_minutes,
            total_minutes: d.total_minutes,
        })
        .collect();
    
    Ok(response)
}

/// Check if translation can start based on current usage
///
/// Returns false if user has reached their monthly limit.
/// BYOK users always return true (no limits).
///
/// Requirement: 10.7 - Block translation at 100%
#[command]
pub async fn can_start_translation(
    state: State<'_, UsageState>,
) -> Result<bool, String> {
    let tracker_guard = state.tracker.read().await;
    let plan = *state.current_plan.read().await;
    
    // BYOK has no limits
    if plan == Plan::ByokFree {
        return Ok(true);
    }
    
    let stats = if let Some(ref tracker) = *tracker_guard {
        tracker.get_current_usage(plan)
            .map_err(|e| format!("Error verificando límites de uso: {}", e))?
    } else {
        // If tracker not initialized, allow translation
        return Ok(true);
    };
    
    Ok(state.notifier.can_start_translation(&stats))
}

/// Get available upgrade options for the current plan
///
/// Returns a list of plans with higher limits.
///
/// Requirement: 10.7 - Show upgrade options when blocked
#[command]
pub async fn get_upgrade_options(
    state: State<'_, UsageState>,
) -> Result<Vec<UpgradeOption>, String> {
    let plan = *state.current_plan.read().await;
    Ok(UsageLimitNotifier::get_upgrade_options(plan))
}

/// Check usage limits and emit notifications if thresholds crossed
///
/// This should be called after each translation session ends.
///
/// Requirements: 10.6, 10.7, 11.8
#[command]
pub async fn check_usage_limits(
    app: AppHandle,
    state: State<'_, UsageState>,
) -> Result<(), String> {
    let tracker_guard = state.tracker.read().await;
    let plan = *state.current_plan.read().await;
    
    let stats = if let Some(ref tracker) = *tracker_guard {
        tracker.get_current_usage(plan)
            .map_err(|e| format!("Error obteniendo estadísticas de uso: {}", e))?
    } else {
        return Ok(());
    };
    
    state.notifier.check_usage_limits(&app, &stats, plan)
        .map_err(|e| format!("Error emitiendo notificaciones de límite: {}", e))?;
    
    Ok(())
}

/// Emit blocked notification when translation is attempted but limit reached
///
/// Requirement: 10.7 - Block translation and show upgrade options
#[command]
pub async fn emit_usage_blocked(
    app: AppHandle,
    state: State<'_, UsageState>,
) -> Result<(), String> {
    let plan = *state.current_plan.read().await;
    
    state.notifier.emit_translation_blocked(&app, plan)
        .map_err(|e| format!("Error emitiendo notificación de bloqueo: {}", e))?;
    
    Ok(())
}

/// Set the current user plan
///
/// Should be called when user logs in or changes plan.
#[command]
pub async fn set_user_plan(
    state: State<'_, UsageState>,
    plan: String,
) -> Result<(), String> {
    let parsed_plan = Plan::from_str(&plan)
        .ok_or_else(|| format!("Plan inválido: {}", plan))?;
    
    let mut plan_guard = state.current_plan.write().await;
    *plan_guard = parsed_plan;
    
    tracing::info!(plan = plan, "User plan updated");
    
    Ok(())
}

/// Reset usage notification flags (for testing or admin purposes)
#[command]
pub async fn reset_usage_notifications(
    state: State<'_, UsageState>,
) -> Result<(), String> {
    state.notifier.reset_notifications();
    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn plan_to_string(plan: Plan) -> String {
    match plan {
        Plan::ByokFree => "byok_free".to_string(),
        Plan::Starter => "starter".to_string(),
        Plan::Pro => "pro".to_string(),
    }
}

// Import chrono traits
use chrono::Datelike;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_to_string() {
        assert_eq!(plan_to_string(Plan::ByokFree), "byok_free");
        assert_eq!(plan_to_string(Plan::Starter), "starter");
        assert_eq!(plan_to_string(Plan::Pro), "pro");
    }

    #[test]
    fn test_usage_state_default() {
        let state = UsageState::default();
        // Notifier should be created
        assert!(!state.notifier.was_warning_sent());
        assert!(!state.notifier.was_limit_sent());
    }
}
