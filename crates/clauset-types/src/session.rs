//! Session types and state machine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Session status in the lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session created but not yet started.
    Created,
    /// Claude process is starting.
    Starting,
    /// Session is active and processing.
    Active,
    /// Waiting for user input.
    WaitingInput,
    /// Session stopped (completed or terminated).
    Stopped,
    /// Session encountered an error.
    Error,
}

/// Session mode determines how we interact with Claude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// Structured JSON streaming via --output-format stream-json.
    StreamJson,
    /// Raw PTY terminal mode.
    Terminal,
}

/// A Claude Code session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Our internal session ID.
    pub id: Uuid,
    /// Claude's session ID (for resumption).
    pub claude_session_id: Uuid,
    /// Project directory path.
    pub project_path: PathBuf,
    /// Model being used (e.g., "sonnet", "opus").
    pub model: String,
    /// Current status.
    pub status: SessionStatus,
    /// Interaction mode.
    pub mode: SessionMode,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_activity_at: DateTime<Utc>,
    /// Total cost in USD (if available).
    pub total_cost_usd: f64,
    /// Input tokens used.
    pub input_tokens: u64,
    /// Output tokens generated.
    pub output_tokens: u64,
    /// Context window usage percentage (0-100).
    pub context_percent: u8,
    /// Preview text (first prompt or last message).
    pub preview: String,
}

/// Summary view of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub claude_session_id: Uuid,
    pub project_path: PathBuf,
    pub model: String,
    pub status: SessionStatus,
    pub mode: SessionMode,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub context_percent: u8,
    pub preview: String,
    /// Current step/activity (e.g., "Thinking", "Read", "Ready")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    /// Recent actions performed by Claude
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_actions: Vec<crate::RecentAction>,
}

impl From<Session> for SessionSummary {
    fn from(s: Session) -> Self {
        Self {
            id: s.id,
            claude_session_id: s.claude_session_id,
            project_path: s.project_path,
            model: s.model,
            status: s.status,
            mode: s.mode,
            created_at: s.created_at,
            last_activity_at: s.last_activity_at,
            total_cost_usd: s.total_cost_usd,
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            context_percent: s.context_percent,
            preview: s.preview,
            current_step: None,
            recent_actions: Vec::new(),
        }
    }
}
