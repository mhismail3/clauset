//! WebSocket message protocol between client and server.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

use crate::{ResultUsage, SessionStatus};

/// Messages sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    /// Send text input to Claude.
    Input { content: String },
    /// Send raw terminal input (PTY mode).
    TerminalInput { data: Vec<u8> },
    /// Resize terminal.
    Resize { rows: u16, cols: u16 },
    /// Ping for keepalive.
    Ping { timestamp: u64 },
    /// Request current session state.
    GetState,
    /// Update session stats from parsed status line.
    StatusUpdate {
        model: String,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
    },
}

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    /// Session initialization info.
    SessionInit {
        session_id: Uuid,
        claude_session_id: Uuid,
        model: String,
        tools: Vec<String>,
        cwd: PathBuf,
    },
    /// Streaming text from Claude.
    Text {
        message_id: String,
        content: String,
        is_complete: bool,
    },
    /// Claude is using a tool.
    ToolUse {
        message_id: String,
        tool_use_id: String,
        tool_name: String,
        input: Value,
    },
    /// Tool execution result.
    ToolResult {
        tool_use_id: String,
        output: String,
        is_error: bool,
    },
    /// Message completed.
    MessageComplete { message_id: String },
    /// Claude is asking for input.
    InputRequired { prompt: String },
    /// Session result/summary.
    Result {
        success: bool,
        duration_ms: u64,
        total_cost_usd: f64,
        usage: Option<ResultUsage>,
    },
    /// Raw terminal output (PTY mode).
    TerminalOutput { data: Vec<u8> },
    /// Session status changed.
    StatusChange {
        old_status: SessionStatus,
        new_status: SessionStatus,
    },
    /// Error occurred.
    Error { code: String, message: String },
    /// Pong response.
    Pong { timestamp: u64 },
    /// Current session state.
    State {
        session_id: Uuid,
        status: SessionStatus,
        messages: Vec<StoredMessage>,
    },
}

/// A stored message for state recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<StoredToolCall>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolCall {
    pub tool_use_id: String,
    pub tool_name: String,
    pub input: Value,
    pub output: Option<String>,
    pub is_error: bool,
}
