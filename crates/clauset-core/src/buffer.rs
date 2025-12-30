//! Terminal output buffering and activity tracking.
//!
//! This module provides reliable terminal streaming with:
//! - Sequenced chunks for ordered delivery and gap detection
//! - Ring buffer eviction with sequence tracking
//! - Activity parsing from terminal output
//! - TUI menu detection for native UI rendering

use crate::TuiMenuParser;
use clauset_types::{CurrentUsage, PermissionMode, TuiMenu};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum buffer size per session (500KB for longer scrollback)
const MAX_BUFFER_SIZE: usize = 500 * 1024;

/// Maximum number of recent actions to track
const MAX_RECENT_ACTIONS: usize = 5;

// ============================================================================
// Reliable Streaming Types
// ============================================================================

/// A single sequenced chunk of terminal output.
#[derive(Debug, Clone)]
pub struct SequencedChunk {
    /// Monotonically increasing sequence number
    pub seq: u64,
    /// Terminal data (raw bytes including ANSI codes)
    pub data: Vec<u8>,
    /// Timestamp when chunk was captured (ms since Unix epoch)
    pub timestamp: u64,
}

/// Ring buffer that maintains sequence numbers for reliable streaming.
///
/// Features:
/// - Automatic sequence number assignment
/// - Bounded memory with oldest chunk eviction
/// - Range queries for gap recovery
/// - Full buffer retrieval for reconnection
#[derive(Debug)]
pub struct SequencedRingBuffer {
    /// Queue of sequenced chunks (oldest at front)
    pub(crate) chunks: VecDeque<SequencedChunk>,
    /// Sequence number of oldest chunk (or next_seq if empty)
    start_seq: u64,
    /// Next sequence number to assign
    next_seq: u64,
    /// Total bytes currently in buffer
    total_bytes: usize,
    /// Maximum buffer size in bytes
    max_bytes: usize,
}

impl SequencedRingBuffer {
    /// Create a new buffer with the specified max size in bytes.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            start_seq: 0,
            next_seq: 0,
            total_bytes: 0,
            max_bytes,
        }
    }

    /// Append data to the buffer, assigning a sequence number.
    /// Returns (assigned sequence, number of chunks evicted).
    pub fn push(&mut self, data: Vec<u8>) -> (u64, u32) {
        let seq = self.next_seq;
        self.next_seq += 1;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let chunk_size = data.len();
        self.total_bytes += chunk_size;

        self.chunks.push_back(SequencedChunk {
            seq,
            data,
            timestamp,
        });

        // Evict oldest chunks if over capacity
        let mut evicted = 0u32;
        while self.total_bytes > self.max_bytes && self.chunks.len() > 1 {
            if let Some(old) = self.chunks.pop_front() {
                self.total_bytes -= old.data.len();
                self.start_seq = self.chunks.front().map(|c| c.seq).unwrap_or(self.next_seq);
                evicted += 1;
            }
        }

        (seq, evicted)
    }

    /// Get chunks in a sequence range (inclusive).
    /// Returns chunks where start_seq <= chunk.seq <= end_seq.
    pub fn get_range(&self, start: u64, end: u64) -> Vec<&SequencedChunk> {
        self.chunks
            .iter()
            .filter(|c| c.seq >= start && c.seq <= end)
            .collect()
    }

    /// Get all chunk data concatenated as a single buffer.
    /// Returns (start_seq, end_seq, concatenated data).
    pub fn get_all(&self) -> (u64, u64, Vec<u8>) {
        let start = self.start_seq;
        let end = self.next_seq.saturating_sub(1);
        let data: Vec<u8> = self.chunks.iter().flat_map(|c| c.data.iter().copied()).collect();
        (start, end, data)
    }

    /// Get raw data without sequence info (for legacy compatibility).
    pub fn get_raw_data(&self) -> Vec<u8> {
        self.chunks.iter().flat_map(|c| c.data.iter().copied()).collect()
    }

    /// Get the oldest available sequence number.
    pub fn start_seq(&self) -> u64 {
        self.start_seq
    }

    /// Get the most recent sequence number (next_seq - 1), or 0 if empty.
    pub fn end_seq(&self) -> u64 {
        if self.next_seq == 0 {
            0
        } else {
            self.next_seq - 1
        }
    }

    /// Get the next sequence number that will be assigned.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Check if a sequence number is still available in the buffer.
    pub fn has_seq(&self, seq: u64) -> bool {
        seq >= self.start_seq && seq < self.next_seq
    }

    /// Get total bytes in buffer.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Get number of chunks in buffer.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Clear all data and reset sequences.
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_bytes = 0;
        // Note: We don't reset start_seq/next_seq to maintain monotonicity
        self.start_seq = self.next_seq;
    }
}

impl Default for SequencedRingBuffer {
    fn default() -> Self {
        Self::new(MAX_BUFFER_SIZE)
    }
}

/// Result of appending data to the sequenced buffer.
#[derive(Debug, Clone)]
pub struct AppendResult {
    /// Sequence number assigned to this chunk
    pub seq: u64,
    /// Timestamp when chunk was captured
    pub timestamp: u64,
    /// Number of old chunks evicted (if any)
    pub evicted_count: u32,
    /// New start_seq after eviction (if changed)
    pub new_start_seq: Option<u64>,
}

/// A single action/step performed by Claude
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecentAction {
    /// Action type: "bash", "read", "write", "edit", "thinking", "searching", etc.
    pub action_type: String,
    /// Short summary of the action
    pub summary: String,
    /// Optional detail (file path, command, etc.)
    pub detail: Option<String>,
    /// Timestamp in milliseconds
    pub timestamp: u64,
}

/// Parsed status information from Claude's status line.
#[derive(Debug, Clone)]
pub struct SessionActivity {
    pub model: String,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Tokens read from cache (for context calculation)
    pub cache_read_tokens: u64,
    /// Tokens written to cache
    pub cache_creation_tokens: u64,
    /// Context window size for the model (from hooks, e.g., 200000)
    pub context_window_size: u64,
    pub context_percent: u8,
    pub permission_mode: Option<PermissionMode>,
    /// Current high-level activity (e.g., "Thinking...", "Reading file.rs")
    pub current_activity: String,
    /// Current step being executed (tool name or phase)
    pub current_step: Option<String>,
    /// Recent actions with details for rich preview
    pub recent_actions: Vec<RecentAction>,
    pub last_update: std::time::Instant,
    /// Tracks if session is in a "busy" state (user sent input, waiting for response)
    /// Once set to true, only transitions to false when we reliably detect completion.
    pub is_busy: bool,
    /// Timestamp when we were marked busy (user sent input)
    pub busy_since: Option<std::time::Instant>,
    /// Whether we've seen any activity (Thinking/tool use) since becoming busy.
    /// We must see activity before we can transition to Ready.
    pub saw_activity_since_busy: bool,
    /// Timestamp when we last saw an activity indicator (thinking/tool use)
    pub last_activity_indicator: std::time::Instant,
    /// Count of bytes received since last activity indicator - used to detect if
    /// Claude has output substantial response content
    pub bytes_since_activity: usize,
    /// Whether usage comes from transcript (authoritative source).
    /// When true, transcript data is used and hook context is skipped.
    pub transcript_usage_received: bool,
    /// Whether we've received context data from hooks.
    /// Hook data is used as a fallback when no status line data is available.
    pub hook_context_received: bool,
    /// Whether we've parsed a terminal status line for tokens/context.
    /// When true, status line values are treated as authoritative for display parity.
    pub status_line_seen: bool,
}

impl Default for SessionActivity {
    fn default() -> Self {
        Self {
            model: String::new(),
            cost: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            context_window_size: 0,
            context_percent: 0,
            permission_mode: None,
            current_activity: String::new(),
            current_step: None,
            recent_actions: Vec::new(),
            last_update: std::time::Instant::now(),
            is_busy: false,
            busy_since: None,
            saw_activity_since_busy: false,
            last_activity_indicator: std::time::Instant::now(),
            bytes_since_activity: 0,
            transcript_usage_received: false,
            hook_context_received: false,
            status_line_seen: false,
        }
    }
}

/// Get default context window size for a Claude model.
///
/// All current Claude models have a 200K context window.
/// This is used as a fallback when hooks don't provide context_window data.
fn default_context_window_for_model(model: &str) -> u64 {
    let model_lower = model.to_lowercase();

    // All Claude 3.5/4 models have 200K context
    if model_lower.contains("opus")
        || model_lower.contains("sonnet")
        || model_lower.contains("haiku")
        || model_lower.contains("claude")
    {
        return 200_000;
    }

    // Legacy models (Claude 2, etc.) - 100K context
    if model_lower.contains("claude-2") {
        return 100_000;
    }

    // Default for unknown models
    200_000
}

/// Ring buffer for terminal output with sequence tracking.
struct TerminalBuffer {
    /// Sequenced ring buffer for reliable streaming
    sequenced: SequencedRingBuffer,
    /// Activity tracking state
    activity: SessionActivity,
    /// TUI menu parser for detecting selection menus
    tui_menu_parser: TuiMenuParser,
}

impl TerminalBuffer {
    fn new() -> Self {
        Self {
            sequenced: SequencedRingBuffer::new(MAX_BUFFER_SIZE),
            activity: SessionActivity::default(),
            tui_menu_parser: TuiMenuParser::new(),
        }
    }

    /// Append data to the buffer.
    /// Returns (sequence number, timestamp, evicted count, new_start_seq if changed).
    fn append(&mut self, chunk: &[u8]) -> AppendResult {
        let old_start = self.sequenced.start_seq();
        let (seq, evicted) = self.sequenced.push(chunk.to_vec());
        let new_start = self.sequenced.start_seq();
        let timestamp = self.sequenced.chunks.back().map(|c| c.timestamp).unwrap_or(0);

        AppendResult {
            seq,
            timestamp,
            evicted_count: evicted,
            new_start_seq: if new_start != old_start { Some(new_start) } else { None },
        }
    }

    /// Get raw data for activity parsing (legacy compatibility).
    fn get_data(&self) -> Vec<u8> {
        self.sequenced.get_raw_data()
    }

    /// Get sequenced buffer info for sync response.
    fn get_buffer_info(&self) -> (u64, u64) {
        (self.sequenced.start_seq(), self.sequenced.end_seq())
    }

    /// Get chunks in a range for gap recovery.
    fn get_range(&self, start: u64, end: u64) -> Vec<&SequencedChunk> {
        self.sequenced.get_range(start, end)
    }

    /// Get full buffer with sequence info.
    fn get_all(&self) -> (u64, u64, Vec<u8>) {
        self.sequenced.get_all()
    }

    /// Check if sequence is available.
    fn has_seq(&self, seq: u64) -> bool {
        self.sequenced.has_seq(seq)
    }

    /// Clear buffer data (but maintain sequence monotonicity).
    fn clear_data(&mut self) {
        self.sequenced.clear();
    }
}

/// Manages terminal output buffers for all sessions.
pub struct SessionBuffers {
    buffers: Arc<RwLock<HashMap<Uuid, TerminalBuffer>>>,
}

impl Default for SessionBuffers {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionBuffers {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Append terminal output to a session's buffer and parse for activity.
    /// Returns (AppendResult, Option<SessionActivity>, Option<TuiMenu>, Option<PermissionMode>) where:
    /// - activity is Some if it changed
    /// - tui_menu is Some if a new TUI menu was detected
    /// - permission_mode is Some if the mode changed
    pub async fn append(&self, session_id: Uuid, data: &[u8]) -> (AppendResult, Option<SessionActivity>, Option<TuiMenu>, Option<PermissionMode>) {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);
        let append_result = buffer.append(data);
        let previous_mode = buffer.activity.permission_mode;

        // Track bytes received since last activity indicator
        buffer.activity.bytes_since_activity += data.len();

        // Convert the NEW chunk to text for activity detection
        // We only want to detect activity indicators in fresh output, not old buffer content
        let new_chunk_text = String::from_utf8_lossy(data).to_string();

        // Parse from the FULL buffer (last N bytes) for status line and Ready detection.
        // This is crucial because terminal output arrives in small pieces.
        let full_buffer_text = {
            let buffer_data = buffer.get_data();
            let parse_start = buffer_data.len().saturating_sub(8192); // Last 8KB
            String::from_utf8_lossy(&buffer_data[parse_start..]).to_string()
        };

        let activity_changed = self.parse_and_update_activity(buffer, &new_chunk_text, &full_buffer_text);

        let activity = if activity_changed {
            Some(buffer.activity.clone())
        } else {
            None
        };

        // Check for TUI menu patterns in terminal output
        let tui_menu = buffer.tui_menu_parser.process(data);

        let mode_change = if buffer.activity.permission_mode != previous_mode {
            buffer.activity.permission_mode
        } else {
            None
        };

        (append_result, activity, tui_menu, mode_change)
    }

    // ========================================================================
    // Reliable Streaming Methods
    // ========================================================================

    /// Get buffer sequence info for a session (start_seq, end_seq).
    /// Returns None if session doesn't exist.
    pub async fn get_buffer_info(&self, session_id: Uuid) -> Option<(u64, u64)> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.get_buffer_info())
    }

    /// Get full buffer with sequence info for sync response.
    /// Returns (start_seq, end_seq, data).
    pub async fn get_full_buffer(&self, session_id: Uuid) -> Option<(u64, u64, Vec<u8>)> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.get_all())
    }

    /// Get chunks in a sequence range for gap recovery.
    /// Returns cloned chunks to avoid holding lock.
    pub async fn get_chunk_range(&self, session_id: Uuid, start: u64, end: u64) -> Option<Vec<SequencedChunk>> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| {
            b.get_range(start, end)
                .into_iter()
                .cloned()
                .collect()
        })
    }

    /// Check if a sequence is still available in the buffer.
    pub async fn has_seq(&self, session_id: Uuid, seq: u64) -> bool {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.has_seq(seq)).unwrap_or(false)
    }

    /// Parse terminal output for status line and current activity.
    ///
    /// KEY DESIGN: Uses STATEFUL tracking to prevent flickering.
    /// - When we detect activity (thinking/tool), we set is_busy = true
    /// - We only transition to Ready when we have POSITIVE evidence that Claude finished:
    ///   1. Substantial output has been received since last activity indicator
    ///   2. The prompt appears in a valid position (end of buffer)
    ///   3. Some time has passed since last activity
    ///
    /// Parameters:
    /// - new_chunk: The fresh data just received (used for activity indicator detection)
    /// - full_buffer: The last 8KB of buffer (used for status line and Ready detection)
    fn parse_and_update_activity(&self, buffer: &mut TerminalBuffer, new_chunk: &str, full_buffer: &str) -> bool {
        let mut changed = false;

        // Strip ANSI escape codes for parsing
        let clean_chunk = strip_ansi_codes(new_chunk);
        let clean_buffer = strip_ansi_codes(full_buffer);

        // Parse status line from FULL BUFFER: "Model | $Cost | InputK/OutputK | ctx:X%"
        //
        // Status line values are treated as authoritative for display parity with the terminal.
        // Hooks/transcript still populate cache tokens and context window metadata.
        if let Some(status) = parse_status_line(&clean_buffer) {
            // Always update model if not set (model comes from transcript too, but regex is faster)
            if buffer.activity.model.is_empty() && !status.model.is_empty() {
                buffer.activity.model = status.model.clone();
                changed = true;
            }

            // Always update cost from regex - it's the only source
            if (buffer.activity.cost - status.cost).abs() > 0.001 {
                buffer.activity.cost = status.cost;
                changed = true;
            }

            let input_changed = buffer.activity.input_tokens != status.input_tokens;
            let output_changed = buffer.activity.output_tokens != status.output_tokens;
            let ctx_changed = buffer.activity.context_percent != status.context_percent;

            if input_changed || output_changed || ctx_changed {
                tracing::debug!(
                    target: "clauset::activity::stats",
                    "Stats updated (status line): tokens={}K/{}K, ctx={}%",
                    status.input_tokens/1000, status.output_tokens/1000, status.context_percent
                );
                buffer.activity.input_tokens = status.input_tokens;
                buffer.activity.output_tokens = status.output_tokens;
                buffer.activity.context_percent = status.context_percent;
                buffer.activity.last_update = std::time::Instant::now();
                changed = true;
            }

            if !buffer.activity.status_line_seen {
                buffer.activity.status_line_seen = true;
                changed = true;
            }
        }

        if let Some(mode) = parse_permission_mode(&clean_buffer) {
            if buffer.activity.permission_mode != Some(mode) {
                buffer.activity.permission_mode = Some(mode);
                buffer.activity.last_update = std::time::Instant::now();
                changed = true;
            }
        }

        // Parse activity from NEW CHUNK ONLY for detecting fresh activity indicators
        // This prevents old "Thinking" lines from resetting timers
        let chunk_parsed = parse_activity_and_action(&clean_chunk);

        if let Some((ref _activity, ref step, ref _actions)) = chunk_parsed {
            // Check if this NEW chunk contains an activity indicator (thinking/tool use)
            let is_activity_indicator = step.as_deref().map(|s| {
                let lower = s.to_lowercase();
                lower == "thinking" || lower == "planning" ||
                // Tool names indicate active work
                ["read", "edit", "write", "bash", "grep", "glob", "task", "search", "webfetch", "websearch"]
                    .iter().any(|t| lower == *t)
            }).unwrap_or(false);

            // If we detect an activity indicator IN THE NEW CHUNK, update tracking
            // IMPORTANT: We do NOT set is_busy = true here. Only mark_busy() (called when
            // user sends input) should transition to busy state. This prevents terminal
            // redraws (which may contain "Thinking" text) from flipping us back to busy
            // after we've already transitioned to Ready.
            if is_activity_indicator {
                if buffer.activity.is_busy {
                    // Already busy - update tracking to confirm activity is happening
                    tracing::debug!(
                        target: "clauset::activity::state",
                        "Activity indicator in NEW chunk: {:?} - updating activity tracking",
                        step
                    );
                    buffer.activity.saw_activity_since_busy = true;
                    buffer.activity.last_activity_indicator = std::time::Instant::now();
                    buffer.activity.bytes_since_activity = 0;
                } else {
                    // Not busy (Ready state) - this is likely a terminal redraw, ignore
                    tracing::debug!(
                        target: "clauset::activity::state",
                        "Activity indicator in NEW chunk: {:?} - but not busy, ignoring (likely terminal redraw)",
                        step
                    );
                }
            }
        }

        // Parse FULL BUFFER for actions list and Ready detection
        let parsed = parse_activity_and_action(&clean_buffer);

        if let Some((ref _activity, ref _step, ref actions)) = parsed {
            // Add all new actions (deduplicating against existing ones)
            for new_action in actions {
                let already_exists = buffer.activity.recent_actions.iter().any(|a| {
                    a.action_type == new_action.action_type && a.summary == new_action.summary
                });

                if !already_exists {
                    buffer.activity.recent_actions.push(new_action.clone());
                    changed = true;

                    while buffer.activity.recent_actions.len() > MAX_RECENT_ACTIONS {
                        buffer.activity.recent_actions.remove(0);
                    }
                }
            }
        }

        // STATEFUL STATUS DETERMINATION
        // Instead of trusting the parsed activity directly, we use state tracking.
        let now = std::time::Instant::now();
        let time_since_activity = now.duration_since(buffer.activity.last_activity_indicator);

        // Determine the new step based on state
        let new_step: Option<String>;
        let new_activity: String;

        let parsed_step = parsed.as_ref().and_then(|(_, s, _)| s.clone());

        // Calculate time since we were marked busy (for fallback timeout)
        let time_since_busy = buffer.activity.busy_since
            .map(|t| now.duration_since(t).as_millis())
            .unwrap_or(0);

        // TRACE: Log state for troubleshooting (high frequency - fires every chunk)
        tracing::trace!(
            target: "clauset::activity::state",
            "is_busy={}, saw_activity={}, time_since_busy={}ms, time_since_activity={}ms, bytes={}, parsed_step={:?}",
            buffer.activity.is_busy,
            buffer.activity.saw_activity_since_busy,
            time_since_busy,
            time_since_activity.as_millis(),
            buffer.activity.bytes_since_activity,
            parsed_step
        );

        if buffer.activity.is_busy {
            // We're in busy state. Check if we should transition to Ready.
            //
            // KEY INSIGHT: We must see REAL activity (Thinking/tool use) after becoming busy
            // before we can transition to Ready. This prevents premature transition when
            // we see the old `>` prompt in the buffer before Claude starts processing.
            //
            // Requirements for transition:
            // 1. saw_activity_since_busy = true (we've seen Claude actually do something)
            // 2. At least 300ms since last activity indicator (activity has stopped)
            // 3. At least 100 bytes received since last activity (Claude's response)
            // 4. The parsed status shows "Ready" (prompt detected in valid position)
            //
            // OR (fallback for quick responses or if Claude never shows activity):
            // - At least 5 seconds since marked busy AND parsed_ready

            let saw_activity = buffer.activity.saw_activity_since_busy;
            let time_ok = time_since_activity.as_millis() >= 300;
            let bytes_ok = buffer.activity.bytes_since_activity >= 100;
            let parsed_ready = parsed.as_ref()
                .map(|(_, step, _)| step.as_deref() == Some("Ready"))
                .unwrap_or(false);

            // Fallback: if we've been busy for 5+ seconds without seeing activity,
            // and parser says Ready, assume Claude responded quickly without showing status
            let fallback_timeout = time_since_busy >= 5000 && parsed_ready;

            tracing::debug!(
                target: "clauset::activity::state",
                "BUSY CHECK: saw_activity={}, time_ok={}, bytes_ok={}, parsed_ready={}, fallback={}",
                saw_activity, time_ok, bytes_ok, parsed_ready, fallback_timeout
            );

            let can_transition = (saw_activity && time_ok && bytes_ok && parsed_ready) || fallback_timeout;

            if can_transition {
                // Transition to Ready
                tracing::info!(target: "clauset::activity", ">>> TRANSITION TO READY <<<");
                buffer.activity.is_busy = false;
                buffer.activity.busy_since = None;
                buffer.activity.saw_activity_since_busy = false;
                new_step = Some("Ready".to_string());
                new_activity = "Ready".to_string();
            } else {
                // Stay busy - use the parsed activity if available, or show "Thinking"
                if let Some((ref activity, ref step, _)) = parsed {
                    if step.as_deref() != Some("Ready") {
                        new_step = step.clone();
                        new_activity = activity.clone();
                    } else {
                        // Parser says Ready but we don't trust it yet
                        new_step = Some("Thinking".to_string());
                        new_activity = "Thinking...".to_string();
                    }
                } else {
                    new_step = Some("Thinking".to_string());
                    new_activity = "Thinking...".to_string();
                }
            }
        } else {
            // Not busy - ALWAYS show Ready state.
            // The regex parser should NOT override the Ready state with old buffer content.
            // Hooks are the authoritative source for activity state transitions.
            // Regex parsing is only used for cost/tokens/model (handled above).
            new_step = Some("Ready".to_string());
            new_activity = "Ready".to_string();
        }

        // Apply the determined status
        if buffer.activity.current_activity != new_activity {
            buffer.activity.current_activity = new_activity;
            buffer.activity.last_update = now;
            changed = true;
        }
        if buffer.activity.current_step != new_step {
            buffer.activity.current_step = new_step;
            changed = true;
        }

        changed
    }

    /// Get the full terminal buffer for a session (for replay on reconnect).
    /// DEPRECATED: Use get_full_buffer() for sequence-aware retrieval.
    pub async fn get_buffer(&self, session_id: Uuid) -> Option<Vec<u8>> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.get_data())
    }

    /// Get current activity for a session.
    pub async fn get_activity(&self, session_id: Uuid) -> Option<SessionActivity> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.activity.clone())
    }

    /// Remove a session's buffer.
    pub async fn remove(&self, session_id: Uuid) {
        self.buffers.write().await.remove(&session_id);
    }

    /// Clear a session's buffer but keep the entry.
    /// Note: This maintains sequence monotonicity - next seq will continue from where it was.
    pub async fn clear(&self, session_id: Uuid) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&session_id) {
            buffer.clear_data();
        }
    }

    /// Reset the context usage percentage without changing token totals.
    pub async fn reset_context_percent(&self, session_id: Uuid) -> Option<SessionActivity> {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.get_mut(&session_id)?;
        buffer.activity.context_percent = 0;
        buffer.activity.last_update = std::time::Instant::now();
        Some(buffer.activity.clone())
    }

    /// Mark a session as busy (user sent input, waiting for Claude's response).
    /// This ensures the status stays "Thinking" until Claude reliably finishes.
    pub async fn mark_busy(&self, session_id: Uuid) {
        tracing::debug!(target: "clauset::session", "mark_busy called for session {}", session_id);
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&session_id) {
            buffer.activity.is_busy = true;
            buffer.activity.busy_since = Some(std::time::Instant::now());
            buffer.activity.saw_activity_since_busy = false; // Reset - must see activity before Ready
            buffer.activity.last_activity_indicator = std::time::Instant::now();
            buffer.activity.bytes_since_activity = 0;
            buffer.activity.current_step = Some("Thinking".to_string());
            buffer.activity.current_activity = "Thinking...".to_string();
        }
    }

    /// Mark a session as ready (Claude finished responding).
    pub async fn mark_ready(&self, session_id: Uuid) {
        tracing::debug!(target: "clauset::session", "mark_ready called for session {}", session_id);
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&session_id) {
            buffer.activity.is_busy = false;
            buffer.activity.busy_since = None;
            buffer.activity.saw_activity_since_busy = false;
            buffer.activity.current_step = Some("Ready".to_string());
            buffer.activity.current_activity = "Ready".to_string();
        }
    }

    /// Initialize a session buffer with Ready state.
    /// Called when a new session starts to ensure it shows "Ready" immediately.
    pub async fn initialize_session(&self, session_id: Uuid) -> SessionActivity {
        tracing::debug!(target: "clauset::session", "initialize_session called for session {}", session_id);
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        // Set initial "Ready" state
        buffer.activity.current_step = Some("Ready".to_string());
        buffer.activity.current_activity = "Ready".to_string();
        buffer.activity.is_busy = false;
        buffer.activity.busy_since = None;
        buffer.activity.last_update = std::time::Instant::now();

        buffer.activity.clone()
    }

    /// Restore a session's buffer from persisted data.
    /// Used when resuming a session to restore terminal history.
    /// Returns true if buffer was restored, false if no data provided.
    pub async fn restore_buffer(
        &self,
        session_id: Uuid,
        data: Vec<u8>,
        start_seq: u64,
        end_seq: u64,
    ) -> bool {
        if data.is_empty() {
            return false;
        }

        tracing::info!(
            target: "clauset::session",
            "Restoring buffer for session {}: {} bytes, seq {}..{}",
            session_id,
            data.len(),
            start_seq,
            end_seq
        );

        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        // Clear existing buffer and restore
        buffer.sequenced.clear();

        // Push the entire persisted data as a single chunk
        // The sequence numbers will be reset to start from the current next_seq
        buffer.sequenced.push(data);

        // Set activity to Ready state (will be updated once Claude responds)
        buffer.activity.current_step = Some("Ready".to_string());
        buffer.activity.current_activity = "Ready".to_string();
        buffer.activity.is_busy = false;
        buffer.activity.last_update = std::time::Instant::now();

        true
    }

    /// Get buffer data for persistence.
    /// Returns (data, start_seq, end_seq) or None if buffer doesn't exist or is empty.
    pub async fn get_buffer_for_persistence(&self, session_id: Uuid) -> Option<(Vec<u8>, u64, u64)> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).and_then(|b| {
            let (start, end, data) = b.get_all();
            if data.is_empty() {
                None
            } else {
                Some((data, start, end))
            }
        })
    }

    /// Update activity from a hook event. This is the authoritative source for activity state.
    /// Returns the updated activity if successful.
    pub async fn update_from_hook(
        &self,
        session_id: Uuid,
        current_activity: String,
        current_step: Option<String>,
        new_action: Option<RecentAction>,
        is_busy: bool,
    ) -> Option<SessionActivity> {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        // Update activity state
        buffer.activity.current_activity = current_activity;
        buffer.activity.current_step = current_step.clone();
        buffer.activity.is_busy = is_busy;
        buffer.activity.last_update = std::time::Instant::now();

        // Track that we've seen activity if we're busy and this is a tool use
        if is_busy && current_step.as_ref().map(|s| s != "Thinking" && s != "Ready").unwrap_or(false) {
            buffer.activity.saw_activity_since_busy = true;
            buffer.activity.last_activity_indicator = std::time::Instant::now();
            buffer.activity.bytes_since_activity = 0;
        }

        // Update busy tracking
        if is_busy && buffer.activity.busy_since.is_none() {
            buffer.activity.busy_since = Some(std::time::Instant::now());
        } else if !is_busy {
            buffer.activity.busy_since = None;
            buffer.activity.saw_activity_since_busy = false;
        }

        // Add new action if provided
        if let Some(action) = new_action {
            // Deduplicate - don't add if we already have this exact action recently
            let already_exists = buffer.activity.recent_actions.iter().any(|a| {
                a.action_type == action.action_type && a.summary == action.summary
            });

            if !already_exists {
                buffer.activity.recent_actions.push(action);
                while buffer.activity.recent_actions.len() > MAX_RECENT_ACTIONS {
                    buffer.activity.recent_actions.remove(0);
                }
            }
        }

        tracing::debug!(
            target: "clauset::hooks",
            "Updated activity for session {}: step={:?}, busy={}, actions={}",
            session_id,
            buffer.activity.current_step,
            buffer.activity.is_busy,
            buffer.activity.recent_actions.len()
        );

        Some(buffer.activity.clone())
    }

    /// Update context window size from hook data.
    ///
    /// Hooks provide context_window_size and total token counts.
    /// When current_usage is available, we use it to compute context percentage
    /// (matches Claude's statusline behavior).
    pub async fn update_context_from_hook(
        &self,
        session_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
        context_window_size: u64,
        current_usage: Option<CurrentUsage>,
        model: Option<String>,
    ) -> Option<SessionActivity> {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        let mut changed = false;
        let use_status_line = buffer.activity.status_line_seen;

        // Mark that we've received hook context
        if !buffer.activity.hook_context_received {
            buffer.activity.hook_context_received = true;
        }

        // Update model if provided
        if let Some(ref m) = model {
            if buffer.activity.model != *m {
                buffer.activity.model = m.clone();
                changed = true;
            }
        }

        // Always store context_window_size (needed for percentage calculation)
        if buffer.activity.context_window_size != context_window_size && context_window_size > 0 {
            buffer.activity.context_window_size = context_window_size;
            changed = true;
        }

        // Update token counts from hooks unless status line parsing is active.
        if !use_status_line {
            // This handles /clear scenarios where tokens reset to low values or zero
            if buffer.activity.input_tokens != input_tokens {
                buffer.activity.input_tokens = input_tokens;
                changed = true;
            }
            if buffer.activity.output_tokens != output_tokens {
                buffer.activity.output_tokens = output_tokens;
                changed = true;
            }
        }

        // Recalculate context percentage (prefer current usage from hooks).
        let effective_window = if buffer.activity.context_window_size > 0 {
            buffer.activity.context_window_size
        } else {
            context_window_size
        };
        if effective_window > 0 && !use_status_line {
            if let Some(usage) = current_usage {
                let current_context =
                    usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens;
                let new_context_percent =
                    ((current_context.saturating_mul(100)) / effective_window).min(100) as u8;
                if buffer.activity.context_percent != new_context_percent {
                    buffer.activity.context_percent = new_context_percent;
                    changed = true;
                }
            } else if input_tokens == 0 && output_tokens == 0 && buffer.activity.context_percent != 0 {
                buffer.activity.context_percent = 0;
                changed = true;
            }
        }

        if changed {
            buffer.activity.last_update = std::time::Instant::now();
            tracing::debug!(
                target: "clauset::hooks",
                "Context updated from hook for session {}: model='{}', {}in/{}out, window={}, ctx={}%{}",
                session_id,
                buffer.activity.model,
                buffer.activity.input_tokens,
                buffer.activity.output_tokens,
                buffer.activity.context_window_size,
                buffer.activity.context_percent,
                if buffer.activity.transcript_usage_received { " (transcript active)" } else { "" }
            );
            Some(buffer.activity.clone())
        } else {
            None
        }
    }

    /// Accumulate per-message token usage from transcript into cumulative session totals.
    ///
    /// Transcript usage provides per-message token data. We use it as a fallback
    /// when hooks have not provided authoritative totals. Each message contains:
    /// - `input_tokens`: New input tokens (not from cache)
    /// - `cache_read_input_tokens`: Tokens read from cache
    /// - `cache_creation_input_tokens`: Tokens written to cache
    /// - `output_tokens`: Tokens generated by the assistant
    ///
    /// For display, we compute total context as:
    /// - Input context = sum of (input + cache_read + cache_creation) from all messages
    /// - Output context = sum of output_tokens from all messages
    pub async fn accumulate_usage(
        &self,
        session_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
        model: &str,
    ) -> Option<SessionActivity> {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        // Mark that we've received transcript data (authoritative source)
        if !buffer.activity.transcript_usage_received {
            buffer.activity.transcript_usage_received = true;
        }

        let use_status_line = buffer.activity.status_line_seen;

        // Accumulate cache token counts from transcript
        buffer.activity.cache_read_tokens += cache_read_tokens;
        buffer.activity.cache_creation_tokens += cache_creation_tokens;

        // Only use transcript totals when hooks haven't provided authoritative totals.
        if !buffer.activity.hook_context_received && !use_status_line {
            let combined_input = input_tokens + cache_read_tokens + cache_creation_tokens;
            buffer.activity.input_tokens += combined_input;
            buffer.activity.output_tokens += output_tokens;
        }

        // Update model if provided and set default context window size
        if !model.is_empty() {
            buffer.activity.model = model.to_string();

            // Set default context window size if not already set (from hooks)
            if buffer.activity.context_window_size == 0 {
                buffer.activity.context_window_size = default_context_window_for_model(model);
            }
        }

        // Recalculate context percentage (only if hooks haven't provided current usage)
        if buffer.activity.context_window_size > 0 && !buffer.activity.hook_context_received && !use_status_line {
            let total_tokens = buffer.activity.input_tokens + buffer.activity.output_tokens;
            buffer.activity.context_percent =
                ((total_tokens as f64 / buffer.activity.context_window_size as f64) * 100.0)
                    .min(100.0) as u8;
        }

        buffer.activity.last_update = std::time::Instant::now();

        tracing::info!(
            target: "clauset::transcript",
            "Token usage from transcript for session {}: +{}in/{}out (total: {}in/{}out, ctx: {}%)",
            session_id,
            input_tokens,
            output_tokens,
            buffer.activity.input_tokens,
            buffer.activity.output_tokens,
            buffer.activity.context_percent,
        );

        Some(buffer.activity.clone())
    }

    /// Update permission mode for a session.
    pub async fn update_permission_mode(&self, session_id: Uuid, mode: PermissionMode) -> bool {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);

        if buffer.activity.permission_mode == Some(mode) {
            return false;
        }

        buffer.activity.permission_mode = Some(mode);
        buffer.activity.last_update = std::time::Instant::now();
        true
    }
}

/// Comprehensive regex for ANSI escape sequences.
/// Matches:
/// - CSI sequences: ESC [ ... letter (colors, cursor, etc.)
/// - OSC sequences: ESC ] ... BEL or ESC \ (window title, etc.)
/// - Character set: ESC ( or ESC ) followed by character
/// - Other escapes: ESC = ESC > ESC M etc.
static ANSI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\x1b\[[0-9;?]*[A-Za-z]",    // CSI sequences (colors, cursor, etc.)
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

/// Parsed status line info.
struct ParsedStatus {
    model: String,
    cost: f64,
    input_tokens: u64,
    output_tokens: u64,
    context_percent: u8,
}

/// Regex for full status line: "Model | $Cost | Input/Output | ctx:X%"
/// K suffix is optional since Claude Code omits it for small values.
/// The ctx:X% suffix helps distinguish from false positives like "804/993 files".
static STATUS_LINE_FULL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^([A-Za-z][A-Za-z0-9.\- ]*?)\s*\|\s*\$([0-9.]+)\s*(?:\|\s*([0-9.]+)([kKmM]?)/([0-9.]+)([kKmM]?)\s*)?(?:\|\s*ctx:(\d+)%!?)?"
    ).unwrap()
});

/// Regex for continuation line with tokens: "Input/Output | ctx:X%"
/// K suffix is optional. Requires ctx suffix to prevent false positives.
static STATUS_LINE_TOKENS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([0-9.]+)([kKmM]?)/([0-9.]+)([kKmM]?)\s*\|\s*ctx:(\d+)%!?").unwrap()
});

/// Regex for continuation line with tokens but no ctx suffix.
/// Used when ctx appears on a separate line.
static STATUS_LINE_TOKENS_NO_CTX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([0-9.]+)([kKmM]?)/([0-9.]+)([kKmM]?)\s*\|?\s*$").unwrap()
});

/// Regex for a standalone ctx line like "ctx:19%".
static STATUS_LINE_CTX_ONLY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^ctx:(\d+)%!?\b.*$").unwrap()
});

const MAX_REASONABLE_TOKENS: u64 = 10_000_000;

/// Regex for model and cost pattern (allows trailing text like "Update available!")
/// Used when tokens are on a separate line or when there's trailing notifications
static STATUS_LINE_MODEL_COST: Lazy<Regex> = Lazy::new(|| {
    // Match "Model | $Cost" optionally followed by " |" but allow any trailing text
    // The \| at the end is optional and indicates tokens may follow (on same or next line)
    Regex::new(r"^([A-Za-z][A-Za-z0-9.\- ]*?)\s*\|\s*\$([0-9.]+)\s*\|?").unwrap()
});

/// Regex for detecting permission mode in terminal output.
/// Claude Code shows modes like:
/// - "plan mode on (shift+tab to cycle)"
/// - "accept edits on (shift+tab to cycle)"
/// - "bypass permissions on (shift+tab to cycle)"
/// - "Mode: Accept edits"
/// - "Permission mode: Plan"
static PERMISSION_MODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:mode|permission(?:s)?(?:\s+mode)?)\b[:\s-]*)?(default|normal|accept edits?|accept_edits|acceptedits|bypass permissions?|bypass_permissions|bypasspermissions|plan(?: mode)?)\b(?:\s+(?:on|mode)\b)?"
    ).unwrap()
});

fn parse_tokens_with_suffix(value: Option<regex::Match>, suffix: Option<regex::Match>) -> u64 {
    let num = value
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);
    let suffix = suffix.map(|m| m.as_str()).unwrap_or("");
    if suffix.eq_ignore_ascii_case("k") {
        (num * 1000.0) as u64
    } else if suffix.eq_ignore_ascii_case("m") {
        (num * 1_000_000.0) as u64
    } else {
        num.round() as u64
    }
}

fn parse_permission_mode(text: &str) -> Option<PermissionMode> {
    let lines: Vec<&str> = text.lines().collect();

    for line in lines.iter().rev().take(50) {
        let clean = strip_ansi_codes(line).trim().to_string();
        if clean.is_empty() || clean.len() > 120 {
            continue;
        }

        if let Some(mode) = PermissionMode::from_hook_value(&clean) {
            return Some(mode);
        }

        if let Some(caps) = PERMISSION_MODE_RE.captures(&clean) {
            if let Some(raw) = caps.get(1) {
                if let Some(mode) = PermissionMode::from_hook_value(raw.as_str()) {
                    return Some(mode);
                }
            }
        }
    }

    None
}

/// Parse Claude's status line format, handling multi-line wrapping.
///
/// The status line can appear in several formats:
/// - Full: "Opus 4.5 | $0.68 | 29.2K/22.5K | ctx:11%"
/// - With trailing text: "Haiku 4.5 | $0.06 |     Update available!"
/// - Wrapped (narrow terminal):
///   Line 1: "Haiku 4.5 | $0.07 |"
///   Line 2: "2.4K/1.2K | ctx:21%"
fn parse_status_line(text: &str) -> Option<ParsedStatus> {
    let lines: Vec<&str> = text.lines().collect();

    // Search from the end (status line is at bottom)
    for (i, line) in lines.iter().enumerate().rev().take(50) {
        let trimmed = line.trim();

        // Skip empty or code-like lines
        if trimmed.is_empty() || trimmed.len() > 100 {
            continue;
        }
        if trimmed.contains('"') || trimmed.contains(';') || trimmed.starts_with("//") {
            continue;
        }

        // Try full status line pattern
        if let Some(caps) = STATUS_LINE_FULL.captures(trimmed) {
            let model = caps.get(1)?.as_str().trim().to_string();
            let cost: f64 = caps.get(2)?.as_str().parse().ok()?;
            let input_tokens = parse_tokens_with_suffix(caps.get(3), caps.get(4));
            let output_tokens = parse_tokens_with_suffix(caps.get(5), caps.get(6));
            let context: u8 = caps.get(7).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let has_ctx = caps.get(7).is_some();
            let has_tokens = caps.get(3).is_some();
            let has_suffix = caps
                .get(4)
                .map(|m| !m.as_str().is_empty())
                .unwrap_or(false)
                || caps.get(6).map(|m| !m.as_str().is_empty()).unwrap_or(false);

            // If tokens are present but ctx and suffixes are missing, ignore as likely false positive.
            if has_tokens && !has_ctx && !has_suffix {
                continue;
            }

            // Sanity check: reject obvious false positives from accidental pattern matches
            if input_tokens > MAX_REASONABLE_TOKENS || output_tokens > MAX_REASONABLE_TOKENS {
                continue;
            }

            return Some(ParsedStatus {
                model,
                cost,
                input_tokens,
                output_tokens,
                context_percent: context,
            });
        }

        // Try model+cost only pattern (might be first line of wrapped status)
        if let Some(caps) = STATUS_LINE_MODEL_COST.captures(trimmed) {
            let model = caps.get(1)?.as_str().trim().to_string();
            let cost: f64 = caps.get(2)?.as_str().parse().ok()?;

            // Check if next line has tokens/context (wrapped status)
            let (input_tokens, output_tokens, context) = if i + 1 < lines.len() {
                let next_line = lines[i + 1].trim();
                if let Some(token_caps) = STATUS_LINE_TOKENS.captures(next_line) {
                    let ink = parse_tokens_with_suffix(token_caps.get(1), token_caps.get(2));
                    let outk = parse_tokens_with_suffix(token_caps.get(3), token_caps.get(4));
                    let ctx: u8 = token_caps.get(5).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    (ink, outk, ctx)
                } else if let Some(token_caps) = STATUS_LINE_TOKENS_NO_CTX.captures(next_line) {
                    let ink = parse_tokens_with_suffix(token_caps.get(1), token_caps.get(2));
                    let outk = parse_tokens_with_suffix(token_caps.get(3), token_caps.get(4));
                    let ctx = if i + 2 < lines.len() {
                        let ctx_line = lines[i + 2].trim();
                        STATUS_LINE_CTX_ONLY
                            .captures(ctx_line)
                            .and_then(|caps| caps.get(1))
                            .and_then(|m| m.as_str().parse().ok())
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    (ink, outk, ctx)
                } else {
                    (0, 0, 0)
                }
            } else {
                (0, 0, 0)
            };

            // Sanity check: reject obvious false positives from accidental pattern matches
            if input_tokens > MAX_REASONABLE_TOKENS || output_tokens > MAX_REASONABLE_TOKENS {
                continue;
            }

            return Some(ParsedStatus {
                model,
                cost,
                input_tokens,
                output_tokens,
                context_percent: context,
            });
        }

        // Try tokens-only line (second line of wrapped status, search backwards for model)
        if let Some(token_caps) = STATUS_LINE_TOKENS.captures(trimmed)
            .or_else(|| STATUS_LINE_TOKENS_NO_CTX.captures(trimmed))
        {
            let input_tokens = parse_tokens_with_suffix(token_caps.get(1), token_caps.get(2));
            let output_tokens = parse_tokens_with_suffix(token_caps.get(3), token_caps.get(4));
            let mut context: u8 = token_caps.get(5).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            if context == 0 && i + 1 < lines.len() {
                let ctx_line = lines[i + 1].trim();
                context = STATUS_LINE_CTX_ONLY
                    .captures(ctx_line)
                    .and_then(|caps| caps.get(1))
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
            }

            // Sanity check: reject obvious false positives from accidental pattern matches
            if input_tokens > MAX_REASONABLE_TOKENS || output_tokens > MAX_REASONABLE_TOKENS {
                continue;
            }

            // Look backwards for model+cost line
            if i > 0 {
                let prev_line = lines[i - 1].trim();
                if let Some(model_caps) = STATUS_LINE_MODEL_COST.captures(prev_line) {
                    let model = model_caps.get(1)?.as_str().trim().to_string();
                    let cost: f64 = model_caps.get(2)?.as_str().parse().ok()?;

                    return Some(ParsedStatus {
                        model,
                        cost,
                        input_tokens,
                        output_tokens,
                        context_percent: context,
                    });
                }
            }
        }
    }

    None
}

/// Get current timestamp in milliseconds
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Parse current activity and status from terminal output.
/// Returns (current_activity, current_step, empty vec)
///
/// NOTE: This function NO LONGER creates actions from terminal parsing.
/// Actions are now exclusively created from hook events (hooks.rs), which
/// provide accurate, structured data. Terminal parsing is only used for:
/// - Status line extraction (model, cost, tokens, context %) - done in parse_status_line
/// - Ready/Thinking state detection (this function)
///
/// KEY INSIGHT: Status is determined by RECENCY - whatever meaningful status
/// indicator appeared MOST RECENTLY is the current state. We iterate from
/// newest to oldest and return on FIRST match.
///
/// The key challenge is distinguishing:
/// - `>` as the actual input prompt (Claude waiting for input) → Ready
/// - `>` appearing in file contents (markdown blockquote, etc.) → ignore
///
/// Solution: When we find a potential `>` prompt, we do a quick look-ahead
/// (further back in the buffer) to check if there's a tool header nearby.
/// If there is, this `>` is likely file output from that tool, not the prompt.
fn parse_activity_and_action(text: &str) -> Option<(String, Option<String>, Vec<RecentAction>)> {
    let lines: Vec<&str> = text.lines().collect();

    let mut current_status: Option<(String, String)> = None; // (activity, step)

    // RECENCY-BASED APPROACH with special handling for Claude Code's UI:
    //
    // Claude Code's UI shows `>` prompt at the bottom even while thinking/working.
    // The key insight is:
    // - When Claude FINISHES: there's prose/output BETWEEN activity indicator and `>`
    // - When Claude is WORKING: `>` appears directly after activity (only empty lines/status between)
    //
    // Strategy:
    // 1. Find the position of `>` prompt (if any)
    // 2. Find the position of the most recent activity indicator
    // 3. Check if there's meaningful output BETWEEN them
    // 4. If meaningful output exists between activity and `>`, then `>` means Ready
    // 5. If `>` appears directly after activity (no meaningful content), activity is current

    // Helper: check if line looks like the `>` prompt
    let is_prompt_line = |line: &str| -> bool {
        let trimmed = line.trim();
        let is_short = trimmed.len() < 80;
        let is_not_indented = !line.starts_with("  ") && !line.starts_with("\t");

        if (trimmed == ">" || trimmed.starts_with("> ")) && is_short && is_not_indented {
            // Verify it's not file content
            if trimmed.len() > 2 {
                let after = &trimmed[2..];
                let is_file = after.contains('│') || after.contains('└') ||
                              after.contains('├') || after.contains('─') ||
                              after.starts_with('#') || after.starts_with("//") ||
                              after.starts_with("/*") ||
                              (after.len() > 50 && !after.contains('?'));
                return !is_file;
            }
            return true;
        }
        false
    };

    // Helper: check if line is meaningful content (not just status/chrome/empty)
    let is_meaningful_content = |line: &str| -> bool {
        let clean = strip_ansi_codes(line.trim());
        !clean.is_empty() &&
        !clean.contains("ctx:") &&
        !clean.contains("| $") &&
        !is_ui_chrome(&clean) &&
        clean.len() >= 3
    };

    // Find position of `>` prompt in the last 15 lines
    let mut prompt_pos: Option<usize> = None;
    for (i, line) in lines.iter().rev().take(15).enumerate() {
        let clean_line = strip_ansi_codes(line.trim());
        if is_prompt_line(&clean_line) {
            prompt_pos = Some(i);
            break;
        }
    }

    // Find position and type of most recent activity indicator in last 100 lines
    // This needs to be large enough to catch thinking status even when Claude
    // outputs verbose responses (which push the thinking indicator up the buffer)
    let mut activity_pos: Option<usize> = None;
    let mut activity_type: Option<(String, String)> = None;

    for (i, line) in lines.iter().rev().take(100).enumerate() {
        let clean_line = strip_ansi_codes(line.trim());
        let clean_lower = clean_line.to_lowercase();

        // Skip non-meaningful lines
        if !is_meaningful_content(line) {
            continue;
        }

        // Skip prompt lines
        if is_prompt_line(&clean_line) {
            continue;
        }

        // Check for thinking/planning status
        if is_thinking_status_line(&clean_line, &clean_lower) {
            activity_pos = Some(i);
            if clean_lower.contains("planning") {
                activity_type = Some(("Planning...".to_string(), "Planning".to_string()));
            } else {
                activity_type = Some(("Thinking...".to_string(), "Thinking".to_string()));
            }
            break;
        }

        // Check for tool invocations
        if let Some((activity, step, _)) = parse_tool_activity_flexible(&clean_line, &clean_lower) {
            activity_pos = Some(i);
            activity_type = Some((activity, step.unwrap_or_default()));
            break;
        }

        // Check for "Actioning" - this means Claude is generating a response (Ready)
        if is_status_indicator(&clean_line) && clean_lower.contains("actioning") {
            activity_pos = Some(i);
            activity_type = Some(("Ready".to_string(), "Ready".to_string()));
            break;
        }
    }

    // Check if prompt has user input (not just empty `>`)
    let prompt_has_user_input = prompt_pos.map(|p_pos| {
        lines.iter().rev().nth(p_pos).map(|line| {
            let clean = strip_ansi_codes(line.trim());
            // `> something` means user has typed, not just empty prompt
            clean.len() > 1 && clean.starts_with("> ")
        }).unwrap_or(false)
    }).unwrap_or(false);

    // Count meaningful lines in the buffer (excluding status/chrome/prompt)
    let meaningful_line_count: usize = lines.iter().rev().take(50).filter(|line| {
        let clean = strip_ansi_codes(line.trim());
        is_meaningful_content(line) && !is_prompt_line(&clean)
    }).count();

    // Determine current status based on positions
    match (prompt_pos, activity_pos, activity_type) {
        // Both prompt and activity found
        (Some(p_pos), Some(a_pos), Some(activity)) => {
            if p_pos < a_pos {
                // Prompt is more recent (closer to end) than activity
                // If prompt has user input, Claude is definitely ready
                if prompt_has_user_input {
                    current_status = Some(("Ready".to_string(), "Ready".to_string()));
                } else {
                    // Empty prompt - check if there's meaningful content between them
                    let mut has_content_between = false;
                    for (i, line) in lines.iter().rev().take(100).enumerate() {
                        if i <= p_pos {
                            continue; // Skip lines at or after prompt
                        }
                        if i >= a_pos {
                            break; // Stop at activity indicator
                        }
                        let clean_line = strip_ansi_codes(line.trim());
                        // Check for meaningful prose/output (not just status lines or chrome)
                        if is_meaningful_content(line) &&
                           !is_prompt_line(&clean_line) &&
                           !is_thinking_status_line(&clean_line, &clean_line.to_lowercase()) &&
                           parse_tool_activity_flexible(&clean_line, &clean_line.to_lowercase()).is_none() {
                            has_content_between = true;
                            break;
                        }
                    }

                    if has_content_between {
                        // There's output between activity and prompt - Claude finished
                        current_status = Some(("Ready".to_string(), "Ready".to_string()));
                    } else {
                        // No output between - Claude is still working (prompt is just UI chrome)
                        current_status = Some(activity);
                    }
                }
            } else {
                // Activity is more recent than prompt - show activity
                current_status = Some(activity);
            }
        }
        // Only prompt found, no activity - but check if buffer is minimal
        (Some(_), None, _) => {
            // If buffer has very few meaningful lines, Claude might still be initializing/processing
            // Don't assume Ready - show Processing instead
            if meaningful_line_count <= 3 {
                current_status = Some(("Processing...".to_string(), "Thinking".to_string()));
            } else if prompt_has_user_input {
                // Has user input, so user has typed and Claude is ready
                current_status = Some(("Ready".to_string(), "Ready".to_string()));
            } else {
                // Buffer has content but no clear activity - assume Ready
                current_status = Some(("Ready".to_string(), "Ready".to_string()));
            }
        }
        // Only activity found, no prompt
        (None, Some(_), Some(activity)) => {
            current_status = Some(activity);
        }
        // Nothing found - if buffer is minimal, might be processing
        _ => {
            if meaningful_line_count <= 3 && lines.len() > 0 {
                current_status = Some(("Processing...".to_string(), "Thinking".to_string()));
            }
        }
    }

    // NOTE: Actions are NO LONGER created from terminal parsing.
    // Hooks (hooks.rs) now provide the authoritative source for tool actions,
    // which gives us accurate, structured data instead of parsing terminal output.
    // This eliminates issues like "Write r," or "Read 535" from malformed parsing.
    let found_actions: Vec<RecentAction> = Vec::new();

    // Return status with empty actions vec (actions come from hooks)
    if let Some((activity, step)) = current_status {
        return Some((activity, Some(step), found_actions));
    }

    // No status found - return None to let hooks/frontend handle it
    None
}

/// Check if a line looks like a status indicator (starts with status prefixes)
/// This helps distinguish "* Thinking..." from prose like "I'm thinking about..."
fn is_status_indicator(line: &str) -> bool {
    let trimmed = line.trim();

    // Status lines typically start with these characters
    if trimmed.starts_with('*')
        || trimmed.starts_with('●')
        || trimmed.starts_with('•')
        || trimmed.starts_with('○')
        || trimmed.starts_with('◐')
        || trimmed.starts_with('◑')
        || trimmed.starts_with('◒')
        || trimmed.starts_with('◓')
    {
        return true;
    }

    // Spinner characters (braille patterns used by CLI spinners)
    let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏',
                         '⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
    if let Some(first_char) = trimmed.chars().next() {
        if spinner_chars.contains(&first_char) {
            return true;
        }
    }

    // Short lines that start with key status words are likely status indicators
    if trimmed.len() < 50 {
        let lower = trimmed.to_lowercase();
        if lower.starts_with("thinking")
            || lower.starts_with("actualizing")
            || lower.starts_with("mustering")
            || lower.starts_with("planning")
            || lower.starts_with("actioning")
        {
            return true;
        }
    }

    false
}

/// Check if a line is a "thinking" status indicator (not prose containing the word "thinking")
fn is_thinking_status_line(line: &str, line_lower: &str) -> bool {
    // Must contain one of the thinking keywords
    let has_thinking_keyword = (line_lower.contains("thinking") && !line_lower.contains("thinking about"))
        || line_lower.contains("actualizing")
        || line_lower.contains("mustering")
        || line_lower.contains("planning")
        || line_lower.contains("philosophising")
        || line_lower.contains("philosophizing")
        || line_lower.contains("pondering")
        || line_lower.contains("considering")
        || line_lower.contains("reasoning")
        || line_lower.contains("reflecting");

    if !has_thinking_keyword {
        return false;
    }

    // Must look like a status line, not prose
    // Option 1: Starts with status indicator prefix
    if is_status_indicator(line) {
        return true;
    }

    // Option 2: Very short line (status lines are typically brief)
    if line.len() < 50 {
        return true;
    }

    // Option 3: Contains timing info like "(2s elapsed)" or "(thinking)"
    if line.contains("elapsed") || line.contains("(thinking)") || line.contains("...") {
        return true;
    }

    false
}

/// Check if a line looks like UI chrome (borders, spinners, etc.) that we should skip
fn is_ui_chrome(line: &str) -> bool {
    // Skip lines that are just box-drawing characters
    if line.chars().all(|c| "─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋═║╔╗╚╝╠╣╦╩╬▀▄█▌▐░▒▓ ".contains(c)) {
        return true;
    }

    // Skip lines that look like progress spinners
    if line.len() < 5 && (line.contains('⠋') || line.contains('⠙') || line.contains('⠹')
        || line.contains('⠸') || line.contains('⠼') || line.contains('⠴')
        || line.contains('⠦') || line.contains('⠧') || line.contains('⠇') || line.contains('⠏')) {
        return true;
    }

    // Skip common UI separators
    if line.chars().all(|c| c == '-' || c == '=' || c == '_' || c == ' ') {
        return true;
    }

    false
}

/// Pre-compiled regex for tool invocation pattern: ToolName(args) or ● ToolName(args)
static TOOL_INVOCATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Match tool invocations like "● Bash(git status)" or "Read(file.txt)"
    // The pattern allows for any bullet/symbol prefix, then captures tool name and args
    Regex::new(r"^[●•\-\*\s]*\s*(Read|Edit|Write|Bash|Grep|Glob|Task|Search|WebFetch|WebSearch|TodoWrite|NotebookEdit)\s*[\(:]?\s*(.*)$").unwrap()
});

/// More flexible tool activity parsing that matches Claude Code's actual output.
fn parse_tool_activity_flexible(line: &str, line_lower: &str) -> Option<(String, Option<String>, Option<RecentAction>)> {
    // Skip lines that are too long (likely prose or file contents)
    if line.len() > 300 || line.len() < 3 {
        return None;
    }

    // Skip clear prose patterns (only at the very start)
    let skip_prefixes = [
        "i'll ", "i will ", "i've ", "i have ", "i'm ", "i am ",
        "let me ", "let's ", "now i ", "now let ",
        "this is ", "this will ", "that ", "the ",
        "here's ", "here is ", "sure,", "okay,", "yes,", "no,",
    ];
    for prefix in skip_prefixes {
        if line_lower.starts_with(prefix) {
            return None;
        }
    }

    // Skip user input prompts (lines starting with "> ") but not tool outputs (lines with "└")
    if line.starts_with("> ") && !line.contains("└") {
        return None;
    }

    let ts = now_ms();

    // === PRIMARY PATTERN: ToolName(args) format used by Claude Code ===
    // This matches lines like:
    //   ● Bash(git status)
    //   ● Read(README.md)
    //   ● Search(pattern: "*.md", path: "/foo")
    //   Read /path/to/file
    if let Some(caps) = TOOL_INVOCATION_REGEX.captures(line) {
        let tool_name = caps.get(1)?.as_str();
        let args_raw = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Clean up the args - remove surrounding parens/colons and trailing paren
        let args = args_raw
            .trim()
            .trim_start_matches(['(', ':'])
            .trim_end_matches(')')
            .trim();

        let action_type = match tool_name {
            "Read" => "read",
            "Edit" => "edit",
            "Write" => "write",
            "Bash" => "bash",
            "Grep" | "Glob" | "Search" => "search",
            "Task" => "task",
            "WebFetch" | "WebSearch" => "web",
            "TodoWrite" => "task",
            "NotebookEdit" => "edit",
            _ => "task",
        };

        // Create a nice summary
        let summary = if !args.is_empty() {
            // For file operations, show the filename
            let display_arg = if args.contains('/') {
                args.split('/').last().unwrap_or(args)
            } else {
                args
            };
            // Truncate and clean up
            let display_arg = display_arg.split_whitespace().next().unwrap_or(display_arg);
            format!("{} {}", tool_name, truncate_str(display_arg, 25))
        } else {
            tool_name.to_string()
        };

        return Some((
            summary.clone(),
            Some(tool_name.to_string()),
            Some(RecentAction {
                action_type: action_type.to_string(),
                summary,
                detail: if args.is_empty() { None } else { Some(truncate_str(args, 70)) },
                timestamp: ts,
            }),
        ));
    }

    // === BASH / COMMAND DETECTION ===
    // Look for command prompts with `$ ` prefix (Claude running shell commands)
    if line.starts_with("$ ") {
        let cmd = line[2..].trim();
        if !cmd.is_empty() && cmd.len() < 100 && looks_like_shell_command(cmd) {
            return Some((
                format!("$ {}", truncate_str(cmd, 40)),
                Some("Bash".to_string()),
                Some(RecentAction {
                    action_type: "bash".to_string(),
                    summary: "Ran command".to_string(),
                    detail: Some(truncate_str(cmd, 80)),
                    timestamp: ts,
                }),
            ));
        }
    }

    // Common command starters (without prompt symbol)
    let cmd_starters = ["cargo ", "npm ", "pnpm ", "yarn ", "git ", "cd ", "ls ", "cat ",
                        "grep ", "find ", "mkdir ", "rm ", "cp ", "mv ", "make ", "python ",
                        "node ", "rustc ", "gcc ", "go ", "docker ", "kubectl "];
    for starter in cmd_starters {
        if line_lower.starts_with(starter) && line.len() < 120 {
            return Some((
                format!("$ {}", truncate_str(line, 40)),
                Some("Bash".to_string()),
                Some(RecentAction {
                    action_type: "bash".to_string(),
                    summary: format!("Ran {}", starter.trim()),
                    detail: Some(truncate_str(line, 80)),
                    timestamp: ts,
                }),
            ));
        }
    }

    // === FALLBACK: Strip leading symbols and check for tool verbs ===
    // This handles cases where bullets/symbols don't match exactly
    let stripped = line.trim_start_matches(|c: char| !c.is_alphanumeric());
    let stripped_lower = stripped.to_lowercase();

    let tool_verbs = [
        ("Read", "read"),
        ("Edit", "edit"),
        ("Write", "write"),
        ("Bash", "bash"),
        ("Grep", "search"),
        ("Glob", "search"),
        ("Task", "task"),
        ("Search", "search"),
        ("WebFetch", "web"),
        ("WebSearch", "web"),
    ];

    for (verb, action_type) in tool_verbs {
        if stripped.starts_with(verb) && stripped.len() > verb.len() {
            let next_char = stripped.chars().nth(verb.len());
            if next_char == Some(' ') || next_char == Some(':') || next_char == Some('(') {
                let detail = stripped[verb.len()..].trim();
                let detail = detail
                    .trim_start_matches([':', '(', ' '])
                    .trim_end_matches(')')
                    .trim();

                let summary = if !detail.is_empty() {
                    let filename = detail.split('/').last().unwrap_or(detail);
                    let filename = filename.split_whitespace().next().unwrap_or(filename);
                    format!("{} {}", verb, truncate_str(filename, 25))
                } else {
                    verb.to_string()
                };

                return Some((
                    summary.clone(),
                    Some(verb.to_string()),
                    Some(RecentAction {
                        action_type: action_type.to_string(),
                        summary,
                        detail: if detail.is_empty() { None } else { Some(truncate_str(detail, 70)) },
                        timestamp: ts,
                    }),
                ));
            }
        }
    }

    // === Progress indicators ===
    let progress_patterns = [
        ("reading ", "Read", "read"),
        ("editing ", "Edit", "edit"),
        ("writing ", "Write", "write"),
        ("searching ", "Search", "search"),
        ("running ", "Bash", "bash"),
        ("executing ", "Bash", "bash"),
        ("fetching ", "Web", "web"),
    ];

    for (pattern, step, action_type) in progress_patterns {
        if stripped_lower.starts_with(pattern) {
            let detail = &stripped[pattern.len()..];
            return Some((
                format!("{}...", step),
                Some(step.to_string()),
                Some(RecentAction {
                    action_type: action_type.to_string(),
                    summary: format!("{}...", step),
                    detail: if detail.trim().is_empty() { None } else { Some(truncate_str(detail.trim(), 70)) },
                    timestamp: ts,
                }),
            ));
        }
    }

    // === BUILD/TEST OUTPUT DETECTION ===
    let build_patterns = [
        ("compiling ", "build", "Building"),
        ("building ", "build", "Building"),
        ("testing ", "test", "Testing"),
        ("installing ", "install", "Installing"),
        ("downloading ", "download", "Downloading"),
        ("error[", "error", "Error"),
        ("warning:", "warning", "Warning"),
        ("finished ", "build", "Finished"),
    ];

    for (pattern, action_type, display) in build_patterns {
        if line_lower.contains(pattern) {
            return Some((
                format!("{}...", display),
                Some(display.to_string()),
                Some(RecentAction {
                    action_type: action_type.to_string(),
                    summary: display.to_string(),
                    detail: Some(truncate_str(line, 60)),
                    timestamp: ts,
                }),
            ));
        }
    }

    None
}

/// Truncate a string to a maximum length with ellipsis.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Check if a string looks like an actual shell command rather than user instruction.
fn looks_like_shell_command(cmd: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();

    // Known command starters
    let shell_commands = [
        "cargo", "npm", "pnpm", "yarn", "git", "cd", "ls", "cat", "grep", "find",
        "mkdir", "rm", "cp", "mv", "make", "python", "node", "rustc", "gcc", "go",
        "docker", "kubectl", "brew", "apt", "pip", "npx", "bunx", "deno", "bun",
        "echo", "export", "source", "curl", "wget", "ssh", "scp", "rsync",
        "chmod", "chown", "tar", "zip", "unzip", "touch", "head", "tail", "sed", "awk",
    ];

    // Check if starts with a known command
    let first_word = cmd_lower.split_whitespace().next().unwrap_or("");
    if shell_commands.iter().any(|&c| first_word == c) {
        return true;
    }

    // Check for absolute paths being executed
    if first_word.starts_with('/') || first_word.starts_with("./") {
        return true;
    }

    // Check for pipes, redirections, or other shell operators
    if cmd.contains(" | ") || cmd.contains(" > ") || cmd.contains(" >> ")
       || cmd.contains(" && ") || cmd.contains(" || ") {
        return true;
    }

    // If it contains spaces and doesn't start with a capital letter or common prose words,
    // it might be a command
    if !cmd_lower.starts_with("please") && !cmd_lower.starts_with("can you")
       && !cmd_lower.starts_with("help") && !cmd_lower.starts_with("fix")
       && !cmd_lower.starts_with("add") && !cmd_lower.starts_with("create")
       && !cmd_lower.starts_with("update") && !cmd_lower.starts_with("change")
       && !cmd_lower.starts_with("make") && !cmd_lower.starts_with("implement")
       && !cmd_lower.starts_with("write") && !cmd_lower.starts_with("show")
       && !cmd_lower.starts_with("what") && !cmd_lower.starts_with("how")
       && !cmd_lower.starts_with("why") && !cmd_lower.starts_with("where")
       && !cmd_lower.starts_with("when") && !cmd_lower.starts_with("who")
    {
        // Check if it looks like a path with extension
        if first_word.contains('.') && !first_word.ends_with('.') {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use clauset_types::CurrentUsage;
    use proptest::prelude::*;

    // ========================================================================
    // BASIC UNIT TESTS
    // ========================================================================

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[32mHello\x1b[0m World";
        assert_eq!(strip_ansi_codes(input), "Hello World");
    }

    // ========================================================================
    // SEQUENCED RING BUFFER TESTS
    // ========================================================================

    #[test]
    fn test_buffer_new() {
        let buf = SequencedRingBuffer::new(1024);
        assert_eq!(buf.start_seq(), 0);
        assert_eq!(buf.end_seq(), 0);
        assert_eq!(buf.chunk_count(), 0);
        assert_eq!(buf.total_bytes(), 0);
    }

    #[test]
    fn test_buffer_push_single() {
        let mut buf = SequencedRingBuffer::new(1024);
        let data = vec![1, 2, 3, 4, 5];
        let (seq, evicted) = buf.push(data.clone());

        assert_eq!(seq, 0);
        assert_eq!(evicted, 0);
        assert_eq!(buf.chunk_count(), 1);
        assert_eq!(buf.total_bytes(), 5);
        assert_eq!(buf.start_seq(), 0);
        assert_eq!(buf.end_seq(), 0);
    }

    #[test]
    fn test_buffer_push_multiple() {
        let mut buf = SequencedRingBuffer::new(1024);

        for i in 0..5u64 {
            let (seq, _) = buf.push(vec![i as u8; 10]);
            assert_eq!(seq, i);
        }

        assert_eq!(buf.chunk_count(), 5);
        assert_eq!(buf.total_bytes(), 50);
        assert_eq!(buf.start_seq(), 0);
        assert_eq!(buf.end_seq(), 4);
    }

    #[test]
    fn test_buffer_eviction() {
        // Small buffer that can only hold ~30 bytes
        let mut buf = SequencedRingBuffer::new(30);

        // Push 5 chunks of 10 bytes each = 50 bytes, should evict
        for i in 0..5u64 {
            buf.push(vec![i as u8; 10]);
        }

        // Should have evicted some chunks
        assert!(buf.chunk_count() < 5);
        assert!(buf.total_bytes() <= 30);
        assert!(buf.start_seq() > 0);
    }

    #[test]
    fn test_buffer_sequence_monotonicity() {
        let mut buf = SequencedRingBuffer::new(1024);
        let mut last_seq = 0u64;

        for _ in 0..100 {
            let (seq, _) = buf.push(vec![0; 5]);
            assert!(seq >= last_seq, "Sequence must be monotonically increasing");
            last_seq = seq;
        }
    }

    #[test]
    fn test_buffer_get_range() {
        let mut buf = SequencedRingBuffer::new(1024);

        for i in 0..10u64 {
            buf.push(vec![i as u8; 5]);
        }

        // Get middle range
        let chunks = buf.get_range(3, 7);
        assert_eq!(chunks.len(), 5);
        assert_eq!(chunks[0].seq, 3);
        assert_eq!(chunks[4].seq, 7);
    }

    #[test]
    fn test_buffer_get_range_after_eviction() {
        let mut buf = SequencedRingBuffer::new(50);

        // Push enough to cause eviction
        for i in 0..20u64 {
            buf.push(vec![i as u8; 10]);
        }

        // Try to get evicted range - should return empty
        let early_chunks = buf.get_range(0, 5);
        assert!(early_chunks.is_empty() || early_chunks[0].seq > 5);

        // Get available range
        let available = buf.get_range(buf.start_seq(), buf.end_seq());
        assert!(!available.is_empty());
    }

    #[test]
    fn test_buffer_get_all() {
        let mut buf = SequencedRingBuffer::new(1024);

        buf.push(vec![65, 66, 67]); // ABC
        buf.push(vec![68, 69, 70]); // DEF
        buf.push(vec![71, 72, 73]); // GHI

        let (start, end, data) = buf.get_all();
        assert_eq!(start, 0);
        assert_eq!(end, 2);
        assert_eq!(data, vec![65, 66, 67, 68, 69, 70, 71, 72, 73]);
    }

    #[test]
    fn test_buffer_has_seq() {
        let mut buf = SequencedRingBuffer::new(1024);

        for _ in 0..10 {
            buf.push(vec![0; 5]);
        }

        assert!(buf.has_seq(0));
        assert!(buf.has_seq(5));
        assert!(buf.has_seq(9));
        assert!(!buf.has_seq(10)); // Not yet assigned
        assert!(!buf.has_seq(100)); // Way out of range
    }

    #[test]
    fn test_buffer_clear() {
        let mut buf = SequencedRingBuffer::new(1024);

        for _ in 0..10 {
            buf.push(vec![0; 5]);
        }

        let last_next_seq = buf.next_seq();
        buf.clear();

        assert_eq!(buf.chunk_count(), 0);
        assert_eq!(buf.total_bytes(), 0);
        // start_seq should be updated to maintain monotonicity
        assert_eq!(buf.start_seq(), last_next_seq);
    }

    #[test]
    fn test_buffer_clear_maintains_monotonicity() {
        let mut buf = SequencedRingBuffer::new(1024);

        // Push some chunks
        for _ in 0..5 {
            buf.push(vec![0; 5]);
        }

        buf.clear();

        // Push more chunks - sequence should continue
        let (seq, _) = buf.push(vec![0; 5]);
        assert_eq!(seq, 5); // Should continue from where we left off
    }

    // ========================================================================
    // PROPERTY-BASED TESTS
    // ========================================================================

    proptest! {
        #[test]
        fn prop_sequence_always_monotonic(chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..100), 1..50)) {
            let mut buf = SequencedRingBuffer::new(10000);
            let mut prev_seq = None;

            for chunk in chunks {
                let (seq, _) = buf.push(chunk);

                if let Some(prev) = prev_seq {
                    prop_assert!(seq > prev, "Sequence must always increase: {} !> {}", seq, prev);
                }
                prev_seq = Some(seq);
            }
        }

        #[test]
        fn prop_buffer_never_exceeds_max_bytes(
            max_bytes in 100usize..1000,
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..50), 1..100)
        ) {
            let mut buf = SequencedRingBuffer::new(max_bytes);

            for chunk in chunks {
                buf.push(chunk);
                // Buffer should never exceed max (except for a single chunk that's larger)
                prop_assert!(
                    buf.total_bytes() <= max_bytes || buf.chunk_count() == 1,
                    "Buffer {} bytes exceeds max {} bytes with {} chunks",
                    buf.total_bytes(), max_bytes, buf.chunk_count()
                );
            }
        }

        #[test]
        fn prop_get_range_returns_correct_sequences(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..20), 5..20)
        ) {
            let mut buf = SequencedRingBuffer::new(10000);

            for chunk in &chunks {
                buf.push(chunk.clone());
            }

            // Test that get_range returns correctly ordered chunks
            let start = buf.start_seq();
            let end = buf.end_seq();

            if start <= end {
                let range = buf.get_range(start, end);

                // Check sequences are in order
                for window in range.windows(2) {
                    prop_assert!(window[0].seq < window[1].seq);
                }
            }
        }

        #[test]
        fn prop_eviction_preserves_recent_chunks(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 10..20), 10..50)
        ) {
            let mut buf = SequencedRingBuffer::new(200); // Small buffer
            let mut last_seqs = Vec::new();

            for chunk in chunks {
                let (seq, _) = buf.push(chunk);
                last_seqs.push(seq);
            }

            // The most recent sequence should always be available
            if let Some(&last) = last_seqs.last() {
                prop_assert!(buf.has_seq(last), "Most recent seq {} should always be available", last);
            }
        }

        #[test]
        fn prop_get_all_concatenates_correctly(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..30), 2..10)
        ) {
            let mut buf = SequencedRingBuffer::new(10000);

            // Calculate expected total
            let expected: Vec<u8> = chunks.iter().flatten().copied().collect();

            for chunk in chunks {
                buf.push(chunk);
            }

            let (_, _, actual) = buf.get_all();
            prop_assert_eq!(actual, expected);
        }

        #[test]
        fn prop_has_seq_consistent_with_range(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..20), 1..30)
        ) {
            let mut buf = SequencedRingBuffer::new(500);

            for chunk in chunks {
                buf.push(chunk);
            }

            let start = buf.start_seq();
            let end = buf.end_seq();

            // All sequences in [start, end) should be available
            for seq in start..=end {
                prop_assert!(buf.has_seq(seq), "Seq {} in range [{}, {}] should be available", seq, start, end);
            }

            // Sequences outside range should not be available
            if start > 0 {
                prop_assert!(!buf.has_seq(start - 1), "Seq {} before start should not be available", start - 1);
            }
            prop_assert!(!buf.has_seq(end + 1), "Seq {} after end should not be available", end + 1);
        }
    }

    // ========================================================================
    // ANSI STRIPPING TESTS
    // ========================================================================

    #[test]
    fn test_strip_ansi_comprehensive() {
        // CSI sequences (colors, cursor)
        assert_eq!(strip_ansi_codes("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi_codes("\x1b[1;32mbold green\x1b[0m"), "bold green");
        assert_eq!(strip_ansi_codes("\x1b[?25l"), ""); // Hide cursor
        assert_eq!(strip_ansi_codes("\x1b[H\x1b[2J"), ""); // Clear screen

        // OSC sequences (window title)
        assert_eq!(strip_ansi_codes("\x1b]0;Title\x07text"), "text");

        // Mixed content
        let mixed = "\x1b[32m● \x1b[0mRead\x1b[90m(file.txt)\x1b[0m";
        assert_eq!(strip_ansi_codes(mixed), "● Read(file.txt)");
    }

    proptest! {
        #[test]
        fn prop_strip_ansi_never_increases_length(text in ".*") {
            let stripped = strip_ansi_codes(&text);
            prop_assert!(stripped.len() <= text.len());
        }

        #[test]
        fn prop_strip_ansi_removes_escape(text in ".*") {
            let stripped = strip_ansi_codes(&text);
            prop_assert!(!stripped.contains('\x1b'), "Stripped text should not contain ESC");
        }
    }

    // ========================================================================
    // STATUS LINE PARSING PROPERTY TESTS
    // ========================================================================

    proptest! {
        #[test]
        fn prop_valid_status_lines_parse(
            model in "[A-Za-z][a-z0-9. -]{2,15}",
            cost in 0.0f64..100.0,
            input_k in 0.0f64..500.0,
            output_k in 0.0f64..500.0,
            ctx in 0u8..100
        ) {
            let line = format!(
                "{} | ${:.2} | {:.1}K/{:.1}K | ctx:{}%",
                model.trim(), cost, input_k, output_k, ctx
            );

            if let Some(parsed) = parse_status_line(&line) {
                prop_assert!((parsed.cost - cost).abs() < 0.01);
                prop_assert_eq!(parsed.context_percent, ctx);
            }
        }

        #[test]
        fn prop_code_lines_never_parse_as_status(
            code_line in r#"(let |const |fn |def |class |if |for |while |return ).*"#
        ) {
            let result = parse_status_line(&code_line);
            prop_assert!(result.is_none(), "Code line should not parse as status: {}", code_line);
        }
    }

    // ========================================================================
    // HELPER FUNCTION TESTS
    // ========================================================================

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
        assert_eq!(truncate_str("hi", 5), "hi");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn test_looks_like_shell_command() {
        assert!(looks_like_shell_command("cargo test"));
        assert!(looks_like_shell_command("npm install"));
        assert!(looks_like_shell_command("git status"));
        assert!(looks_like_shell_command("./script.sh"));
        assert!(looks_like_shell_command("echo hello | grep h"));

        assert!(!looks_like_shell_command("please help me"));
        assert!(!looks_like_shell_command("can you fix this"));
        assert!(!looks_like_shell_command("What is happening?"));
    }

    #[test]
    fn test_is_status_indicator() {
        assert!(is_status_indicator("* Thinking..."));
        assert!(is_status_indicator("● Bash(git status)"));
        assert!(is_status_indicator("⠋ Processing..."));
        assert!(is_status_indicator("Thinking..."));

        assert!(!is_status_indicator("I am thinking about this"));
        assert!(!is_status_indicator("The quick brown fox"));
    }

    #[test]
    fn test_is_ui_chrome() {
        assert!(is_ui_chrome("───────────────"));
        assert!(is_ui_chrome("│"));
        assert!(is_ui_chrome("⠋"));
        assert!(is_ui_chrome("═══════════════"));

        assert!(!is_ui_chrome("Hello world"));
        assert!(!is_ui_chrome("● Read(file.txt)"));
    }

    #[test]
    fn test_parse_status_line() {
        // Test full format
        let input = "Opus 4.5 | $0.68 | 29.2K/22.5K | ctx:11%";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.model, "Opus 4.5");
        assert!((status.cost - 0.68).abs() < 0.01);
        assert_eq!(status.input_tokens, 29200);
        assert_eq!(status.output_tokens, 22500);
        assert_eq!(status.context_percent, 11);
    }

    #[test]
    fn test_parse_status_line_partial() {
        // Test minimal format: just model and cost
        let input = "haiku | $0.00";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.model, "haiku");
        assert!((status.cost - 0.0).abs() < 0.01);
        assert_eq!(status.input_tokens, 0);
        assert_eq!(status.output_tokens, 0);
        assert_eq!(status.context_percent, 0);

        // Test with tokens but no context
        let input2 = "sonnet | $0.50 | 5.2K/3.1K";
        let status2 = parse_status_line(input2).unwrap();
        assert_eq!(status2.model, "sonnet");
        assert!((status2.cost - 0.50).abs() < 0.01);
        assert_eq!(status2.input_tokens, 5200);
        assert_eq!(status2.output_tokens, 3100);
        assert_eq!(status2.context_percent, 0);

        // Test with model containing dashes
        let input3 = "opus-4-5 | $1.23 | 10K/8K | ctx:5%";
        let status3 = parse_status_line(input3).unwrap();
        assert_eq!(status3.model, "opus-4-5");
        assert!((status3.cost - 1.23).abs() < 0.01);
        assert_eq!(status3.input_tokens, 10000);
        assert_eq!(status3.output_tokens, 8000);
        assert_eq!(status3.context_percent, 5);
    }

    #[test]
    fn test_parse_status_line_without_k_suffix() {
        let input = "Haiku 4.5 | $0.02 | 12/4 | ctx:1%";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.input_tokens, 12);
        assert_eq!(status.output_tokens, 4);
        assert_eq!(status.context_percent, 1);
    }

    #[test]
    fn test_parse_status_line_mixed_suffixes() {
        let input = "Haiku 4.5 | $0.02 | 1.2k/400 | ctx:1%";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.input_tokens, 1200);
        assert_eq!(status.output_tokens, 400);
    }

    #[test]
    fn test_parse_status_line_m_suffix() {
        let input = "Sonnet 4.5 | $1.00 | 1.2M/850K | ctx:80%!";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.input_tokens, 1_200_000);
        assert_eq!(status.output_tokens, 850_000);
        assert_eq!(status.context_percent, 80);
    }

    #[test]
    fn test_parse_permission_mode_from_output() {
        let input = "Some content\nMode: Accept edits\nMore content";
        assert_eq!(parse_permission_mode(input), Some(PermissionMode::AcceptEdits));

        let input2 = "Permissions: Bypass permissions";
        assert_eq!(parse_permission_mode(input2), Some(PermissionMode::BypassPermissions));

        let input3 = "Plan mode";
        assert_eq!(parse_permission_mode(input3), Some(PermissionMode::Plan));

        let input4 = "Permission mode: Accept edits";
        assert_eq!(parse_permission_mode(input4), Some(PermissionMode::AcceptEdits));
    }

    #[test]
    fn test_parse_permission_mode_claude_code_format() {
        // Claude Code shows modes like: "plan mode on (shift+tab to cycle)"
        let plan = "plan mode on (shift+tab to cycle)";
        assert_eq!(parse_permission_mode(plan), Some(PermissionMode::Plan));

        let accept = "accept edits on (shift+tab to cycle)";
        assert_eq!(parse_permission_mode(accept), Some(PermissionMode::AcceptEdits));

        let bypass = "bypass permissions on (shift+tab to cycle)";
        assert_eq!(parse_permission_mode(bypass), Some(PermissionMode::BypassPermissions));

        // With emoji prefix as shown in screenshots
        let with_emoji = ">> accept edits on (shift+tab to cycle)";
        assert_eq!(parse_permission_mode(with_emoji), Some(PermissionMode::AcceptEdits));
    }

    #[test]
    fn test_parse_status_line_false_positives() {
        // Should NOT match status line embedded in code
        let code = r#"let price = "haiku | $0.00";"#;
        assert!(parse_status_line(code).is_none(), "Should not match status line in code");

        // Should NOT match very long lines
        let long_line = "This is a very long line of text that happens to contain haiku | $0.00 somewhere in the middle of it and should not be matched";
        assert!(parse_status_line(long_line).is_none(), "Should not match in long lines");

        // Should NOT match when embedded in larger text (match doesn't start at beginning)
        let embedded = "Model: haiku | $0.00 - some extra text here";
        assert!(parse_status_line(embedded).is_none(), "Should not match when embedded after prefix");

        // SHOULD match status line with trailing "Update available!" text
        // This is Claude Code's actual format when an update is available
        let with_update = "Haiku 4.5 | $0.06 |     Update available!";
        let status_update = parse_status_line(with_update).unwrap();
        assert_eq!(status_update.model, "Haiku 4.5");
        assert!((status_update.cost - 0.06).abs() < 0.01);

        // SHOULD match a clean status line
        let clean = "haiku | $0.50 | 5K/3K | ctx:5%";
        assert!(parse_status_line(clean).is_some(), "Should match clean status line");

        // SHOULD match with surrounding whitespace
        let with_space = "   haiku | $0.50   ";
        assert!(parse_status_line(with_space).is_some(), "Should match with whitespace");
    }

    #[test]
    fn test_parse_status_line_multiline() {
        // Test wrapped status line (narrow terminal)
        // Line 1: "Haiku 4.5 | $0.07 |"
        // Line 2: "2.4K/1.2K | ctx:21%"
        let wrapped = "Some content\nHaiku 4.5 | $0.07 |\n2.4K/1.2K | ctx:21%";
        let status = parse_status_line(wrapped).unwrap();
        assert_eq!(status.model, "Haiku 4.5");
        assert!((status.cost - 0.07).abs() < 0.01);
        assert_eq!(status.input_tokens, 2400);
        assert_eq!(status.output_tokens, 1200);
        assert_eq!(status.context_percent, 21);

        // Test where we find the token line first and look back for model
        let wrapped2 = "Content\nOpus 4.5 | $1.23 |\n10.5K/8.2K | ctx:15%\nMore content";
        let status2 = parse_status_line(wrapped2).unwrap();
        assert_eq!(status2.model, "Opus 4.5");
        assert!((status2.cost - 1.23).abs() < 0.01);
        assert_eq!(status2.input_tokens, 10500);
        assert_eq!(status2.output_tokens, 8200);
        assert_eq!(status2.context_percent, 15);
    }

    #[test]
    fn test_parse_status_line_ctx_on_separate_line() {
        let wrapped = "Haiku 4.5 | $0.08 |\n3.9K/1.5K |\nctx:19%";
        let status = parse_status_line(wrapped).unwrap();
        assert_eq!(status.model, "Haiku 4.5");
        assert!((status.cost - 0.08).abs() < 0.01);
        assert_eq!(status.input_tokens, 3900);
        assert_eq!(status.output_tokens, 1500);
        assert_eq!(status.context_percent, 19);
    }

    #[test]
    fn test_parse_status_line_with_notifications() {
        // Wide terminal with notifications on the right
        // "Opus 4.5 | $0.00 | 0/0 | ctx:0%     1 MCP server failed..."
        // The regex should match the status part and ignore the rest
        let with_notif = "Opus 4.5 | $0.00 | 0/0 | ctx:0%";
        let status = parse_status_line(with_notif).unwrap();
        assert_eq!(status.model, "Opus 4.5");
        assert!((status.cost - 0.0).abs() < 0.01);
        assert_eq!(status.input_tokens, 0);
        assert_eq!(status.output_tokens, 0);
        assert_eq!(status.context_percent, 0);
    }

    #[test]
    fn test_parse_status_line_update_available_multiline() {
        // Real scenario: narrow terminal with "Update available!" on first line
        // Line 1: "Haiku 4.5 | $0.10 |     Update available!"
        // Line 2: "5.3K/2.1K | ctx:21%"
        let with_update = "Some content\nHaiku 4.5 | $0.10 |     Update available!\n5.3K/2.1K | ctx:21%";
        let status = parse_status_line(with_update).unwrap();
        assert_eq!(status.model, "Haiku 4.5");
        assert!((status.cost - 0.10).abs() < 0.01);
        assert_eq!(status.input_tokens, 5300);
        assert_eq!(status.output_tokens, 2100);
        assert_eq!(status.context_percent, 21);

        // Also test finding tokens line first and looking back
        let with_update2 = "Content\nHaiku 4.5 | $0.10 |     Update available!\n5.3K/2.1K | ctx:21%\nMore content";
        let status2 = parse_status_line(with_update2).unwrap();
        assert_eq!(status2.model, "Haiku 4.5");
        assert!((status2.cost - 0.10).abs() < 0.01);
        assert_eq!(status2.input_tokens, 5300);
        assert_eq!(status2.output_tokens, 2100);
        assert_eq!(status2.context_percent, 21);
    }

    #[tokio::test]
    async fn test_update_context_from_hook_uses_current_usage_for_context() {
        let buffers = SessionBuffers::new();
        let session_id = Uuid::new_v4();
        let usage = CurrentUsage {
            input_tokens: 4000,
            output_tokens: 0,
            cache_creation_input_tokens: 1000,
            cache_read_input_tokens: 2000,
        };

        let activity = buffers
            .update_context_from_hook(session_id, 12000, 3000, 200000, Some(usage), None)
            .await
            .unwrap();

        assert_eq!(activity.input_tokens, 12000);
        assert_eq!(activity.output_tokens, 3000);
        assert_eq!(activity.context_percent, 3);
    }

    #[tokio::test]
    async fn test_transcript_usage_does_not_override_hook_totals() {
        let buffers = SessionBuffers::new();
        let session_id = Uuid::new_v4();

        buffers
            .update_context_from_hook(session_id, 12000, 3000, 200000, None, None)
            .await
            .unwrap();

        let activity = buffers
            .accumulate_usage(session_id, 500, 200, 50, 25, "claude-sonnet-4-5")
            .await
            .unwrap();

        assert_eq!(activity.input_tokens, 12000);
        assert_eq!(activity.output_tokens, 3000);
        assert_eq!(activity.cache_read_tokens, 50);
        assert_eq!(activity.cache_creation_tokens, 25);
    }

    #[test]
    fn test_parse_tool_invocation() {
        // Test that tool invocation patterns are detected for status tracking
        // NOTE: Actions are no longer created from buffer parsing (they come from hooks)
        // We still detect tool usage for activity status purposes
        let result = parse_activity_and_action("● Bash(git status)").unwrap();
        assert!(result.0.contains("Bash") || result.1.as_deref() == Some("Bash"));
        assert!(result.2.is_empty()); // Actions now come from hooks

        let result = parse_activity_and_action("● Read(README.md)").unwrap();
        assert!(result.0.contains("Read") || result.1.as_deref() == Some("Read"));
        assert!(result.2.is_empty()); // Actions now come from hooks
    }

    #[test]
    fn test_parse_thinking_with_actions() {
        // Test that thinking status is captured
        // NOTE: Actions are no longer created from buffer parsing (they come from hooks)
        let input = "● Bash(git status)\n● Read(file.txt)\n* Actualizing... (thinking)";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking..."); // activity
        assert_eq!(result.1.as_deref(), Some("Thinking")); // step
        assert!(result.2.is_empty()); // Actions now come from hooks
    }

    #[test]
    fn test_parse_ready_state() {
        // Test that user input prompt (> ) is detected as Ready state
        // NOTE: Actions are no longer created from buffer parsing (they come from hooks)
        let input = "● Bash(git status)\n● Read(file.txt)\n> run the tests";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready"); // activity
        assert_eq!(result.1.as_deref(), Some("Ready")); // step
        assert!(result.2.is_empty()); // Actions now come from hooks

        // Test with prompt and suggestion
        let input2 = "● Read(file.txt)\n> what next?";
        let result2 = parse_activity_and_action(input2).unwrap();
        assert_eq!(result2.0, "Ready");
        assert_eq!(result2.1.as_deref(), Some("Ready"));

        // Test with just ">" (empty prompt, no suggestion yet)
        let input3 = "● Read(file.txt)\nSome response text\n>";
        let result3 = parse_activity_and_action(input3).unwrap();
        assert_eq!(result3.0, "Ready");
        assert_eq!(result3.1.as_deref(), Some("Ready"));
    }

    #[test]
    fn test_parse_actioning_as_ready() {
        // Test that "Actioning" is detected as Ready (Claude generating suggestion)
        let input = "● Read(file.txt)\n* Actioning... (esc to interrupt)";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready"); // activity
        assert_eq!(result.1.as_deref(), Some("Ready")); // step
    }

    #[test]
    fn test_priority_thinking_over_prompt() {
        // Test that Thinking takes precedence over ">" prompt
        // Even if there's a ">" in the output, if Thinking is more recent, show Thinking
        let input = "> old prompt\n● Read(file.txt)\n* Thinking... (thought for 3s)";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking..."); // Should be Thinking, NOT Ready
        assert_eq!(result.1.as_deref(), Some("Thinking"));
    }

    #[test]
    fn test_priority_tool_over_prompt() {
        // Test that tool invocation takes precedence over ">" prompt
        let input = "> old prompt\n● Read(README.md)";
        let result = parse_activity_and_action(input).unwrap();
        assert!(result.0.contains("Read")); // Should show tool, NOT Ready
    }

    #[test]
    fn test_ready_after_thinking() {
        // KEY TEST: When Claude finishes thinking and shows ">", should be Ready
        // This was the main bug - we were showing "Thinking" even when ">" appeared after
        let input = "● Read(file.txt)\n* Thinking... (3s elapsed)\nHere's my analysis...\n>";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready"); // ">" is most recent, should be Ready
        assert_eq!(result.1.as_deref(), Some("Ready"));
    }

    #[test]
    fn test_prose_with_thinking_word() {
        // Prose containing "thinking" should NOT trigger Thinking status
        // Only status lines like "* Thinking..." should
        let input = "● Read(file.txt)\nThis document discusses critical thinking skills and problem solving.\n>";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready"); // Should be Ready, NOT Thinking
        assert_eq!(result.1.as_deref(), Some("Ready"));
    }

    #[test]
    fn test_long_prose_with_thinking_word() {
        // Long lines containing "thinking" are definitely prose, not status
        let input = "I've been thinking about this problem for a while and I believe the best approach is to refactor the authentication module to use JWT tokens instead of session cookies. This will improve security and scalability.\n● Bash(cargo test)";
        let result = parse_activity_and_action(input).unwrap();
        // Should show the tool, not "Thinking"
        assert!(result.0.contains("Bash") || result.1.as_deref() == Some("Bash"));
    }

    #[test]
    fn test_recency_wins_complex_scenario() {
        // Complex scenario: old prompt → tool → thinking → tool → prompt
        // The LAST item (prompt) should win
        let input = "> first prompt\n● Read(a.txt)\n* Thinking...\n● Bash(ls)\nSome output\n>";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready");
        assert_eq!(result.1.as_deref(), Some("Ready"));
    }

    #[test]
    fn test_thinking_most_recent() {
        // When thinking is most recent, should show Thinking
        let input = "> prompt\n● Read(file.txt)\nSome output\n* Thinking... (2s)";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking...");
        assert_eq!(result.1.as_deref(), Some("Thinking"));
    }

    #[test]
    fn test_spinner_thinking() {
        // Spinner character + Thinking should be detected
        let input = "● Read(file.txt)\n⠋ Thinking...";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking...");
    }

    #[test]
    fn test_actualizing_detected() {
        // "Actualizing" is a thinking state
        let input = "> old\n● Read(file.txt)\n* Actualizing...";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking...");
    }

    #[test]
    fn test_is_thinking_status_line() {
        // Test the helper function directly
        assert!(is_thinking_status_line("* Thinking...", "* thinking..."));
        assert!(is_thinking_status_line("⠋ Thinking... (2s)", "⠋ thinking... (2s)"));
        assert!(is_thinking_status_line("Thinking...", "thinking..."));
        assert!(!is_thinking_status_line(
            "I'm thinking about this problem and believe we should...",
            "i'm thinking about this problem and believe we should..."
        ));
        assert!(!is_thinking_status_line(
            "The document covers critical thinking skills for developers",
            "the document covers critical thinking skills for developers"
        ));
    }

    #[test]
    fn test_file_content_with_blockquote_not_ready() {
        // When Claude reads a file containing markdown blockquotes (>),
        // should NOT detect as Ready - should show the tool instead
        let input = "> user prompt\n● Read(README.md)\nSome file content\n> This is a blockquote in the file\nMore content";
        let result = parse_activity_and_action(input).unwrap();
        // Should detect the tool, not the blockquote as Ready
        assert!(result.0.contains("Read") || result.1.as_deref() == Some("Read"),
            "Expected tool detection, got: {} / {:?}", result.0, result.1);
    }

    #[test]
    fn test_deep_prompt_ignored() {
        // Old prompt deep in buffer should be ignored, recent tool should be detected
        let input = "> old user prompt\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\n● Read(file.txt)\nfile contents here";
        let result = parse_activity_and_action(input).unwrap();
        // Should detect the tool, not the old prompt
        assert!(result.0.contains("Read") || result.1.as_deref() == Some("Read"),
            "Expected tool detection, got: {} / {:?}", result.0, result.1);
    }

    #[test]
    fn test_tool_with_many_lines_of_output() {
        // Tool followed by many lines of output (simulating file read)
        // The old prompt should be ignored
        let input = "> original prompt\n● Read(big_file.rs)\nfn main() {\n    println!(\"hello\");\n}\n// comment\n> nested quote\nmore code";
        let result = parse_activity_and_action(input).unwrap();
        // Should detect the tool
        assert!(result.0.contains("Read") || result.1.as_deref() == Some("Read"),
            "Expected tool detection, got: {} / {:?}", result.0, result.1);
    }
}
