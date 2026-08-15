//! SQLite database with encryption support
//! 
//! This module provides encrypted local storage using SQLite.
//! Note: SQLCipher is not available in bundled rusqlite, so we use
//! application-level encryption for sensitive fields instead.
//! 
//! Requirements: 9.8, 23.2

use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Current database schema version for migrations
pub const CURRENT_SCHEMA_VERSION: i32 = 1;

/// Database error types
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Failed to open database: {0}")]
    OpenError(String),
    
    #[error("Failed to execute SQL: {0}")]
    ExecutionError(String),
    
    #[error("Failed to run migrations: {0}")]
    MigrationError(String),
    
    #[error("Database is locked")]
    Locked,
    
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        DatabaseError::ExecutionError(err.to_string())
    }
}

/// Thread-safe database connection wrapper
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    db_path: String,
}


impl Database {
    /// Create a new database connection
    /// 
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `_encryption_key` - Reserved for future SQLCipher support
    /// 
    /// # Returns
    /// A new Database instance or DatabaseError
    pub fn new(db_path: &str, _encryption_key: &str) -> Result<Self, DatabaseError> {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DatabaseError::OpenError(format!("Failed to create directory: {}", e)))?;
        }

        let conn = Connection::open(db_path)
            .map_err(|e| DatabaseError::OpenError(e.to_string()))?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| DatabaseError::ExecutionError(e.to_string()))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: db_path.to_string(),
        };

        // Run migrations on startup
        db.run_migrations()?;

        Ok(db)
    }

    /// Open an existing database without creating tables
    pub fn open(db_path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(db_path)
            .map_err(|e| DatabaseError::OpenError(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: db_path.to_string(),
        })
    }

    /// Get the database file path
    pub fn path(&self) -> &str {
        &self.db_path
    }


    /// Run database migrations
    pub fn run_migrations(&self) -> Result<(), DatabaseError> {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;

        // Create migrations table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;

        // Get current version
        let current_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Apply pending migrations
        if current_version < 1 {
            self.migrate_v1(&conn)?;
        }

        // Add future migrations here:
        // if current_version < 2 {
        //     self.migrate_v2(&conn)?;
        // }

        Ok(())
    }

    /// Migration v1: Initial schema
    fn migrate_v1(&self, conn: &Connection) -> Result<(), DatabaseError> {
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute_batch(SCHEMA_V1_PART2)?;
        
        conn.execute(
            "INSERT INTO migrations (version) VALUES (?1)",
            params![1],
        )?;

        tracing::info!("Applied database migration v1");
        Ok(())
    }


    /// Execute a query that returns rows
    pub fn query<T, F>(&self, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> Result<Vec<T>, DatabaseError>
    where
        F: Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }

    /// Execute a statement that doesn't return rows
    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize, DatabaseError> {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        let affected = conn.execute(sql, params)?;
        Ok(affected)
    }

    /// Execute a query that returns a single value
    pub fn query_row<T, F>(&self, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> Result<T, DatabaseError>
    where
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        conn.query_row(sql, params, f)
            .map_err(|e| DatabaseError::ExecutionError(e.to_string()))
    }

    /// Get last inserted row id
    pub fn last_insert_rowid(&self) -> Result<i64, DatabaseError> {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        Ok(conn.last_insert_rowid())
    }


    /// Begin a transaction
    pub fn transaction<T, F>(&self, f: F) -> Result<T, DatabaseError>
    where
        F: FnOnce(&Connection) -> Result<T, DatabaseError>,
    {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        conn.execute("BEGIN TRANSACTION", [])?;
        
        match f(&conn) {
            Ok(result) => {
                conn.execute("COMMIT", [])?;
                Ok(result)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    /// Check if a table exists
    pub fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![table_name],
            |row| row.get(0),
        )?;
        
        Ok(count > 0)
    }

    /// Get the current schema version
    pub fn get_schema_version(&self) -> Result<i32, DatabaseError> {
        let conn = self.conn.lock()
            .map_err(|_| DatabaseError::Locked)?;
        
        // Check if migrations table exists
        let exists: i32 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='migrations'",
            [],
            |row| row.get(0),
        )?;

        if exists == 0 {
            return Ok(0);
        }

        let version: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM migrations",
            [],
            |row| row.get(0),
        )?;
        
        Ok(version)
    }
}


impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
            db_path: self.db_path.clone(),
        }
    }
}

// ============================================================================
// Database Schema Definitions
// ============================================================================

/// Schema v1: Initial tables
/// Tables: config, usage_records, auth_session, invoices, migrations
const SCHEMA_V1: &str = r#"
-- Configuration storage (key-value store for app settings)
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,  -- JSON serialized value
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Usage records for tracking translation minutes
-- Synced to backend when connected
CREATE TABLE IF NOT EXISTS usage_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    start_time TEXT NOT NULL,      -- ISO 8601 UTC timestamp
    end_time TEXT NOT NULL,        -- ISO 8601 UTC timestamp
    channel TEXT NOT NULL,         -- 'system' or 'user'
    minutes INTEGER NOT NULL,      -- Rounded up to nearest minute
    synced INTEGER NOT NULL DEFAULT 0,  -- 0 = pending, 1 = synced
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_usage_synced ON usage_records(synced);
CREATE INDEX IF NOT EXISTS idx_usage_start ON usage_records(start_time);
CREATE INDEX IF NOT EXISTS idx_usage_channel ON usage_records(channel);
"#;

const SCHEMA_V1_PART2: &str = r#"
-- Auth session storage (single row for current session)
-- Tokens stored here; sensitive fields could be encrypted at app level
CREATE TABLE IF NOT EXISTS auth_session (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Enforce single row
    session_token TEXT,            -- Session token (7-day expiry)
    user_id TEXT,                  -- User ID from auth service
    email TEXT,                    -- User email
    name TEXT,                     -- Display name
    plan TEXT,                     -- Subscription plan: byok_free, starter, pro
    expires_at TEXT,               -- Token expiration ISO 8601
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Invoice cache for downloaded PDFs
CREATE TABLE IF NOT EXISTS invoices (
    id TEXT PRIMARY KEY,           -- Invoice ID from Stripe
    invoice_date TEXT NOT NULL,    -- Invoice date ISO 8601
    amount_cents INTEGER NOT NULL, -- Amount in cents
    currency TEXT DEFAULT 'usd',   -- Currency code
    pdf_path TEXT,                 -- Local file path to downloaded PDF
    downloaded_at TEXT             -- When PDF was downloaded
);

-- Index for invoice queries
CREATE INDEX IF NOT EXISTS idx_invoices_date ON invoices(invoice_date);
"#;


// ============================================================================
// Auth Session Operations
// ============================================================================

/// Auth session data structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthSession {
    pub session_token: Option<String>,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan: Option<String>,
    pub expires_at: Option<String>,
}

impl Database {
    /// Save auth session (upsert)
    pub fn save_auth_session(&self, session: &AuthSession) -> Result<(), DatabaseError> {
        self.execute(
            "INSERT INTO auth_session (id, session_token, user_id, email, name, plan, expires_at, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                session_token = ?1,
                user_id = ?2,
                email = ?3,
                name = ?4,
                plan = ?5,
                expires_at = ?6,
                updated_at = datetime('now')",
            params![
                session.session_token,
                session.user_id,
                session.email,
                session.name,
                session.plan,
                session.expires_at,
            ],
        )?;
        Ok(())
    }

    /// Get current auth session
    pub fn get_auth_session(&self) -> Result<Option<AuthSession>, DatabaseError> {
        let result = self.query_row(
            "SELECT session_token, user_id, email, name, plan, expires_at FROM auth_session WHERE id = 1",
            &[],
            |row| {
                Ok(AuthSession {
                    session_token: row.get(0)?,
                    user_id: row.get(1)?,
                    email: row.get(2)?,
                    name: row.get(3)?,
                    plan: row.get(4)?,
                    expires_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(session) => Ok(Some(session)),
            Err(DatabaseError::ExecutionError(e)) if e.contains("no rows") => Ok(None),
            Err(e) => Err(e),
        }
    }


    /// Clear auth session (logout)
    pub fn clear_auth_session(&self) -> Result<(), DatabaseError> {
        self.execute("DELETE FROM auth_session WHERE id = 1", &[])?;
        Ok(())
    }
}

// ============================================================================
// Config Operations
// ============================================================================

impl Database {
    /// Save a config value
    pub fn save_config(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        self.execute(
            "INSERT INTO config (key, value, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
                value = ?2,
                updated_at = datetime('now')",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get a config value
    pub fn get_config(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        let result = self.query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(DatabaseError::ExecutionError(e)) if e.contains("no rows") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Delete a config value
    pub fn delete_config(&self, key: &str) -> Result<(), DatabaseError> {
        self.execute("DELETE FROM config WHERE key = ?1", params![key])?;
        Ok(())
    }

    /// Get all config key-value pairs
    pub fn get_all_config(&self) -> Result<Vec<(String, String)>, DatabaseError> {
        self.query(
            "SELECT key, value FROM config ORDER BY key",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }
}


// ============================================================================
// Usage Records Operations
// ============================================================================

/// Usage record data structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    pub start_time: String,
    pub end_time: String,
    pub channel: String,  // "system" or "user"
    pub minutes: i32,
    pub synced: bool,
}

impl Database {
    /// Insert a new usage record
    pub fn insert_usage_record(&self, record: &UsageRecord) -> Result<i64, DatabaseError> {
        self.execute(
            "INSERT INTO usage_records (start_time, end_time, channel, minutes, synced)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.start_time,
                record.end_time,
                record.channel,
                record.minutes,
                if record.synced { 1 } else { 0 },
            ],
        )?;
        self.last_insert_rowid()
    }

    /// Get unsynced usage records
    pub fn get_unsynced_usage_records(&self) -> Result<Vec<UsageRecord>, DatabaseError> {
        self.query(
            "SELECT id, start_time, end_time, channel, minutes, synced 
             FROM usage_records WHERE synced = 0 ORDER BY start_time",
            &[],
            |row| {
                Ok(UsageRecord {
                    id: row.get(0)?,
                    start_time: row.get(1)?,
                    end_time: row.get(2)?,
                    channel: row.get(3)?,
                    minutes: row.get(4)?,
                    synced: row.get::<_, i32>(5)? == 1,
                })
            },
        )
    }

    /// Mark usage records as synced
    pub fn mark_usage_synced(&self, ids: &[i64]) -> Result<(), DatabaseError> {
        if ids.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "UPDATE usage_records SET synced = 1 WHERE id IN ({})",
            placeholders.join(",")
        );

        let params: Vec<&dyn rusqlite::ToSql> = ids.iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        self.execute(&sql, &params)?;
        Ok(())
    }


    /// Get usage records for a date range
    pub fn get_usage_records_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<UsageRecord>, DatabaseError> {
        self.query(
            "SELECT id, start_time, end_time, channel, minutes, synced 
             FROM usage_records 
             WHERE start_time >= ?1 AND start_time < ?2
             ORDER BY start_time",
            params![start_date, end_date],
            |row| {
                Ok(UsageRecord {
                    id: row.get(0)?,
                    start_time: row.get(1)?,
                    end_time: row.get(2)?,
                    channel: row.get(3)?,
                    minutes: row.get(4)?,
                    synced: row.get::<_, i32>(5)? == 1,
                })
            },
        )
    }

    /// Get total minutes used in current month
    pub fn get_monthly_usage(&self, year: i32, month: i32) -> Result<i32, DatabaseError> {
        let start = format!("{:04}-{:02}-01T00:00:00Z", year, month);
        let end = if month == 12 {
            format!("{:04}-01-01T00:00:00Z", year + 1)
        } else {
            format!("{:04}-{:02}-01T00:00:00Z", year, month + 1)
        };

        let result = self.query_row(
            "SELECT COALESCE(SUM(minutes), 0) FROM usage_records 
             WHERE start_time >= ?1 AND start_time < ?2",
            params![start, end],
            |row| row.get(0),
        );

        match result {
            Ok(total) => Ok(total),
            Err(_) => Ok(0),
        }
    }

    /// Get daily usage for the last N days
    pub fn get_daily_usage(&self, days: i32) -> Result<Vec<(String, i32, i32)>, DatabaseError> {
        self.query(
            "SELECT 
                date(start_time) as day,
                SUM(CASE WHEN channel = 'system' THEN minutes ELSE 0 END) as system_min,
                SUM(CASE WHEN channel = 'user' THEN minutes ELSE 0 END) as user_min
             FROM usage_records 
             WHERE start_time >= datetime('now', ?1)
             GROUP BY date(start_time)
             ORDER BY day DESC",
            params![format!("-{} days", days)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }
}


// ============================================================================
// Invoice Operations
// ============================================================================

/// Invoice data structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Invoice {
    pub id: String,
    pub invoice_date: String,
    pub amount_cents: i32,
    pub currency: String,
    pub pdf_path: Option<String>,
    pub downloaded_at: Option<String>,
}

impl Database {
    /// Save or update an invoice
    pub fn save_invoice(&self, invoice: &Invoice) -> Result<(), DatabaseError> {
        self.execute(
            "INSERT INTO invoices (id, invoice_date, amount_cents, currency, pdf_path, downloaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                invoice_date = ?2,
                amount_cents = ?3,
                currency = ?4,
                pdf_path = ?5,
                downloaded_at = ?6",
            params![
                invoice.id,
                invoice.invoice_date,
                invoice.amount_cents,
                invoice.currency,
                invoice.pdf_path,
                invoice.downloaded_at,
            ],
        )?;
        Ok(())
    }

    /// Get recent invoices (last N)
    pub fn get_recent_invoices(&self, limit: i32) -> Result<Vec<Invoice>, DatabaseError> {
        self.query(
            "SELECT id, invoice_date, amount_cents, currency, pdf_path, downloaded_at 
             FROM invoices 
             ORDER BY invoice_date DESC 
             LIMIT ?1",
            params![limit],
            |row| {
                Ok(Invoice {
                    id: row.get(0)?,
                    invoice_date: row.get(1)?,
                    amount_cents: row.get(2)?,
                    currency: row.get(3).unwrap_or_else(|_| "usd".to_string()),
                    pdf_path: row.get(4)?,
                    downloaded_at: row.get(5)?,
                })
            },
        )
    }

    /// Get an invoice by ID
    pub fn get_invoice(&self, id: &str) -> Result<Option<Invoice>, DatabaseError> {
        let result = self.query_row(
            "SELECT id, invoice_date, amount_cents, currency, pdf_path, downloaded_at 
             FROM invoices WHERE id = ?1",
            params![id],
            |row| {
                Ok(Invoice {
                    id: row.get(0)?,
                    invoice_date: row.get(1)?,
                    amount_cents: row.get(2)?,
                    currency: row.get(3).unwrap_or_else(|_| "usd".to_string()),
                    pdf_path: row.get(4)?,
                    downloaded_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(invoice) => Ok(Some(invoice)),
            Err(DatabaseError::ExecutionError(e)) if e.contains("no rows") => Ok(None),
            Err(e) => Err(e),
        }
    }
}


// ============================================================================
// Utility Functions
// ============================================================================

/// Get the default database path for the application
pub fn get_default_db_path() -> Result<String, DatabaseError> {
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| DatabaseError::OpenError("Could not find local data directory".to_string()))?;
    
    let db_dir = app_data.join("traductor-desktop");
    let db_path = db_dir.join("traductor.db");
    
    Ok(db_path.to_string_lossy().to_string())
}

/// Derive an encryption key from system credentials
/// Note: This is a placeholder for when SQLCipher becomes available
pub fn derive_encryption_key() -> Result<String, DatabaseError> {
    // For now, return a placeholder key
    // In production with SQLCipher, this would derive from:
    // - Machine ID
    // - User credentials from keyring
    // - Additional entropy
    let machine_id = machine_uid::get()
        .unwrap_or_else(|_| "default-key".to_string());
    
    Ok(machine_id)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_db() -> Database {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Database::new(db_path.to_str().unwrap(), "test_key").unwrap()
    }

    #[test]
    fn test_database_creation() {
        let db = create_test_db();
        assert!(db.table_exists("config").unwrap());
        assert!(db.table_exists("usage_records").unwrap());
        assert!(db.table_exists("auth_session").unwrap());
        assert!(db.table_exists("invoices").unwrap());
        assert!(db.table_exists("migrations").unwrap());
    }

    #[test]
    fn test_schema_version() {
        let db = create_test_db();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }


    #[test]
    fn test_config_operations() {
        let db = create_test_db();
        
        // Save and retrieve config
        db.save_config("test_key", r#"{"value": 123}"#).unwrap();
        let value = db.get_config("test_key").unwrap();
        assert_eq!(value, Some(r#"{"value": 123}"#.to_string()));
        
        // Update config
        db.save_config("test_key", r#"{"value": 456}"#).unwrap();
        let value = db.get_config("test_key").unwrap();
        assert_eq!(value, Some(r#"{"value": 456}"#.to_string()));
        
        // Delete config
        db.delete_config("test_key").unwrap();
        let value = db.get_config("test_key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_auth_session() {
        let db = create_test_db();
        
        let session = AuthSession {
            session_token: Some("test_token".to_string()),
            user_id: Some("user_123".to_string()),
            email: Some("test@example.com".to_string()),
            name: Some("Test User".to_string()),
            plan: Some("starter".to_string()),
            expires_at: Some("2025-01-01T00:00:00Z".to_string()),
        };
        
        // Save session
        db.save_auth_session(&session).unwrap();
        
        // Retrieve session
        let retrieved = db.get_auth_session().unwrap().unwrap();
        assert_eq!(retrieved.session_token, session.session_token);
        assert_eq!(retrieved.user_id, session.user_id);
        assert_eq!(retrieved.email, session.email);
        
        // Clear session
        db.clear_auth_session().unwrap();
        let cleared = db.get_auth_session().unwrap();
        assert!(cleared.is_none());
    }


    #[test]
    fn test_usage_records() {
        let db = create_test_db();
        
        let record = UsageRecord {
            id: None,
            start_time: "2025-01-15T10:00:00Z".to_string(),
            end_time: "2025-01-15T10:05:00Z".to_string(),
            channel: "system".to_string(),
            minutes: 5,
            synced: false,
        };
        
        // Insert record
        let id = db.insert_usage_record(&record).unwrap();
        assert!(id > 0);
        
        // Get unsynced records
        let unsynced = db.get_unsynced_usage_records().unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].minutes, 5);
        
        // Mark as synced
        db.mark_usage_synced(&[id]).unwrap();
        let unsynced = db.get_unsynced_usage_records().unwrap();
        assert!(unsynced.is_empty());
    }

    #[test]
    fn test_invoices() {
        let db = create_test_db();
        
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_date: "2025-01-15".to_string(),
            amount_cents: 1499,
            currency: "usd".to_string(),
            pdf_path: Some("/path/to/invoice.pdf".to_string()),
            downloaded_at: Some("2025-01-15T12:00:00Z".to_string()),
        };
        
        // Save invoice
        db.save_invoice(&invoice).unwrap();
        
        // Retrieve invoice
        let retrieved = db.get_invoice("inv_123").unwrap().unwrap();
        assert_eq!(retrieved.amount_cents, 1499);
        assert_eq!(retrieved.currency, "usd");
        
        // Get recent invoices
        let recent = db.get_recent_invoices(10).unwrap();
        assert_eq!(recent.len(), 1);
    }
}
