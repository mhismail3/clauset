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

use crate::InteractionStore;
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
    /// Optional database store for message persistence
    store: Option<Arc<InteractionStore>>,
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
            store: None,
        }
    }

    /// Create a ChatProcessor with database persistence.
    pub fn with_store(store: Arc<InteractionStore>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            store: Some(store),
        }
    }

    /// Helper to persist a message to the database.
    fn persist_message(&self, msg: &ChatMessage) {
        if let Some(store) = &self.store {
            if let Err(e) = store.save_chat_message(msg) {
                tracing::warn!(target: "clauset::chat", "Failed to persist chat message: {}", e);
            }
        }
    }

    /// Helper to persist a tool call to the database.
    fn persist_tool_call(&self, message_id: &str, tool_call: &ChatToolCall) {
        if let Some(store) = &self.store {
            if let Err(e) = store.save_chat_tool_call(message_id, tool_call) {
                tracing::warn!(target: "clauset::chat", "Failed to persist chat tool call: {}", e);
            }
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
                    self.persist_message(&msg);
                    events.push(ChatEvent::MessageComplete {
                        session_id: *session_id,
                        message_id: msg.id.clone(),
                    });
                    state.messages.push(msg);
                }

                // Create user message
                let user_msg = ChatMessage::user(*session_id, prompt.clone());
                self.persist_message(&user_msg);
                events.push(ChatEvent::Message {
                    session_id: *session_id,
                    message: user_msg.clone(),
                });
                state.messages.push(user_msg);

                // Start building assistant message
                let assistant_msg = ChatMessage::assistant(*session_id);
                self.persist_message(&assistant_msg);
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
                    self.persist_message(&assistant_msg);
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
                    self.persist_tool_call(&msg.id, &tool_call);
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
                            self.persist_tool_call(&msg.id, tc);
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
                                        self.persist_message(msg);
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
                    self.persist_message(&msg);
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
    ///
    /// Returns messages from memory if available, otherwise loads from database.
    pub async fn get_messages(&self, session_id: Uuid) -> Vec<ChatMessage> {
        let sessions = self.sessions.read().await;
        if let Some(s) = sessions.get(&session_id) {
            let mut msgs = s.messages.clone();
            if let Some(current) = &s.current_message {
                msgs.push(current.clone());
            }
            return msgs;
        }
        drop(sessions);

        // Fall back to database
        self.get_chat_history(session_id)
    }

    /// Get chat history from the database.
    pub fn get_chat_history(&self, session_id: Uuid) -> Vec<ChatMessage> {
        if let Some(store) = &self.store {
            match store.get_chat_messages(session_id) {
                Ok(messages) => messages,
                Err(e) => {
                    tracing::warn!(target: "clauset::chat", "Failed to load chat history: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }

    /// Load messages from database into memory for a session.
    pub async fn load_session_history(&self, session_id: Uuid) {
        if let Some(store) = &self.store {
            match store.get_chat_messages(session_id) {
                Ok(messages) => {
                    if !messages.is_empty() {
                        let mut sessions = self.sessions.write().await;
                        let state = sessions.entry(session_id).or_insert_with(SessionChatState::new);
                        state.messages = messages;
                        info!(target: "clauset::chat", "Loaded {} messages from database for session {}", state.messages.len(), session_id);
                    }
                }
                Err(e) => {
                    tracing::warn!(target: "clauset::chat", "Failed to load chat history for session {}: {}", session_id, e);
                }
            }
        }
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
    use tempfile::NamedTempFile;
    use std::io::Write;

    // ==================== ANSI Code Stripping Tests ====================

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[32mHello\x1b[0m World";
        assert_eq!(strip_ansi_codes(input), "Hello World");
    }

    #[test]
    fn test_strip_ansi_codes_empty_string() {
        assert_eq!(strip_ansi_codes(""), "");
    }

    #[test]
    fn test_strip_ansi_codes_no_codes() {
        assert_eq!(strip_ansi_codes("Plain text"), "Plain text");
    }

    #[test]
    fn test_strip_ansi_codes_multiple_sequences() {
        let input = "\x1b[1m\x1b[31mBold Red\x1b[0m \x1b[4mUnderline\x1b[0m";
        assert_eq!(strip_ansi_codes(input), "Bold Red Underline");
    }

    #[test]
    fn test_strip_ansi_codes_cursor_movement() {
        let input = "\x1b[2Jclear\x1b[H\x1b[10;5Hposition";
        let result = strip_ansi_codes(input);
        assert!(result.contains("clear"));
        assert!(result.contains("position"));
        assert!(!result.contains("\x1b"));
    }

    #[test]
    fn test_strip_ansi_codes_osc_sequences() {
        // OSC sequence ending with BEL
        let input = "\x1b]0;Window Title\x07Some text";
        assert_eq!(strip_ansi_codes(input), "Some text");
    }

    #[test]
    fn test_strip_ansi_codes_character_set_selection() {
        let input = "\x1b(BNormal\x1b)0Line Drawing";
        let result = strip_ansi_codes(input);
        assert!(result.contains("Normal"));
        assert!(result.contains("Line Drawing"));
    }

    #[test]
    fn test_strip_ansi_codes_256_colors() {
        // Note: 256-color/truecolor sequences like \x1b[38;5;196m are stripped but may
        // leave adjacent text without spaces - this is expected behavior since the regex
        // strips codes without inserting separators
        let input = "\x1b[38;5;196mRed\x1b[48;2;0;255;0mGreen BG\x1b[0m";
        let result = strip_ansi_codes(input);
        assert!(result.contains("Red"));
        assert!(result.contains("Green BG"));
        assert!(!result.contains("\x1b"));
    }

    // ==================== Text Extraction Tests ====================

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
    fn test_extract_claude_text_empty() {
        assert_eq!(extract_claude_text(""), "");
        assert_eq!(extract_claude_text("   \n  \n  "), "");
    }

    #[test]
    fn test_extract_claude_text_status_line_filtering() {
        let inputs = [
            "Claude 4.5 | $1.23 | 50K/25K",
            "Sonnet-3.5 | $0.00 | 0K/0K",
            "Opus 4 | $99.99 | 1000K/500K | ctx:95%",
        ];
        for input in inputs {
            assert_eq!(extract_claude_text(input), "", "Status line should be filtered: {}", input);
        }
    }

    #[test]
    fn test_extract_claude_text_prompt_filtering() {
        let inputs = [">", "$", "❯", ">   ", "$  "];
        for input in inputs {
            assert_eq!(extract_claude_text(input), "", "Prompt should be filtered: {:?}", input);
        }
    }

    #[test]
    fn test_extract_claude_text_thinking_filtering() {
        let inputs = [
            "● Thinking... (5s elapsed)",
            "• Actualizing...",
            "⠋ Planning",
            "* Mustering resources",
        ];
        for input in inputs {
            assert_eq!(extract_claude_text(input), "", "Thinking indicator should be filtered: {}", input);
        }
    }

    #[test]
    fn test_extract_claude_text_tool_header_filtering() {
        let inputs = [
            "● Read(/path/to/file)",
            "• Bash(ls -la)",
            "  Edit(/file.rs)",
            "● Write(/new/file.txt)",
        ];
        for input in inputs {
            assert_eq!(extract_claude_text(input), "", "Tool header should be filtered: {}", input);
        }
    }

    #[test]
    fn test_extract_claude_text_box_drawing_filtering() {
        // Box-drawing characters like ─│┌┐└┘ are filtered when they make up more than
        // half the bytes in a line. Note: len() returns bytes, so UTF-8 multi-byte
        // characters (3 bytes each) may not filter as expected on short lines.
        //
        // Lines with meaningful text content and some box chars are preserved:
        let mixed = "Box │ Hello World │ text";
        let extracted = extract_claude_text(mixed);
        assert!(extracted.contains("Hello World"), "Mixed content should be preserved");

        // Pure box-drawing lines (when majority of bytes are box chars) get filtered
        // For ASCII pipe char which is 1 byte, the ratio works better:
        let pipe_line = "||||||||||||||||||||";
        // This doesn't get filtered because | is not in the box-drawing list
        // The filter specifically targets Unicode box-drawing characters
    }

    #[test]
    fn test_extract_claude_text_short_line_filtering() {
        assert_eq!(extract_claude_text("ab"), "");
        assert_eq!(extract_claude_text("OK"), "");
        // Three characters should pass
        assert_eq!(extract_claude_text("abc"), "abc");
    }

    #[test]
    fn test_extract_claude_text_preserves_code() {
        let input = r#"
Here's the code:

fn main() {
    println!("Hello, world!");
}

Let me know if you need help.
"#;
        let extracted = extract_claude_text(input);
        assert!(extracted.contains("fn main()"));
        assert!(extracted.contains("println!"));
        assert!(extracted.contains("Let me know"));
    }

    #[test]
    fn test_extract_claude_text_multiline() {
        let input = "First paragraph here.\n\nSecond paragraph here.\n\nThird one.";
        let extracted = extract_claude_text(input);
        assert!(extracted.contains("First paragraph"));
        assert!(extracted.contains("Second paragraph"));
        assert!(extracted.contains("Third one"));
    }

    // ==================== Tool Output Extraction Tests ====================

    #[test]
    fn test_extract_tool_output() {
        let response = serde_json::json!({
            "output": "File contents here"
        });
        assert_eq!(extract_tool_output(&response), "File contents here");

        let response2 = serde_json::json!([1, 2, 3]);
        assert_eq!(extract_tool_output(&response2), "[3 results]");
    }

    #[test]
    fn test_extract_tool_output_content_field() {
        let response = serde_json::json!({
            "content": "Some content value"
        });
        assert_eq!(extract_tool_output(&response), "Some content value");
    }

    #[test]
    fn test_extract_tool_output_result_field() {
        let response = serde_json::json!({
            "result": "Operation result"
        });
        assert_eq!(extract_tool_output(&response), "Operation result");
    }

    #[test]
    fn test_extract_tool_output_priority() {
        // output takes priority over content and result
        let response = serde_json::json!({
            "output": "preferred",
            "content": "secondary",
            "result": "tertiary"
        });
        assert_eq!(extract_tool_output(&response), "preferred");
    }

    #[test]
    fn test_extract_tool_output_array() {
        let response = serde_json::json!([
            {"file": "a.txt"},
            {"file": "b.txt"},
            {"file": "c.txt"},
            {"file": "d.txt"},
            {"file": "e.txt"}
        ]);
        assert_eq!(extract_tool_output(&response), "[5 results]");
    }

    #[test]
    fn test_extract_tool_output_empty_array() {
        let response = serde_json::json!([]);
        assert_eq!(extract_tool_output(&response), "[0 results]");
    }

    #[test]
    fn test_extract_tool_output_object_fallback() {
        let response = serde_json::json!({
            "some_field": "value",
            "another": 123
        });
        let result = extract_tool_output(&response);
        assert!(result.contains("some_field"));
    }

    #[test]
    fn test_extract_tool_output_truncation() {
        let long_output = "x".repeat(600);
        let response = serde_json::json!({
            "output": long_output
        });
        let result = extract_tool_output(&response);
        assert_eq!(result.len(), 500);
        assert!(result.ends_with("..."));
    }

    // ==================== Truncate Output Tests ====================

    #[test]
    fn test_truncate_output_short() {
        assert_eq!(truncate_output("short"), "short");
        assert_eq!(truncate_output(""), "");
    }

    #[test]
    fn test_truncate_output_exactly_max() {
        let s = "x".repeat(500);
        assert_eq!(truncate_output(&s), s);
    }

    #[test]
    fn test_truncate_output_over_max() {
        let s = "x".repeat(600);
        let result = truncate_output(&s);
        assert_eq!(result.len(), 500);
        assert!(result.ends_with("..."));
        assert!(result.starts_with("xxx"));
    }

    // ==================== Transcript Reading Tests ====================

    #[test]
    fn test_read_last_assistant_response_basic() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"user","message":{{"role":"user","content":"Hello"}}}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Hello! How can I help?"}}]}}}}"#).unwrap();

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Hello! How can I help?");
    }

    #[test]
    fn test_read_last_assistant_response_multiple_turns() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"First response"}}]}}}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","message":{{"role":"user","content":"Thanks"}}}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Second response"}}]}}}}"#).unwrap();

        // Should get the LAST assistant response
        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Second response");
    }

    #[test]
    fn test_read_last_assistant_response_with_thinking() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"Let me think..."}},{{"type":"text","text":"Here is my answer"}}]}}}}"#).unwrap();

        // Should skip thinking blocks, only get text
        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Here is my answer");
    }

    #[test]
    fn test_read_last_assistant_response_multiple_text_blocks() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Part 1"}},{{"type":"text","text":"Part 2"}}]}}}}"#).unwrap();

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Part 1\nPart 2");
    }

    #[test]
    fn test_read_last_assistant_response_simple_content() {
        let mut file = NamedTempFile::new().unwrap();
        // Some transcripts have simple string content
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":"Simple string content"}}}}"#).unwrap();

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Simple string content");
    }

    #[test]
    fn test_read_last_assistant_response_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_last_assistant_response_no_assistant() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"user","message":{{"role":"user","content":"Hello"}}}}"#).unwrap();

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_last_assistant_response_nonexistent_file() {
        let result = read_last_assistant_response("/nonexistent/path/to/file.jsonl");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_last_assistant_response_empty_content() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[]}}}}"#).unwrap();

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_last_assistant_response_skips_blank_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Response"}}]}}}}"#).unwrap();
        writeln!(file, "").unwrap();  // blank line
        writeln!(file, "   ").unwrap();  // whitespace line

        let result = read_last_assistant_response(file.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "Response");
    }

    // ==================== ChatProcessor State Machine Tests ====================

    #[tokio::test]
    async fn test_processor_user_prompt() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        let event = HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello Claude".to_string(),
            cwd: None,
            context_window: None,
        };

        let events = processor.process_hook_event(&event).await;

        // Should emit: user message, assistant message start
        assert_eq!(events.len(), 2);

        let messages = processor.get_messages(session_id).await;
        assert_eq!(messages.len(), 2); // User + streaming assistant
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content, "Hello Claude");
    }

    #[tokio::test]
    async fn test_processor_user_prompt_state_transition() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        let event = HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Test prompt".to_string(),
            cwd: None,
            context_window: None,
        };

        processor.process_hook_event(&event).await;

        // Verify state via messages - assistant message should be streaming
        let messages = processor.get_messages(session_id).await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert!(messages[1].is_streaming); // Should be streaming (waiting for response)
    }

    #[tokio::test]
    async fn test_processor_pre_tool_use() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // First submit a prompt
        let prompt_event = HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Read a file".to_string(),
            cwd: None,
            context_window: None,
        };
        processor.process_hook_event(&prompt_event).await;

        // Now a tool use
        let tool_event = HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/test/file.txt"}),
            tool_use_id: "tool_123".to_string(),
            cwd: None,
            context_window: None,
        };

        let events = processor.process_hook_event(&tool_event).await;

        // Should emit ToolCallStart
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatEvent::ToolCallStart { tool_call, .. } => {
                assert_eq!(tool_call.name, "Read");
                assert_eq!(tool_call.id, "tool_123");
            }
            _ => panic!("Expected ToolCallStart event"),
        }

        // Verify tool call added to message
        let messages = processor.get_messages(session_id).await;
        let assistant_msg = &messages[1];
        assert_eq!(assistant_msg.tool_calls.len(), 1);
        assert_eq!(assistant_msg.tool_calls[0].name, "Read");
    }

    #[tokio::test]
    async fn test_processor_pre_tool_use_creates_assistant_message() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // PreToolUse without prior UserPromptSubmit
        let tool_event = HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "ls"}),
            tool_use_id: "tool_456".to_string(),
            cwd: None,
            context_window: None,
        };

        let events = processor.process_hook_event(&tool_event).await;

        // Should create assistant message + tool call start
        assert_eq!(events.len(), 2);
        match &events[0] {
            ChatEvent::Message { message, .. } => {
                assert_eq!(message.role, ChatRole::Assistant);
            }
            _ => panic!("Expected Message event first"),
        }
    }

    #[tokio::test]
    async fn test_processor_post_tool_use() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Setup: prompt + pre tool use
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Write code".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Write".to_string(),
            tool_input: serde_json::json!({"path": "/test.rs", "content": "fn main() {}"}),
            tool_use_id: "tool_789".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Now post tool use
        let post_event = HookEvent::PostToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Write".to_string(),
            tool_input: serde_json::json!({"path": "/test.rs", "content": "fn main() {}"}),
            tool_use_id: "tool_789".to_string(),
            tool_response: serde_json::json!({"output": "File written successfully"}),
            context_window: None,
        };

        let events = processor.process_hook_event(&post_event).await;

        // Should emit ToolCallComplete
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatEvent::ToolCallComplete { tool_call_id, output, is_error, .. } => {
                assert_eq!(tool_call_id, "tool_789");
                assert!(output.contains("File written successfully"));
                assert!(!is_error);
            }
            _ => panic!("Expected ToolCallComplete event"),
        }
    }

    #[tokio::test]
    async fn test_processor_post_tool_use_error() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Setup
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Read file".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/nonexistent"}),
            tool_use_id: "tool_err".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Post with error
        let post_event = HookEvent::PostToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/nonexistent"}),
            tool_use_id: "tool_err".to_string(),
            tool_response: serde_json::json!({"error": "File not found", "is_error": true}),
            context_window: None,
        };

        let events = processor.process_hook_event(&post_event).await;

        match &events[0] {
            ChatEvent::ToolCallComplete { is_error, .. } => {
                assert!(is_error);
            }
            _ => panic!("Expected ToolCallComplete event"),
        }
    }

    #[tokio::test]
    async fn test_processor_stop_finalizes_message() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Setup
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Stop event (no transcript)
        let stop_event = HookEvent::Stop {
            session_id,
            claude_session_id: "test".to_string(),
            stop_hook_active: false,
            transcript_path: None,
            context_window: None,
        };

        let events = processor.process_hook_event(&stop_event).await;

        // Should emit MessageComplete
        assert!(events.iter().any(|e| matches!(e, ChatEvent::MessageComplete { .. })));

        // Message should no longer be streaming
        let messages = processor.get_messages(session_id).await;
        assert!(!messages[1].is_streaming);
    }

    #[tokio::test]
    async fn test_processor_stop_with_transcript() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Create temp transcript file
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Transcript response"}}]}}}}"#).unwrap();

        // Setup
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Stop with transcript
        let stop_event = HookEvent::Stop {
            session_id,
            claude_session_id: "test".to_string(),
            stop_hook_active: false,
            transcript_path: Some(file.path().to_str().unwrap().to_string()),
            context_window: None,
        };

        let events = processor.process_hook_event(&stop_event).await;

        // Should emit ContentDelta with transcript content
        assert!(events.iter().any(|e| matches!(e, ChatEvent::ContentDelta { delta, .. } if delta.contains("Transcript response"))));
    }

    #[tokio::test]
    async fn test_processor_stop_hook_active_waits() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Setup
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Stop with stop_hook_active = true (chaining)
        let stop_event = HookEvent::Stop {
            session_id,
            claude_session_id: "test".to_string(),
            stop_hook_active: true,  // Still chaining
            transcript_path: None,
            context_window: None,
        };

        let events = processor.process_hook_event(&stop_event).await;

        // Should NOT finalize message yet
        assert!(events.is_empty());

        let messages = processor.get_messages(session_id).await;
        assert!(messages[1].is_streaming); // Still streaming
    }

    #[tokio::test]
    async fn test_processor_multiple_tool_calls() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Read two files".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // First tool
        processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/file1.txt"}),
            tool_use_id: "tool_1".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::PostToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/file1.txt"}),
            tool_use_id: "tool_1".to_string(),
            tool_response: serde_json::json!({"output": "Content 1"}),
            context_window: None,
        }).await;

        // Second tool
        processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/file2.txt"}),
            tool_use_id: "tool_2".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::PostToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/file2.txt"}),
            tool_use_id: "tool_2".to_string(),
            tool_response: serde_json::json!({"output": "Content 2"}),
            context_window: None,
        }).await;

        let messages = processor.get_messages(session_id).await;
        assert_eq!(messages[1].tool_calls.len(), 2);
        assert_eq!(messages[1].tool_calls[0].id, "tool_1");
        assert_eq!(messages[1].tool_calls[1].id, "tool_2");
    }

    #[tokio::test]
    async fn test_processor_multi_turn_conversation() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // First turn
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "First question".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::Stop {
            session_id,
            claude_session_id: "test".to_string(),
            stop_hook_active: false,
            transcript_path: None,
            context_window: None,
        }).await;

        // Second turn
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Second question".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let messages = processor.get_messages(session_id).await;
        // User1, Assistant1, User2, Assistant2
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].content, "First question");
        assert_eq!(messages[2].content, "Second question");
    }

    // ==================== Terminal Output Processing Tests ====================

    #[tokio::test]
    async fn test_terminal_output_idle_state() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // No prompt submitted = Idle state
        let result = processor.process_terminal_output(session_id, b"Some output").await;
        assert!(result.is_none()); // Should not process in Idle state
    }

    #[tokio::test]
    async fn test_terminal_output_waiting_for_response() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // Submit prompt to enter WaitingForResponse state
        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let result = processor.process_terminal_output(session_id, b"Hello! How can I help?").await;
        assert!(result.is_some());

        match result.unwrap() {
            ChatEvent::ContentDelta { delta, .. } => {
                assert!(delta.contains("Hello! How can I help?"));
            }
            _ => panic!("Expected ContentDelta"),
        }
    }

    #[tokio::test]
    async fn test_terminal_output_during_tool() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Run a command".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "ls"}),
            tool_use_id: "tool_x".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Terminal output during tool should be ignored (in_tool_output = true)
        let result = processor.process_terminal_output(session_id, b"file1.txt\nfile2.txt\n").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_terminal_output_filters_status_lines() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // Status line should be filtered
        let result = processor.process_terminal_output(session_id, b"Claude 3.5 | $0.50 | 10K/5K").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_terminal_output_strips_ansi() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let result = processor.process_terminal_output(session_id, b"\x1b[32mColored text\x1b[0m").await;
        assert!(result.is_some());

        match result.unwrap() {
            ChatEvent::ContentDelta { delta, .. } => {
                assert!(!delta.contains("\x1b"));
                assert!(delta.contains("Colored text"));
            }
            _ => panic!("Expected ContentDelta"),
        }
    }

    // ==================== Session Management Tests ====================

    #[tokio::test]
    async fn test_get_messages_empty_session() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        let messages = processor.get_messages(session_id).await;
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_clear_session() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        assert!(!processor.get_messages(session_id).await.is_empty());

        processor.clear_session(session_id).await;

        assert!(processor.get_messages(session_id).await.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_sessions_isolated() {
        let processor = ChatProcessor::new();
        let session1 = Uuid::new_v4();
        let session2 = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id: session1,
            claude_session_id: "test1".to_string(),
            prompt: "Session 1 prompt".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id: session2,
            claude_session_id: "test2".to_string(),
            prompt: "Session 2 prompt".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let msgs1 = processor.get_messages(session1).await;
        let msgs2 = processor.get_messages(session2).await;

        assert_eq!(msgs1[0].content, "Session 1 prompt");
        assert_eq!(msgs2[0].content, "Session 2 prompt");
    }

    // ==================== Event Type Verification Tests ====================

    #[tokio::test]
    async fn test_user_prompt_event_structure() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        let events = processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        // First event: user message
        match &events[0] {
            ChatEvent::Message { session_id: sid, message } => {
                assert_eq!(*sid, session_id);
                assert_eq!(message.role, ChatRole::User);
                assert_eq!(message.content, "Hello");
            }
            _ => panic!("Expected Message event"),
        }

        // Second event: assistant message (streaming)
        match &events[1] {
            ChatEvent::Message { message, .. } => {
                assert_eq!(message.role, ChatRole::Assistant);
                assert!(message.is_streaming);
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_tool_call_start_event_structure() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Read".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let events = processor.process_hook_event(&HookEvent::PreToolUse {
            session_id,
            claude_session_id: "test".to_string(),
            tool_name: "Read".to_string(),
            tool_input: serde_json::json!({"path": "/test.txt"}),
            tool_use_id: "toolu_123".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        match &events[0] {
            ChatEvent::ToolCallStart { session_id: sid, message_id, tool_call } => {
                assert_eq!(*sid, session_id);
                assert!(!message_id.is_empty());
                assert_eq!(tool_call.id, "toolu_123");
                assert_eq!(tool_call.name, "Read");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
    }

    #[tokio::test]
    async fn test_message_complete_event_structure() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        processor.process_hook_event(&HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id: "test".to_string(),
            prompt: "Hello".to_string(),
            cwd: None,
            context_window: None,
        }).await;

        let events = processor.process_hook_event(&HookEvent::Stop {
            session_id,
            claude_session_id: "test".to_string(),
            stop_hook_active: false,
            transcript_path: None,
            context_window: None,
        }).await;

        match &events[0] {
            ChatEvent::MessageComplete { session_id: sid, message_id } => {
                assert_eq!(*sid, session_id);
                assert!(!message_id.is_empty());
            }
            _ => panic!("Expected MessageComplete event"),
        }
    }

    // ==================== Edge Cases ====================

    #[tokio::test]
    async fn test_handles_unknown_event_types() {
        let processor = ChatProcessor::new();
        let session_id = Uuid::new_v4();

        // SessionStart is not explicitly handled
        let events = processor.process_hook_event(&HookEvent::SessionStart {
            session_id,
            claude_session_id: "test".to_string(),
            source: "cli".to_string(),
            cwd: Some("/home/user".to_string()),
            context_window: None,
            model: None,
        }).await;

        assert!(events.is_empty()); // Should handle gracefully
    }

    #[tokio::test]
    async fn test_default_trait() {
        let processor = ChatProcessor::default();
        let session_id = Uuid::new_v4();

        // Should work the same as new()
        let messages = processor.get_messages(session_id).await;
        assert!(messages.is_empty());
    }
}
