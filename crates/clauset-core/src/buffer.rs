//! Terminal output buffering and activity tracking.

use std::collections::HashMap;
use std::sync::Arc;
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

        // Try to parse status line and activity from the new data
        let text = String::from_utf8_lossy(data);
        let activity_changed = self.parse_and_update_activity(buffer, &text);

        if activity_changed {
            Some(buffer.activity.clone())
        } else {
            None
        }
    }

    /// Parse terminal output for status line and current activity.
    fn parse_and_update_activity(&self, buffer: &mut TerminalBuffer, text: &str) -> bool {
        let mut changed = false;

        // Strip ANSI escape codes for parsing
        let clean_text = strip_ansi_codes(text);

        // Parse status line: "Model | $Cost | InputK/OutputK | ctx:X%"
        if let Some(status) = parse_status_line(&clean_text) {
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

        // Parse current activity and extract structured actions
        if let Some((activity, step, action)) = parse_activity_and_action(&clean_text) {
            if buffer.activity.current_activity != activity {
                buffer.activity.current_activity = activity;
                buffer.activity.last_update = std::time::Instant::now();
                changed = true;
            }
            if buffer.activity.current_step != step {
                buffer.activity.current_step = step;
                changed = true;
            }
            // Add new action if present
            if let Some(new_action) = action {
                // Avoid duplicate actions
                let dominated = buffer.activity.recent_actions.iter().any(|a| {
                    a.action_type == new_action.action_type && a.summary == new_action.summary
                });
                if !dominated {
                    buffer.activity.recent_actions.push(new_action);
                    // Keep only the most recent actions
                    if buffer.activity.recent_actions.len() > MAX_RECENT_ACTIONS {
                        buffer.activity.recent_actions.remove(0);
                    }
                    changed = true;
                }
            }
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
}

/// Strip ANSI escape codes from text.
fn strip_ansi_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (end of sequence)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Parsed status line info.
struct ParsedStatus {
    model: String,
    cost: f64,
    input_tokens: u64,
    output_tokens: u64,
    context_percent: u8,
}

/// Parse Claude's status line format: "Model | $Cost | InputK/OutputK | ctx:X%"
fn parse_status_line(text: &str) -> Option<ParsedStatus> {
    // Look for the pattern in each line
    for line in text.lines() {
        let line = line.trim();

        // Match pattern: "Opus 4.5 | $0.68 | 29.2K/22.5K | ctx:11%"
        let re_pattern = regex::Regex::new(
            r"([A-Za-z0-9. ]+?)\s*\|\s*\$([0-9.]+)\s*\|\s*([0-9.]+)K?/([0-9.]+)K?\s*\|\s*ctx:(\d+)%"
        ).ok()?;

        if let Some(caps) = re_pattern.captures(line) {
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

/// Parse current activity from terminal output.
/// Returns (current_activity, current_step, optional new action)
fn parse_activity_and_action(text: &str) -> Option<(String, Option<String>, Option<RecentAction>)> {
    // Process lines in reverse order (most recent first)
    let lines: Vec<&str> = text.lines().collect();

    let mut best_activity: Option<(String, Option<String>, Option<RecentAction>)> = None;

    for line in lines.iter().rev().take(100) {
        let line = line.trim();
        let clean_line = strip_ansi_codes(line);
        let clean_lower = clean_line.to_lowercase();

        // Skip empty lines, very short lines, and status lines
        if clean_line.len() < 3 || clean_line.contains("ctx:") || clean_line.contains("| $") {
            continue;
        }

        // Parse structured tool activity - now more flexible
        if let Some(result) = parse_tool_activity_flexible(&clean_line, &clean_lower) {
            return Some(result);
        }

        // Check for thinking/planning states
        if clean_lower.contains("thinking") && !clean_lower.contains("thinking about") {
            best_activity = Some((
                "Thinking...".to_string(),
                Some("Thinking".to_string()),
                None,
            ));
            // Don't return yet - keep looking for more specific activity
        }
        if clean_lower.contains("planning") && best_activity.is_none() {
            best_activity = Some((
                "Planning...".to_string(),
                Some("Planning".to_string()),
                None,
            ));
        }
    }

    best_activity
}

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

    let ts = now_ms();

    // === BASH / COMMAND DETECTION ===
    // Look for command prompts: $, >, or lines starting with common commands
    if line.starts_with("$ ") || line.starts_with("> ") {
        let cmd = &line[2..].trim();
        if !cmd.is_empty() && cmd.len() < 100 {
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

    // === FILE PATH DETECTION ===
    // Any line containing an absolute path or relative path with extension
    if let Some(path) = extract_file_path_flexible(line) {
        let filename = path.split('/').last().unwrap_or(&path);

        // Determine action type from context
        let (action_type, action_verb, step) = if line_lower.contains("read") || line_lower.contains("reading") {
            ("read", "Read", "Read")
        } else if line_lower.contains("writ") || line_lower.contains("creat") {
            ("write", "Wrote", "Write")
        } else if line_lower.contains("edit") || line_lower.contains("modif") || line_lower.contains("updat") {
            ("edit", "Edited", "Edit")
        } else if line_lower.contains("delet") || line_lower.contains("remov") {
            ("delete", "Deleted", "Delete")
        } else {
            // Default to read for file mentions
            ("read", "Read", "Read")
        };

        return Some((
            format!("{} {}", action_verb, truncate_str(filename, 30)),
            Some(step.to_string()),
            Some(RecentAction {
                action_type: action_type.to_string(),
                summary: format!("{} {}", action_verb, truncate_str(filename, 25)),
                detail: Some(truncate_str(&path, 70)),
                timestamp: ts,
            }),
        ));
    }

    // === TOOL KEYWORD DETECTION ===
    // Look for tool names at start of line or after bullets/markers
    let tool_patterns = [
        ("read", "Read", "read"),
        ("write", "Write", "write"),
        ("edit", "Edit", "edit"),
        ("bash", "Bash", "bash"),
        ("grep", "Grep", "search"),
        ("glob", "Glob", "search"),
        ("search", "Search", "search"),
        ("task", "Task", "task"),
        ("todowrite", "Todo", "task"),
        ("webfetch", "Web", "web"),
        ("websearch", "Web", "web"),
    ];

    for (keyword, step, action_type) in tool_patterns {
        // Check if line contains the tool name in a way that suggests tool invocation
        // Look for patterns like "Read /path", "● Read", "Tool: Read", etc.
        let has_tool = line_lower.starts_with(keyword)
            || line_lower.contains(&format!(" {}", keyword))
            || line_lower.contains(&format!("●{}", keyword))
            || line_lower.contains(&format!("● {}", keyword))
            || line_lower.contains(&format!("tool: {}", keyword));

        if has_tool && line.len() < 150 {
            // Extract what follows the keyword as detail
            let detail = if let Some(pos) = line_lower.find(keyword) {
                let rest = &line[pos + keyword.len()..].trim();
                if !rest.is_empty() && rest.len() < 100 {
                    Some(truncate_str(rest, 70))
                } else {
                    None
                }
            } else {
                None
            };

            return Some((
                format!("{}...", step),
                Some(step.to_string()),
                Some(RecentAction {
                    action_type: action_type.to_string(),
                    summary: format!("{}", step),
                    detail,
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
        ("running ", "run", "Running"),
        ("installing ", "install", "Installing"),
        ("downloading ", "download", "Downloading"),
        ("error[", "error", "Error"),
        ("warning:", "warning", "Warning"),
        ("success", "success", "Success"),
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

/// Extract a file path from a line - more flexible matching
fn extract_file_path_flexible(line: &str) -> Option<String> {
    // Look for absolute paths
    if let Some(start) = line.find('/') {
        // Extract path starting from /
        let rest = &line[start..];
        let end = rest.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ']' || c == '>')
            .unwrap_or(rest.len());
        let path = &rest[..end];

        // Validate it looks like a real path
        if path.len() > 3 && (path.contains('.') || path.ends_with('/')) {
            // Skip if it looks like a URL
            if !path.contains("://") && !path.starts_with("//") {
                return Some(path.to_string());
            }
        }
    }

    // Look for relative paths with common extensions
    let extensions = [".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".json",
                      ".toml", ".yaml", ".yml", ".md", ".txt", ".sh", ".css", ".html"];
    for ext in extensions {
        if let Some(pos) = line.find(ext) {
            // Walk backwards to find start of filename
            let before = &line[..pos + ext.len()];
            let start = before.rfind(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == '[')
                .map(|i| i + 1)
                .unwrap_or(0);
            let path = &before[start..];
            if path.len() > 2 && !path.contains(' ') {
                return Some(path.to_string());
            }
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
    fn test_parse_current_activity() {
        assert_eq!(parse_current_activity("Reading file.txt"), Some("Reading file".to_string()));
        assert_eq!(parse_current_activity("Writing to output.js"), Some("Writing file".to_string()));
    }
}
