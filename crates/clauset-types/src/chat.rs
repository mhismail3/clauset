//! Chat message types for the chat mode view.
//!
//! These types represent structured chat messages extracted from terminal output
//! and hook events. They power the chat view which displays messages as bubbles.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A chat message representing either a user prompt or Claude's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique message identifier
    pub id: String,
    /// Session this message belongs to
    pub session_id: Uuid,
    /// Who sent this message
    pub role: ChatRole,
    /// Message text content (may be partial during streaming)
    pub content: String,
    /// Tool calls made during this assistant message
    #[serde(default)]
    pub tool_calls: Vec<ChatToolCall>,
    /// Whether content is still being streamed
    #[serde(default)]
    pub is_streaming: bool,
    /// Whether the message is fully complete
    #[serde(default)]
    pub is_complete: bool,
    /// Message timestamp (ms since Unix epoch)
    pub timestamp: u64,
}

/// Role of the message sender in chat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// User's prompt
    User,
    /// Claude's response
    Assistant,
}

/// A tool call within an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    /// Tool use ID from Claude
    pub id: String,
    /// Tool name (Read, Write, Bash, etc.)
    pub name: String,
    /// Tool input parameters
    pub input: Value,
    /// Tool output (None if still executing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Whether the tool execution resulted in an error
    #[serde(default)]
    pub is_error: bool,
    /// Whether the tool has completed execution
    #[serde(default)]
    pub is_complete: bool,
}

impl ChatMessage {
    /// Create a new user message.
    pub fn user(session_id: Uuid, content: String) -> Self {
        Self {
            id: format!("user-{}", uuid::Uuid::new_v4()),
            session_id,
            role: ChatRole::User,
            content,
            tool_calls: Vec::new(),
            is_streaming: false,
            is_complete: true,
            timestamp: now_ms(),
        }
    }

    /// Create a new assistant message (starts streaming).
    pub fn assistant(session_id: Uuid) -> Self {
        Self {
            id: format!("assistant-{}", uuid::Uuid::new_v4()),
            session_id,
            role: ChatRole::Assistant,
            content: String::new(),
            tool_calls: Vec::new(),
            is_streaming: true,
            is_complete: false,
            timestamp: now_ms(),
        }
    }

    /// Append content to a streaming message.
    pub fn append_content(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    /// Add a tool call to this message.
    pub fn add_tool_call(&mut self, tool_call: ChatToolCall) {
        self.tool_calls.push(tool_call);
    }

    /// Mark the message as complete.
    pub fn complete(&mut self) {
        self.is_streaming = false;
        self.is_complete = true;
    }
}

impl ChatToolCall {
    /// Create a new tool call (execution starting).
    pub fn new(id: String, name: String, input: Value) -> Self {
        Self {
            id,
            name,
            input,
            output: None,
            is_error: false,
            is_complete: false,
        }
    }

    /// Complete the tool call with output.
    pub fn complete_with_output(&mut self, output: String, is_error: bool) {
        self.output = Some(output);
        self.is_error = is_error;
        self.is_complete = true;
    }
}

/// Event types for chat message updates sent via WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    /// A new message was created (user or assistant)
    Message {
        session_id: Uuid,
        message: ChatMessage,
    },
    /// Content was appended to a streaming message
    ContentDelta {
        session_id: Uuid,
        message_id: String,
        delta: String,
    },
    /// A tool call was started
    ToolCallStart {
        session_id: Uuid,
        message_id: String,
        tool_call: ChatToolCall,
    },
    /// A tool call completed
    ToolCallComplete {
        session_id: Uuid,
        message_id: String,
        tool_call_id: String,
        output: String,
        is_error: bool,
    },
    /// Message streaming completed
    MessageComplete {
        session_id: Uuid,
        message_id: String,
    },
}

/// Get current time in milliseconds since Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_creation() {
        let session_id = Uuid::new_v4();
        let msg = ChatMessage::user(session_id, "Hello Claude".to_string());

        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "Hello Claude");
        assert!(msg.is_complete);
        assert!(!msg.is_streaming);
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn test_assistant_message_streaming() {
        let session_id = Uuid::new_v4();
        let mut msg = ChatMessage::assistant(session_id);

        assert_eq!(msg.role, ChatRole::Assistant);
        assert!(msg.is_streaming);
        assert!(!msg.is_complete);

        msg.append_content("Hello");
        msg.append_content(" world");
        assert_eq!(msg.content, "Hello world");

        msg.complete();
        assert!(!msg.is_streaming);
        assert!(msg.is_complete);
    }

    #[test]
    fn test_tool_call() {
        let session_id = Uuid::new_v4();
        let mut msg = ChatMessage::assistant(session_id);

        let mut tool_call = ChatToolCall::new(
            "toolu_123".to_string(),
            "Read".to_string(),
            serde_json::json!({"file_path": "/test/file.rs"}),
        );

        assert!(!tool_call.is_complete);
        assert!(tool_call.output.is_none());

        tool_call.complete_with_output("file contents".to_string(), false);
        assert!(tool_call.is_complete);
        assert!(!tool_call.is_error);

        msg.add_tool_call(tool_call);
        assert_eq!(msg.tool_calls.len(), 1);
    }
}
