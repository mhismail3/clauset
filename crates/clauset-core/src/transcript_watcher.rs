//! Real-time JSONL transcript file watcher for Claude Code sessions.
//!
//! Watches the transcript file written by Claude Code at `~/.claude/projects/<path>/<session>.jsonl`
//! and emits events for each content block (user messages, thinking, text, tool_use, tool_result).
//!
//! **The transcript file is the authoritative source for token usage data.**
//! Each assistant message contains accurate API usage including cache tokens.

use crate::Result;
use clauset_types::{ChatEvent, ChatToolCall};
use notify::{
    event::{AccessKind, AccessMode, ModifyKind},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

/// Token usage from a single API call (assistant message).
///
/// This is the authoritative source for token data, extracted directly from
/// Claude Code's transcript file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptUsage {
    /// Direct input tokens (not from cache)
    pub input_tokens: u64,
    /// Output tokens generated
    pub output_tokens: u64,
    /// Tokens written to cache
    pub cache_creation_input_tokens: u64,
    /// Tokens read from cache
    pub cache_read_input_tokens: u64,
    /// Model used for this call
    pub model: String,
}

impl TranscriptUsage {
    /// Total input tokens including cache (for context window calculation)
    pub fn total_input(&self) -> u64 {
        self.input_tokens + self.cache_read_input_tokens + self.cache_creation_input_tokens
    }

    /// Merge another usage into this one (for cumulative totals)
    pub fn accumulate(&mut self, other: &TranscriptUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
        // Keep the most recent model
        if !other.model.is_empty() {
            self.model = other.model.clone();
        }
    }
}

/// Cumulative session usage computed from transcript.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUsage {
    /// Total input tokens across all turns
    pub total_input_tokens: u64,
    /// Total output tokens across all turns
    pub total_output_tokens: u64,
    /// Total cache read tokens across all turns
    pub total_cache_read_tokens: u64,
    /// Total cache creation tokens across all turns
    pub total_cache_creation_tokens: u64,
    /// Model from most recent message
    pub model: String,
    /// Number of assistant messages processed
    pub message_count: u64,
}

impl SessionUsage {
    /// Add a single message's usage to the cumulative total.
    pub fn add_message(&mut self, usage: &TranscriptUsage) {
        self.total_input_tokens += usage.input_tokens;
        self.total_output_tokens += usage.output_tokens;
        self.total_cache_read_tokens += usage.cache_read_input_tokens;
        self.total_cache_creation_tokens += usage.cache_creation_input_tokens;
        if !usage.model.is_empty() {
            self.model = usage.model.clone();
        }
        self.message_count += 1;
    }

    /// Total tokens for context window percentage calculation.
    /// Uses input + cache_read as that's what's in the context.
    pub fn context_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_cache_read_tokens + self.total_cache_creation_tokens
    }
}

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
    /// Token usage update from an assistant message.
    /// This is the authoritative source for token data.
    Usage {
        message_id: String,
        usage: TranscriptUsage,
    },
    /// System event (stop_hook_summary, compact_boundary, etc.)
    SystemEvent {
        message_id: String,
        /// Subtype of system event (e.g., "stop_hook_summary", "compact_boundary")
        subtype: String,
        /// Content text if present
        content: Option<String>,
        /// Additional metadata
        metadata: Option<Value>,
        timestamp: u64,
    },
    /// File history snapshot (tracking file modifications)
    FileSnapshot {
        message_id: String,
        /// Paths to files included in the snapshot
        file_paths: Vec<String>,
        timestamp: u64,
    },
    /// Context compaction boundary marker
    ContextCompacted {
        timestamp: u64,
        /// Metadata about the compaction (e.g., hook counts)
        metadata: Option<Value>,
    },
}

/// Entry from Claude's transcript JSONL.
#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<TranscriptMessageEntry>,
    timestamp: Option<String>,
    /// Subtype for system entries (e.g., "stop_hook_summary", "compact_boundary")
    #[serde(default)]
    subtype: Option<String>,
    /// Metadata for file-history-snapshot entries
    #[serde(rename = "filePaths", default)]
    file_paths: Option<Vec<String>>,
    /// Content for system entries
    #[serde(default)]
    content: Option<Value>,
    /// Hook count for stop_hook_summary
    #[serde(rename = "hookCount", default)]
    hook_count: Option<u64>,
    /// Compact metadata for compact_boundary
    #[serde(rename = "compactMetadata", default)]
    compact_metadata: Option<Value>,
}

/// Usage data embedded in assistant messages.
#[derive(Debug, Deserialize, Default)]
struct TranscriptMessageUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessageEntry {
    id: Option<String>,
    content: Value,
    /// Model used for this message (assistant messages only)
    #[serde(default)]
    model: Option<String>,
    /// Token usage (assistant messages only)
    #[serde(default)]
    usage: Option<TranscriptMessageUsage>,
}

/// Watches a Claude Code transcript file and emits content events in real-time.
pub struct TranscriptWatcher {
    path: PathBuf,
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
        event_tx: mpsc::UnboundedSender<TranscriptEvent>,
    ) -> Self {
        Self {
            path,
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
        let path_for_task = path.clone();

        // Watch the parent directory instead of the file directly.
        // This handles the case where the transcript file doesn't exist yet
        // (Claude Code creates it when the session starts processing).
        let parent_dir = path.parent().ok_or_else(|| {
            crate::ClausetError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot get parent directory of {:?}", path),
            ))
        })?;

        // Ensure parent directory exists
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir).map_err(|e| {
                crate::ClausetError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create directory {:?}: {}", parent_dir, e),
                ))
            })?;
        }

        // Start file watcher on parent directory
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
        let mut file_watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = notify_tx.send(event);
            }
        }).map_err(|e| crate::ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        file_watcher.watch(parent_dir, RecursiveMode::NonRecursive)
            .map_err(|e| crate::ClausetError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        // Spawn task to process file events
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = notify_rx.recv() => {
                        // Only process events for our specific file
                        let is_our_file = event.paths.iter().any(|p| p == &path_for_task);
                        if !is_our_file {
                            continue;
                        }

                        // Check if file was created, modified, or written
                        let should_read = matches!(
                            event.kind,
                            EventKind::Create(_)
                                | EventKind::Modify(ModifyKind::Data(_))
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
            .as_ref()
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
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
            "system" => {
                self.process_system_entry(&entry, timestamp);
            }
            "file-history-snapshot" => {
                self.process_file_snapshot(&entry, timestamp);
            }
            other => {
                debug!(
                    target: "clauset::transcript_watcher",
                    "Unhandled transcript entry type: {}",
                    other
                );
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
            .clone()
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

        // Emit usage event if usage data is present (authoritative token source)
        if let Some(ref usage) = message.usage {
            let transcript_usage = TranscriptUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
                model: message.model.clone().unwrap_or_default(),
            };
            let _ = self.event_tx.send(TranscriptEvent::Usage {
                message_id: message_id.clone(),
                usage: transcript_usage,
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

    /// Process a system entry (stop_hook_summary, compact_boundary, etc.)
    fn process_system_entry(&mut self, entry: &TranscriptEntry, timestamp: u64) {
        let subtype = match &entry.subtype {
            Some(s) => s.clone(),
            None => {
                debug!(
                    target: "clauset::transcript_watcher",
                    "System entry without subtype, skipping"
                );
                return;
            }
        };

        let message_id = format!("system-{}-{}", subtype, uuid::Uuid::new_v4());

        // Extract text content if present
        let content_text = entry.content.as_ref().and_then(|c| {
            if let Value::String(s) = c {
                Some(s.clone())
            } else if let Value::Array(arr) = c {
                // Content might be an array of blocks
                let texts: Vec<String> = arr
                    .iter()
                    .filter_map(|b| {
                        if let Some(text) = b.get("text").and_then(|t| t.as_str()) {
                            Some(text.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            } else {
                None
            }
        });

        // Handle compact_boundary specially
        if subtype == "compact_boundary" {
            info!(
                target: "clauset::transcript_watcher",
                "Context compaction boundary detected at timestamp {}",
                timestamp
            );
            let _ = self.event_tx.send(TranscriptEvent::ContextCompacted {
                timestamp,
                metadata: entry.compact_metadata.clone(),
            });
            return;
        }

        // Build metadata from various fields
        let metadata = if entry.hook_count.is_some() || entry.compact_metadata.is_some() {
            let mut meta = serde_json::Map::new();
            if let Some(hook_count) = entry.hook_count {
                meta.insert("hookCount".to_string(), Value::Number(hook_count.into()));
            }
            if let Some(ref compact_meta) = entry.compact_metadata {
                meta.insert("compactMetadata".to_string(), compact_meta.clone());
            }
            Some(Value::Object(meta))
        } else {
            None
        };

        debug!(
            target: "clauset::transcript_watcher",
            "Emitting SystemEvent: subtype={}, has_content={}",
            subtype,
            content_text.is_some()
        );

        let _ = self.event_tx.send(TranscriptEvent::SystemEvent {
            message_id,
            subtype,
            content: content_text,
            metadata,
            timestamp,
        });
    }

    /// Process a file-history-snapshot entry.
    fn process_file_snapshot(&mut self, entry: &TranscriptEntry, timestamp: u64) {
        let file_paths = entry.file_paths.clone().unwrap_or_default();

        if file_paths.is_empty() {
            debug!(
                target: "clauset::transcript_watcher",
                "File snapshot with no file paths, skipping"
            );
            return;
        }

        let message_id = format!("file-snapshot-{}", uuid::Uuid::new_v4());

        debug!(
            target: "clauset::transcript_watcher",
            "Emitting FileSnapshot: {} files",
            file_paths.len()
        );

        let _ = self.event_tx.send(TranscriptEvent::FileSnapshot {
            message_id,
            file_paths,
            timestamp,
        });
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

/// Compute cumulative session usage by reading an entire transcript file.
///
/// This is the authoritative source for session token usage. Call this when:
/// - A session starts (to initialize from existing transcript)
/// - A session resumes (to restore accurate totals)
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn compute_session_usage(transcript_path: &Path) -> Option<SessionUsage> {
    if !transcript_path.exists() {
        return None;
    }

    let file = match File::open(transcript_path) {
        Ok(f) => f,
        Err(e) => {
            warn!(
                target: "clauset::transcript",
                "Failed to open transcript file {:?}: {}",
                transcript_path, e
            );
            return None;
        }
    };

    let reader = BufReader::new(file);
    let mut session_usage = SessionUsage::default();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        // Parse the JSONL entry
        let entry: TranscriptEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Only process assistant messages with usage data
        if entry.entry_type != "assistant" {
            continue;
        }

        if let Some(message) = entry.message {
            if let Some(usage) = message.usage {
                let transcript_usage = TranscriptUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_creation_input_tokens: usage.cache_creation_input_tokens,
                    cache_read_input_tokens: usage.cache_read_input_tokens,
                    model: message.model.unwrap_or_default(),
                };
                session_usage.add_message(&transcript_usage);
            }
        }
    }

    if session_usage.message_count > 0 {
        info!(
            target: "clauset::transcript",
            "Computed session usage from transcript: {} messages, {} input tokens, {} output tokens, model={}",
            session_usage.message_count,
            session_usage.total_input_tokens,
            session_usage.total_output_tokens,
            session_usage.model
        );
        Some(session_usage)
    } else {
        None
    }
}

/// Get the transcript file path for a Claude session.
///
/// The path format is: `~/.claude/projects/<encoded-project-path>/<session-id>.jsonl`
pub fn get_transcript_path(claude_session_id: &str, project_path: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let claude_projects = home.join(".claude").join("projects");

    // Encode project path (replace / with -)
    let encoded_path = project_path
        .to_string_lossy()
        .replace('/', "-")
        .trim_start_matches('-')
        .to_string();

    let transcript_path = claude_projects
        .join(&encoded_path)
        .join(format!("{}.jsonl", claude_session_id));

    if transcript_path.exists() {
        Some(transcript_path)
    } else {
        // Try without the leading dash
        let alt_path = claude_projects
            .join(format!("-{}", encoded_path))
            .join(format!("{}.jsonl", claude_session_id));
        if alt_path.exists() {
            Some(alt_path)
        } else {
            None
        }
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

        // Usage events are handled separately (not chat events) - skip
        TranscriptEvent::Usage { .. } => None,

        // System events - not converted to chat events
        // These are handled separately for session state updates
        TranscriptEvent::SystemEvent { .. } => None,

        // File snapshots - not converted to chat events
        // Could be used for file tracking in the future
        TranscriptEvent::FileSnapshot { .. } => None,

        // Context compaction - not converted to chat events
        // Handled separately to notify frontend of context reset
        TranscriptEvent::ContextCompacted { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
