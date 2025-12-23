//! SQLite persistence for sessions.

use crate::{ClausetError, Result};
use clauset_types::{Session, SessionMode, SessionStatus, SessionSummary};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// SQLite-based session store.
pub struct SessionStore {
    conn: Mutex<Connection>,
}

impl SessionStore {
    /// Open or create the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        store.migrate()?;
        Ok(store)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                claude_session_id TEXT NOT NULL,
                project_path TEXT NOT NULL,
                model TEXT NOT NULL,
                status TEXT NOT NULL,
                mode TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_activity_at TEXT NOT NULL,
                total_cost_usd REAL NOT NULL DEFAULT 0.0,
                preview TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
            CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity_at);
            "#,
        )?;
        Ok(())
    }

    /// Run migrations for schema updates.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Check if input_tokens column exists
        let has_input_tokens: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('sessions') WHERE name = 'input_tokens'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_input_tokens {
            conn.execute_batch(
                r#"
                ALTER TABLE sessions ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE sessions ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE sessions ADD COLUMN context_percent INTEGER NOT NULL DEFAULT 0;
                "#,
            )?;
        }

        // Check if recent_actions column exists
        let has_recent_actions: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('sessions') WHERE name = 'recent_actions'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_recent_actions {
            conn.execute_batch(
                r#"
                ALTER TABLE sessions ADD COLUMN recent_actions TEXT NOT NULL DEFAULT '[]';
                ALTER TABLE sessions ADD COLUMN current_step TEXT;
                "#,
            )?;
        }

        Ok(())
    }

    /// Insert a new session.
    pub fn insert(&self, session: &Session) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO sessions (
                id, claude_session_id, project_path, model, status, mode,
                created_at, last_activity_at, total_cost_usd, input_tokens,
                output_tokens, context_percent, preview
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                session.id.to_string(),
                session.claude_session_id.to_string(),
                session.project_path.to_string_lossy(),
                session.model,
                serde_json::to_string(&session.status)?,
                serde_json::to_string(&session.mode)?,
                session.created_at.to_rfc3339(),
                session.last_activity_at.to_rfc3339(),
                session.total_cost_usd,
                session.input_tokens as i64,
                session.output_tokens as i64,
                session.context_percent as i32,
                session.preview,
            ],
        )?;
        Ok(())
    }

    /// Get a session by ID.
    pub fn get(&self, id: Uuid) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let session = conn
            .query_row(
                "SELECT * FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| Self::row_to_session(row),
            )
            .optional()?;
        Ok(session)
    }

    /// List all sessions, ordered by last activity (most recent first).
    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT * FROM sessions ORDER BY last_activity_at DESC")?;
        let sessions = stmt
            .query_map([], |row| Self::row_to_session_summary(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    /// List active sessions (not stopped/error).
    pub fn list_active(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT * FROM sessions
            WHERE status NOT IN ('"stopped"', '"error"')
            ORDER BY last_activity_at DESC
            "#,
        )?;
        let sessions = stmt
            .query_map([], |row| Self::row_to_session(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    /// Update session status.
    pub fn update_status(&self, id: Uuid, status: SessionStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let status_str = serde_json::to_string(&status)
            .map_err(|e| ClausetError::ParseError(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET status = ?1, last_activity_at = ?2 WHERE id = ?3",
            params![
                status_str,
                chrono::Utc::now().to_rfc3339(),
                id.to_string()
            ],
        )?;
        Ok(())
    }

    /// Update session cost.
    pub fn update_cost(&self, id: Uuid, cost: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET total_cost_usd = ?1 WHERE id = ?2",
            params![cost, id.to_string()],
        )?;
        Ok(())
    }

    /// Update session preview.
    pub fn update_preview(&self, id: Uuid, preview: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET preview = ?1, last_activity_at = ?2 WHERE id = ?3",
            params![preview, chrono::Utc::now().to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    /// Update session stats from Claude status line.
    pub fn update_stats(
        &self,
        id: Uuid,
        model: &str,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE sessions SET
                model = ?1,
                total_cost_usd = ?2,
                input_tokens = ?3,
                output_tokens = ?4,
                context_percent = ?5,
                last_activity_at = ?6
            WHERE id = ?7
            "#,
            params![
                model,
                cost,
                input_tokens as i64,
                output_tokens as i64,
                context_percent as i32,
                chrono::Utc::now().to_rfc3339(),
                id.to_string()
            ],
        )?;
        Ok(())
    }

    /// Delete a session.
    pub fn delete(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    /// Update recent actions and current step (for persisting on session stop).
    pub fn update_activity(
        &self,
        id: Uuid,
        current_step: Option<&str>,
        recent_actions: &[clauset_types::RecentAction],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let actions_json = serde_json::to_string(recent_actions)
            .map_err(|e| ClausetError::ParseError(e.to_string()))?;
        conn.execute(
            r#"
            UPDATE sessions SET
                current_step = ?1,
                recent_actions = ?2,
                last_activity_at = ?3
            WHERE id = ?4
            "#,
            params![
                current_step,
                actions_json,
                chrono::Utc::now().to_rfc3339(),
                id.to_string()
            ],
        )?;
        Ok(())
    }

    fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
        let id: String = row.get("id")?;
        let claude_session_id: String = row.get("claude_session_id")?;
        let project_path: String = row.get("project_path")?;
        let model: String = row.get("model")?;
        let status: String = row.get("status")?;
        let mode: String = row.get("mode")?;
        let created_at: String = row.get("created_at")?;
        let last_activity_at: String = row.get("last_activity_at")?;
        let total_cost_usd: f64 = row.get("total_cost_usd")?;
        let input_tokens: i64 = row.get("input_tokens").unwrap_or(0);
        let output_tokens: i64 = row.get("output_tokens").unwrap_or(0);
        let context_percent: i32 = row.get("context_percent").unwrap_or(0);
        let preview: String = row.get("preview")?;

        Ok(Session {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            claude_session_id: Uuid::parse_str(&claude_session_id).unwrap_or_default(),
            project_path: project_path.into(),
            model,
            status: serde_json::from_str(&status).unwrap_or(SessionStatus::Error),
            mode: serde_json::from_str(&mode).unwrap_or(SessionMode::StreamJson),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_default(),
            last_activity_at: chrono::DateTime::parse_from_rfc3339(&last_activity_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_default(),
            total_cost_usd,
            input_tokens: input_tokens as u64,
            output_tokens: output_tokens as u64,
            context_percent: context_percent as u8,
            preview,
        })
    }

    fn row_to_session_summary(row: &rusqlite::Row) -> rusqlite::Result<SessionSummary> {
        let id: String = row.get("id")?;
        let claude_session_id: String = row.get("claude_session_id")?;
        let project_path: String = row.get("project_path")?;
        let model: String = row.get("model")?;
        let status: String = row.get("status")?;
        let mode: String = row.get("mode")?;
        let created_at: String = row.get("created_at")?;
        let last_activity_at: String = row.get("last_activity_at")?;
        let total_cost_usd: f64 = row.get("total_cost_usd")?;
        let input_tokens: i64 = row.get("input_tokens").unwrap_or(0);
        let output_tokens: i64 = row.get("output_tokens").unwrap_or(0);
        let context_percent: i32 = row.get("context_percent").unwrap_or(0);
        let preview: String = row.get("preview")?;
        let current_step: Option<String> = row.get("current_step").ok();
        let recent_actions_json: String = row.get("recent_actions").unwrap_or_else(|_| "[]".to_string());
        let recent_actions: Vec<clauset_types::RecentAction> =
            serde_json::from_str(&recent_actions_json).unwrap_or_default();

        Ok(SessionSummary {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            claude_session_id: Uuid::parse_str(&claude_session_id).unwrap_or_default(),
            project_path: project_path.into(),
            model,
            status: serde_json::from_str(&status).unwrap_or(SessionStatus::Error),
            mode: serde_json::from_str(&mode).unwrap_or(SessionMode::StreamJson),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_default(),
            last_activity_at: chrono::DateTime::parse_from_rfc3339(&last_activity_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_default(),
            total_cost_usd,
            input_tokens: input_tokens as u64,
            output_tokens: output_tokens as u64,
            context_percent: context_percent as u8,
            preview,
            current_step,
            recent_actions,
        })
    }
}
