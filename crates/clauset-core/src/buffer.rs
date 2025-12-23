//! Terminal output buffering and activity tracking.

use std::collections::HashMap;
use std::sync::Arc;
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum buffer size per session (500KB for longer scrollback)
const MAX_BUFFER_SIZE: usize = 500 * 1024;

/// Maximum number of recent actions to track
const MAX_RECENT_ACTIONS: usize = 5;

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
    pub context_percent: u8,
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
}

impl Default for SessionActivity {
    fn default() -> Self {
        Self {
            model: String::new(),
            cost: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            context_percent: 0,
            current_activity: String::new(),
            current_step: None,
            recent_actions: Vec::new(),
            last_update: std::time::Instant::now(),
            is_busy: false,
            busy_since: None,
            saw_activity_since_busy: false,
            last_activity_indicator: std::time::Instant::now(),
            bytes_since_activity: 0,
        }
    }
}

/// Ring buffer for terminal output.
#[derive(Debug)]
struct TerminalBuffer {
    data: Vec<u8>,
    activity: SessionActivity,
}

impl TerminalBuffer {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(MAX_BUFFER_SIZE),
            activity: SessionActivity::default(),
        }
    }

    fn append(&mut self, chunk: &[u8]) {
        // If adding this chunk would exceed max size, trim from the beginning
        let new_len = self.data.len() + chunk.len();
        if new_len > MAX_BUFFER_SIZE {
            let to_remove = new_len - MAX_BUFFER_SIZE;
            self.data.drain(0..to_remove.min(self.data.len()));
        }
        self.data.extend_from_slice(chunk);
    }

    fn get_data(&self) -> &[u8] {
        &self.data
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
    pub async fn append(&self, session_id: Uuid, data: &[u8]) -> Option<SessionActivity> {
        let mut buffers = self.buffers.write().await;
        let buffer = buffers.entry(session_id).or_insert_with(TerminalBuffer::new);
        buffer.append(data);

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

        if activity_changed {
            Some(buffer.activity.clone())
        } else {
            None
        }
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
        if let Some(status) = parse_status_line(&clean_buffer) {
            if buffer.activity.model != status.model
                || (buffer.activity.cost - status.cost).abs() > 0.001
                || buffer.activity.input_tokens != status.input_tokens
                || buffer.activity.output_tokens != status.output_tokens
                || buffer.activity.context_percent != status.context_percent
            {
                buffer.activity.model = status.model;
                buffer.activity.cost = status.cost;
                buffer.activity.input_tokens = status.input_tokens;
                buffer.activity.output_tokens = status.output_tokens;
                buffer.activity.context_percent = status.context_percent;
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
                        "[STATUS] Activity indicator in NEW chunk: {:?} - updating activity tracking",
                        step
                    );
                    buffer.activity.saw_activity_since_busy = true;
                    buffer.activity.last_activity_indicator = std::time::Instant::now();
                    buffer.activity.bytes_since_activity = 0;
                } else {
                    // Not busy (Ready state) - this is likely a terminal redraw, ignore
                    tracing::debug!(
                        "[STATUS] Activity indicator in NEW chunk: {:?} - but not busy, ignoring (likely terminal redraw)",
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

        // DEBUG: Log state for troubleshooting
        tracing::debug!(
            "[STATUS] is_busy={}, saw_activity={}, time_since_busy={}ms, time_since_activity={}ms, bytes={}, parsed_step={:?}",
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
                "[STATUS] BUSY CHECK: saw_activity={}, time_ok={}, bytes_ok={}, parsed_ready={}, fallback={}",
                saw_activity, time_ok, bytes_ok, parsed_ready, fallback_timeout
            );

            let can_transition = (saw_activity && time_ok && bytes_ok && parsed_ready) || fallback_timeout;

            if can_transition {
                // Transition to Ready
                tracing::info!("[STATUS] >>> TRANSITION TO READY <<<");
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
    pub async fn get_buffer(&self, session_id: Uuid) -> Option<Vec<u8>> {
        let buffers = self.buffers.read().await;
        buffers.get(&session_id).map(|b| b.get_data().to_vec())
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
    pub async fn clear(&self, session_id: Uuid) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&session_id) {
            buffer.data.clear();
        }
    }

    /// Mark a session as busy (user sent input, waiting for Claude's response).
    /// This ensures the status stays "Thinking" until Claude reliably finishes.
    pub async fn mark_busy(&self, session_id: Uuid) {
        tracing::info!("[STATUS] mark_busy called for session {}", session_id);
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
        tracing::info!("[STATUS] mark_ready called for session {}", session_id);
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
        tracing::info!("[STATUS] initialize_session called for session {}", session_id);
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
            "[HOOK] Updated activity for session {}: step={:?}, busy={}, actions={}",
            session_id,
            buffer.activity.current_step,
            buffer.activity.is_busy,
            buffer.activity.recent_actions.len()
        );

        Some(buffer.activity.clone())
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

/// Pre-compiled regex for status line parsing
static STATUS_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Model must start with a letter, followed by alphanumeric, dots, dashes, and spaces
    // Examples: "Opus 4.5", "Claude 3", "Sonnet 3.5", "haiku", "opus-4-5", "claude-3-sonnet"
    Regex::new(
        r"([A-Za-z][A-Za-z0-9.\- ]*?)\s*\|\s*\$([0-9.]+)\s*\|\s*([0-9.]+)K?/([0-9.]+)K?\s*\|\s*ctx:(\d+)%"
    ).unwrap()
});

/// Parse Claude's status line format: "Model | $Cost | InputK/OutputK | ctx:X%"
fn parse_status_line(text: &str) -> Option<ParsedStatus> {
    // Look for the pattern in each line
    for line in text.lines() {
        let line = line.trim();

        if let Some(caps) = STATUS_LINE_REGEX.captures(line) {
            let model = caps.get(1)?.as_str().trim().to_string();
            let cost = caps.get(2)?.as_str().parse().ok()?;
            let input_k: f64 = caps.get(3)?.as_str().parse().ok()?;
            let output_k: f64 = caps.get(4)?.as_str().parse().ok()?;
            let context_percent: u8 = caps.get(5)?.as_str().parse().ok()?;

            return Some(ParsedStatus {
                model,
                cost,
                input_tokens: (input_k * 1000.0) as u64,
                output_tokens: (output_k * 1000.0) as u64,
                context_percent,
            });
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

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[32mHello\x1b[0m World";
        assert_eq!(strip_ansi_codes(input), "Hello World");
    }

    #[test]
    fn test_parse_status_line() {
        let input = "Opus 4.5 | $0.68 | 29.2K/22.5K | ctx:11%";
        let status = parse_status_line(input).unwrap();
        assert_eq!(status.model, "Opus 4.5");
        assert!((status.cost - 0.68).abs() < 0.01);
        assert_eq!(status.input_tokens, 29200);
        assert_eq!(status.output_tokens, 22500);
        assert_eq!(status.context_percent, 11);
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
