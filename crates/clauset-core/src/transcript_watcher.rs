//! Real-time JSONL transcript file watcher for Claude Code sessions.
//!
//! Watches the transcript file written by Claude Code at `~/.claude/projects/<path>/<session>.jsonl`
//! and emits events for each content block (user messages, thinking, text, tool_use, tool_result).

use crate::Result;
use clauset_types::{ChatEvent, ChatMessage, ChatToolCall};
use notify::{
    event::{AccessKind, AccessMode, ModifyKind},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};
use uuid::Uuid;

/// Events emitted by the transcript watcher.
#[derive(Debug, Clone)]
pub enum TranscriptEvent {
    /// User submitted a message
    UserMessage {
        message_id: String,
        content: String,
        timestamp: u64,
    },
    /// Assistant started a new turn
    AssistantTurnStart { message_id: String, timestamp: u64 },
    /// Thinking content block
    Thinking {
        message_id: String,
        content: String,
    },
    /// Text content block
    Text {
        message_id: String,
        content: String,
    },
    /// Tool use block (Claude decided to use a tool)
    ToolUse {
        message_id: String,
        tool_use_id: String,
        name: String,
        input: Value,
    },
    /// Tool result block
    ToolResult {
        message_id: String,
        tool_use_id: String,
        content: Value,
        is_error: bool,
    },
    /// End of assistant turn
    AssistantTurnEnd { message_id: String },
}

/// Entry from Claude's transcript JSONL.
#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<TranscriptMessageEntry>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessageEntry {
    id: Option<String>,
    role: Option<String>,
    content: Value,
}

/// Watches a Claude Code transcript file and emits content events in real-time.
pub struct TranscriptWatcher {
    path: PathBuf,
    session_id: Uuid,
    file_position: u64,
    line_buffer: String,
    event_tx: mpsc::UnboundedSender<TranscriptEvent>,
    current_assistant_message_id: Option<String>,
}

impl TranscriptWatcher {
    /// Create a new transcript watcher.
    /// Events will be sent through the provided channel.
    pub fn new(
        path: PathBuf,
        session_id: Uuid,
        event_tx: mpsc::UnboundedSender<TranscriptEvent>,
    ) -> Self {
        Self {
            path,
            session_id,
            file_position: 0,
            line_buffer: String::new(),
            event_tx,
            current_assistant_message_id: None,
        }
    }

    /// Start watching the transcript file.
    /// Returns a handle that can be used to stop watching.
    pub fn start(
        self,
    ) -> Result<TranscriptWatcherHandle> {
        let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();
        let watcher = Arc::new(tokio::sync::Mutex::new(self));

        // Read any existing content first
        {
            let mut watcher_guard = futures::executor::block_on(watcher.lock());
            if let Err(e) = watcher_guard.process_new_content() {
                warn!(
                    target: "clauset::transcript_watcher",
                    "Failed to read initial transcript content: {}",
                    e
                );
            }
        }

        let watcher_clone = watcher.clone();
        let path = futures::executor::block_on(watcher.lock()).path.clone();

        // Start file watcher
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
        let mut file_watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = notify_tx.send(event);
            }
        }).map_err(|e| crate::ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        file_watcher.watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| crate::ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        // Spawn task to process file events
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = notify_rx.recv() => {
                        // Check if file was modified or written
                        let should_read = matches!(
                            event.kind,
                            EventKind::Modify(ModifyKind::Data(_))
                                | EventKind::Modify(ModifyKind::Any)
                                | EventKind::Access(AccessKind::Close(AccessMode::Write))
                        );

                        if should_read {
                            let mut watcher_guard = watcher_clone.lock().await;
                            if let Err(e) = watcher_guard.process_new_content() {
                                warn!(
                                    target: "clauset::transcript_watcher",
                                    "Failed to process transcript update: {}",
                                    e
                                );
                            }
                        }
                    }
                    Some(()) = stop_rx.recv() => {
                        debug!(
                            target: "clauset::transcript_watcher",
                            "Stopping transcript watcher"
                        );
                        break;
                    }
                    else => {
                        break;
                    }
                }
            }
        });

        Ok(TranscriptWatcherHandle {
            stop_tx,
            _file_watcher: file_watcher,
        })
    }

    /// Process new content added to the file since last read.
    fn process_new_content(&mut self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }

        let mut file = File::open(&self.path)?;
        let file_len = file.metadata()?.len();

        // If file is smaller than our position, it was truncated (new session)
        if file_len < self.file_position {
            debug!(
                target: "clauset::transcript_watcher",
                "File was truncated, resetting position"
            );
            self.file_position = 0;
            self.line_buffer.clear();
            self.current_assistant_message_id = None;
        }

        // Seek to where we left off
        file.seek(SeekFrom::Start(self.file_position))?;

        // Read new content
        let mut reader = BufReader::new(file);
        let mut new_content = String::new();
        reader.read_to_string(&mut new_content)?;

        if new_content.is_empty() {
            return Ok(());
        }

        // Update position
        self.file_position += new_content.len() as u64;

        // Append to line buffer and process complete lines
        self.line_buffer.push_str(&new_content);

        // Process complete lines
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();

            if !line.trim().is_empty() {
                self.process_line(&line);
            }
        }

        Ok(())
    }

    /// Process a single JSONL line.
    fn process_line(&mut self, line: &str) {
        let entry: TranscriptEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => {
                trace!(
                    target: "clauset::transcript_watcher",
                    "Failed to parse transcript line: {} - {}",
                    e,
                    &line[..line.len().min(100)]
                );
                return;
            }
        };

        let timestamp = entry
            .timestamp
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.timestamp_millis() as u64)
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            });

        match entry.entry_type.as_str() {
            "user" => {
                self.process_user_message(entry.message, timestamp);
            }
            "assistant" => {
                self.process_assistant_message(entry.message, timestamp);
            }
            _ => {
                // System events, etc. - skip for now
            }
        }
    }

    fn process_user_message(&mut self, message: Option<TranscriptMessageEntry>, timestamp: u64) {
        let message = match message {
            Some(m) => m,
            None => return,
        };

        let content = extract_text_content(&message.content);
        if content.is_empty() {
            return;
        }

        let message_id = message
            .id
            .unwrap_or_else(|| format!("user-{}", uuid::Uuid::new_v4()));

        let _ = self.event_tx.send(TranscriptEvent::UserMessage {
            message_id,
            content,
            timestamp,
        });
    }

    fn process_assistant_message(
        &mut self,
        message: Option<TranscriptMessageEntry>,
        timestamp: u64,
    ) {
        let message = match message {
            Some(m) => m,
            None => return,
        };

        let message_id = message
            .id
            .unwrap_or_else(|| format!("assistant-{}", uuid::Uuid::new_v4()));

        // Check if this is a new assistant turn
        if self.current_assistant_message_id.as_ref() != Some(&message_id) {
            // End previous turn if there was one
            if let Some(prev_id) = self.current_assistant_message_id.take() {
                let _ = self.event_tx.send(TranscriptEvent::AssistantTurnEnd {
                    message_id: prev_id,
                });
            }

            // Start new turn
            self.current_assistant_message_id = Some(message_id.clone());
            let _ = self.event_tx.send(TranscriptEvent::AssistantTurnStart {
                message_id: message_id.clone(),
                timestamp,
            });
        }

        // Process content blocks
        if let Value::Array(blocks) = &message.content {
            for block in blocks {
                self.process_content_block(&message_id, block);
            }
        } else if let Value::String(text) = &message.content {
            // Simple string content
            let _ = self.event_tx.send(TranscriptEvent::Text {
                message_id: message_id.clone(),
                content: text.clone(),
            });
        }
    }

    fn process_content_block(&mut self, message_id: &str, block: &Value) {
        let block_type = match block.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return,
        };

        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    let _ = self.event_tx.send(TranscriptEvent::Text {
                        message_id: message_id.to_string(),
                        content: text.to_string(),
                    });
                }
            }
            "thinking" => {
                if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                    let _ = self.event_tx.send(TranscriptEvent::Thinking {
                        message_id: message_id.to_string(),
                        content: thinking.to_string(),
                    });
                }
            }
            "tool_use" => {
                let tool_use_id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(Value::Null);

                let _ = self.event_tx.send(TranscriptEvent::ToolUse {
                    message_id: message_id.to_string(),
                    tool_use_id,
                    name,
                    input,
                });
            }
            "tool_result" => {
                let tool_use_id = block
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = block.get("content").cloned().unwrap_or(Value::Null);
                let is_error = block
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let _ = self.event_tx.send(TranscriptEvent::ToolResult {
                    message_id: message_id.to_string(),
                    tool_use_id,
                    content,
                    is_error,
                });
            }
            _ => {
                // Unknown block type - skip
            }
        }
    }
}

/// Handle to control a running transcript watcher.
pub struct TranscriptWatcherHandle {
    stop_tx: mpsc::UnboundedSender<()>,
    _file_watcher: RecommendedWatcher,
}

impl TranscriptWatcherHandle {
    /// Stop the transcript watcher.
    pub fn stop(&self) {
        let _ = self.stop_tx.send(());
    }
}

/// Extract text content from a message content value.
fn extract_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let mut text_parts: Vec<String> = Vec::new();
            for block in blocks {
                if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                    if block_type == "text" {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }
            }
            text_parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Convert a TranscriptEvent to a ChatEvent for WebSocket broadcast.
///
/// Note: Message lifecycle (create/complete) is handled by hooks, not transcript watcher.
/// The transcript watcher only provides real-time content streaming.
/// - AssistantTurnStart: Skipped (UserPromptSubmit hook creates the message)
/// - AssistantTurnEnd: Skipped (Stop hook completes the message)
/// - UserMessage: Skipped (UserPromptSubmit hook handles this)
///
/// Content events (Thinking, Text, ToolUse, ToolResult) are emitted with Claude's
/// message ID. The caller is responsible for remapping to the internal message ID.
pub fn transcript_event_to_chat_event(
    session_id: Uuid,
    event: TranscriptEvent,
) -> Option<ChatEvent> {
    match event {
        // User messages are handled by UserPromptSubmit hook - skip to avoid duplicates
        TranscriptEvent::UserMessage { .. } => None,

        // Assistant message creation is handled by UserPromptSubmit hook - skip to avoid duplicates
        TranscriptEvent::AssistantTurnStart { .. } => None,

        // Thinking content - emit delta (message_id will be remapped by caller)
        TranscriptEvent::Thinking { message_id, content } => Some(ChatEvent::ThinkingDelta {
            session_id,
            message_id,
            delta: content,
        }),

        // Text content - emit delta (message_id will be remapped by caller)
        TranscriptEvent::Text { message_id, content } => Some(ChatEvent::ContentDelta {
            session_id,
            message_id,
            delta: content,
        }),

        // Tool use events - emit for real-time tracking (message_id will be remapped by caller)
        TranscriptEvent::ToolUse {
            message_id,
            tool_use_id,
            name,
            input,
        } => {
            let tool_call = ChatToolCall::new(tool_use_id, name, input);
            Some(ChatEvent::ToolCallStart {
                session_id,
                message_id,
                tool_call,
            })
        }

        // Tool result events - emit for real-time tracking (message_id will be remapped by caller)
        TranscriptEvent::ToolResult {
            message_id,
            tool_use_id,
            content,
            is_error,
        } => {
            // Convert content to string
            let output = match content {
                Value::String(s) => s,
                other => serde_json::to_string(&other).unwrap_or_default(),
            };
            Some(ChatEvent::ToolCallComplete {
                session_id,
                message_id,
                tool_call_id: tool_use_id,
                output,
                is_error,
            })
        }

        // Message completion is handled by Stop hook - skip to avoid duplicates
        TranscriptEvent::AssistantTurnEnd { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_extract_text_content_string() {
        let content = serde_json::json!("Hello world");
        assert_eq!(extract_text_content(&content), "Hello world");
    }

    #[test]
    fn test_extract_text_content_array() {
        let content = serde_json::json!([
            {"type": "text", "text": "First"},
            {"type": "tool_use", "name": "Read"},
            {"type": "text", "text": "Second"}
        ]);
        assert_eq!(extract_text_content(&content), "First\nSecond");
    }

    #[test]
    fn test_transcript_event_to_chat_event() {
        let session_id = Uuid::new_v4();

        // User messages are now skipped (handled by hooks)
        let event = TranscriptEvent::UserMessage {
            message_id: "user-123".to_string(),
            content: "Hello".to_string(),
            timestamp: 1234567890,
        };
        assert!(transcript_event_to_chat_event(session_id, event).is_none());

        // Assistant turn start is skipped (handled by hooks)
        let event = TranscriptEvent::AssistantTurnStart {
            message_id: "msg-123".to_string(),
            timestamp: 1234567890,
        };
        assert!(transcript_event_to_chat_event(session_id, event).is_none());

        // Assistant turn end is skipped (handled by hooks)
        let event = TranscriptEvent::AssistantTurnEnd {
            message_id: "msg-123".to_string(),
        };
        assert!(transcript_event_to_chat_event(session_id, event).is_none());

        // Test thinking delta conversion (still emitted)
        let event = TranscriptEvent::Thinking {
            message_id: "msg-123".to_string(),
            content: "Let me think...".to_string(),
        };
        let chat_event = transcript_event_to_chat_event(session_id, event).unwrap();
        assert!(matches!(chat_event, ChatEvent::ThinkingDelta { .. }));

        // Test text delta conversion (still emitted)
        let event = TranscriptEvent::Text {
            message_id: "msg-123".to_string(),
            content: "Here is my response".to_string(),
        };
        let chat_event = transcript_event_to_chat_event(session_id, event).unwrap();
        assert!(matches!(chat_event, ChatEvent::ContentDelta { .. }));

        // Test tool use conversion (still emitted)
        let event = TranscriptEvent::ToolUse {
            message_id: "msg-123".to_string(),
            tool_use_id: "tool_abc".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({"path": "/test.txt"}),
        };
        let chat_event = transcript_event_to_chat_event(session_id, event).unwrap();
        assert!(matches!(chat_event, ChatEvent::ToolCallStart { .. }));
    }
}
