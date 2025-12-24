//! Chat message extraction from terminal sessions.
//!
//! This module provides real-time extraction of structured chat messages
//! from terminal PTY output and hook events. It powers the chat view which
//! displays Claude's responses as message bubbles.
//!
//! Architecture:
//! - Terminal output is parsed to extract Claude's prose text
//! - Hook events provide structured data (prompts, tool calls)
//! - State machine tracks conversation flow
//! - Messages are broadcast via ProcessEvent for WebSocket delivery

use clauset_types::{ChatEvent, ChatMessage, ChatRole, ChatToolCall, HookEvent};
use tracing::info;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// State machine for tracking message building.
#[derive(Debug, Clone, PartialEq)]
enum ProcessorState {
    /// Waiting for user input
    Idle,
    /// User submitted prompt, waiting for Claude's response
    WaitingForResponse,
    /// Building Claude's response (accumulating text)
    BuildingResponse,
    /// Tool is currently executing
    ToolInProgress { tool_id: String },
}

/// Per-session chat processor state.
#[derive(Debug)]
struct SessionChatState {
    /// Current state machine state
    state: ProcessorState,
    /// Current assistant message being built (if any)
    current_message: Option<ChatMessage>,
    /// All messages for this session
    messages: Vec<ChatMessage>,
    /// Text buffer for accumulating Claude's response
    text_buffer: String,
    /// Last position in terminal output that was processed
    last_processed_offset: usize,
    /// Whether we're inside a tool output block
    in_tool_output: bool,
    /// Current tool output being captured
    current_tool_output: String,
}

impl SessionChatState {
    fn new() -> Self {
        Self {
            state: ProcessorState::Idle,
            current_message: None,
            messages: Vec::new(),
            text_buffer: String::new(),
            last_processed_offset: 0,
            in_tool_output: false,
            current_tool_output: String::new(),
        }
    }
}

/// Manages chat message extraction for all sessions.
pub struct ChatProcessor {
    sessions: Arc<RwLock<HashMap<Uuid, SessionChatState>>>,
}

impl Default for ChatProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatProcessor {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process a hook event and generate chat events.
    ///
    /// Returns a list of ChatEvents to broadcast.
    pub async fn process_hook_event(&self, event: &HookEvent) -> Vec<ChatEvent> {
        let mut events = Vec::new();
        info!(target: "clauset::chat", "Processing hook event: {:?}", std::mem::discriminant(event));

        match event {
            HookEvent::UserPromptSubmit {
                session_id, prompt, ..
            } => {
                let mut sessions = self.sessions.write().await;
                let state = sessions.entry(*session_id).or_insert_with(SessionChatState::new);

                // Finalize any in-progress assistant message
                if let Some(mut msg) = state.current_message.take() {
                    msg.complete();
                    events.push(ChatEvent::MessageComplete {
                        session_id: *session_id,
                        message_id: msg.id.clone(),
                    });
                    state.messages.push(msg);
                }

                // Create user message
                let user_msg = ChatMessage::user(*session_id, prompt.clone());
                events.push(ChatEvent::Message {
                    session_id: *session_id,
                    message: user_msg.clone(),
                });
                state.messages.push(user_msg);

                // Start building assistant message
                let assistant_msg = ChatMessage::assistant(*session_id);
                events.push(ChatEvent::Message {
                    session_id: *session_id,
                    message: assistant_msg.clone(),
                });
                state.current_message = Some(assistant_msg);

                // Transition state
                state.state = ProcessorState::WaitingForResponse;
                state.text_buffer.clear();
            }

            HookEvent::PreToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_use_id,
                ..
            } => {
                let mut sessions = self.sessions.write().await;
                let state = sessions.entry(*session_id).or_insert_with(SessionChatState::new);

                // Ensure we have an assistant message
                if state.current_message.is_none() {
                    let assistant_msg = ChatMessage::assistant(*session_id);
                    events.push(ChatEvent::Message {
                        session_id: *session_id,
                        message: assistant_msg.clone(),
                    });
                    state.current_message = Some(assistant_msg);
                }

                // Add tool call to current message
                let tool_call = ChatToolCall::new(
                    tool_use_id.clone(),
                    tool_name.clone(),
                    tool_input.clone(),
                );

                if let Some(msg) = &mut state.current_message {
                    msg.add_tool_call(tool_call.clone());
                }

                events.push(ChatEvent::ToolCallStart {
                    session_id: *session_id,
                    message_id: state.current_message.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
                    tool_call,
                });

                state.state = ProcessorState::ToolInProgress {
                    tool_id: tool_use_id.clone(),
                };
                state.in_tool_output = true;
                state.current_tool_output.clear();
            }

            HookEvent::PostToolUse {
                session_id,
                tool_use_id,
                tool_response,
                ..
            } => {
                let mut sessions = self.sessions.write().await;
                let state = sessions.entry(*session_id).or_insert_with(SessionChatState::new);

                // Extract output from tool response
                let output = extract_tool_output(tool_response);
                let is_error = tool_response.get("error").is_some()
                    || tool_response
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                // Update tool call in current message
                if let Some(msg) = &mut state.current_message {
                    for tc in &mut msg.tool_calls {
                        if tc.id == *tool_use_id {
                            tc.complete_with_output(output.clone(), is_error);
                        }
                    }
                }

                events.push(ChatEvent::ToolCallComplete {
                    session_id: *session_id,
                    message_id: state.current_message.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
                    tool_call_id: tool_use_id.clone(),
                    output,
                    is_error,
                });

                state.state = ProcessorState::BuildingResponse;
                state.in_tool_output = false;
                state.current_tool_output.clear();
            }

            HookEvent::Stop { session_id, stop_hook_active, transcript_path, .. } => {
                info!(target: "clauset::chat", "Stop hook: stop_hook_active={}, transcript_path={:?}", stop_hook_active, transcript_path);

                if *stop_hook_active {
                    // Stop hook is chaining, wait for final stop
                    info!(target: "clauset::chat", "Stop hook is active, waiting for final stop");
                    return events;
                }

                let mut sessions = self.sessions.write().await;
                let state = sessions.entry(*session_id).or_insert_with(SessionChatState::new);

                info!(target: "clauset::chat", "Current message exists: {}", state.current_message.is_some());

                // Read Claude's response from transcript file
                if let Some(path) = transcript_path {
                    info!(target: "clauset::chat", "Reading transcript from: {}", path);
                    match read_last_assistant_response(path) {
                        Ok(response_text) => {
                            info!(target: "clauset::chat", "Transcript read: {} chars", response_text.len());
                            if !response_text.is_empty() {
                                if let Some(msg) = &mut state.current_message {
                                    info!(target: "clauset::chat", "Current message content: {} chars", msg.content.len());
                                    // Only add if message is currently empty (no streamed content)
                                    if msg.content.is_empty() {
                                        msg.append_content(&response_text);
                                        events.push(ChatEvent::ContentDelta {
                                            session_id: *session_id,
                                            message_id: msg.id.clone(),
                                            delta: response_text,
                                        });
                                        info!(target: "clauset::chat", "Added ContentDelta event");
                                    } else {
                                        info!(target: "clauset::chat", "Message already has content, skipping");
                                    }
                                } else {
                                    info!(target: "clauset::chat", "No current message to update");
                                }
                            } else {
                                info!(target: "clauset::chat", "Transcript response was empty");
                            }
                        }
                        Err(e) => {
                            info!(target: "clauset::chat", "Failed to read transcript: {}", e);
                        }
                    }
                } else {
                    info!(target: "clauset::chat", "No transcript_path provided");
                }

                // Finalize current assistant message
                if let Some(mut msg) = state.current_message.take() {
                    msg.complete();
                    events.push(ChatEvent::MessageComplete {
                        session_id: *session_id,
                        message_id: msg.id.clone(),
                    });
                    state.messages.push(msg);
                }

                state.state = ProcessorState::Idle;
            }

            _ => {}
        }

        info!(target: "clauset::chat", "Generated {} chat events", events.len());
        events
    }

    /// Process terminal output and extract Claude's text.
    ///
    /// Returns content delta if new text was extracted.
    pub async fn process_terminal_output(
        &self,
        session_id: Uuid,
        data: &[u8],
    ) -> Option<ChatEvent> {
        let text = String::from_utf8_lossy(data);
        let clean_text = strip_ansi_codes(&text);

        let mut sessions = self.sessions.write().await;
        let state = sessions.entry(session_id).or_insert_with(SessionChatState::new);

        // Only process if we're building a response
        match &state.state {
            ProcessorState::WaitingForResponse | ProcessorState::BuildingResponse => {
                // Skip if inside tool output
                if state.in_tool_output {
                    return None;
                }

                // Extract meaningful text (filter out status lines, prompts, etc.)
                let extracted = extract_claude_text(&clean_text);
                if extracted.is_empty() {
                    return None;
                }

                // Update state
                state.state = ProcessorState::BuildingResponse;

                // Append to text buffer
                state.text_buffer.push_str(&extracted);

                // Update current message
                if let Some(msg) = &mut state.current_message {
                    msg.append_content(&extracted);

                    return Some(ChatEvent::ContentDelta {
                        session_id,
                        message_id: msg.id.clone(),
                        delta: extracted,
                    });
                }
            }
            _ => {}
        }

        None
    }

    /// Get all messages for a session.
    pub async fn get_messages(&self, session_id: Uuid) -> Vec<ChatMessage> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session_id)
            .map(|s| {
                let mut msgs = s.messages.clone();
                if let Some(current) = &s.current_message {
                    msgs.push(current.clone());
                }
                msgs
            })
            .unwrap_or_default()
    }

    /// Clear messages for a session.
    pub async fn clear_session(&self, session_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
    }
}

/// Comprehensive regex for ANSI escape sequences.
static ANSI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\x1b\[[0-9;?]*[A-Za-z]",    // CSI sequences
        r"|\x1b\][^\x07]*\x07",        // OSC sequences ending with BEL
        r"|\x1b\][^\x1b]*\x1b\\",      // OSC sequences ending with ST
        r"|\x1b[()][A-Z0-9]",          // Character set selection
        r"|\x1b[=>MNOP78]",            // Other single-char escapes
        r"|\x1b",                       // Catch any remaining bare ESC
    )).unwrap()
});

/// Strip ANSI escape codes from text.
fn strip_ansi_codes(text: &str) -> String {
    ANSI_REGEX.replace_all(text, "").to_string()
}

/// Regex patterns for filtering non-content lines.
static STATUS_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z][A-Za-z0-9.\- ]*\s*\|\s*\$[0-9.]+").unwrap()
});

static PROMPT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[>\$❯]\s*$").unwrap()
});

static THINKING_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[●•⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏\*]\s*(Thinking|Actualizing|Planning|Mustering)").unwrap()
});

static TOOL_HEADER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[●•\s]*(Read|Edit|Write|Bash|Grep|Glob|Task|Search|WebFetch|WebSearch)\s*[\(:]").unwrap()
});

/// Extract Claude's meaningful text from terminal output.
///
/// Filters out:
/// - Status lines (model | cost | tokens)
/// - Prompts (> or $)
/// - Thinking indicators
/// - Tool headers
/// - Empty lines
fn extract_claude_text(text: &str) -> String {
    let mut result = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip status lines
        if STATUS_LINE_REGEX.is_match(trimmed) {
            continue;
        }

        // Skip prompts
        if PROMPT_REGEX.is_match(trimmed) {
            continue;
        }

        // Skip thinking indicators
        if THINKING_REGEX.is_match(trimmed) {
            continue;
        }

        // Skip tool headers
        if TOOL_HEADER_REGEX.is_match(trimmed) {
            continue;
        }

        // Skip very short lines that look like UI chrome
        if trimmed.len() < 3 {
            continue;
        }

        // Skip lines that are mostly box-drawing characters
        let box_chars: usize = trimmed
            .chars()
            .filter(|c| "─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋═║╔╗╚╝╠╣╦╩╬▀▄█▌▐░▒▓".contains(*c))
            .count();
        if box_chars > trimmed.len() / 2 {
            continue;
        }

        // Add the line
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(trimmed);
    }

    result
}

/// Extract a readable output string from tool response JSON.
fn extract_tool_output(response: &serde_json::Value) -> String {
    // Try common output field names
    if let Some(output) = response.get("output").and_then(|v| v.as_str()) {
        return truncate_output(output);
    }

    if let Some(content) = response.get("content").and_then(|v| v.as_str()) {
        return truncate_output(content);
    }

    if let Some(result) = response.get("result").and_then(|v| v.as_str()) {
        return truncate_output(result);
    }

    // For arrays (like search results), summarize
    if let Some(arr) = response.as_array() {
        return format!("[{} results]", arr.len());
    }

    // Fallback to JSON stringification (truncated)
    let json_str = response.to_string();
    truncate_output(&json_str)
}

/// Truncate output to a reasonable length for display.
fn truncate_output(s: &str) -> String {
    const MAX_LEN: usize = 500;
    if s.len() <= MAX_LEN {
        s.to_string()
    } else {
        format!("{}...", &s[..MAX_LEN - 3])
    }
}

/// Read the last assistant response from a Claude Code transcript file.
///
/// The transcript is a JSONL file where each line is a conversation message.
/// We read backwards to find the most recent assistant turn with text content.
///
/// Claude Code transcript format:
/// ```json
/// {"type":"assistant", "message":{"role":"assistant", "content":[{"type":"text", "text":"..."}]}}
/// ```
fn read_last_assistant_response(path: &str) -> std::io::Result<String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Collect all lines and process from the end
    let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;

    // Find the last assistant message with text content by reading backwards
    for line in lines.iter().rev() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse the JSONL line
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            // Check if this is an assistant message (outer type field)
            let entry_type = entry.get("type").and_then(|v| v.as_str());
            if entry_type != Some("assistant") {
                continue;
            }

            // Get the nested message object
            let message = match entry.get("message") {
                Some(m) => m,
                None => continue,
            };

            // Extract text content from message.content array
            if let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                let mut text_parts = Vec::new();

                for part in content {
                    // Handle text content blocks (skip thinking blocks)
                    if let Some("text") = part.get("type").and_then(|v| v.as_str()) {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }

                if !text_parts.is_empty() {
                    return Ok(text_parts.join("\n"));
                }
            }

            // Also handle simple string content format
            if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                return Ok(content.to_string());
            }
        }
    }

    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[32mHello\x1b[0m World";
        assert_eq!(strip_ansi_codes(input), "Hello World");
    }

    #[test]
    fn test_extract_claude_text() {
        let input = r#"
Opus 4.5 | $0.50 | 10K/5K | ctx:15%
● Thinking... (2s elapsed)
Here is my analysis of the code:

The function works correctly.

>
"#;
        let extracted = extract_claude_text(input);
        assert!(extracted.contains("Here is my analysis"));
        assert!(extracted.contains("The function works correctly"));
        assert!(!extracted.contains("Opus"));
        assert!(!extracted.contains("Thinking"));
    }

    #[test]
    fn test_extract_tool_output() {
        let response = serde_json::json!({
            "output": "File contents here"
        });
        assert_eq!(extract_tool_output(&response), "File contents here");

        let response2 = serde_json::json!([1, 2, 3]);
        assert_eq!(extract_tool_output(&response2), "[3 results]");
    }

    #[tokio::test]
    async fn test_processor_user_prompt() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        let event = HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello Claude".to_string(),
        };

        let events = processor.process_hook_event(&event).await;

        // Should emit: user message, assistant message start
        assert_eq!(events.len(), 2);

        let messages = processor.get_messages(session_id).await;
        assert_eq!(messages.len(), 2); // User + streaming assistant
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content, "Hello Claude");
    }
}
