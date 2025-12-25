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

/// A message extracted from Claude's transcript.
#[derive(Debug, Clone)]
pub struct TranscriptMessage {
    /// "user" or "assistant"
    pub role: String,
    /// The text content of the message
    pub content: String,
    /// Timestamp of the message
    pub timestamp: DateTime<Utc>,
}

/// Transcript entry types from Claude's JSONL format.
#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<TranscriptMessage_>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage_ {
    role: Option<String>,
    content: serde_json::Value,
}

impl ClaudeSessionReader {
    /// Read messages from a Claude transcript file.
    /// Returns user and assistant messages in chronological order.
    pub fn read_transcript(&self, session_id: &str, project_path: &Path) -> Result<Vec<TranscriptMessage>> {
        let transcript_path = self.get_transcript_path(session_id, project_path);

        if !transcript_path.exists() {
            debug!(
                target: "clauset::claude_sessions",
                "No transcript found at {:?}",
                transcript_path
            );
            return Ok(Vec::new());
        }

        let file = File::open(&transcript_path)?;
        let reader = BufReader::new(file);

        let mut messages: Vec<TranscriptMessage> = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let entry: TranscriptEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Only process user and assistant messages
            if entry.entry_type != "user" && entry.entry_type != "assistant" {
                continue;
            }

            let message = match entry.message {
                Some(m) => m,
                None => continue,
            };

            let role = message.role.unwrap_or_else(|| entry.entry_type.clone());

            // Extract text content from the message
            let content = extract_text_content(&message.content);
            if content.is_empty() {
                continue;
            }

            // Parse timestamp
            let timestamp = entry
                .timestamp
                .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            messages.push(TranscriptMessage {
                role,
                content,
                timestamp,
            });
        }

        debug!(
            target: "clauset::claude_sessions",
            "Read {} messages from transcript for session {}",
            messages.len(),
            session_id
        );

        Ok(messages)
    }

    /// Get the path to a transcript file.
    fn get_transcript_path(&self, session_id: &str, project_path: &Path) -> PathBuf {
        // Encode project path (replace / with -)
        let encoded_path = project_path
            .to_string_lossy()
            .replace('/', "-");

        self.claude_dir
            .join("projects")
            .join(encoded_path)
            .join(format!("{}.jsonl", session_id))
    }
}

/// Extract text content from a message content value.
/// Handles both string content and array of content blocks.
fn extract_text_content(content: &serde_json::Value) -> String {
    match content {
        // Simple string content
        serde_json::Value::String(s) => s.clone(),

        // Array of content blocks (Claude's format)
        serde_json::Value::Array(blocks) => {
            let mut text_parts: Vec<String> = Vec::new();

            for block in blocks {
                // Check for text blocks
                if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                    if block_type == "text" {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                    // Skip tool_use, tool_result, thinking blocks
                }
            }

            text_parts.join("\n\n")
        }

        _ => String::new(),
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

    #[test]
    fn test_extract_text_content_string() {
        let content = serde_json::json!("Hello world");
        assert_eq!(extract_text_content(&content), "Hello world");
    }

    #[test]
    fn test_extract_text_content_array() {
        let content = serde_json::json!([
            {"type": "text", "text": "First part"},
            {"type": "tool_use", "name": "Read"},
            {"type": "text", "text": "Second part"}
        ]);
        assert_eq!(extract_text_content(&content), "First part\n\nSecond part");
    }
}
