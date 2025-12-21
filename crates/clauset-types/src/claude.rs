//! Claude CLI event types for stream-json output parsing.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

/// Events emitted by Claude CLI in stream-json mode.
/// Matches the actual output format from `claude -p --verbose --output-format stream-json`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// System initialization with session info.
    System(SystemEvent),
    /// Assistant message (contains the full message object).
    Assistant(AssistantEvent),
    /// User message.
    User(UserEvent),
    /// Result/summary at the end.
    Result(ResultEvent),
}

/// System event with subtype.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub subtype: String,
    pub session_id: Uuid,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Assistant event containing the message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantEvent {
    pub message: AssistantMessage,
    pub session_id: Uuid,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

/// The assistant's message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<MessageUsage>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Content block in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
}

/// Token usage in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(flatten)]
    pub extra: Value,
}

/// User message event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEvent {
    pub message: Value,
    pub session_id: Uuid,
    #[serde(flatten)]
    pub extra: Value,
}

/// Result event at the end of a session/turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEvent {
    pub subtype: String,
    pub session_id: Uuid,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub usage: Option<ResultUsage>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Usage statistics in a result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(flatten)]
    pub extra: Value,
}
