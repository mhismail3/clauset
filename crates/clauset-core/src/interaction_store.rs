//! SQLite persistence for interactions, tool invocations, and file snapshots.
//!
//! This module provides the database layer for the interaction tracking system.
//! It uses the same SQLite database as SessionStore but manages separate tables.

use crate::diff::FileDiff;
use crate::{ClausetError, Result};
use chrono::{DateTime, Utc};
use clauset_types::{
    FileChange, FileChangeType, FileSnapshot, Interaction, InteractionStatus, InteractionSummary,
    SnapshotType, ToolInvocation,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

/// Maximum file size for snapshots (1 MB).
pub const MAX_SNAPSHOT_SIZE: u64 = 1_048_576;

/// A file change with its computed diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeWithDiff {
    /// File path.
    pub file_path: PathBuf,
    /// Type of change.
    pub change_type: FileChangeType,
    /// The computed diff.
    pub diff: FileDiff,
}

/// Which field matched in a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchField {
    /// Matched in user prompt.
    Prompt,
    /// Matched in assistant summary.
    Summary,
    /// Matched in file path.
    FilePath,
    /// Matched in tool input.
    ToolInput,
}

/// A search result containing an interaction and relevance info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The matched interaction.
    pub interaction: Interaction,
    /// Relevance score (higher = more relevant).
    pub relevance_score: f64,
    /// Which field matched.
    pub matched_field: SearchField,
}

/// A file path match from search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePathMatch {
    /// The file path that matched.
    pub file_path: PathBuf,
    /// Interaction where this file was modified.
    pub interaction_id: Uuid,
    /// Session where this file was modified.
    pub session_id: Uuid,
    /// When the file was modified.
    pub modified_at: DateTime<Utc>,
    /// Number of snapshots for this file in this interaction.
    pub snapshot_count: u32,
}

/// Results from a global search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchResults {
    /// Matching interactions.
    pub interactions: Vec<SearchResult>,
    /// Matching tool invocations.
    pub tool_invocations: Vec<ToolInvocation>,
    /// Matching file paths.
    pub file_matches: Vec<FilePathMatch>,
}

/// Analytics for a single session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAnalytics {
    /// Session ID.
    pub session_id: Uuid,
    /// Number of interactions.
    pub interaction_count: u32,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// First interaction timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_interaction_at: Option<DateTime<Utc>>,
    /// Last interaction timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_interaction_at: Option<DateTime<Utc>>,
}

/// Daily cost breakdown entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCostEntry {
    /// Date (YYYY-MM-DD format).
    pub date: String,
    /// Number of interactions.
    pub interaction_count: u32,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Total input tokens.
    pub input_tokens: u64,
    /// Total output tokens.
    pub output_tokens: u64,
}

/// Cost breakdown by tool type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCostEntry {
    /// Tool name.
    pub tool_name: String,
    /// Number of invocations.
    pub invocation_count: u32,
    /// Average duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_duration_ms: Option<f64>,
}

/// Overall analytics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    /// Number of unique sessions.
    pub session_count: u32,
    /// Total interactions.
    pub interaction_count: u32,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Average cost per interaction.
    pub avg_cost_per_interaction: f64,
    /// Total tool invocations.
    pub total_tool_invocations: u32,
    /// Total file changes.
    pub total_file_changes: u32,
}

/// Default retention period in days.
pub const DEFAULT_RETENTION_DAYS: i64 = 30;

/// SQLite-based store for interaction tracking.
pub struct InteractionStore {
    conn: Mutex<Connection>,
}

impl InteractionStore {
    /// Open or create the interaction store at the given path.
    ///
    /// Uses the same database file as SessionStore.
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

    /// Create an InteractionStore from an existing connection.
    ///
    /// Useful when sharing a connection with SessionStore.
    pub fn from_connection(conn: Connection) -> Result<Self> {
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        store.migrate()?;
        Ok(store)
    }

    /// Initialize the schema for interaction tracking tables.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create interactions table
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS interactions (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                sequence_number INTEGER NOT NULL,
                user_prompt TEXT NOT NULL,
                assistant_summary TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                cost_usd_delta REAL NOT NULL DEFAULT 0.0,
                input_tokens_delta INTEGER NOT NULL DEFAULT 0,
                output_tokens_delta INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'active',
                error_message TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_interactions_session_id
                ON interactions(session_id);
            CREATE INDEX IF NOT EXISTS idx_interactions_started_at
                ON interactions(started_at);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_interactions_session_seq
                ON interactions(session_id, sequence_number);
            "#,
        )?;

        // Create tool_invocations table
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tool_invocations (
                id TEXT PRIMARY KEY,
                interaction_id TEXT NOT NULL,
                tool_use_id TEXT,
                sequence_number INTEGER NOT NULL,
                tool_name TEXT NOT NULL,
                tool_input TEXT NOT NULL,
                tool_output_preview TEXT,
                file_path TEXT,
                is_error INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_ms INTEGER,
                FOREIGN KEY (interaction_id) REFERENCES interactions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tool_invocations_interaction_id
                ON tool_invocations(interaction_id);
            CREATE INDEX IF NOT EXISTS idx_tool_invocations_tool_name
                ON tool_invocations(tool_name);
            CREATE INDEX IF NOT EXISTS idx_tool_invocations_file_path
                ON tool_invocations(file_path);
            CREATE INDEX IF NOT EXISTS idx_tool_invocations_started_at
                ON tool_invocations(started_at);
            "#,
        )?;

        // Create file_contents table (content-addressed storage)
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS file_contents (
                content_hash TEXT PRIMARY KEY,
                compressed_content BLOB NOT NULL,
                original_size INTEGER NOT NULL,
                compression_ratio REAL,
                created_at TEXT NOT NULL,
                reference_count INTEGER NOT NULL DEFAULT 1
            );

            CREATE INDEX IF NOT EXISTS idx_file_contents_created_at
                ON file_contents(created_at);
            "#,
        )?;

        // Create file_snapshots table
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS file_snapshots (
                id TEXT PRIMARY KEY,
                interaction_id TEXT NOT NULL,
                tool_invocation_id TEXT,
                file_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                snapshot_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (interaction_id) REFERENCES interactions(id) ON DELETE CASCADE,
                FOREIGN KEY (tool_invocation_id) REFERENCES tool_invocations(id) ON DELETE SET NULL,
                FOREIGN KEY (content_hash) REFERENCES file_contents(content_hash)
            );

            CREATE INDEX IF NOT EXISTS idx_file_snapshots_interaction_id
                ON file_snapshots(interaction_id);
            CREATE INDEX IF NOT EXISTS idx_file_snapshots_file_path
                ON file_snapshots(file_path);
            CREATE INDEX IF NOT EXISTS idx_file_snapshots_content_hash
                ON file_snapshots(content_hash);
            CREATE INDEX IF NOT EXISTS idx_file_snapshots_created_at
                ON file_snapshots(created_at);
            "#,
        )?;

        // Create chat_messages table for chat view persistence
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                sequence_number INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                is_streaming INTEGER NOT NULL DEFAULT 0,
                is_complete INTEGER NOT NULL DEFAULT 1,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_chat_messages_session_id
                ON chat_messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_session_seq
                ON chat_messages(session_id, sequence_number);
            "#,
        )?;

        // Create chat_tool_calls table for tool calls within chat messages
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS chat_tool_calls (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                sequence_number INTEGER NOT NULL,
                tool_name TEXT NOT NULL,
                tool_input TEXT,
                tool_output TEXT,
                is_error INTEGER NOT NULL DEFAULT 0,
                is_complete INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (message_id) REFERENCES chat_messages(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_chat_tool_calls_message_id
                ON chat_tool_calls(message_id);
            "#,
        )?;

        Ok(())
    }

    /// Run migrations for schema updates.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Check if FTS tables exist and create them if not
        let has_fts: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='interactions_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_fts {
            self.create_fts_tables(&conn)?;
        }

        // Check if reference count triggers exist
        let has_triggers: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='trigger' AND name='file_snapshots_insert_ref'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_triggers {
            self.create_reference_triggers(&conn)?;
        }

        Ok(())
    }

    /// Check if FTS tables need migration (e.g., missing prefix indexes).
    /// Returns true if tables exist but need to be recreated with new options.
    fn check_fts_needs_migration(&self, conn: &Connection) -> Result<bool> {
        // Check if interactions_fts exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='interactions_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !table_exists {
            return Ok(false); // No migration needed, tables will be created fresh
        }

        // Check if the FTS table has prefix indexes by looking at the config
        // FTS5 stores config in tablename_config; prefix option creates entries
        let has_prefix: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM interactions_fts_config WHERE k = 'pgsz'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        // If the table exists but doesn't have expected config, it needs migration
        // Actually, let's check for prefix specifically - the config table stores 'prefix' key
        let has_prefix_config: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='interactions_fts_idx' AND sql LIKE '%prefix%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        // Simple heuristic: if the FTS table exists but we can't verify prefix config,
        // assume it needs migration. The prefix tables would be: tablename_idx
        // Actually, the safest check is: try to query with a prefix and see if it's indexed
        // But that's complex. Let's use a simpler approach: check the row count of _config
        let config_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM interactions_fts_config",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Tables with prefix='2 3' have more config entries (one for each prefix size)
        // Without prefix, typically 2-3 entries; with prefix='2 3', typically 4-5 entries
        Ok(config_count < 4)
    }

    /// Drop FTS tables and their triggers for recreation.
    fn drop_fts_tables(&self, conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- Drop triggers first
            DROP TRIGGER IF EXISTS interactions_fts_insert;
            DROP TRIGGER IF EXISTS interactions_fts_delete;
            DROP TRIGGER IF EXISTS interactions_fts_update;
            DROP TRIGGER IF EXISTS tool_invocations_fts_insert;
            DROP TRIGGER IF EXISTS tool_invocations_fts_delete;
            DROP TRIGGER IF EXISTS tool_invocations_fts_update;

            -- Drop FTS virtual tables
            DROP TABLE IF EXISTS interactions_fts;
            DROP TABLE IF EXISTS tool_invocations_fts;
            "#,
        )?;
        Ok(())
    }

    /// Create FTS5 virtual tables and sync triggers.
    /// Includes prefix='2 3' for optimized prefix matching queries.
    fn create_fts_tables(&self, conn: &Connection) -> Result<()> {
        tracing::info!(target: "clauset::db", "Creating FTS5 tables for interactions");

        // Check if we need to migrate (recreate with prefix indexes)
        let needs_migration = self.check_fts_needs_migration(conn)?;
        if needs_migration {
            tracing::info!(target: "clauset::db", "Migrating FTS5 tables to add prefix indexes");
            self.drop_fts_tables(conn)?;
        }

        conn.execute_batch(
            r#"
            -- FTS5 index for interactions (prompts and summaries)
            -- prefix='2 3' optimizes 2 and 3 character prefix queries
            CREATE VIRTUAL TABLE IF NOT EXISTS interactions_fts USING fts5(
                user_prompt,
                assistant_summary,
                content='interactions',
                content_rowid='rowid',
                prefix='2 3'
            );

            -- FTS5 index for tool invocations (file paths and inputs)
            CREATE VIRTUAL TABLE IF NOT EXISTS tool_invocations_fts USING fts5(
                file_path,
                tool_input,
                tool_name,
                content='tool_invocations',
                content_rowid='rowid',
                prefix='2 3'
            );

            -- Triggers to keep interactions_fts in sync
            CREATE TRIGGER IF NOT EXISTS interactions_fts_insert
            AFTER INSERT ON interactions BEGIN
                INSERT INTO interactions_fts(rowid, user_prompt, assistant_summary)
                VALUES (NEW.rowid, NEW.user_prompt, NEW.assistant_summary);
            END;

            CREATE TRIGGER IF NOT EXISTS interactions_fts_delete
            AFTER DELETE ON interactions BEGIN
                INSERT INTO interactions_fts(interactions_fts, rowid, user_prompt, assistant_summary)
                VALUES ('delete', OLD.rowid, OLD.user_prompt, OLD.assistant_summary);
            END;

            CREATE TRIGGER IF NOT EXISTS interactions_fts_update
            AFTER UPDATE ON interactions BEGIN
                INSERT INTO interactions_fts(interactions_fts, rowid, user_prompt, assistant_summary)
                VALUES ('delete', OLD.rowid, OLD.user_prompt, OLD.assistant_summary);
                INSERT INTO interactions_fts(rowid, user_prompt, assistant_summary)
                VALUES (NEW.rowid, NEW.user_prompt, NEW.assistant_summary);
            END;

            -- Triggers to keep tool_invocations_fts in sync
            CREATE TRIGGER IF NOT EXISTS tool_invocations_fts_insert
            AFTER INSERT ON tool_invocations BEGIN
                INSERT INTO tool_invocations_fts(rowid, file_path, tool_input, tool_name)
                VALUES (NEW.rowid, NEW.file_path, NEW.tool_input, NEW.tool_name);
            END;

            CREATE TRIGGER IF NOT EXISTS tool_invocations_fts_delete
            AFTER DELETE ON tool_invocations BEGIN
                INSERT INTO tool_invocations_fts(tool_invocations_fts, rowid, file_path, tool_input, tool_name)
                VALUES ('delete', OLD.rowid, OLD.file_path, OLD.tool_input, OLD.tool_name);
            END;

            CREATE TRIGGER IF NOT EXISTS tool_invocations_fts_update
            AFTER UPDATE ON tool_invocations BEGIN
                INSERT INTO tool_invocations_fts(tool_invocations_fts, rowid, file_path, tool_input, tool_name)
                VALUES ('delete', OLD.rowid, OLD.file_path, OLD.tool_input, OLD.tool_name);
                INSERT INTO tool_invocations_fts(rowid, file_path, tool_input, tool_name)
                VALUES (NEW.rowid, NEW.file_path, NEW.tool_input, NEW.tool_name);
            END;
            "#,
        )?;

        // If we migrated, rebuild the FTS index from existing data
        if needs_migration {
            self.rebuild_fts_index(conn)?;
        }

        Ok(())
    }

    /// Rebuild FTS index from existing data in source tables.
    /// Called after FTS tables are recreated during migration.
    fn rebuild_fts_index(&self, conn: &Connection) -> Result<()> {
        tracing::info!(target: "clauset::db", "Rebuilding FTS index from existing data");

        // Rebuild interactions_fts from interactions table
        conn.execute(
            r#"
            INSERT INTO interactions_fts(rowid, user_prompt, assistant_summary)
            SELECT rowid, user_prompt, assistant_summary FROM interactions
            "#,
            [],
        )?;

        // Rebuild tool_invocations_fts from tool_invocations table
        conn.execute(
            r#"
            INSERT INTO tool_invocations_fts(rowid, file_path, tool_input, tool_name)
            SELECT rowid, file_path, tool_input, tool_name FROM tool_invocations
            "#,
            [],
        )?;

        tracing::info!(target: "clauset::db", "FTS index rebuild complete");
        Ok(())
    }

    /// Create reference count triggers for file_contents deduplication.
    fn create_reference_triggers(&self, conn: &Connection) -> Result<()> {
        tracing::info!(target: "clauset::db", "Creating reference count triggers");

        conn.execute_batch(
            r#"
            -- Increment reference count when a new snapshot references content
            CREATE TRIGGER IF NOT EXISTS file_snapshots_insert_ref
            AFTER INSERT ON file_snapshots BEGIN
                UPDATE file_contents
                SET reference_count = reference_count + 1
                WHERE content_hash = NEW.content_hash;
            END;

            -- Decrement reference count when a snapshot is deleted
            CREATE TRIGGER IF NOT EXISTS file_snapshots_delete_ref
            AFTER DELETE ON file_snapshots BEGIN
                UPDATE file_contents
                SET reference_count = reference_count - 1
                WHERE content_hash = OLD.content_hash;
            END;
            "#,
        )?;

        Ok(())
    }

    // =========================================================================
    // Interaction CRUD
    // =========================================================================

    /// Insert a new interaction.
    pub fn insert_interaction(&self, interaction: &Interaction) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO interactions (
                id, session_id, sequence_number, user_prompt, assistant_summary,
                started_at, ended_at, cost_usd_delta, input_tokens_delta,
                output_tokens_delta, status, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                interaction.id.to_string(),
                interaction.session_id.to_string(),
                interaction.sequence_number,
                interaction.user_prompt,
                interaction.assistant_summary,
                interaction.started_at.to_rfc3339(),
                interaction.ended_at.map(|t| t.to_rfc3339()),
                interaction.cost_usd_delta,
                interaction.input_tokens_delta as i64,
                interaction.output_tokens_delta as i64,
                status_to_string(interaction.status),
                interaction.error_message,
            ],
        )?;
        Ok(())
    }

    /// Get an interaction by ID.
    pub fn get_interaction(&self, id: Uuid) -> Result<Option<Interaction>> {
        let conn = self.conn.lock().unwrap();
        let interaction = conn
            .query_row(
                "SELECT * FROM interactions WHERE id = ?1",
                params![id.to_string()],
                |row| self.row_to_interaction(row),
            )
            .optional()?;
        Ok(interaction)
    }

    /// Get the active (in-progress) interaction for a session.
    pub fn get_active_interaction(&self, session_id: Uuid) -> Result<Option<Interaction>> {
        let conn = self.conn.lock().unwrap();
        let interaction = conn
            .query_row(
                "SELECT * FROM interactions WHERE session_id = ?1 AND status = 'active' ORDER BY sequence_number DESC LIMIT 1",
                params![session_id.to_string()],
                |row| self.row_to_interaction(row),
            )
            .optional()?;
        Ok(interaction)
    }

    /// Get the next sequence number for a session.
    pub fn next_sequence_number(&self, session_id: Uuid) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let max_seq: Option<i64> = conn
            .query_row(
                "SELECT MAX(sequence_number) FROM interactions WHERE session_id = ?1",
                params![session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(max_seq.map(|n| n as u32 + 1).unwrap_or(1))
    }

    /// List interactions for a session (paginated, newest first).
    pub fn list_interactions(
        &self,
        session_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Interaction>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT * FROM interactions
            WHERE session_id = ?1
            ORDER BY sequence_number DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let interactions = stmt
            .query_map(
                params![session_id.to_string(), limit, offset],
                |row| self.row_to_interaction(row),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(interactions)
    }

    /// List interaction summaries for a session.
    pub fn list_interaction_summaries(
        &self,
        session_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<InteractionSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                i.*,
                (SELECT COUNT(*) FROM tool_invocations WHERE interaction_id = i.id) as tool_count,
                (SELECT COUNT(DISTINCT file_path) FROM file_snapshots WHERE interaction_id = i.id AND snapshot_type = 'after') as files_changed
            FROM interactions i
            WHERE i.session_id = ?1
            ORDER BY i.sequence_number DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let summaries = stmt
            .query_map(params![session_id.to_string(), limit, offset], |row| {
                let interaction = self.row_to_interaction(row)?;
                let tool_count: i64 = row.get("tool_count")?;
                let files_changed: i64 = row.get("files_changed")?;
                Ok(InteractionSummary::from_interaction(
                    &interaction,
                    tool_count as u32,
                    files_changed as u32,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(summaries)
    }

    /// Update an interaction.
    pub fn update_interaction(&self, interaction: &Interaction) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE interactions SET
                assistant_summary = ?1,
                ended_at = ?2,
                cost_usd_delta = ?3,
                input_tokens_delta = ?4,
                output_tokens_delta = ?5,
                status = ?6,
                error_message = ?7
            WHERE id = ?8
            "#,
            params![
                interaction.assistant_summary,
                interaction.ended_at.map(|t| t.to_rfc3339()),
                interaction.cost_usd_delta,
                interaction.input_tokens_delta as i64,
                interaction.output_tokens_delta as i64,
                status_to_string(interaction.status),
                interaction.error_message,
                interaction.id.to_string(),
            ],
        )?;
        Ok(())
    }

    /// Mark an interaction as completed.
    pub fn complete_interaction(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE interactions SET status = 'completed', ended_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    /// Mark an interaction as completed with cost/token deltas.
    pub fn complete_interaction_with_costs(
        &self,
        id: Uuid,
        cost_usd_delta: f64,
        input_tokens_delta: u64,
        output_tokens_delta: u64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"UPDATE interactions
               SET status = 'completed',
                   ended_at = ?1,
                   cost_usd_delta = ?2,
                   input_tokens_delta = ?3,
                   output_tokens_delta = ?4
               WHERE id = ?5"#,
            params![
                Utc::now().to_rfc3339(),
                cost_usd_delta,
                input_tokens_delta as i64,
                output_tokens_delta as i64,
                id.to_string()
            ],
        )?;
        Ok(())
    }

    /// Update costs for the most recent interaction in a session.
    /// Called when costs are updated after the interaction was marked complete.
    pub fn update_latest_interaction_costs(
        &self,
        session_id: Uuid,
        cost_usd_delta: f64,
        input_tokens_delta: u64,
        output_tokens_delta: u64,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // Only update if the new values are greater (costs only increase)
        let updated = conn.execute(
            r#"UPDATE interactions
               SET cost_usd_delta = MAX(cost_usd_delta, ?1),
                   input_tokens_delta = MAX(input_tokens_delta, ?2),
                   output_tokens_delta = MAX(output_tokens_delta, ?3)
               WHERE session_id = ?4
                 AND sequence_number = (
                     SELECT MAX(sequence_number) FROM interactions WHERE session_id = ?4
                 )"#,
            params![
                cost_usd_delta,
                input_tokens_delta as i64,
                output_tokens_delta as i64,
                session_id.to_string()
            ],
        )?;
        Ok(updated > 0)
    }

    /// Mark an interaction as failed.
    pub fn fail_interaction(&self, id: Uuid, error: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE interactions SET status = 'failed', ended_at = ?1, error_message = ?2 WHERE id = ?3",
            params![Utc::now().to_rfc3339(), error, id.to_string()],
        )?;
        Ok(())
    }

    /// Fail all active interactions for a session (e.g., on unexpected termination).
    pub fn fail_active_interactions(&self, session_id: Uuid, error: &str) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE interactions SET status = 'failed', ended_at = ?1, error_message = ?2 WHERE session_id = ?3 AND status = 'active'",
            params![Utc::now().to_rfc3339(), error, session_id.to_string()],
        )?;
        Ok(count as u32)
    }

    // =========================================================================
    // Tool Invocation CRUD
    // =========================================================================

    /// Insert a new tool invocation.
    pub fn insert_tool_invocation(&self, invocation: &ToolInvocation) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO tool_invocations (
                id, interaction_id, tool_use_id, sequence_number, tool_name,
                tool_input, tool_output_preview, file_path, is_error,
                error_message, started_at, ended_at, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                invocation.id.to_string(),
                invocation.interaction_id.to_string(),
                invocation.tool_use_id,
                invocation.sequence_number,
                invocation.tool_name,
                invocation.tool_input.to_string(),
                invocation.tool_output_preview,
                invocation.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                invocation.is_error as i32,
                invocation.error_message,
                invocation.started_at.to_rfc3339(),
                invocation.ended_at.map(|t| t.to_rfc3339()),
                invocation.duration_ms,
            ],
        )?;
        Ok(())
    }

    /// Get a tool invocation by ID.
    pub fn get_tool_invocation(&self, id: Uuid) -> Result<Option<ToolInvocation>> {
        let conn = self.conn.lock().unwrap();
        let invocation = conn
            .query_row(
                "SELECT * FROM tool_invocations WHERE id = ?1",
                params![id.to_string()],
                |row| self.row_to_tool_invocation(row),
            )
            .optional()?;
        Ok(invocation)
    }

    /// Get a tool invocation by Claude's tool_use_id.
    pub fn get_tool_invocation_by_tool_use_id(
        &self,
        tool_use_id: &str,
    ) -> Result<Option<ToolInvocation>> {
        let conn = self.conn.lock().unwrap();
        let invocation = conn
            .query_row(
                "SELECT * FROM tool_invocations WHERE tool_use_id = ?1",
                params![tool_use_id],
                |row| self.row_to_tool_invocation(row),
            )
            .optional()?;
        Ok(invocation)
    }

    /// List tool invocations for an interaction (in order).
    pub fn list_tool_invocations(&self, interaction_id: Uuid) -> Result<Vec<ToolInvocation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT * FROM tool_invocations
            WHERE interaction_id = ?1
            ORDER BY sequence_number ASC
            "#,
        )?;
        let invocations = stmt
            .query_map(params![interaction_id.to_string()], |row| {
                self.row_to_tool_invocation(row)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(invocations)
    }

    /// Get the next tool sequence number for an interaction.
    pub fn next_tool_sequence_number(&self, interaction_id: Uuid) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let max_seq: Option<i64> = conn
            .query_row(
                "SELECT MAX(sequence_number) FROM tool_invocations WHERE interaction_id = ?1",
                params![interaction_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(max_seq.map(|n| n as u32 + 1).unwrap_or(1))
    }

    /// Update a tool invocation (typically after completion).
    pub fn update_tool_invocation(&self, invocation: &ToolInvocation) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE tool_invocations SET
                tool_output_preview = ?1,
                is_error = ?2,
                error_message = ?3,
                ended_at = ?4,
                duration_ms = ?5
            WHERE id = ?6
            "#,
            params![
                invocation.tool_output_preview,
                invocation.is_error as i32,
                invocation.error_message,
                invocation.ended_at.map(|t| t.to_rfc3339()),
                invocation.duration_ms,
                invocation.id.to_string(),
            ],
        )?;
        Ok(())
    }

    /// Complete a tool invocation with output, error status, and timing.
    pub fn complete_tool_invocation(
        &self,
        id: Uuid,
        output_preview: Option<String>,
        is_error: bool,
        error_message: Option<String>,
    ) -> Result<()> {
        let now = Utc::now();
        let conn = self.conn.lock().unwrap();

        // Get start time to calculate duration
        let started_at: Option<String> = conn
            .query_row(
                "SELECT started_at FROM tool_invocations WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        let duration_ms = started_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|start| (now - start.with_timezone(&Utc)).num_milliseconds())
        });

        conn.execute(
            r#"
            UPDATE tool_invocations SET
                tool_output_preview = ?1,
                is_error = ?2,
                error_message = ?3,
                ended_at = ?4,
                duration_ms = ?5
            WHERE id = ?6
            "#,
            params![
                output_preview,
                is_error as i32,
                error_message,
                now.to_rfc3339(),
                duration_ms,
                id.to_string(),
            ],
        )?;
        Ok(())
    }

    // =========================================================================
    // File Content & Snapshot CRUD
    // =========================================================================

    /// Store file content with deduplication.
    ///
    /// Returns (content_hash, is_new) where is_new indicates if content was inserted.
    pub fn store_file_content(&self, content: &[u8]) -> Result<(String, bool)> {
        // Compute SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = format!("{:x}", hasher.finalize());

        let conn = self.conn.lock().unwrap();

        // Check if content already exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM file_contents WHERE content_hash = ?1",
                params![&hash],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if exists {
            // Content already stored, no need to insert
            return Ok((hash, false));
        }

        // Compress with zstd (level 3 is a good balance of speed/ratio)
        let compressed = zstd::encode_all(content, 3)
            .map_err(|e| ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let compression_ratio = if compressed.is_empty() {
            1.0
        } else {
            content.len() as f64 / compressed.len() as f64
        };

        conn.execute(
            r#"
            INSERT INTO file_contents (
                content_hash, compressed_content, original_size,
                compression_ratio, created_at, reference_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, 0)
            "#,
            params![
                hash,
                compressed,
                content.len() as i64,
                compression_ratio,
                Utc::now().to_rfc3339(),
            ],
        )?;

        Ok((hash, true))
    }

    /// Retrieve file content by hash.
    pub fn get_file_content(&self, content_hash: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let result: Option<Vec<u8>> = conn
            .query_row(
                "SELECT compressed_content FROM file_contents WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get(0),
            )
            .optional()?;

        match result {
            Some(compressed) => {
                let decompressed = zstd::decode_all(&compressed[..]).map_err(|e| {
                    ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
                })?;
                Ok(Some(decompressed))
            }
            None => Ok(None),
        }
    }

    /// Insert a file snapshot.
    pub fn insert_file_snapshot(&self, snapshot: &FileSnapshot) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO file_snapshots (
                id, interaction_id, tool_invocation_id, file_path,
                content_hash, snapshot_type, file_size, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                snapshot.id.to_string(),
                snapshot.interaction_id.to_string(),
                snapshot.tool_invocation_id.map(|id| id.to_string()),
                snapshot.file_path.to_string_lossy(),
                snapshot.content_hash,
                snapshot_type_to_string(snapshot.snapshot_type),
                snapshot.file_size as i64,
                snapshot.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get a file snapshot by ID.
    pub fn get_file_snapshot(&self, id: Uuid) -> Result<Option<FileSnapshot>> {
        let conn = self.conn.lock().unwrap();
        let snapshot = conn
            .query_row(
                "SELECT * FROM file_snapshots WHERE id = ?1",
                params![id.to_string()],
                |row| self.row_to_file_snapshot(row),
            )
            .optional()?;
        Ok(snapshot)
    }

    /// Get before/after snapshots for a tool invocation.
    pub fn get_tool_snapshots(
        &self,
        tool_invocation_id: Uuid,
    ) -> Result<(Option<FileSnapshot>, Option<FileSnapshot>)> {
        let conn = self.conn.lock().unwrap();

        let before: Option<FileSnapshot> = conn
            .query_row(
                "SELECT * FROM file_snapshots WHERE tool_invocation_id = ?1 AND snapshot_type = 'before'",
                params![tool_invocation_id.to_string()],
                |row| self.row_to_file_snapshot(row),
            )
            .optional()?;

        let after: Option<FileSnapshot> = conn
            .query_row(
                "SELECT * FROM file_snapshots WHERE tool_invocation_id = ?1 AND snapshot_type = 'after'",
                params![tool_invocation_id.to_string()],
                |row| self.row_to_file_snapshot(row),
            )
            .optional()?;

        Ok((before, after))
    }

    /// Get the content of a snapshot for a specific file in an interaction.
    pub fn get_snapshot_content(
        &self,
        interaction_id: Uuid,
        file_path: &str,
        snapshot_type: SnapshotType,
    ) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();

        let type_str = match snapshot_type {
            SnapshotType::Before => "before",
            SnapshotType::After => "after",
        };

        // First get the content hash from file_snapshots
        let content_hash: Option<String> = conn
            .query_row(
                r#"
                SELECT content_hash
                FROM file_snapshots
                WHERE interaction_id = ?1 AND file_path = ?2 AND snapshot_type = ?3
                "#,
                params![interaction_id.to_string(), file_path, type_str],
                |row| row.get(0),
            )
            .optional()?;

        // If we have a content hash, fetch the actual content
        if let Some(hash) = content_hash {
            self.get_file_content(&hash)
        } else {
            Ok(None)
        }
    }

    /// List file changes for an interaction.
    pub fn list_file_changes(&self, interaction_id: Uuid) -> Result<Vec<FileChange>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                file_path,
                MAX(CASE WHEN snapshot_type = 'before' THEN id END) as before_id,
                MAX(CASE WHEN snapshot_type = 'after' THEN id END) as after_id
            FROM file_snapshots
            WHERE interaction_id = ?1
            GROUP BY file_path
            "#,
        )?;

        let changes = stmt
            .query_map(params![interaction_id.to_string()], |row| {
                let file_path: String = row.get(0)?;
                let before_id: Option<String> = row.get(1)?;
                let after_id: Option<String> = row.get(2)?;

                let change_type = match (before_id.is_some(), after_id.is_some()) {
                    (false, true) => FileChangeType::Created,
                    (true, false) => FileChangeType::Deleted,
                    _ => FileChangeType::Modified,
                };

                Ok(FileChange {
                    file_path: file_path.into(),
                    change_type,
                    before_snapshot_id: before_id.and_then(|s| Uuid::parse_str(&s).ok()),
                    after_snapshot_id: after_id.and_then(|s| Uuid::parse_str(&s).ok()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(changes)
    }

    /// Get file changes with computed diffs for an interaction.
    ///
    /// Returns a list of file changes with the actual diff content.
    pub fn get_file_changes_with_diffs(
        &self,
        interaction_id: Uuid,
        context_lines: usize,
    ) -> Result<Vec<FileChangeWithDiff>> {
        let changes = self.list_file_changes(interaction_id)?;
        let mut results = Vec::new();

        for change in changes {
            let before_content = change
                .before_snapshot_id
                .and_then(|id| self.get_file_snapshot(id).ok().flatten())
                .and_then(|snap| self.get_file_content(&snap.content_hash).ok().flatten());

            let after_content = change
                .after_snapshot_id
                .and_then(|id| self.get_file_snapshot(id).ok().flatten())
                .and_then(|snap| self.get_file_content(&snap.content_hash).ok().flatten());

            let diff = crate::diff::compute_diff(
                before_content.as_deref(),
                after_content.as_deref(),
                context_lines,
            );

            results.push(FileChangeWithDiff {
                file_path: change.file_path,
                change_type: change.change_type,
                diff,
            });
        }

        Ok(results)
    }

    /// Get unified diff string for a tool invocation's file changes.
    pub fn get_unified_diff(
        &self,
        tool_invocation_id: Uuid,
        context_lines: usize,
    ) -> Result<Option<String>> {
        let (before, after) = self.get_tool_snapshots(tool_invocation_id)?;

        // If no snapshots, no diff
        if before.is_none() && after.is_none() {
            return Ok(None);
        }

        let file_path = after
            .as_ref()
            .or(before.as_ref())
            .map(|s| s.file_path.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let before_content = before
            .as_ref()
            .and_then(|snap| self.get_file_content(&snap.content_hash).ok().flatten());

        let after_content = after
            .as_ref()
            .and_then(|snap| self.get_file_content(&snap.content_hash).ok().flatten());

        let unified = crate::diff::generate_unified_diff(
            before_content.as_deref(),
            after_content.as_deref(),
            &format!("a/{}", file_path),
            &format!("b/{}", file_path),
            context_lines,
        );

        Ok(Some(unified))
    }

    // =========================================================================
    // Cleanup & Retention
    // =========================================================================

    /// Clean up data older than the specified number of days.
    ///
    /// Returns the number of interactions deleted.
    pub fn cleanup_old_data(&self, retention_days: i64) -> Result<CleanupStats> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days);
        let cutoff_str = cutoff.to_rfc3339();

        let conn = self.conn.lock().unwrap();

        // Delete old interactions (cascades to tool_invocations and file_snapshots)
        let interactions_deleted = conn.execute(
            "DELETE FROM interactions WHERE started_at < ?1",
            params![&cutoff_str],
        )?;

        // Delete orphaned file_contents (reference_count = 0)
        let contents_deleted = conn.execute(
            "DELETE FROM file_contents WHERE reference_count <= 0",
            [],
        )?;

        // Optimize FTS tables
        let _ = conn.execute(
            "INSERT INTO interactions_fts(interactions_fts) VALUES('optimize')",
            [],
        );
        let _ = conn.execute(
            "INSERT INTO tool_invocations_fts(tool_invocations_fts) VALUES('optimize')",
            [],
        );

        Ok(CleanupStats {
            interactions_deleted: interactions_deleted as u32,
            contents_deleted: contents_deleted as u32,
        })
    }

    /// Vacuum the database to reclaim space.
    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", [])?;
        Ok(())
    }

    /// Get storage statistics.
    pub fn get_storage_stats(&self) -> Result<StorageStats> {
        let conn = self.conn.lock().unwrap();

        let interaction_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM interactions",
            [],
            |row| row.get(0),
        )?;

        let tool_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tool_invocations",
            [],
            |row| row.get(0),
        )?;

        let snapshot_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_snapshots",
            [],
            |row| row.get(0),
        )?;

        let content_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_contents",
            [],
            |row| row.get(0),
        )?;

        let total_content_size: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(original_size), 0) FROM file_contents",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_compressed_size: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(compressed_content)), 0) FROM file_contents",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(StorageStats {
            interaction_count: interaction_count as u64,
            tool_count: tool_count as u64,
            snapshot_count: snapshot_count as u64,
            content_count: content_count as u64,
            total_content_size: total_content_size as u64,
            total_compressed_size: total_compressed_size as u64,
        })
    }

    // =========================================================================
    // Full-Text Search
    // =========================================================================

    /// Escape and format a query string for FTS5 prefix matching.
    ///
    /// Each token is wrapped in double quotes (to handle special chars like '.')
    /// and suffixed with '*' for prefix matching. Multiple tokens are joined
    /// with AND so all must match.
    ///
    /// Examples:
    /// - "Re" → "\"Re\"*" → matches "Read", "Return", "Rebuild"
    /// - "describe project" → "\"describe\"* AND \"project\"*"
    /// - "package.json" → "\"package.json\"*" → handles special chars
    fn escape_fts5_query(query: &str) -> String {
        let tokens: Vec<String> = query
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| {
                // Escape internal double quotes and wrap in quotes with wildcard
                let escaped = t.replace('"', "\"\"");
                format!("\"{}\"*", escaped)
            })
            .collect();

        if tokens.is_empty() {
            return String::new();
        }

        // Join with AND - all tokens must match as prefixes
        tokens.join(" AND ")
    }

    /// Search interactions using full-text search.
    ///
    /// Searches across user prompts and assistant summaries.
    /// Returns interactions matching the query, ordered by relevance.
    pub fn search_interactions(
        &self,
        query: &str,
        session_id: Option<Uuid>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchResult>> {
        let conn = self.conn.lock().unwrap();
        let escaped_query = Self::escape_fts5_query(query);

        let mut results = Vec::new();

        if let Some(sid) = session_id {
            let mut stmt = conn.prepare(
                r#"
                SELECT i.*, bm25(interactions_fts) as rank
                FROM interactions_fts fts
                JOIN interactions i ON i.rowid = fts.rowid
                WHERE interactions_fts MATCH ?1
                AND i.session_id = ?2
                ORDER BY rank
                LIMIT ?3 OFFSET ?4
                "#,
            )?;

            let rows = stmt.query_map(
                params![&escaped_query, sid.to_string(), limit as i64, offset as i64],
                |row| {
                    let interaction = self.row_to_interaction(row)?;
                    let rank: f64 = row.get("rank")?;
                    Ok(SearchResult {
                        interaction,
                        relevance_score: -rank,
                        matched_field: SearchField::Prompt,
                    })
                },
            )?;

            for result in rows {
                results.push(result?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT i.*, bm25(interactions_fts) as rank
                FROM interactions_fts fts
                JOIN interactions i ON i.rowid = fts.rowid
                WHERE interactions_fts MATCH ?1
                ORDER BY rank
                LIMIT ?2 OFFSET ?3
                "#,
            )?;

            let rows = stmt.query_map(params![&escaped_query, limit as i64, offset as i64], |row| {
                let interaction = self.row_to_interaction(row)?;
                let rank: f64 = row.get("rank")?;
                Ok(SearchResult {
                    interaction,
                    relevance_score: -rank,
                    matched_field: SearchField::Prompt,
                })
            })?;

            for result in rows {
                results.push(result?);
            }
        }

        Ok(results)
    }

    /// Search tool invocations by file path or input content.
    pub fn search_tool_invocations(
        &self,
        query: &str,
        interaction_id: Option<Uuid>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ToolInvocation>> {
        let conn = self.conn.lock().unwrap();
        let escaped_query = Self::escape_fts5_query(query);

        let mut results = Vec::new();

        if let Some(iid) = interaction_id {
            let mut stmt = conn.prepare(
                r#"
                SELECT t.*
                FROM tool_invocations_fts fts
                JOIN tool_invocations t ON t.rowid = fts.rowid
                WHERE tool_invocations_fts MATCH ?1
                AND t.interaction_id = ?2
                ORDER BY bm25(tool_invocations_fts)
                LIMIT ?3 OFFSET ?4
                "#,
            )?;

            let rows = stmt.query_map(
                params![&escaped_query, iid.to_string(), limit as i64, offset as i64],
                |row| self.row_to_tool_invocation(row),
            )?;

            for result in rows {
                results.push(result?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT t.*
                FROM tool_invocations_fts fts
                JOIN tool_invocations t ON t.rowid = fts.rowid
                WHERE tool_invocations_fts MATCH ?1
                ORDER BY bm25(tool_invocations_fts)
                LIMIT ?2 OFFSET ?3
                "#,
            )?;

            let rows = stmt.query_map(params![&escaped_query, limit as i64, offset as i64], |row| {
                self.row_to_tool_invocation(row)
            })?;

            for result in rows {
                results.push(result?);
            }
        }

        Ok(results)
    }

    /// Search for files by path pattern.
    ///
    /// This is a simple LIKE search, not FTS5.
    pub fn search_files_by_path(
        &self,
        path_pattern: &str,
        limit: usize,
    ) -> Result<Vec<FilePathMatch>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT
                fs.file_path,
                i.id as interaction_id,
                i.session_id,
                i.started_at,
                COUNT(*) as snapshot_count
            FROM file_snapshots fs
            JOIN interactions i ON fs.interaction_id = i.id
            WHERE fs.file_path LIKE ?1
            GROUP BY fs.file_path, i.id
            ORDER BY i.started_at DESC
            LIMIT ?2
            "#,
        )?;

        let pattern = format!("%{}%", path_pattern);
        let results = stmt
            .query_map(params![pattern, limit as i64], |row| {
                Ok(FilePathMatch {
                    file_path: PathBuf::from(row.get::<_, String>(0)?),
                    interaction_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    session_id: Uuid::parse_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                    modified_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_default(),
                    snapshot_count: row.get::<_, i64>(4)? as u32,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Global search across prompts, files, and tool inputs.
    pub fn global_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<GlobalSearchResults> {
        let interactions = self.search_interactions(query, None, limit, 0)?;
        let tools = self.search_tool_invocations(query, None, limit, 0)?;
        let files = self.search_files_by_path(query, limit)?;

        Ok(GlobalSearchResults {
            interactions,
            tool_invocations: tools,
            file_matches: files,
        })
    }

    // =========================================================================
    // Cost Analytics
    // =========================================================================

    /// Get total cost and token usage for a session.
    pub fn get_session_analytics(&self, session_id: Uuid) -> Result<SessionAnalytics> {
        let conn = self.conn.lock().unwrap();

        let row = conn.query_row(
            r#"
            SELECT
                COUNT(*) as interaction_count,
                COALESCE(SUM(cost_usd_delta), 0.0) as total_cost_usd,
                COALESCE(SUM(input_tokens_delta), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens_delta), 0) as total_output_tokens,
                MIN(started_at) as first_interaction_at,
                MAX(started_at) as last_interaction_at
            FROM interactions
            WHERE session_id = ?1
            "#,
            params![session_id.to_string()],
            |row| {
                Ok(SessionAnalytics {
                    session_id,
                    interaction_count: row.get::<_, i64>(0)? as u32,
                    total_cost_usd: row.get(1)?,
                    total_input_tokens: row.get::<_, i64>(2)? as u64,
                    total_output_tokens: row.get::<_, i64>(3)? as u64,
                    first_interaction_at: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    last_interaction_at: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                })
            },
        )?;

        Ok(row)
    }

    /// Get all session IDs that have interactions.
    pub fn get_all_session_ids(&self) -> Result<Vec<Uuid>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT session_id
            FROM interactions
            GROUP BY session_id
            ORDER BY MAX(started_at) DESC
            "#,
        )?;

        let rows = stmt
            .query_map([], |row| {
                let session_id: String = row.get(0)?;
                Ok(Uuid::parse_str(&session_id).unwrap_or_default())
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Get daily cost breakdown for the last N days.
    pub fn get_daily_cost_breakdown(&self, days: u32) -> Result<Vec<DailyCostEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                DATE(started_at) as date,
                COUNT(*) as interaction_count,
                COALESCE(SUM(cost_usd_delta), 0.0) as total_cost_usd,
                COALESCE(SUM(input_tokens_delta), 0) as input_tokens,
                COALESCE(SUM(output_tokens_delta), 0) as output_tokens
            FROM interactions
            WHERE started_at >= DATE('now', '-' || ?1 || ' days')
            GROUP BY DATE(started_at)
            ORDER BY date DESC
            "#,
        )?;

        let rows = stmt
            .query_map(params![days as i64], |row| {
                Ok(DailyCostEntry {
                    date: row.get(0)?,
                    interaction_count: row.get::<_, i64>(1)? as u32,
                    total_cost_usd: row.get(2)?,
                    input_tokens: row.get::<_, i64>(3)? as u64,
                    output_tokens: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Get cost breakdown by tool type.
    pub fn get_tool_cost_breakdown(&self, session_id: Option<Uuid>) -> Result<Vec<ToolCostEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut results = Vec::new();

        if let Some(sid) = session_id {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    t.tool_name,
                    COUNT(*) as invocation_count,
                    AVG(t.duration_ms) as avg_duration_ms
                FROM tool_invocations t
                JOIN interactions i ON t.interaction_id = i.id
                WHERE i.session_id = ?1
                GROUP BY t.tool_name
                ORDER BY invocation_count DESC
                "#,
            )?;

            let rows = stmt.query_map(params![sid.to_string()], |row| {
                Ok(ToolCostEntry {
                    tool_name: row.get(0)?,
                    invocation_count: row.get::<_, i64>(1)? as u32,
                    avg_duration_ms: row.get::<_, Option<f64>>(2)?,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    tool_name,
                    COUNT(*) as invocation_count,
                    AVG(duration_ms) as avg_duration_ms
                FROM tool_invocations
                GROUP BY tool_name
                ORDER BY invocation_count DESC
                "#,
            )?;

            let rows = stmt.query_map([], |row| {
                Ok(ToolCostEntry {
                    tool_name: row.get(0)?,
                    invocation_count: row.get::<_, i64>(1)? as u32,
                    avg_duration_ms: row.get::<_, Option<f64>>(2)?,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    /// Get overall analytics summary.
    pub fn get_analytics_summary(&self) -> Result<AnalyticsSummary> {
        let conn = self.conn.lock().unwrap();

        let row = conn.query_row(
            r#"
            SELECT
                COUNT(DISTINCT session_id) as session_count,
                COUNT(*) as interaction_count,
                COALESCE(SUM(cost_usd_delta), 0.0) as total_cost_usd,
                COALESCE(SUM(input_tokens_delta), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens_delta), 0) as total_output_tokens,
                AVG(cost_usd_delta) as avg_cost_per_interaction,
                (SELECT COUNT(*) FROM tool_invocations) as total_tool_invocations,
                (SELECT COUNT(*) FROM file_snapshots) as total_file_changes
            FROM interactions
            "#,
            [],
            |row| {
                Ok(AnalyticsSummary {
                    session_count: row.get::<_, i64>(0)? as u32,
                    interaction_count: row.get::<_, i64>(1)? as u32,
                    total_cost_usd: row.get(2)?,
                    total_input_tokens: row.get::<_, i64>(3)? as u64,
                    total_output_tokens: row.get::<_, i64>(4)? as u64,
                    avg_cost_per_interaction: row.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                    total_tool_invocations: row.get::<_, i64>(6)? as u32,
                    total_file_changes: row.get::<_, i64>(7)? as u32,
                })
            },
        )?;

        Ok(row)
    }

    /// Get top N most expensive interactions.
    pub fn get_most_expensive_interactions(&self, limit: usize) -> Result<Vec<Interaction>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT *
            FROM interactions
            WHERE cost_usd_delta > 0
            ORDER BY cost_usd_delta DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt
            .query_map(params![limit as i64], |row| self.row_to_interaction(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    // =========================================================================
    // Chat Message CRUD (for chat view persistence)
    // =========================================================================

    /// Save a chat message (insert or update).
    pub fn save_chat_message(&self, msg: &clauset_types::ChatMessage) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Get next sequence number if this is a new message
        let seq_num: i64 = conn
            .query_row(
                "SELECT sequence_number FROM chat_messages WHERE id = ?1",
                params![&msg.id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| {
                // New message - get next sequence
                conn.query_row(
                    "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM chat_messages WHERE session_id = ?1",
                    params![msg.session_id.to_string()],
                    |row| row.get(0),
                )
                .unwrap_or(1)
            });

        let role_str = match msg.role {
            clauset_types::ChatRole::User => "user",
            clauset_types::ChatRole::Assistant => "assistant",
        };

        conn.execute(
            r#"
            INSERT INTO chat_messages (id, session_id, sequence_number, role, content, is_streaming, is_complete, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                is_streaming = excluded.is_streaming,
                is_complete = excluded.is_complete
            "#,
            params![
                &msg.id,
                msg.session_id.to_string(),
                seq_num,
                role_str,
                &msg.content,
                msg.is_streaming as i32,
                msg.is_complete as i32,
                msg.timestamp as i64,
            ],
        )?;

        Ok(())
    }

    /// Save a chat tool call (insert or update).
    pub fn save_chat_tool_call(&self, message_id: &str, tool_call: &clauset_types::ChatToolCall) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Get next sequence number if this is a new tool call
        let seq_num: i64 = conn
            .query_row(
                "SELECT sequence_number FROM chat_tool_calls WHERE id = ?1",
                params![&tool_call.id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| {
                // New tool call - get next sequence
                conn.query_row(
                    "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM chat_tool_calls WHERE message_id = ?1",
                    params![message_id],
                    |row| row.get(0),
                )
                .unwrap_or(1)
            });

        conn.execute(
            r#"
            INSERT INTO chat_tool_calls (id, message_id, sequence_number, tool_name, tool_input, tool_output, is_error, is_complete)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                tool_output = excluded.tool_output,
                is_error = excluded.is_error,
                is_complete = excluded.is_complete
            "#,
            params![
                &tool_call.id,
                message_id,
                seq_num,
                &tool_call.name,
                tool_call.input.to_string(),
                tool_call.output.as_deref(),
                tool_call.is_error as i32,
                tool_call.is_complete as i32,
            ],
        )?;

        Ok(())
    }

    /// Get all chat messages for a session (ordered by sequence).
    pub fn get_chat_messages(&self, session_id: Uuid) -> Result<Vec<clauset_types::ChatMessage>> {
        let conn = self.conn.lock().unwrap();

        // Get all messages
        let mut stmt = conn.prepare(
            r#"
            SELECT id, session_id, role, content, is_streaming, is_complete, timestamp
            FROM chat_messages
            WHERE session_id = ?1
            ORDER BY sequence_number ASC
            "#,
        )?;

        let messages: Vec<(String, String, String, i32, i32, i64)> = stmt
            .query_map(params![session_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>("id")?,
                    row.get::<_, String>("role")?,
                    row.get::<_, String>("content")?,
                    row.get::<_, i32>("is_streaming")?,
                    row.get::<_, i32>("is_complete")?,
                    row.get::<_, i64>("timestamp")?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        drop(stmt);

        // Build ChatMessage objects with tool calls
        let mut result = Vec::new();
        for (id, role, content, is_streaming, is_complete, timestamp) in messages {
            // Get tool calls for this message
            let tool_calls = self.get_chat_tool_calls_internal(&conn, &id)?;

            result.push(clauset_types::ChatMessage {
                id,
                session_id,
                role: if role == "user" {
                    clauset_types::ChatRole::User
                } else {
                    clauset_types::ChatRole::Assistant
                },
                content,
                tool_calls,
                is_streaming: is_streaming != 0,
                is_complete: is_complete != 0,
                timestamp: timestamp as u64,
            });
        }

        Ok(result)
    }

    /// Internal helper to get tool calls for a message.
    fn get_chat_tool_calls_internal(
        &self,
        conn: &Connection,
        message_id: &str,
    ) -> Result<Vec<clauset_types::ChatToolCall>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT id, tool_name, tool_input, tool_output, is_error, is_complete
            FROM chat_tool_calls
            WHERE message_id = ?1
            ORDER BY sequence_number ASC
            "#,
        )?;

        let tool_calls = stmt
            .query_map(params![message_id], |row| {
                let id: String = row.get("id")?;
                let name: String = row.get("tool_name")?;
                let input_str: Option<String> = row.get("tool_input")?;
                let output: Option<String> = row.get("tool_output")?;
                let is_error: i32 = row.get("is_error")?;
                let is_complete: i32 = row.get("is_complete")?;

                let input = input_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::Value::Null);

                Ok(clauset_types::ChatToolCall {
                    id,
                    name,
                    input,
                    output,
                    is_error: is_error != 0,
                    is_complete: is_complete != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tool_calls)
    }

    /// Delete all chat messages for a session.
    pub fn delete_chat_messages(&self, session_id: Uuid) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM chat_messages WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        Ok(count as u32)
    }

    /// Get the count of chat messages for a session.
    pub fn get_chat_message_count(&self, session_id: Uuid) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE session_id = ?1",
            params![session_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    // =========================================================================
    // Row conversion helpers
    // =========================================================================

    fn row_to_interaction(&self, row: &rusqlite::Row) -> rusqlite::Result<Interaction> {
        let id: String = row.get("id")?;
        let session_id: String = row.get("session_id")?;
        let sequence_number: i64 = row.get("sequence_number")?;
        let user_prompt: String = row.get("user_prompt")?;
        let assistant_summary: Option<String> = row.get("assistant_summary")?;
        let started_at: String = row.get("started_at")?;
        let ended_at: Option<String> = row.get("ended_at")?;
        let cost_usd_delta: f64 = row.get("cost_usd_delta")?;
        let input_tokens_delta: i64 = row.get("input_tokens_delta")?;
        let output_tokens_delta: i64 = row.get("output_tokens_delta")?;
        let status: String = row.get("status")?;
        let error_message: Option<String> = row.get("error_message")?;

        Ok(Interaction {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            session_id: Uuid::parse_str(&session_id).unwrap_or_default(),
            sequence_number: sequence_number as u32,
            user_prompt,
            assistant_summary,
            started_at: DateTime::parse_from_rfc3339(&started_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
            ended_at: ended_at.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            cost_usd_delta,
            input_tokens_delta: input_tokens_delta as u64,
            output_tokens_delta: output_tokens_delta as u64,
            status: string_to_status(&status),
            error_message,
        })
    }

    fn row_to_tool_invocation(&self, row: &rusqlite::Row) -> rusqlite::Result<ToolInvocation> {
        let id: String = row.get("id")?;
        let interaction_id: String = row.get("interaction_id")?;
        let tool_use_id: Option<String> = row.get("tool_use_id")?;
        let sequence_number: i64 = row.get("sequence_number")?;
        let tool_name: String = row.get("tool_name")?;
        let tool_input: String = row.get("tool_input")?;
        let tool_output_preview: Option<String> = row.get("tool_output_preview")?;
        let file_path: Option<String> = row.get("file_path")?;
        let is_error: i32 = row.get("is_error")?;
        let error_message: Option<String> = row.get("error_message")?;
        let started_at: String = row.get("started_at")?;
        let ended_at: Option<String> = row.get("ended_at")?;
        let duration_ms: Option<i64> = row.get("duration_ms")?;

        Ok(ToolInvocation {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            interaction_id: Uuid::parse_str(&interaction_id).unwrap_or_default(),
            tool_use_id,
            sequence_number: sequence_number as u32,
            tool_name,
            tool_input: serde_json::from_str(&tool_input).unwrap_or(serde_json::Value::Null),
            tool_output_preview,
            file_path: file_path.map(|s| s.into()),
            is_error: is_error != 0,
            error_message,
            started_at: DateTime::parse_from_rfc3339(&started_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
            ended_at: ended_at.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            duration_ms,
        })
    }

    fn row_to_file_snapshot(&self, row: &rusqlite::Row) -> rusqlite::Result<FileSnapshot> {
        let id: String = row.get("id")?;
        let interaction_id: String = row.get("interaction_id")?;
        let tool_invocation_id: Option<String> = row.get("tool_invocation_id")?;
        let file_path: String = row.get("file_path")?;
        let content_hash: String = row.get("content_hash")?;
        let snapshot_type: String = row.get("snapshot_type")?;
        let file_size: i64 = row.get("file_size")?;
        let created_at: String = row.get("created_at")?;

        Ok(FileSnapshot {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            interaction_id: Uuid::parse_str(&interaction_id).unwrap_or_default(),
            tool_invocation_id: tool_invocation_id
                .and_then(|s| Uuid::parse_str(&s).ok()),
            file_path: file_path.into(),
            content_hash,
            snapshot_type: string_to_snapshot_type(&snapshot_type),
            file_size: file_size as u64,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
        })
    }
}

/// Statistics from a cleanup operation.
#[derive(Debug, Clone)]
pub struct CleanupStats {
    pub interactions_deleted: u32,
    pub contents_deleted: u32,
}

/// Storage usage statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageStats {
    pub interaction_count: u64,
    pub tool_count: u64,
    pub snapshot_count: u64,
    pub content_count: u64,
    pub total_content_size: u64,
    pub total_compressed_size: u64,
}

impl StorageStats {
    /// Calculate compression ratio.
    pub fn compression_ratio(&self) -> f64 {
        if self.total_compressed_size == 0 {
            1.0
        } else {
            self.total_content_size as f64 / self.total_compressed_size as f64
        }
    }
}

// Helper functions

fn status_to_string(status: InteractionStatus) -> &'static str {
    match status {
        InteractionStatus::Active => "active",
        InteractionStatus::Completed => "completed",
        InteractionStatus::Failed => "failed",
    }
}

fn string_to_status(s: &str) -> InteractionStatus {
    match s {
        "active" => InteractionStatus::Active,
        "completed" => InteractionStatus::Completed,
        "failed" => InteractionStatus::Failed,
        _ => InteractionStatus::Active,
    }
}

fn snapshot_type_to_string(t: SnapshotType) -> &'static str {
    match t {
        SnapshotType::Before => "before",
        SnapshotType::After => "after",
    }
}

fn string_to_snapshot_type(s: &str) -> SnapshotType {
    match s {
        "before" => SnapshotType::Before,
        "after" => SnapshotType::After,
        _ => SnapshotType::After,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (InteractionStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create sessions table first (normally done by SessionStore)
        // This is required for the foreign key constraint on interactions
        let conn = Connection::open(&db_path).unwrap();
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
            "#,
        )
        .unwrap();
        drop(conn);

        let store = InteractionStore::open(&db_path).unwrap();
        (store, temp_dir)
    }

    fn create_test_session(store: &InteractionStore, session_id: Uuid) {
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, claude_session_id, project_path, model, status, mode, created_at, last_activity_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session_id.to_string(),
                "test-claude-session",
                "/test/path",
                "claude-opus-4-5-20251101",
                "active",
                "agent",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ],
        ).unwrap();
    }

    #[test]
    fn test_interaction_crud() {
        let (store, _dir) = create_test_store();
        let session_id = Uuid::new_v4();
        create_test_session(&store, session_id);

        // Create interaction
        let interaction = Interaction::new(session_id, 1, "Test prompt".to_string());
        store.insert_interaction(&interaction).unwrap();

        // Read interaction
        let loaded = store.get_interaction(interaction.id).unwrap().unwrap();
        assert_eq!(loaded.user_prompt, "Test prompt");
        assert_eq!(loaded.status, InteractionStatus::Active);

        // Complete interaction
        store.complete_interaction(interaction.id).unwrap();
        let loaded = store.get_interaction(interaction.id).unwrap().unwrap();
        assert_eq!(loaded.status, InteractionStatus::Completed);
    }

    #[test]
    fn test_tool_invocation_crud() {
        let (store, _dir) = create_test_store();
        let session_id = Uuid::new_v4();
        create_test_session(&store, session_id);
        let interaction = Interaction::new(session_id, 1, "Test".to_string());
        store.insert_interaction(&interaction).unwrap();

        // Create tool invocation
        let invocation = ToolInvocation::new(
            interaction.id,
            1,
            "Read".to_string(),
            serde_json::json!({"file_path": "/test.rs"}),
            Some("toolu_123".to_string()),
        );
        store.insert_tool_invocation(&invocation).unwrap();

        // Read by ID
        let loaded = store.get_tool_invocation(invocation.id).unwrap().unwrap();
        assert_eq!(loaded.tool_name, "Read");

        // Read by tool_use_id
        let loaded = store
            .get_tool_invocation_by_tool_use_id("toolu_123")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.id, invocation.id);
    }

    #[test]
    fn test_file_content_deduplication() {
        let (store, _dir) = create_test_store();

        let content = b"Hello, world!";

        // Store content first time
        let (hash1, is_new1) = store.store_file_content(content).unwrap();
        assert!(is_new1);

        // Store same content again
        let (hash2, is_new2) = store.store_file_content(content).unwrap();
        assert!(!is_new2);
        assert_eq!(hash1, hash2);

        // Retrieve content
        let loaded = store.get_file_content(&hash1).unwrap().unwrap();
        assert_eq!(loaded, content);
    }

    #[test]
    fn test_sequence_numbers() {
        let (store, _dir) = create_test_store();
        let session_id = Uuid::new_v4();
        create_test_session(&store, session_id);

        // First interaction should be 1
        let seq = store.next_sequence_number(session_id).unwrap();
        assert_eq!(seq, 1);

        // Insert interaction
        let interaction = Interaction::new(session_id, 1, "First".to_string());
        store.insert_interaction(&interaction).unwrap();

        // Next should be 2
        let seq = store.next_sequence_number(session_id).unwrap();
        assert_eq!(seq, 2);
    }
}
