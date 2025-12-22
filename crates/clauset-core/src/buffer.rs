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

        // Parse from the FULL buffer (last N bytes) to catch tool calls that span chunks.
        // This is crucial because terminal output arrives in small pieces and tool calls
        // might be in earlier chunks that we need to still detect.
        // We need to extract the text first, then parse it, to avoid borrow conflicts.
        let text = {
            let buffer_data = buffer.get_data();
            let parse_start = buffer_data.len().saturating_sub(8192); // Last 8KB
            String::from_utf8_lossy(&buffer_data[parse_start..]).to_string()
        };

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
        if let Some((activity, step, actions)) = parse_activity_and_action(&clean_text) {
            if buffer.activity.current_activity != activity {
                buffer.activity.current_activity = activity;
                buffer.activity.last_update = std::time::Instant::now();
                changed = true;
            }
            if buffer.activity.current_step != step {
                buffer.activity.current_step = step;
                changed = true;
            }

            // Add all new actions (deduplicating against existing ones)
            for new_action in actions {
                // Check if this action already exists (by type + summary)
                let already_exists = buffer.activity.recent_actions.iter().any(|a| {
                    a.action_type == new_action.action_type && a.summary == new_action.summary
                });

                if !already_exists {
                    buffer.activity.recent_actions.push(new_action);
                    changed = true;

                    // Keep only the most recent actions
                    while buffer.activity.recent_actions.len() > MAX_RECENT_ACTIONS {
                        buffer.activity.recent_actions.remove(0);
                    }
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
    // Model must start with a letter, followed by alphanumeric, dots, and spaces
    // Examples: "Opus 4.5", "Claude 3", "Sonnet 3.5"
    Regex::new(
        r"([A-Za-z][A-Za-z0-9. ]*?)\s*\|\s*\$([0-9.]+)\s*\|\s*([0-9.]+)K?/([0-9.]+)K?\s*\|\s*ctx:(\d+)%"
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

/// Parse current activity from terminal output.
/// Returns (current_activity, current_step, list of new actions)
///
/// This function now properly handles the case where Claude is "Thinking" but we still
/// want to capture tool actions from earlier in the output.
fn parse_activity_and_action(text: &str) -> Option<(String, Option<String>, Vec<RecentAction>)> {
    let lines: Vec<&str> = text.lines().collect();

    // First pass: Find the current status (thinking, planning, etc.) from recent lines
    let mut current_status: Option<(String, String)> = None; // (activity, step)

    // Check the last 30 lines for current status
    // We process in reverse order (most recent first) and take the first meaningful status
    // PRIORITY ORDER (check first = highest priority):
    //   1. Thinking/Actualizing/Planning = actively processing
    //   2. Tool invocations = actively executing tools
    //   3. Actioning = generating suggestion (response done)
    //   4. ">" prompt = waiting for input (lowest priority - only if nothing else active)
    for line in lines.iter().rev().take(30) {
        let clean_line = strip_ansi_codes(line.trim());
        let clean_lower = clean_line.to_lowercase();

        // Skip status lines (the bottom status bar)
        if clean_line.contains("ctx:") || clean_line.contains("| $") {
            continue;
        }

        // Skip very short lines (but we'll check for ">" prompt separately at the end)
        if clean_line.len() < 3 && clean_line.trim() != ">" {
            continue;
        }
        if is_ui_chrome(&clean_line) {
            continue;
        }

        // HIGHEST PRIORITY: Check for thinking/planning/actualizing states
        // These indicate Claude is actively processing - this should ALWAYS take precedence
        if clean_lower.contains("thinking") && !clean_lower.contains("thinking about") {
            current_status = Some(("Thinking...".to_string(), "Thinking".to_string()));
            break;
        }
        if clean_lower.contains("actualizing") || clean_lower.contains("mustering") {
            current_status = Some(("Thinking...".to_string(), "Thinking".to_string()));
            break;
        }
        if clean_lower.contains("planning") {
            current_status = Some(("Planning...".to_string(), "Planning".to_string()));
            break;
        }

        // SECOND PRIORITY: Tool invocations - Claude is actively executing a tool
        if let Some((activity, step, _)) = parse_tool_activity_flexible(&clean_line, &clean_lower) {
            current_status = Some((activity, step.unwrap_or_default()));
            break;
        }

        // THIRD PRIORITY: "Actioning" - Claude generating a suggested next message
        // This means the main response is DONE, treat as Ready
        if clean_lower.contains("actioning") {
            current_status = Some(("Ready".to_string(), "Ready".to_string()));
            break;
        }

        // LOWEST PRIORITY: User input prompt (>) - only if nothing else is happening
        // The prompt can be just ">" or "> suggestion" or "> suggestion ↵ send"
        let trimmed = clean_line.trim();
        if trimmed == ">" || trimmed.starts_with("> ") {
            current_status = Some(("Ready".to_string(), "Ready".to_string()));
            break;
        }
    }

    // Second pass: Find ALL tool actions in the buffer (scan more lines)
    // We collect multiple actions and return them all for deduplication by the caller
    let mut found_actions: Vec<RecentAction> = Vec::new();
    let mut seen_summaries: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in lines.iter().rev().take(150) {
        let clean_line = strip_ansi_codes(line.trim());
        let clean_lower = clean_line.to_lowercase();

        // Skip empty, status lines, and UI chrome
        if clean_line.len() < 3 || clean_line.contains("ctx:") || clean_line.contains("| $") {
            continue;
        }
        if is_ui_chrome(&clean_line) {
            continue;
        }

        // Look for tool invocations to record as actions
        if let Some((_, _, Some(action))) = parse_tool_activity_flexible(&clean_line, &clean_lower) {
            // Deduplicate within this parse by summary
            let key = format!("{}:{}", action.action_type, action.summary);
            if !seen_summaries.contains(&key) {
                seen_summaries.insert(key);
                found_actions.push(action);

                // Limit to 10 actions per parse to avoid excessive processing
                if found_actions.len() >= 10 {
                    break;
                }
            }
        }
    }

    // Reverse so oldest is first (they'll be added in order)
    found_actions.reverse();

    // Combine results
    if let Some((activity, step)) = current_status {
        return Some((activity, Some(step), found_actions));
    }

    // Fallback: if we found actions but no explicit status, we're probably mid-execution
    // Show "Processing" rather than the last action's summary to avoid confusion
    if !found_actions.is_empty() {
        return Some((
            "Processing...".to_string(),
            None,
            found_actions,
        ));
    }

    // No status and no actions found - return None to let the frontend handle it
    // We explicitly do NOT pick up random terminal text as the status
    None
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
        // Test Claude Code's tool invocation format
        let (_, _, actions) = parse_activity_and_action("● Bash(git status)").unwrap();
        assert!(!actions.is_empty());
        assert_eq!(actions[0].action_type, "bash");

        let (_, _, actions) = parse_activity_and_action("● Read(README.md)").unwrap();
        assert!(!actions.is_empty());
        assert_eq!(actions[0].action_type, "read");
    }

    #[test]
    fn test_parse_thinking_with_actions() {
        // Test that thinking status is captured while also capturing tool actions
        let input = "● Bash(git status)\n● Read(file.txt)\n* Actualizing... (thinking)";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Thinking..."); // activity
        assert_eq!(result.1.as_deref(), Some("Thinking")); // step
        assert!(!result.2.is_empty()); // actions should be captured
    }

    #[test]
    fn test_parse_ready_state() {
        // Test that user input prompt (> ) is detected as Ready state
        let input = "● Bash(git status)\n● Read(file.txt)\n> run the tests";
        let result = parse_activity_and_action(input).unwrap();
        assert_eq!(result.0, "Ready"); // activity
        assert_eq!(result.1.as_deref(), Some("Ready")); // step
        assert!(!result.2.is_empty()); // actions should still be captured

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
}
