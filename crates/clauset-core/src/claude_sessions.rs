//! Read Claude Code session data from ~/.claude.
//!
//! Claude Code stores all session information in ~/.claude/:
//! - history.jsonl: Session metadata (ID, project, timestamp, preview)
//! - projects/<path>/<session-id>.jsonl: Full conversation history

use crate::Result;
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::debug;

/// A session from Claude's ~/.claude storage.
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    /// Claude's session UUID
    pub session_id: String,
    /// Project path where the session was started
    pub project_path: PathBuf,
    /// When the session was last active
    pub timestamp: DateTime<Utc>,
    /// Preview text (first prompt or display text)
    pub preview: String,
}

/// Entry from ~/.claude/history.jsonl
#[derive(Debug, Deserialize)]
struct HistoryEntry {
    /// Display text shown in history
    display: String,
    /// Unix timestamp in milliseconds
    timestamp: i64,
    /// Project path
    project: String,
    /// Session ID (UUID)
    #[serde(rename = "sessionId")]
    session_id: String,
}

/// Reads Claude Code session data from ~/.claude.
pub struct ClaudeSessionReader {
    claude_dir: PathBuf,
}

impl ClaudeSessionReader {
    /// Create a new reader with the default ~/.claude directory.
    pub fn new() -> Self {
        let claude_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".claude");
        Self { claude_dir }
    }

    /// Create a reader with a custom Claude directory (for testing).
    pub fn with_dir(claude_dir: PathBuf) -> Self {
        Self { claude_dir }
    }

    /// List all sessions from ~/.claude/history.jsonl for a specific project.
    /// Returns sessions sorted by timestamp (most recent first).
    pub fn list_sessions_for_project(&self, project_path: &Path) -> Result<Vec<ClaudeSession>> {
        let history_path = self.claude_dir.join("history.jsonl");

        if !history_path.exists() {
            debug!(target: "clauset::claude_sessions", "No history.jsonl found at {:?}", history_path);
            return Ok(Vec::new());
        }

        let file = File::open(&history_path)?;
        let reader = BufReader::new(file);

        let project_str = project_path.to_string_lossy();
        let mut sessions: Vec<ClaudeSession> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let entry: HistoryEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Filter by project path
            if entry.project != project_str {
                continue;
            }

            // Skip duplicates (keep most recent)
            if seen_ids.contains(&entry.session_id) {
                continue;
            }
            seen_ids.insert(entry.session_id.clone());

            // Convert timestamp (milliseconds since epoch)
            let timestamp = Utc.timestamp_millis_opt(entry.timestamp)
                .single()
                .unwrap_or_else(Utc::now);

            sessions.push(ClaudeSession {
                session_id: entry.session_id,
                project_path: PathBuf::from(&entry.project),
                timestamp,
                preview: truncate_preview(&entry.display),
            });
        }

        // Sort by timestamp descending (most recent first)
        sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        debug!(
            target: "clauset::claude_sessions",
            "Found {} sessions for project {:?}",
            sessions.len(),
            project_path
        );

        Ok(sessions)
    }

    /// List all sessions from ~/.claude/history.jsonl (across all projects).
    /// Returns sessions sorted by timestamp (most recent first).
    pub fn list_all_sessions(&self) -> Result<Vec<ClaudeSession>> {
        let history_path = self.claude_dir.join("history.jsonl");

        if !history_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&history_path)?;
        let reader = BufReader::new(file);

        let mut sessions: Vec<ClaudeSession> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let entry: HistoryEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Skip duplicates (keep most recent)
            if seen_ids.contains(&entry.session_id) {
                continue;
            }
            seen_ids.insert(entry.session_id.clone());

            let timestamp = Utc.timestamp_millis_opt(entry.timestamp)
                .single()
                .unwrap_or_else(Utc::now);

            sessions.push(ClaudeSession {
                session_id: entry.session_id,
                project_path: PathBuf::from(&entry.project),
                timestamp,
                preview: truncate_preview(&entry.display),
            });
        }

        sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(sessions)
    }

    /// Check if a session exists in Claude's storage.
    pub fn session_exists(&self, session_id: &str) -> bool {
        // Check history.jsonl for the session
        let history_path = self.claude_dir.join("history.jsonl");

        if !history_path.exists() {
            return false;
        }

        let file = match File::open(&history_path) {
            Ok(f) => f,
            Err(_) => return false,
        };

        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let entry: HistoryEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.session_id == session_id {
                return true;
            }
        }

        false
    }

    /// Get a specific session by ID.
    pub fn get_session(&self, session_id: &str) -> Result<Option<ClaudeSession>> {
        let history_path = self.claude_dir.join("history.jsonl");

        if !history_path.exists() {
            return Ok(None);
        }

        let file = File::open(&history_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let entry: HistoryEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.session_id == session_id {
                let timestamp = Utc.timestamp_millis_opt(entry.timestamp)
                    .single()
                    .unwrap_or_else(Utc::now);

                return Ok(Some(ClaudeSession {
                    session_id: entry.session_id,
                    project_path: PathBuf::from(&entry.project),
                    timestamp,
                    preview: truncate_preview(&entry.display),
                }));
            }
        }

        Ok(None)
    }
}

impl Default for ClaudeSessionReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate preview text to a reasonable length.
fn truncate_preview(s: &str) -> String {
    const MAX_LEN: usize = 100;
    if s.len() <= MAX_LEN {
        s.to_string()
    } else {
        format!("{}...", &s[..MAX_LEN - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reader_creation() {
        let reader = ClaudeSessionReader::new();
        assert!(reader.claude_dir.ends_with(".claude"));
    }
}
