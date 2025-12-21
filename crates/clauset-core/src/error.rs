//! Error types for Clauset.

use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ClausetError {
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("Session already exists: {0}")]
    SessionAlreadyExists(Uuid),

    #[error("Session limit exceeded: max {0} concurrent sessions")]
    SessionLimitExceeded(usize),

    #[error("Invalid session state: expected {expected}, got {actual}")]
    InvalidSessionState { expected: String, actual: String },

    #[error("Process spawn failed: {0}")]
    ProcessSpawnFailed(String),

    #[error("PTY error: {0}")]
    PtyError(String),

    #[error("Claude CLI error: {0}")]
    ClaudeCliError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Channel send error")]
    ChannelSendError,
}
