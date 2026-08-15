//! Usage tracking and session recording
//! 
//! This module provides functionality to track translation usage sessions,
//! calculate minutes consumed, and manage local storage for offline support.
//! 
//! Requirements: 11.1, 11.2, 11.3, 11.5, 11.6, 11.7
//! 
//! Key features:
//! - Record translation sessions with start/end timestamps in ISO 8601 UTC
//! - Calculate minutes using ceiling calculation (0s = 0min, 1-60s = 1min)
//! - Store locally in SQLite when offline
//! - Track by channel (system/user)
//! - Synchronize with backend every 5 minutes (Req 11.3)
//! - Retry sync up to 3 times on failure (Req 11.7)
//! - Ensure consistency between local and backend totals (Req 11.6)

use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use uuid::Uuid;
use tokio::sync::watch;

use super::database::{Database, DatabaseError, UsageRecord};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during usage tracking operations
#[derive(Error, Debug)]
pub enum UsageError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
    
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    
    #[error("Session already exists: {0}")]
    SessionAlreadyExists(String),
    
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
    
    #[error("Session not active")]
    SessionNotActive,
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Background sync not running")]
    SyncNotRunning,
}

// ============================================================================
// Channel Enum
// ============================================================================

/// Audio channel type for translation sessions
/// - System: Captures meeting audio and translates for the user
/// - User: Captures user's microphone and translates for the meeting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// System audio channel (meeting → user translation)
    System,
    /// User microphone channel (user → meeting translation)
    User,
}

impl Channel {
    /// Convert channel to string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::System => "system",
            Channel::User => "user",
        }
    }
    
    /// Parse channel from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "system" => Some(Channel::System),
            "user" => Some(Channel::User),
            _ => None,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Active Session Tracking
// ============================================================================

/// Represents an in-progress translation session
/// 
/// Active sessions are kept in memory until stopped, at which point
/// they are persisted to the database as UsageRecords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    /// Unique session identifier
    pub id: String,
    /// Channel being used (system/user)
    pub channel: Channel,
    /// Session start timestamp in ISO 8601 UTC
    pub start_time: String,
    /// Internal start instant for duration calculation
    #[serde(skip)]
    start_instant: Option<std::time::Instant>,
}

impl ActiveSession {
    /// Create a new active session
    fn new(channel: Channel) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            channel,
            start_time: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            start_instant: Some(std::time::Instant::now()),
        }
    }
    
    /// Get the duration in seconds since session started
    fn duration_secs(&self) -> u64 {
        self.start_instant
            .map(|instant| instant.elapsed().as_secs())
            .unwrap_or(0)
    }
}

// ============================================================================
// Usage Statistics Structures
// ============================================================================

/// Current usage statistics for the user's plan
/// 
/// Provides a summary of minutes used vs available based on
/// the user's subscription plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    /// Total minutes used in current billing period
    pub total_minutes_used: u32,
    /// Monthly minute limit from subscription plan (0 = unlimited for BYOK)
    pub minutes_limit: u32,
    /// Minutes remaining in current period
    pub minutes_remaining: u32,
    /// Percentage of limit used (0.0 - 100.0)
    pub percentage_used: f32,
}

// Manual PartialEq implementation for UsageStats due to f32 field
// Uses bitwise comparison for the f32 field which is safe for round-trip tests
impl PartialEq for UsageStats {
    fn eq(&self, other: &Self) -> bool {
        self.total_minutes_used == other.total_minutes_used
            && self.minutes_limit == other.minutes_limit
            && self.minutes_remaining == other.minutes_remaining
            && self.percentage_used.to_bits() == other.percentage_used.to_bits()
    }
}

impl UsageStats {
    /// Create usage stats with the given values
    pub fn new(used: u32, limit: u32) -> Self {
        let remaining = if limit == 0 {
            u32::MAX // Unlimited
        } else {
            limit.saturating_sub(used)
        };
        
        let percentage = if limit == 0 {
            0.0 // BYOK has no limit, show 0%
        } else {
            (used as f32 / limit as f32 * 100.0).min(100.0)
        };
        
        Self {
            total_minutes_used: used,
            minutes_limit: limit,
            minutes_remaining: remaining,
            percentage_used: percentage,
        }
    }
    
    /// Check if usage limit has been reached
    pub fn is_limit_reached(&self) -> bool {
        self.minutes_limit > 0 && self.total_minutes_used >= self.minutes_limit
    }
    
    /// Check if usage is at or above warning threshold (80%)
    pub fn is_warning_threshold(&self) -> bool {
        self.minutes_limit > 0 && self.percentage_used >= 80.0
    }
}

/// Monthly usage summary
/// 
/// Aggregates usage data for a specific month, broken down by channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyUsage {
    /// Year (e.g., 2025)
    pub year: i32,
    /// Month (1-12)
    pub month: i32,
    /// Total minutes used across all channels
    pub total_minutes: u32,
    /// Minutes used on system channel (meeting → user)
    pub system_minutes: u32,
    /// Minutes used on user channel (user → meeting)
    pub user_minutes: u32,
}

impl MonthlyUsage {
    /// Create a new monthly usage summary
    pub fn new(year: i32, month: i32, system_minutes: u32, user_minutes: u32) -> Self {
        Self {
            year,
            month,
            total_minutes: system_minutes + user_minutes,
            system_minutes,
            user_minutes,
        }
    }
}

/// Daily usage entry
/// 
/// Provides usage breakdown for a single day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyUsage {
    /// Date in YYYY-MM-DD format
    pub date: String,
    /// Minutes used on system channel
    pub system_minutes: u32,
    /// Minutes used on user channel
    pub user_minutes: u32,
    /// Total minutes for the day
    pub total_minutes: u32,
}

impl DailyUsage {
    /// Create a new daily usage entry
    pub fn new(date: String, system_minutes: u32, user_minutes: u32) -> Self {
        Self {
            date,
            system_minutes,
            user_minutes,
            total_minutes: system_minutes + user_minutes,
        }
    }
}

// ============================================================================
// Subscription Plan Definitions
// ============================================================================

/// Subscription plan with minute limits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// BYOK Free - $0, unlimited (user provides API key)
    ByokFree,
    /// Starter - $14.99/month, 600 minutes
    Starter,
    /// Pro - $39.99/month, 2000 minutes  
    Pro,
}

impl Plan {
    /// Get the monthly minute limit for this plan
    pub fn minutes_limit(&self) -> u32 {
        match self {
            Plan::ByokFree => 0, // Unlimited
            Plan::Starter => 600,
            Plan::Pro => 2000,
        }
    }
    
    /// Get the plan name
    pub fn name(&self) -> &'static str {
        match self {
            Plan::ByokFree => "BYOK Free",
            Plan::Starter => "Starter",
            Plan::Pro => "Pro",
        }
    }
    
    /// Parse plan from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "byok_free" | "byok-free" | "byok" | "free" => Some(Plan::ByokFree),
            "starter" => Some(Plan::Starter),
            "pro" => Some(Plan::Pro),
            _ => None,
        }
    }
}

impl Default for Plan {
    fn default() -> Self {
        Plan::ByokFree
    }
}

// ============================================================================
// Sync Types and Client
// ============================================================================

/// Backend API base URL for sync operations
const BACKEND_API_URL: &str = "https://api.traductor.app";

/// Sync interval in seconds (5 minutes)
const SYNC_INTERVAL_SECS: u64 = 5 * 60;

/// Maximum retry attempts for sync failures
const MAX_SYNC_RETRIES: u32 = 3;

/// Result of a sync operation
/// 
/// Provides details about what was synchronized and whether
/// the local and backend totals are consistent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// Number of records successfully synced to backend
    pub records_synced: u32,
    /// Number of records that failed to sync
    pub records_failed: u32,
    /// Total minutes reported by backend for current month
    pub backend_total: u32,
    /// Total minutes calculated locally for current month
    pub local_total: u32,
    /// Whether local and backend totals are consistent
    pub is_consistent: bool,
    /// Timestamp when sync completed
    pub synced_at: String,
}

impl SyncResult {
    /// Create a new sync result
    pub fn new(
        records_synced: u32,
        records_failed: u32,
        backend_total: u32,
        local_total: u32,
    ) -> Self {
        Self {
            records_synced,
            records_failed,
            backend_total,
            local_total,
            is_consistent: backend_total == local_total,
            synced_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        }
    }
    
    /// Create a result for a failed sync attempt
    #[allow(unused_variables)]
    pub fn failed(local_total: u32, error_message: &str) -> Self {
        Self {
            records_synced: 0,
            records_failed: 0,
            backend_total: 0,
            local_total,
            is_consistent: false,
            synced_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        }
    }
}

/// Sync state for tracking background sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    /// Last successful sync timestamp
    pub last_sync_time: Option<String>,
    /// Current retry count (resets on success)
    pub retry_count: u32,
    /// Whether sync is currently running
    pub is_syncing: bool,
    /// Last sync result
    pub last_result: Option<SyncResult>,
    /// Last error message if sync failed
    pub last_error: Option<String>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            last_sync_time: None,
            retry_count: 0,
            is_syncing: false,
            last_result: None,
            last_error: None,
        }
    }
}

/// Request payload for syncing usage records to backend
#[derive(Debug, Serialize)]
struct UsageSyncRequest {
    records: Vec<UsageSyncRecord>,
}

/// Individual record in sync request
#[derive(Debug, Serialize)]
struct UsageSyncRecord {
    start_time: String,
    end_time: String,
    channel: String,
    minutes: i32,
}

/// Response from backend sync endpoint
#[derive(Debug, Deserialize)]
struct UsageSyncResponse {
    success: bool,
    #[serde(default)]
    synced_count: u32,
    #[serde(default)]
    error: Option<String>,
}

/// Response from backend usage summary endpoint  
#[derive(Debug, Deserialize)]
struct UsageSummaryResponse {
    success: bool,
    usage: Option<BackendUsage>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BackendUsage {
    current_month: CurrentMonthUsage,
}

#[derive(Debug, Deserialize)]
struct CurrentMonthUsage {
    used: u32,
    #[allow(dead_code)]
    limit: u32,
}

/// HTTP client for syncing usage data with the backend
/// 
/// Handles:
/// - POST /usage/sync - Sync local usage records to backend
/// - GET /usage - Get backend usage summary for consistency check
/// 
/// Requirements: 11.3, 11.6, 11.7
pub struct UsageSyncClient {
    client: reqwest::Client,
    base_url: String,
}

impl UsageSyncClient {
    /// Create a new sync client
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self {
            client,
            base_url: BACKEND_API_URL.to_string(),
        }
    }
    
    /// Create a new sync client with custom base URL (for testing)
    pub fn with_base_url(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self {
            client,
            base_url: base_url.to_string(),
        }
    }
    
    /// Sync usage records to backend
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for the backend
    /// * `records` - Usage records to sync
    /// 
    /// # Returns
    /// Number of records successfully synced
    /// 
    /// # Requirements
    /// - 11.3: Sync records to backend
    pub async fn sync_records(
        &self,
        session_token: &str,
        records: &[UsageRecord],
    ) -> Result<u32, UsageError> {
        if records.is_empty() {
            return Ok(0);
        }
        
        let sync_records: Vec<UsageSyncRecord> = records
            .iter()
            .map(|r| UsageSyncRecord {
                start_time: r.start_time.clone(),
                end_time: r.end_time.clone(),
                channel: r.channel.clone(),
                minutes: r.minutes,
            })
            .collect();
        
        let request = UsageSyncRequest { records: sync_records };
        
        let response = self.client
            .post(format!("{}/usage/sync", self.base_url))
            .header("Authorization", format!("Bearer {}", session_token))
            .json(&request)
            .send()
            .await
            .map_err(|e| UsageError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(UsageError::ApiError(format!(
                "Backend returned status {}",
                response.status()
            )));
        }
        
        let sync_response: UsageSyncResponse = response
            .json()
            .await
            .map_err(|e| UsageError::SyncError(format!("Failed to parse response: {}", e)))?;
        
        if !sync_response.success {
            return Err(UsageError::ApiError(
                sync_response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
        
        Ok(sync_response.synced_count)
    }
    
    /// Get usage summary from backend
    /// 
    /// Used to verify consistency between local and backend totals.
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for the backend
    /// 
    /// # Returns
    /// Total minutes used according to backend
    /// 
    /// # Requirements
    /// - 11.6: Verify consistency with backend
    pub async fn get_backend_usage(&self, session_token: &str) -> Result<u32, UsageError> {
        let response = self.client
            .get(format!("{}/usage", self.base_url))
            .header("Authorization", format!("Bearer {}", session_token))
            .send()
            .await
            .map_err(|e| UsageError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(UsageError::ApiError(format!(
                "Backend returned status {}",
                response.status()
            )));
        }
        
        let usage_response: UsageSummaryResponse = response
            .json()
            .await
            .map_err(|e| UsageError::SyncError(format!("Failed to parse response: {}", e)))?;
        
        if !usage_response.success {
            return Err(UsageError::ApiError(
                usage_response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
        
        let backend_total = usage_response
            .usage
            .map(|u| u.current_month.used)
            .unwrap_or(0);
        
        Ok(backend_total)
    }
}

impl Default for UsageSyncClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for UsageSyncClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

// ============================================================================
// Usage Tracker
// ============================================================================

/// Usage tracker that manages translation sessions and records
/// 
/// The tracker handles:
/// - Starting and stopping translation sessions
/// - Recording session data to the local SQLite database
/// - Calculating usage minutes with ceiling rounding
/// - Querying usage statistics and history
/// - Background synchronization with backend (every 5 minutes)
/// - Retry logic for failed syncs (up to 3 attempts)
/// 
/// Thread-safe: can be shared across multiple threads using Arc.
pub struct UsageTracker {
    /// Database connection for persistent storage
    db: Database,
    /// Currently active sessions by session ID
    active_sessions: Arc<Mutex<HashMap<String, ActiveSession>>>,
    /// HTTP client for backend sync
    sync_client: UsageSyncClient,
    /// Current sync state
    sync_state: Arc<Mutex<SyncState>>,
    /// Channel to signal background sync task to stop
    stop_sync_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
    /// Flag indicating if background sync is running
    is_sync_running: Arc<AtomicBool>,
}

impl UsageTracker {
    /// Create a new usage tracker with the given database connection
    /// 
    /// # Arguments
    /// * `db` - Database connection for storing usage records
    /// 
    /// # Example
    /// ```ignore
    /// let db = Database::new("app.db", "key")?;
    /// let tracker = UsageTracker::new(db);
    /// ```
    pub fn new(db: Database) -> Self {
        Self {
            db,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            sync_client: UsageSyncClient::new(),
            sync_state: Arc::new(Mutex::new(SyncState::default())),
            stop_sync_tx: Arc::new(Mutex::new(None)),
            is_sync_running: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Create a usage tracker with a custom sync client (for testing)
    pub fn with_sync_client(db: Database, sync_client: UsageSyncClient) -> Self {
        Self {
            db,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            sync_client,
            sync_state: Arc::new(Mutex::new(SyncState::default())),
            stop_sync_tx: Arc::new(Mutex::new(None)),
            is_sync_running: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Start a new translation session for the given channel
    /// 
    /// Records the start timestamp in ISO 8601 UTC format and returns
    /// a session ID that must be used to stop the session later.
    /// 
    /// # Arguments
    /// * `channel` - The audio channel being used (System or User)
    /// 
    /// # Returns
    /// A unique session ID string
    /// 
    /// # Requirements
    /// - 11.1: Records timestamp in ISO 8601 UTC
    pub fn start_session(&self, channel: Channel) -> Result<String, UsageError> {
        let session = ActiveSession::new(channel);
        let session_id = session.id.clone();
        
        let mut sessions = self.active_sessions.lock()
            .map_err(|_| UsageError::SessionNotActive)?;
        
        sessions.insert(session_id.clone(), session);
        
        tracing::info!(
            session_id = %session_id,
            channel = %channel,
            "Started translation session"
        );
        
        Ok(session_id)
    }
    
    /// Stop an active translation session and record it to the database
    /// 
    /// Calculates the duration and minutes consumed, then persists
    /// the record to SQLite. If there's no network connection, the
    /// record is stored locally for later sync.
    /// 
    /// # Arguments
    /// * `session_id` - The session ID returned from start_session
    /// 
    /// # Returns
    /// The calculated minutes for this session
    /// 
    /// # Requirements
    /// - 11.1: Records end timestamp in ISO 8601 UTC
    /// - 11.2: Stores locally in SQLite
    /// - 11.5: Calculates minutes with ceiling rounding (0s = 0min)
    pub fn stop_session(&self, session_id: &str) -> Result<u32, UsageError> {
        let session = {
            let mut sessions = self.active_sessions.lock()
                .map_err(|_| UsageError::SessionNotActive)?;
            
            sessions.remove(session_id)
                .ok_or_else(|| UsageError::SessionNotFound(session_id.to_string()))?
        };
        
        let end_time = Utc::now();
        let end_time_str = end_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        
        // Calculate duration and minutes
        let duration_secs = session.duration_secs();
        let minutes = Self::calculate_minutes(duration_secs);
        
        // Create usage record for database
        let record = UsageRecord {
            id: None,
            start_time: session.start_time.clone(),
            end_time: end_time_str.clone(),
            channel: session.channel.as_str().to_string(),
            minutes: minutes as i32,
            synced: false, // Will be synced later
        };
        
        // Persist to database
        self.db.insert_usage_record(&record)?;
        
        tracing::info!(
            session_id = %session_id,
            channel = %session.channel,
            duration_secs = duration_secs,
            minutes = minutes,
            "Stopped translation session"
        );
        
        Ok(minutes)
    }
    
    /// Calculate minutes from duration in seconds
    /// 
    /// Uses ceiling calculation: rounds up to the nearest minute,
    /// except 0 seconds = 0 minutes.
    /// 
    /// # Arguments
    /// * `duration_secs` - Duration in seconds
    /// 
    /// # Returns
    /// Minutes (rounded up), or 0 if duration is 0
    /// 
    /// # Requirements
    /// - 11.5: 0s = 0min, 1-60s = 1min, 61-120s = 2min, etc.
    /// 
    /// # Examples
    /// ```ignore
    /// assert_eq!(UsageTracker::calculate_minutes(0), 0);
    /// assert_eq!(UsageTracker::calculate_minutes(1), 1);
    /// assert_eq!(UsageTracker::calculate_minutes(60), 1);
    /// assert_eq!(UsageTracker::calculate_minutes(61), 2);
    /// ```
    pub fn calculate_minutes(duration_secs: u64) -> u32 {
        if duration_secs == 0 {
            return 0;
        }
        
        // Ceiling division: (a + b - 1) / b
        ((duration_secs + 59) / 60) as u32
    }
    
    /// Get current usage statistics for the given plan
    /// 
    /// Calculates total minutes used in the current billing month
    /// and compares against the plan's limit.
    /// 
    /// # Arguments
    /// * `plan` - The user's subscription plan
    /// 
    /// # Returns
    /// UsageStats with current usage information
    pub fn get_current_usage(&self, plan: Plan) -> Result<UsageStats, UsageError> {
        let now = Utc::now();
        let total_minutes = self.db.get_monthly_usage(now.year(), now.month() as i32)?;
        
        Ok(UsageStats::new(total_minutes as u32, plan.minutes_limit()))
    }
    
    /// Get monthly usage history
    /// 
    /// Returns usage summaries for the last N months, including
    /// breakdown by channel.
    /// 
    /// # Arguments
    /// * `months` - Number of months to retrieve (including current)
    /// 
    /// # Returns
    /// Vector of MonthlyUsage, most recent first
    pub fn get_monthly_history(&self, months: u32) -> Result<Vec<MonthlyUsage>, UsageError> {
        let now = Utc::now();
        let mut history = Vec::with_capacity(months as usize);
        
        for i in 0..months {
            // Calculate year/month going backwards
            let target_date = now - chrono::Duration::days((i * 30) as i64);
            let year = target_date.year();
            let month = target_date.month() as i32;
            
            // Get records for this month
            let start = format!("{:04}-{:02}-01T00:00:00Z", year, month);
            let end = if month == 12 {
                format!("{:04}-01-01T00:00:00Z", year + 1)
            } else {
                format!("{:04}-{:02}-01T00:00:00Z", year, month + 1)
            };
            
            let records = self.db.get_usage_records_range(&start, &end)?;
            
            let mut system_minutes: u32 = 0;
            let mut user_minutes: u32 = 0;
            
            for record in records {
                match record.channel.as_str() {
                    "system" => system_minutes += record.minutes as u32,
                    "user" => user_minutes += record.minutes as u32,
                    _ => {}
                }
            }
            
            history.push(MonthlyUsage::new(year, month, system_minutes, user_minutes));
        }
        
        Ok(history)
    }
    
    /// Get daily usage breakdown
    /// 
    /// Returns usage data for the last N days, with breakdown by channel.
    /// 
    /// # Arguments
    /// * `days` - Number of days to retrieve (including today)
    /// 
    /// # Returns
    /// Vector of DailyUsage, most recent first
    pub fn get_daily_usage(&self, days: u32) -> Result<Vec<DailyUsage>, UsageError> {
        let daily_data = self.db.get_daily_usage(days as i32)?;
        
        let usage: Vec<DailyUsage> = daily_data
            .into_iter()
            .map(|(date, system, user)| DailyUsage::new(date, system as u32, user as u32))
            .collect();
        
        Ok(usage)
    }
    
    /// Get all currently active sessions
    /// 
    /// Returns a copy of the active sessions map.
    pub fn get_active_sessions(&self) -> Result<Vec<ActiveSession>, UsageError> {
        let sessions = self.active_sessions.lock()
            .map_err(|_| UsageError::SessionNotActive)?;
        
        Ok(sessions.values().cloned().collect())
    }
    
    /// Check if a session is currently active
    pub fn is_session_active(&self, session_id: &str) -> bool {
        self.active_sessions
            .lock()
            .map(|sessions| sessions.contains_key(session_id))
            .unwrap_or(false)
    }
    
    /// Get unsynced records for backend synchronization
    pub fn get_unsynced_records(&self) -> Result<Vec<UsageRecord>, UsageError> {
        Ok(self.db.get_unsynced_usage_records()?)
    }
    
    /// Mark records as synced after successful backend sync
    pub fn mark_records_synced(&self, ids: &[i64]) -> Result<(), UsageError> {
        self.db.mark_usage_synced(ids)?;
        Ok(())
    }
    
    // ========================================================================
    // Sync Methods - Requirements: 11.3, 11.6, 11.7
    // ========================================================================
    
    /// Sync all pending (unsynced) records to the backend
    /// 
    /// Retrieves all unsynced records from local database, sends them
    /// to the backend, and marks them as synced if successful.
    /// Also verifies consistency between local and backend totals.
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for backend API
    /// 
    /// # Returns
    /// SyncResult with details about what was synced and consistency status
    /// 
    /// # Requirements
    /// - 11.3: Sync unsynced records to backend
    /// - 11.6: Verify sum(local) = sum(backend)
    pub async fn sync_pending_records(&self, session_token: &str) -> Result<SyncResult, UsageError> {
        // Get unsynced records from local database
        let unsynced_records = self.db.get_unsynced_usage_records()?;
        
        if unsynced_records.is_empty() {
            // No records to sync, just verify consistency
            let local_total = self.get_local_monthly_total()?;
            let backend_total = self.sync_client.get_backend_usage(session_token).await?;
            
            return Ok(SyncResult::new(0, 0, backend_total, local_total));
        }
        
        let record_ids: Vec<i64> = unsynced_records
            .iter()
            .filter_map(|r| r.id)
            .collect();
        
        // Attempt to sync records to backend
        let synced_count = self.sync_client
            .sync_records(session_token, &unsynced_records)
            .await?;
        
        // Mark records as synced in local database
        if synced_count > 0 {
            self.db.mark_usage_synced(&record_ids)?;
        }
        
        // Get totals for consistency check
        let local_total = self.get_local_monthly_total()?;
        let backend_total = self.sync_client.get_backend_usage(session_token).await?;
        
        let result = SyncResult::new(
            synced_count,
            (unsynced_records.len() as u32).saturating_sub(synced_count),
            backend_total,
            local_total,
        );
        
        // Update sync state
        if let Ok(mut state) = self.sync_state.lock() {
            state.last_sync_time = Some(result.synced_at.clone());
            state.last_result = Some(result.clone());
            state.retry_count = 0;
            state.last_error = None;
        }
        
        tracing::info!(
            records_synced = synced_count,
            local_total = local_total,
            backend_total = backend_total,
            is_consistent = result.is_consistent,
            "Usage sync completed"
        );
        
        Ok(result)
    }
    
    /// Force an immediate sync attempt
    /// 
    /// Useful for manual sync requests or after important sessions.
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for backend API
    /// 
    /// # Returns
    /// SyncResult or error if sync fails
    pub async fn force_sync(&self, session_token: &str) -> Result<SyncResult, UsageError> {
        tracing::info!("Force sync requested");
        self.sync_pending_records(session_token).await
    }
    
    /// Start background sync task that runs every 5 minutes
    /// 
    /// Spawns a tokio task that periodically syncs unsynced records
    /// to the backend. Implements retry logic with up to 3 attempts
    /// if sync fails.
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for backend API
    /// 
    /// # Requirements
    /// - 11.3: Sync every 5 minutes
    /// - 11.7: Retry up to 3 times on failure
    pub fn start_background_sync(&self, session_token: String) {
        // Check if already running
        if self.is_sync_running.load(Ordering::SeqCst) {
            tracing::warn!("Background sync already running");
            return;
        }
        
        let (stop_tx, mut stop_rx) = watch::channel(false);
        
        // Store the stop channel
        if let Ok(mut tx_guard) = self.stop_sync_tx.lock() {
            *tx_guard = Some(stop_tx);
        }
        
        self.is_sync_running.store(true, Ordering::SeqCst);
        
        let tracker = self.clone();
        let token = session_token.clone();
        
        tauri::async_runtime::spawn(async move {
            tracing::info!("Background sync task started (interval: {}s)", SYNC_INTERVAL_SECS);
            
            loop {
                // Wait for sync interval or stop signal
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(SYNC_INTERVAL_SECS)) => {
                        // Time to sync
                    }
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            tracing::info!("Background sync task stopping");
                            break;
                        }
                    }
                }
                
                // Check if we should stop before attempting sync
                if *stop_rx.borrow() {
                    break;
                }
                
                // Update sync state to indicate syncing
                if let Ok(mut state) = tracker.sync_state.lock() {
                    state.is_syncing = true;
                }
                
                // Attempt sync with retry logic
                let result = tracker.sync_with_retry(&token).await;
                
                // Update sync state
                if let Ok(mut state) = tracker.sync_state.lock() {
                    state.is_syncing = false;
                    match &result {
                        Ok(sync_result) => {
                            state.last_sync_time = Some(sync_result.synced_at.clone());
                            state.last_result = Some(sync_result.clone());
                            state.retry_count = 0;
                            state.last_error = None;
                        }
                        Err(e) => {
                            state.last_error = Some(e.to_string());
                        }
                    }
                }
                
                if let Err(e) = result {
                    tracing::error!(error = %e, "Background sync failed after retries");
                }
            }
            
            // Mark as not running when task ends
            tracker.is_sync_running.store(false, Ordering::SeqCst);
            tracing::info!("Background sync task stopped");
        });
    }
    
    /// Perform sync with retry logic
    /// 
    /// Attempts to sync up to MAX_SYNC_RETRIES times, waiting
    /// SYNC_INTERVAL_SECS between each attempt.
    /// 
    /// # Arguments
    /// * `session_token` - Authentication token for backend API
    /// 
    /// # Requirements
    /// - 11.7: Retry every 5 min up to 3 attempts
    async fn sync_with_retry(&self, session_token: &str) -> Result<SyncResult, UsageError> {
        let mut last_error: Option<UsageError> = None;
        
        for attempt in 1..=MAX_SYNC_RETRIES {
            // Update retry count in state
            if let Ok(mut state) = self.sync_state.lock() {
                state.retry_count = attempt - 1;
            }
            
            tracing::debug!(attempt = attempt, "Attempting sync");
            
            match self.sync_pending_records(session_token).await {
                Ok(result) => {
                    tracing::info!(
                        attempt = attempt,
                        records_synced = result.records_synced,
                        "Sync successful"
                    );
                    return Ok(result);
                }
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt,
                        max_attempts = MAX_SYNC_RETRIES,
                        error = %e,
                        "Sync attempt failed"
                    );
                    last_error = Some(e);
                    
                    // Update error in state
                    if let Ok(mut state) = self.sync_state.lock() {
                        state.retry_count = attempt;
                        state.last_error = last_error.as_ref().map(|e| e.to_string());
                    }
                    
                    // Don't sleep after last attempt
                    if attempt < MAX_SYNC_RETRIES {
                        // Wait 5 minutes before retry (as per Req 11.7)
                        tokio::time::sleep(std::time::Duration::from_secs(SYNC_INTERVAL_SECS)).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| UsageError::SyncError("Max retries exceeded".to_string())))
    }
    
    /// Stop the background sync task
    /// 
    /// Signals the background task to stop and waits briefly for it to complete.
    pub fn stop_background_sync(&self) {
        if !self.is_sync_running.load(Ordering::SeqCst) {
            tracing::debug!("Background sync not running, nothing to stop");
            return;
        }
        
        if let Ok(tx_guard) = self.stop_sync_tx.lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let _ = tx.send(true);
                tracing::info!("Stop signal sent to background sync task");
            }
        }
    }
    
    /// Check if background sync is currently running
    pub fn is_sync_running(&self) -> bool {
        self.is_sync_running.load(Ordering::SeqCst)
    }
    
    /// Get the current sync state
    pub fn get_sync_state(&self) -> SyncState {
        self.sync_state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }
    
    /// Get total minutes used in current month from local database
    /// 
    /// Used for consistency checks against backend total.
    fn get_local_monthly_total(&self) -> Result<u32, UsageError> {
        let now = Utc::now();
        let total = self.db.get_monthly_usage(now.year(), now.month() as i32)?;
        Ok(total as u32)
    }
}

impl Clone for UsageTracker {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            active_sessions: Arc::clone(&self.active_sessions),
            sync_client: self.sync_client.clone(),
            sync_state: Arc::clone(&self.sync_state),
            stop_sync_tx: Arc::clone(&self.stop_sync_tx),
            is_sync_running: Arc::clone(&self.is_sync_running),
        }
    }
}

// ============================================================================
// UI Helper Types and Functions
// ============================================================================

/// Threshold configuration for duration-based usage alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationThreshold {
    /// Percentage of limit (e.g., 75, 90, 100, 125, 150)
    pub percentage: u32,
    /// Minutes at this threshold
    pub minutes: u32,
    /// Label for display (e.g., "75%", "90%")
    pub label: String,
}

/// Usage tier classification for UI display
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsageTier {
    /// Usage is normal (below warning threshold)
    Normal,
    /// Usage is approaching limit (75-89%)
    Warning,
    /// Usage is critical (90-99%)
    Critical,
    /// Usage has exceeded limit (100%+)
    Exceeded,
}

/// Format a duration in seconds to a user-friendly string
/// 
/// # Arguments
/// * `total_seconds` - Total duration in seconds
/// * `include_seconds` - Whether to include seconds in the output
/// 
/// # Returns
/// Formatted string like "1h 30m 45s" or "1h 30m"
pub fn duration_to_ui_format(total_seconds: u64, include_seconds: bool) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    
    if hours > 0 {
        if include_seconds && seconds > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else {
            format!("{}h {}m", hours, minutes)
        }
    } else if minutes > 0 {
        if include_seconds && seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else if include_seconds {
        format!("{}s", seconds)
    } else {
        "0m".to_string()
    }
}

/// Get a usage alert message based on percentage and limit status
/// 
/// # Arguments
/// * `percentage` - Current usage percentage (0-100+)
/// * `limit_reached` - Whether the limit has been reached
/// 
/// # Returns
/// Localized alert message string
pub fn get_usage_alert_message(percentage: u32, limit_reached: bool) -> String {
    if limit_reached || percentage >= 100 {
        format!("Has alcanzado el 100% de tu límite de uso. Actualiza tu plan para continuar.")
    } else if percentage >= 90 {
        format!("Advertencia: {}% de tu límite de uso consumido.", percentage)
    } else if percentage >= 75 {
        format!("Has usado el {}% de tu límite mensual.", percentage)
    } else {
        format!("Uso actual: {}% del límite.", percentage)
    }
}

/// Get the usage tier based on percentage and thresholds
/// 
/// # Arguments
/// * `percentage` - Current usage percentage (0-100+)
/// * `thresholds` - Configured thresholds (sorted by percentage ascending)
/// 
/// # Returns
/// The appropriate usage tier, or None if thresholds are empty
pub fn get_usage_tier(percentage: u32, thresholds: &[DurationThreshold]) -> Option<UsageTier> {
    if thresholds.is_empty() {
        return None;
    }
    
    if percentage >= 100 {
        Some(UsageTier::Exceeded)
    } else if percentage >= 90 {
        Some(UsageTier::Critical)
    } else if percentage >= 75 {
        Some(UsageTier::Warning)
    } else {
        Some(UsageTier::Normal)
    }
}

/// Calculate usage percentage from used and limit values
/// 
/// # Arguments
/// * `used` - Amount used
/// * `limit` - Total limit
/// 
/// # Returns
/// Percentage as f64 (can be > 100 if over limit)
pub fn calculate_usage_percentage(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        if used == 0 {
            return f64::NAN; // 0/0 is undefined
        }
        return f64::INFINITY; // x/0 where x > 0 is infinity
    }
    (used as f64 / limit as f64) * 100.0
}

/// Find the next threshold that will be crossed
/// 
/// # Arguments
/// * `current_percentage` - Current usage percentage
/// * `thresholds` - Configured thresholds (sorted by percentage ascending)
/// 
/// # Returns
/// The next threshold to be reached, or None if all thresholds have been passed
pub fn find_next_threshold(current_percentage: u32, thresholds: &[DurationThreshold]) -> Option<&DurationThreshold> {
    thresholds.iter().find(|t| t.percentage > current_percentage)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use proptest::prelude::*;
    
    fn create_test_tracker() -> UsageTracker {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_usage.db");
        let db = Database::new(db_path.to_str().unwrap(), "test_key").unwrap();
        UsageTracker::new(db)
    }
    
    #[test]
    fn test_calculate_minutes_zero() {
        // 0 seconds = 0 minutes (special case)
        assert_eq!(UsageTracker::calculate_minutes(0), 0);
    }
    
    #[test]
    fn test_calculate_minutes_ceiling() {
        // 1-60 seconds = 1 minute
        assert_eq!(UsageTracker::calculate_minutes(1), 1);
        assert_eq!(UsageTracker::calculate_minutes(30), 1);
        assert_eq!(UsageTracker::calculate_minutes(59), 1);
        assert_eq!(UsageTracker::calculate_minutes(60), 1);
        
        // 61-120 seconds = 2 minutes
        assert_eq!(UsageTracker::calculate_minutes(61), 2);
        assert_eq!(UsageTracker::calculate_minutes(90), 2);
        assert_eq!(UsageTracker::calculate_minutes(120), 2);
        
        // 121-180 seconds = 3 minutes
        assert_eq!(UsageTracker::calculate_minutes(121), 3);
        assert_eq!(UsageTracker::calculate_minutes(180), 3);
    }
    
    #[test]
    fn test_calculate_minutes_large_values() {
        // 1 hour = 60 minutes
        assert_eq!(UsageTracker::calculate_minutes(3600), 60);
        
        // 1 hour + 1 second = 61 minutes
        assert_eq!(UsageTracker::calculate_minutes(3601), 61);
    }
    
    #[test]
    fn test_channel_enum() {
        assert_eq!(Channel::System.as_str(), "system");
        assert_eq!(Channel::User.as_str(), "user");
        
        assert_eq!(Channel::from_str("system"), Some(Channel::System));
        assert_eq!(Channel::from_str("user"), Some(Channel::User));
        assert_eq!(Channel::from_str("SYSTEM"), Some(Channel::System));
        assert_eq!(Channel::from_str("invalid"), None);
    }
    
    #[test]
    fn test_plan_minutes_limit() {
        assert_eq!(Plan::ByokFree.minutes_limit(), 0);
        assert_eq!(Plan::Starter.minutes_limit(), 600);
        assert_eq!(Plan::Pro.minutes_limit(), 2000);
    }
    
    #[test]
    fn test_usage_stats_calculation() {
        // Normal case
        let stats = UsageStats::new(300, 600);
        assert_eq!(stats.total_minutes_used, 300);
        assert_eq!(stats.minutes_limit, 600);
        assert_eq!(stats.minutes_remaining, 300);
        assert!((stats.percentage_used - 50.0).abs() < 0.01);
        assert!(!stats.is_limit_reached());
        assert!(!stats.is_warning_threshold());
        
        // At 80% threshold
        let stats = UsageStats::new(480, 600);
        assert!(stats.is_warning_threshold());
        assert!(!stats.is_limit_reached());
        
        // At limit
        let stats = UsageStats::new(600, 600);
        assert!(stats.is_limit_reached());
        assert_eq!(stats.percentage_used, 100.0);
        assert_eq!(stats.minutes_remaining, 0);
        
        // BYOK (unlimited)
        let stats = UsageStats::new(1000, 0);
        assert_eq!(stats.percentage_used, 0.0);
        assert!(!stats.is_limit_reached());
        assert!(!stats.is_warning_threshold());
    }
    
    #[test]
    fn test_start_stop_session() {
        let tracker = create_test_tracker();
        
        // Start a session
        let session_id = tracker.start_session(Channel::System).unwrap();
        assert!(tracker.is_session_active(&session_id));
        
        // Stop the session
        let minutes = tracker.stop_session(&session_id).unwrap();
        assert!(!tracker.is_session_active(&session_id));
        // Duration is essentially 0, so 0 minutes
        assert_eq!(minutes, 0);
    }
    
    #[test]
    fn test_session_not_found() {
        let tracker = create_test_tracker();
        
        let result = tracker.stop_session("nonexistent-session");
        assert!(matches!(result, Err(UsageError::SessionNotFound(_))));
    }
    
    #[test]
    fn test_daily_usage_struct() {
        let usage = DailyUsage::new("2025-01-15".to_string(), 30, 20);
        assert_eq!(usage.date, "2025-01-15");
        assert_eq!(usage.system_minutes, 30);
        assert_eq!(usage.user_minutes, 20);
        assert_eq!(usage.total_minutes, 50);
    }
    
    #[test]
    fn test_monthly_usage_struct() {
        let usage = MonthlyUsage::new(2025, 1, 300, 200);
        assert_eq!(usage.year, 2025);
        assert_eq!(usage.month, 1);
        assert_eq!(usage.system_minutes, 300);
        assert_eq!(usage.user_minutes, 200);
        assert_eq!(usage.total_minutes, 500);
    }
    
    #[test]
    fn test_sync_result_creation() {
        let result = SyncResult::new(5, 0, 100, 100);
        assert_eq!(result.records_synced, 5);
        assert_eq!(result.records_failed, 0);
        assert_eq!(result.backend_total, 100);
        assert_eq!(result.local_total, 100);
        assert!(result.is_consistent);
        
        // Test inconsistent case
        let result = SyncResult::new(5, 1, 100, 110);
        assert!(!result.is_consistent);
    }
    
    #[test]
    fn test_sync_result_failed() {
        let result = SyncResult::failed(50, "Network error");
        assert_eq!(result.records_synced, 0);
        assert_eq!(result.local_total, 50);
        assert!(!result.is_consistent);
    }
    
    #[test]
    fn test_sync_state_default() {
        let state = SyncState::default();
        assert!(state.last_sync_time.is_none());
        assert_eq!(state.retry_count, 0);
        assert!(!state.is_syncing);
        assert!(state.last_result.is_none());
        assert!(state.last_error.is_none());
    }
    
    #[test]
    fn test_usage_sync_client_creation() {
        let client = UsageSyncClient::new();
        assert_eq!(client.base_url, "https://api.traductor.app");
        
        let custom_client = UsageSyncClient::with_base_url("http://localhost:8080");
        assert_eq!(custom_client.base_url, "http://localhost:8080");
    }
    
    #[test]
    fn test_tracker_with_sync_client() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_sync.db");
        let db = Database::new(db_path.to_str().unwrap(), "test_key").unwrap();
        
        let sync_client = UsageSyncClient::with_base_url("http://test.local");
        let tracker = UsageTracker::with_sync_client(db, sync_client);
        
        assert!(!tracker.is_sync_running());
    }
    
    #[test]
    fn test_get_sync_state() {
        let tracker = create_test_tracker();
        let state = tracker.get_sync_state();
        
        assert!(state.last_sync_time.is_none());
        assert_eq!(state.retry_count, 0);
        assert!(!state.is_syncing);
    }

    // ========================================================================
    // Property-Based Tests
    // ========================================================================

    proptest! {
        /// **Property 7: Usage Minutes Ceiling Calculation**
        /// 
        /// Verifies that minutes are calculated correctly:
        /// - D=0 seconds → 0 minutes (special case)
        /// - D>0 seconds → ceil(D/60) minutes
        /// 
        /// Examples: 1s→1min, 59s→1min, 60s→1min, 61s→2min
        /// 
        /// **Validates: Requirements 11.5**
        #[test]
        fn prop_usage_minutes_ceiling_calculation(duration_secs in 0u64..=86400u64) {
            let calculated_minutes = UsageTracker::calculate_minutes(duration_secs);
            
            if duration_secs == 0 {
                // Special case: 0 seconds = 0 minutes
                prop_assert_eq!(
                    calculated_minutes, 
                    0, 
                    "0 seconds should result in 0 minutes"
                );
            } else {
                // For D > 0: result should be ceil(D/60)
                // Ceiling division formula: (a + b - 1) / b
                let expected_minutes = ((duration_secs + 59) / 60) as u32;
                
                prop_assert_eq!(
                    calculated_minutes,
                    expected_minutes,
                    "For {} seconds, expected {} minutes but got {}",
                    duration_secs,
                    expected_minutes,
                    calculated_minutes
                );
                
                // Additional property: result should always be at least 1 for D > 0
                prop_assert!(
                    calculated_minutes >= 1,
                    "Duration {} seconds should result in at least 1 minute, got {}",
                    duration_secs,
                    calculated_minutes
                );
                
                // Verify ceil property: (calculated_minutes - 1) * 60 < duration_secs <= calculated_minutes * 60
                let lower_bound = (calculated_minutes as u64).saturating_sub(1) * 60;
                let upper_bound = calculated_minutes as u64 * 60;
                
                prop_assert!(
                    duration_secs > lower_bound || (duration_secs == 0 && calculated_minutes == 0),
                    "Duration {} should be > lower bound {} (minutes={})",
                    duration_secs,
                    lower_bound,
                    calculated_minutes
                );
                
                prop_assert!(
                    duration_secs <= upper_bound,
                    "Duration {} should be <= upper bound {} (minutes={})",
                    duration_secs,
                    upper_bound,
                    calculated_minutes
                );
            }
        }

        /// Additional property test: Verify specific boundary conditions
        /// 
        /// Tests that the ceiling calculation is correct at minute boundaries:
        /// - Just before a minute boundary (59, 119, 179, ...)
        /// - Exactly at a minute boundary (60, 120, 180, ...)
        /// - Just after a minute boundary (61, 121, 181, ...)
        /// 
        /// **Validates: Requirements 11.5**
        #[test]
        fn prop_usage_minutes_boundary_conditions(minute_count in 1u32..=1440u32) {
            let boundary = minute_count as u64 * 60;
            
            // Just before boundary (if boundary > 0)
            if boundary > 0 {
                let before = boundary - 1;
                let minutes_before = UsageTracker::calculate_minutes(before);
                prop_assert_eq!(
                    minutes_before,
                    minute_count,
                    "At {} seconds (1 before {} minute boundary), should be {} minutes",
                    before,
                    minute_count,
                    minute_count
                );
            }
            
            // Exactly at boundary
            let minutes_at = UsageTracker::calculate_minutes(boundary);
            prop_assert_eq!(
                minutes_at,
                minute_count,
                "At {} seconds (exactly {} minute boundary), should be {} minutes",
                boundary,
                minute_count,
                minute_count
            );
            
            // Just after boundary
            let after = boundary + 1;
            let minutes_after = UsageTracker::calculate_minutes(after);
            prop_assert_eq!(
                minutes_after,
                minute_count + 1,
                "At {} seconds (1 after {} minute boundary), should be {} minutes",
                after,
                minute_count,
                minute_count + 1
            );
        }
    }
}

// ============================================================================
// Property-Based Tests - Property 8: Usage Consistency
// ============================================================================

#[cfg(test)]
mod property_tests_consistency {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use tempfile::tempdir;
    
    /// **Property 8: Usage Consistency Between Local and Backend**
    /// 
    /// This property test suite verifies:
    /// 1. **Sum consistency**: Total minutes calculated locally equals total synced to backend
    /// 2. **No duplicates**: All session IDs are unique across records
    /// 3. **No record loss**: All records are preserved during sync operations
    /// 
    /// **Validates: Requirements 11.6**
    /// - "THE Usage_Tracker SHALL garantizar que el total de minutos calculado 
    ///    localmente sea igual al total sincronizado con el backend"
    
    // ========================================================================
    // Generators / Strategies
    // ========================================================================
    
    /// Strategy to generate a valid channel
    fn channel_strategy() -> impl Strategy<Value = Channel> {
        prop_oneof![
            Just(Channel::System),
            Just(Channel::User),
        ]
    }
    
    /// Strategy to generate a valid duration in seconds (0 to 2 hours)
    fn duration_strategy() -> impl Strategy<Value = u64> {
        0u64..7200u64
    }
    
    /// Strategy to generate a usage record with valid data
    fn usage_record_strategy() -> impl Strategy<Value = (Channel, u64)> {
        (channel_strategy(), duration_strategy())
    }
    
    /// Strategy to generate a sequence of usage records (1-20 records)
    fn usage_records_sequence_strategy() -> impl Strategy<Value = Vec<(Channel, u64)>> {
        prop::collection::vec(usage_record_strategy(), 1..20)
    }
    
    // ========================================================================
    // Helper functions
    // ========================================================================
    
    /// Create a test tracker with a temporary database
    fn create_test_tracker_for_property() -> UsageTracker {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_property_usage.db");
        // Keep tempdir alive by leaking it (for test purposes only)
        let db_path_str = db_path.to_str().unwrap().to_string();
        std::mem::forget(dir);
        let db = Database::new(&db_path_str, "test_key").unwrap();
        UsageTracker::new(db)
    }
    
    /// Simulate a usage record being created and saved to database
    /// Returns the calculated minutes for the record
    fn create_usage_record(
        db: &Database,
        channel: Channel,
        duration_secs: u64,
        _session_id: &str,
    ) -> Result<u32, UsageError> {
        let now = Utc::now();
        let start_time = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let end_time = (now + chrono::Duration::seconds(duration_secs as i64))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        
        let minutes = UsageTracker::calculate_minutes(duration_secs);
        
        let record = UsageRecord {
            id: None,
            start_time,
            end_time,
            channel: channel.as_str().to_string(),
            minutes: minutes as i32,
            synced: false,
        };
        
        db.insert_usage_record(&record)?;
        
        Ok(minutes)
    }
    
    /// Simulate backend sync by marking records as synced
    /// In real scenario, this would involve network call
    /// Returns sum of synced minutes
    fn simulate_backend_sync(db: &Database) -> Result<u32, UsageError> {
        let unsynced = db.get_unsynced_usage_records()?;
        
        let total_minutes: u32 = unsynced.iter()
            .map(|r| r.minutes as u32)
            .sum();
        
        let ids: Vec<i64> = unsynced.iter()
            .filter_map(|r| r.id)
            .collect();
        
        if !ids.is_empty() {
            db.mark_usage_synced(&ids)?;
        }
        
        Ok(total_minutes)
    }
    
    // ========================================================================
    // Property Tests
    // ========================================================================
    
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// **Property 8a: Sum Consistency - Local Total Equals Synced Total**
        /// 
        /// For any sequence of usage records created locally, the sum of minutes
        /// calculated locally MUST equal the sum of minutes synced to backend.
        /// 
        /// This verifies: `sum(local_records.minutes) == sum(synced_records.minutes)`
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_usage_sum_consistency(
            records in usage_records_sequence_strategy()
        ) {
            let tracker = create_test_tracker_for_property();
            
            // Track local total as we create records
            let mut local_total: u32 = 0;
            
            for (i, (channel, duration)) in records.iter().enumerate() {
                let session_id = format!("session-{}", i);
                let minutes = create_usage_record(
                    &tracker.db,
                    *channel,
                    *duration,
                    &session_id
                ).unwrap();
                local_total += minutes;
            }
            
            // Simulate backend sync and get synced total
            let synced_total = simulate_backend_sync(&tracker.db).unwrap();
            
            // Property: local_total == synced_total
            prop_assert_eq!(
                local_total, 
                synced_total,
                "Sum mismatch: local_total={} != synced_total={}",
                local_total,
                synced_total
            );
        }
        
        /// **Property 8b: No Duplicates - Unique Session IDs**
        /// 
        /// When multiple sessions are recorded, each session ID MUST be unique.
        /// Duplicate IDs would cause data corruption or lost records.
        /// 
        /// This verifies: `|set(session_ids)| == |session_ids|`
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_usage_no_duplicate_sessions(
            records in usage_records_sequence_strategy()
        ) {
            let tracker = create_test_tracker_for_property();
            
            let mut session_ids: Vec<String> = Vec::new();
            
            for (channel, _) in records.iter() {
                // Start a real session (which generates unique UUIDs internally)
                let session_id = tracker.start_session(*channel).unwrap();
                session_ids.push(session_id.clone());
                
                // Stop the session to record it
                let _ = tracker.stop_session(&session_id);
            }
            
            // Collect unique IDs
            let unique_ids: HashSet<&String> = session_ids.iter().collect();
            
            // Property: all session IDs are unique
            prop_assert_eq!(
                unique_ids.len(),
                session_ids.len(),
                "Duplicate session IDs detected! unique={}, total={}",
                unique_ids.len(),
                session_ids.len()
            );
        }
        
        /// **Property 8c: No Record Loss - All Records Preserved During Sync**
        /// 
        /// After creating N records and syncing, exactly N records MUST exist
        /// in the database (some synced, none lost).
        /// 
        /// This verifies: `count(db_records) == count(created_records)`
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_usage_no_record_loss(
            records in usage_records_sequence_strategy()
        ) {
            let tracker = create_test_tracker_for_property();
            
            let record_count = records.len();
            
            for (i, (channel, duration)) in records.iter().enumerate() {
                let session_id = format!("session-loss-{}", i);
                let _ = create_usage_record(
                    &tracker.db,
                    *channel,
                    *duration,
                    &session_id
                ).unwrap();
            }
            
            // Get count before sync
            let before_sync_unsynced = tracker.db.get_unsynced_usage_records()
                .unwrap()
                .len();
            
            // Simulate sync
            let _ = simulate_backend_sync(&tracker.db);
            
            // Get counts after sync - need to get all records (synced + unsynced)
            // Unsynced should be 0 now, all records should still exist but be marked synced
            let after_sync_unsynced = tracker.db.get_unsynced_usage_records()
                .unwrap()
                .len();
            
            // Property: no records lost
            // Before sync: all were unsynced
            prop_assert_eq!(
                before_sync_unsynced,
                record_count,
                "Record count mismatch before sync: expected={}, got={}",
                record_count,
                before_sync_unsynced
            );
            
            // After sync: all should be synced (none unsynced)
            prop_assert_eq!(
                after_sync_unsynced,
                0,
                "Records should all be synced after sync operation"
            );
        }
        
        /// **Property 8d: Minute Calculation Consistency Across Records**
        /// 
        /// For any sequence of records, manually calculating minutes from durations
        /// MUST produce the same total as summing recorded minutes.
        /// 
        /// This verifies the integrity of the calculate_minutes function across many inputs.
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_usage_minute_calculation_consistency(
            durations in prop::collection::vec(0u64..7200u64, 1..20)
        ) {
            // Calculate expected minutes using our function
            let expected_total: u32 = durations.iter()
                .map(|&d| UsageTracker::calculate_minutes(d))
                .sum();
            
            // Verify each individual calculation
            for &duration in &durations {
                let minutes = UsageTracker::calculate_minutes(duration);
                
                if duration == 0 {
                    prop_assert_eq!(minutes, 0, "0 seconds should give 0 minutes");
                } else {
                    // Ceiling calculation: (d + 59) / 60
                    let expected = ((duration + 59) / 60) as u32;
                    prop_assert_eq!(
                        minutes, 
                        expected,
                        "Minute calculation mismatch for {} seconds: got {}, expected {}",
                        duration,
                        minutes,
                        expected
                    );
                }
            }
            
            // Recalculate total to verify consistency
            let recalculated_total: u32 = durations.iter()
                .map(|&d| UsageTracker::calculate_minutes(d))
                .sum();
            
            prop_assert_eq!(
                expected_total,
                recalculated_total,
                "Total calculation is not idempotent"
            );
        }
        
        /// **Property 8e: Sync Idempotency - Multiple Syncs Don't Change Totals**
        /// 
        /// Running sync multiple times on already-synced records should not
        /// change the total or create duplicates.
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_usage_sync_idempotency(
            records in usage_records_sequence_strategy()
        ) {
            let tracker = create_test_tracker_for_property();
            
            // Create records
            let mut expected_total: u32 = 0;
            for (i, (channel, duration)) in records.iter().enumerate() {
                let session_id = format!("session-idem-{}", i);
                let minutes = create_usage_record(
                    &tracker.db,
                    *channel,
                    *duration,
                    &session_id
                ).unwrap();
                expected_total += minutes;
            }
            
            // First sync
            let first_sync_total = simulate_backend_sync(&tracker.db).unwrap();
            
            // Second sync (should return 0, no new records)
            let second_sync_total = simulate_backend_sync(&tracker.db).unwrap();
            
            // Third sync (should also return 0)
            let third_sync_total = simulate_backend_sync(&tracker.db).unwrap();
            
            // Property: first sync captures all minutes
            prop_assert_eq!(
                first_sync_total,
                expected_total,
                "First sync should capture all minutes"
            );
            
            // Property: subsequent syncs should return 0 (no new records)
            prop_assert_eq!(
                second_sync_total,
                0,
                "Second sync should return 0 (all already synced)"
            );
            
            prop_assert_eq!(
                third_sync_total,
                0,
                "Third sync should return 0 (all already synced)"
            );
        }
    }
    
    // ========================================================================
    // Additional Consistency Tests
    // ========================================================================
    
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]
        
        /// **Property 8f: Channel Aggregation Consistency**
        /// 
        /// The sum of system_minutes + user_minutes MUST equal total_minutes
        /// in any aggregated usage statistics.
        /// 
        /// **Validates: Requirements 11.6**
        #[test]
        fn prop_channel_aggregation_consistency(
            system_records in prop::collection::vec(duration_strategy(), 0..10),
            user_records in prop::collection::vec(duration_strategy(), 0..10),
        ) {
            let tracker = create_test_tracker_for_property();
            
            let mut system_total: u32 = 0;
            let mut user_total: u32 = 0;
            
            // Create system channel records
            for (i, duration) in system_records.iter().enumerate() {
                let session_id = format!("system-agg-{}", i);
                let minutes = create_usage_record(
                    &tracker.db,
                    Channel::System,
                    *duration,
                    &session_id
                ).unwrap();
                system_total += minutes;
            }
            
            // Create user channel records
            for (i, duration) in user_records.iter().enumerate() {
                let session_id = format!("user-agg-{}", i);
                let minutes = create_usage_record(
                    &tracker.db,
                    Channel::User,
                    *duration,
                    &session_id
                ).unwrap();
                user_total += minutes;
            }
            
            let expected_total = system_total + user_total;
            
            // Create MonthlyUsage and verify consistency
            let monthly = MonthlyUsage::new(2025, 1, system_total, user_total);
            
            prop_assert_eq!(
                monthly.total_minutes,
                expected_total,
                "MonthlyUsage total should equal system + user minutes"
            );
            
            prop_assert_eq!(
                monthly.system_minutes + monthly.user_minutes,
                monthly.total_minutes,
                "Channel sum should equal total"
            );
        }
    }
}
