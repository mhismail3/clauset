//! TUI menu parser for detecting and extracting selection menus from terminal output.
//!
//! This module provides a state machine parser that detects TUI selection menus
//! (like /model, /config) in terminal output and converts them to structured data
//! for native UI rendering.

use clauset_types::{TuiMenu, TuiMenuOption};
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// Maximum time to wait for a complete menu before resetting
const MENU_ACCUMULATION_TIMEOUT: Duration = Duration::from_secs(3);

/// Minimum number of options required to consider something a menu
const MIN_MENU_OPTIONS: usize = 2;

/// Parser states for TUI menu detection.
#[derive(Debug, Clone)]
enum ParserState {
    /// Waiting for menu patterns in terminal output
    Idle,
    /// Detected potential menu, accumulating lines
    Accumulating {
        /// Clean (ANSI-stripped) lines accumulated
        lines: Vec<String>,
        /// When accumulation started
        started_at: Instant,
    },
    /// Menu fully parsed and active
    MenuActive {
        /// The parsed menu
        menu: TuiMenu,
    },
}

/// Patterns that indicate footer/instruction lines (confirm end of menu)
/// Note: Claude Code may output Unicode as literal escape sequences (e.g., "\u2191" instead of ↑)
static FOOTER_PATTERNS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        // Standard patterns
        "Enter to confirm",
        "Esc to exit",
        "↑/↓ to navigate",
        "to navigate",
        "to confirm",
        "to exit",
        "to select",
        // Patterns that work with literal escape sequences (Claude Code CLI output)
        "Navigate",      // Matches "↑↓ Navigate" or "\u2191\u2193 Navigate"
        "Enter Select",  // Matches "Enter Select" in footer
        "Esc Cancel",    // Matches "Esc Cancel" in footer
    ]
});

/// Regex for detecting numbered option lines
/// Matches patterns like:
/// - "1. Option label"
/// - "  2. Another option   Description here"
/// - "▸ 3. Highlighted option"
/// - "> 4. Also highlighted"
static NUMBERED_OPTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*[▸>]?\s*(\d+)\.\s+(.+)$").expect("Invalid numbered option regex")
});

/// Regex for detecting selection indicators
static SELECTION_INDICATOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[✓✔]").expect("Invalid selection indicator regex")
});

/// Regex for detecting highlight indicator at start of line
static HIGHLIGHT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*[▸>]").expect("Invalid highlight regex")
});

/// State machine parser for detecting TUI menus in terminal output.
pub struct TuiMenuParser {
    state: ParserState,
    /// Timeout for accumulation (configurable for testing)
    timeout: Duration,
}

impl Default for TuiMenuParser {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiMenuParser {
    /// Create a new TUI menu parser.
    pub fn new() -> Self {
        Self {
            state: ParserState::Idle,
            timeout: MENU_ACCUMULATION_TIMEOUT,
        }
    }

    /// Create a parser with a custom timeout (for testing).
    #[cfg(test)]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            state: ParserState::Idle,
            timeout,
        }
    }

    /// Process terminal output chunk and return detected menu if complete.
    ///
    /// The parser accumulates terminal output and attempts to detect TUI menus.
    /// Returns `Some(TuiMenu)` when a complete menu is detected.
    pub fn process(&mut self, data: &[u8]) -> Option<TuiMenu> {
        let raw_text = String::from_utf8_lossy(data);
        // Strip ANSI codes first, then normalize any literal Unicode escapes
        let clean_text = normalize_unicode_escapes(&strip_ansi_codes(&raw_text));

        // Split into lines for processing
        let new_lines: Vec<String> = clean_text
            .lines()
            .map(|l| l.to_string())
            .collect();

        if new_lines.is_empty() {
            // Still check for dismissal even with empty lines
            if matches!(self.state, ParserState::MenuActive { .. }) {
                if self.is_menu_dismissed(&raw_text, &clean_text) {
                    debug!(target: "clauset::tui_parser", "Menu dismissed, resetting to idle");
                    self.state = ParserState::Idle;
                }
            }
            return None;
        }

        match &mut self.state {
            ParserState::Idle => {
                // Check if this looks like it could be start of a menu
                // (has numbered options or appears to be a title)
                let has_numbered_options = new_lines.iter().any(|l| NUMBERED_OPTION_RE.is_match(l));

                if has_numbered_options {
                    trace!(target: "clauset::tui_parser", "Detected potential menu start, beginning accumulation");
                    self.state = ParserState::Accumulating {
                        lines: new_lines,
                        started_at: Instant::now(),
                    };

                    // Check if this chunk already contains a complete menu
                    return self.try_parse_complete_menu();
                }
            }

            ParserState::Accumulating { lines, started_at } => {
                // Check timeout
                if started_at.elapsed() > self.timeout {
                    debug!(target: "clauset::tui_parser", "Menu accumulation timed out, resetting");
                    self.state = ParserState::Idle;
                    return None;
                }

                // Add new lines to accumulated buffer
                lines.extend(new_lines);

                // Try to parse complete menu
                return self.try_parse_complete_menu();
            }

            ParserState::MenuActive { .. } => {
                // Menu is active, check for dismissal patterns (use raw for ANSI, clean for text)
                if self.is_menu_dismissed(&raw_text, &clean_text) {
                    debug!(target: "clauset::tui_parser", "Menu dismissed, resetting to idle");
                    self.state = ParserState::Idle;
                }
            }
        }

        None
    }

    /// Try to parse a complete menu from accumulated lines.
    fn try_parse_complete_menu(&mut self) -> Option<TuiMenu> {
        let lines = match &self.state {
            ParserState::Accumulating { lines, .. } => lines.clone(),
            _ => return None,
        };

        // Check if we have footer pattern (indicates complete menu)
        let has_footer = lines.iter().any(|l| {
            FOOTER_PATTERNS.iter().any(|p| l.to_lowercase().contains(&p.to_lowercase()))
        });

        if !has_footer {
            trace!(target: "clauset::tui_parser", "No footer pattern found, continuing accumulation");
            return None;
        }

        // Parse the menu
        if let Some(menu) = Self::parse_menu_from_lines(&lines) {
            debug!(target: "clauset::tui_parser", "Parsed complete menu: {} options", menu.options.len());
            self.state = ParserState::MenuActive { menu: menu.clone() };
            return Some(menu);
        }

        None
    }

    /// Parse a TuiMenu from accumulated clean lines.
    fn parse_menu_from_lines(lines: &[String]) -> Option<TuiMenu> {
        let mut title: Option<String> = None;
        let mut description: Option<String> = None;
        let mut options: Vec<TuiMenuOption> = Vec::new();
        let mut highlighted_index: usize = 0;
        let mut found_first_option = false;

        for line in lines {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Skip footer/instruction lines
            if FOOTER_PATTERNS.iter().any(|p| trimmed.to_lowercase().contains(&p.to_lowercase())) {
                continue;
            }

            // Try to parse as numbered option
            if let Some(caps) = NUMBERED_OPTION_RE.captures(line) {
                found_first_option = true;

                let _option_num: usize = caps.get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(options.len() + 1);

                let rest = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                // Parse label and description (split by multiple spaces)
                let (label, opt_description) = split_label_description(rest);

                // Check for selection indicator
                let is_selected = SELECTION_INDICATOR_RE.is_match(line);

                // Check for highlight indicator
                let is_highlighted = HIGHLIGHT_RE.is_match(line);

                if is_highlighted {
                    highlighted_index = options.len();
                }

                // Clean up label (remove selection indicators)
                let clean_label = label
                    .replace('✓', "")
                    .replace('✔', "")
                    .trim()
                    .to_string();

                options.push(TuiMenuOption::new(
                    options.len(),
                    clean_label,
                    opt_description,
                    is_selected,
                ));
            } else if !found_first_option {
                // Lines before first option are title/description
                if title.is_none() {
                    title = Some(trimmed.to_string());
                } else if description.is_none() {
                    description = Some(trimmed.to_string());
                } else {
                    // Append to description
                    if let Some(ref mut desc) = description {
                        desc.push(' ');
                        desc.push_str(trimmed);
                    }
                }
            }
        }

        // Validate we have enough options
        if options.len() < MIN_MENU_OPTIONS {
            trace!(target: "clauset::tui_parser", "Not enough options ({}) for a menu", options.len());
            return None;
        }

        // Validate we have a title
        let title = title.unwrap_or_else(|| "Select an option".to_string());

        // Infer menu type from title
        let menu_type = TuiMenu::infer_menu_type(&title);

        Some(TuiMenu::with_details(
            title,
            description,
            options,
            menu_type,
            highlighted_index,
        ))
    }

    /// Check if terminal output indicates menu was dismissed.
    ///
    /// Takes both raw text (for ANSI codes) and clean text (for content patterns).
    fn is_menu_dismissed(&self, raw_text: &str, clean_text: &str) -> bool {
        // Screen clear sequences (check raw text for ANSI codes)
        if raw_text.contains("\x1b[2J") || raw_text.contains("\x1b[H") {
            return true;
        }

        // New prompt indicator (> at start of line after clear)
        if clean_text.contains("\n> ") || clean_text.starts_with("> ") {
            return true;
        }

        // Claude's thinking indicator
        if clean_text.contains("Thinking...") || clean_text.contains("Working...") {
            return true;
        }

        false
    }

    /// Check if a menu is currently active.
    pub fn has_active_menu(&self) -> bool {
        matches!(self.state, ParserState::MenuActive { .. })
    }

    /// Get the active menu if present.
    pub fn get_active_menu(&self) -> Option<&TuiMenu> {
        match &self.state {
            ParserState::MenuActive { menu } => Some(menu),
            _ => None,
        }
    }

    /// Mark the current menu as dismissed.
    pub fn dismiss_menu(&mut self) {
        self.state = ParserState::Idle;
    }

    /// Reset the parser to idle state.
    pub fn reset(&mut self) {
        self.state = ParserState::Idle;
    }

    /// Check if currently accumulating.
    #[cfg(test)]
    pub fn is_accumulating(&self) -> bool {
        matches!(self.state, ParserState::Accumulating { .. })
    }
}

/// Strip ANSI escape codes from text.
fn strip_ansi_codes(text: &str) -> String {
    static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
        // Comprehensive ANSI escape sequence matching:
        // - CSI sequences: ESC [ ... letter (includes DEC private sequences with ?)
        // - OSC sequences: ESC ] ... BEL or ESC \
        // - Charset switching: ESC ( letter or ESC ) letter
        // - Keypad mode: ESC = or ESC >
        // - Save/restore cursor: ESC 7 or ESC 8
        // - Other single-char ESC sequences
        Regex::new(concat!(
            r"\x1b\[[0-9;?]*[a-zA-Z]",      // CSI sequences (including DEC private with ?)
            r"|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)",  // OSC sequences (BEL or ST terminator)
            r"|\x1b[()][AB012]",             // Charset switching
            r"|\x1b[=>]",                    // Keypad mode
            r"|\x1b[78]",                    // Save/restore cursor
            r"|\x1b[DEHMNOPVWXYZ\\^_c]",    // Other single-char ESC sequences
        )).expect("Invalid ANSI regex")
    });

    ANSI_RE.replace_all(text, "").to_string()
}

/// Normalize literal Unicode escape sequences (e.g., "\u2191") to actual Unicode characters.
/// Claude Code CLI sometimes outputs these as literal text in the terminal.
fn normalize_unicode_escapes(text: &str) -> String {
    static UNICODE_ESCAPE_RE: Lazy<Regex> = Lazy::new(|| {
        // Match literal \uXXXX patterns (not actual escape sequences, but the text representation)
        Regex::new(r"\\u([0-9a-fA-F]{4})").expect("Invalid unicode escape regex")
    });

    UNICODE_ESCAPE_RE.replace_all(text, |caps: &regex::Captures| {
        if let Some(hex_str) = caps.get(1) {
            if let Ok(code) = u32::from_str_radix(hex_str.as_str(), 16) {
                if let Some(c) = char::from_u32(code) {
                    return c.to_string();
                }
            }
        }
        // If conversion fails, keep the original
        caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default()
    }).into_owned()
}

/// Split option text into label and optional description.
/// Claude Code often uses multiple spaces to separate label from description.
fn split_label_description(text: &str) -> (String, Option<String>) {
    // Look for 2+ spaces as separator
    if let Some(idx) = text.find("  ") {
        let label = text[..idx].trim().to_string();
        let desc = text[idx..].trim().to_string();
        if desc.is_empty() {
            (label, None)
        } else {
            (label, Some(desc))
        }
    } else {
        (text.trim().to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clauset_types::TuiMenuType;

    // Sample menu output from /model command
    const MODEL_MENU_OUTPUT: &str = r#"
Select model
Switch between Claude models. Applies to
this session and future Claude Code
sessions.

  1. Default (recommended)   Opus 4.5 - Most
                            capable for
                            complex work
  2. Sonnet                  Sonnet 4.5 -
                            Best for
                            everyday tasks
▸ 3. Haiku ✓                Haiku 4.5 -
                            Fastest for
                            quick answers

Enter to confirm · Esc to exit
"#;

    // Simple menu output
    const SIMPLE_MENU: &str = r#"
Select option
  1. Option A
  2. Option B
  3. Option C ✓

Enter to confirm
"#;

    // Menu without description
    const MENU_NO_DESC: &str = r#"
Choose mode
  1. Normal
  2. Plan
  3. Chat

↑/↓ to navigate · Enter to confirm
"#;

    #[test]
    fn test_detects_simple_menu() {
        let mut parser = TuiMenuParser::new();
        let result = parser.process(SIMPLE_MENU.as_bytes());

        assert!(result.is_some());
        let menu = result.unwrap();
        assert_eq!(menu.title, "Select option");
        assert_eq!(menu.options.len(), 3);
    }

    #[test]
    fn test_parses_menu_title() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        assert_eq!(menu.title, "Select model");
    }

    #[test]
    fn test_parses_menu_description() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        assert!(menu.description.is_some());
        assert!(menu.description.as_ref().unwrap().contains("Switch between Claude models"));
    }

    #[test]
    fn test_parses_menu_options() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        assert_eq!(menu.options.len(), 3);
        assert_eq!(menu.options[0].label, "Default (recommended)");
        assert_eq!(menu.options[1].label, "Sonnet");
        assert_eq!(menu.options[2].label, "Haiku");
    }

    #[test]
    fn test_detects_selected_option() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        // Option 3 (Haiku) has ✓
        assert!(!menu.options[0].is_selected);
        assert!(!menu.options[1].is_selected);
        assert!(menu.options[2].is_selected);
    }

    #[test]
    fn test_detects_highlighted_option() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        // Option 3 (Haiku) has ▸
        assert_eq!(menu.highlighted_index, 2);
    }

    #[test]
    fn test_identifies_model_menu_type() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        assert_eq!(menu.menu_type, TuiMenuType::ModelSelect);
    }

    #[test]
    fn test_identifies_mode_menu_type() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MENU_NO_DESC.as_bytes()).unwrap();

        assert_eq!(menu.menu_type, TuiMenuType::Mode);
    }

    #[test]
    fn test_parses_option_descriptions() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MODEL_MENU_OUTPUT.as_bytes()).unwrap();

        // First option should have description
        assert!(menu.options[0].description.is_some());
        assert!(menu.options[0].description.as_ref().unwrap().contains("Opus"));
    }

    #[test]
    fn test_handles_menu_without_description() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(MENU_NO_DESC.as_bytes()).unwrap();

        assert!(menu.description.is_none());
        assert_eq!(menu.options.len(), 3);
    }

    #[test]
    fn test_handles_chunked_output() {
        let mut parser = TuiMenuParser::new();

        // Simulate output arriving in chunks
        let chunks = [
            "Select model\n",
            "  1. Default\n",
            "  2. Sonnet\n",
            "  3. Haiku\n",
            "Enter to confirm\n",
        ];

        let mut result = None;
        for chunk in chunks {
            if let Some(menu) = parser.process(chunk.as_bytes()) {
                result = Some(menu);
            }
        }

        assert!(result.is_some());
        let menu = result.unwrap();
        assert_eq!(menu.options.len(), 3);
    }

    #[test]
    fn test_accumulation_timeout() {
        let mut parser = TuiMenuParser::with_timeout(Duration::from_millis(1));

        // Start accumulation
        parser.process(b"Select model\n  1. Default\n");
        assert!(parser.is_accumulating());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(10));

        // Next process should reset due to timeout
        parser.process(b"  2. Sonnet\n");

        // Parser should have reset to idle (no complete menu without footer)
        assert!(!parser.has_active_menu());
    }

    #[test]
    fn test_no_menu_without_footer() {
        let mut parser = TuiMenuParser::new();

        let incomplete = "Select model\n  1. Default\n  2. Sonnet\n";
        let result = parser.process(incomplete.as_bytes());

        // Should be accumulating, not complete
        assert!(result.is_none());
        assert!(parser.is_accumulating());
    }

    #[test]
    fn test_no_menu_with_single_option() {
        let mut parser = TuiMenuParser::new();

        let single_option = "Select\n  1. Only option\nEnter to confirm\n";
        let result = parser.process(single_option.as_bytes());

        // Single option is not a valid menu
        assert!(result.is_none());
    }

    #[test]
    fn test_dismiss_on_screen_clear() {
        let mut parser = TuiMenuParser::new();

        // First, parse a menu
        parser.process(SIMPLE_MENU.as_bytes());
        assert!(parser.has_active_menu());

        // Screen clear should dismiss
        parser.process(b"\x1b[2J");
        assert!(!parser.has_active_menu());
    }

    #[test]
    fn test_dismiss_on_new_prompt() {
        let mut parser = TuiMenuParser::new();

        // First, parse a menu
        parser.process(SIMPLE_MENU.as_bytes());
        assert!(parser.has_active_menu());

        // New prompt should dismiss
        parser.process(b"\n> ");
        assert!(!parser.has_active_menu());
    }

    #[test]
    fn test_manual_dismiss() {
        let mut parser = TuiMenuParser::new();

        parser.process(SIMPLE_MENU.as_bytes());
        assert!(parser.has_active_menu());

        parser.dismiss_menu();
        assert!(!parser.has_active_menu());
    }

    #[test]
    fn test_reset() {
        let mut parser = TuiMenuParser::new();

        parser.process(b"Select\n  1. A\n");
        assert!(parser.is_accumulating());

        parser.reset();
        assert!(!parser.is_accumulating());
        assert!(!parser.has_active_menu());
    }

    #[test]
    fn test_strip_ansi_codes() {
        let with_ansi = "\x1b[32mGreen text\x1b[0m and \x1b[1mbold\x1b[0m";
        let stripped = strip_ansi_codes(with_ansi);
        assert_eq!(stripped, "Green text and bold");
    }

    #[test]
    fn test_strip_ansi_hyperlinks() {
        let with_link = "\x1b]8;;https://example.com\x07link text\x1b]8;;\x07";
        let stripped = strip_ansi_codes(with_link);
        assert_eq!(stripped, "link text");
    }

    #[test]
    fn test_split_label_description() {
        let (label, desc) = split_label_description("Label   Description here");
        assert_eq!(label, "Label");
        assert_eq!(desc, Some("Description here".to_string()));

        let (label2, desc2) = split_label_description("Just a label");
        assert_eq!(label2, "Just a label");
        assert!(desc2.is_none());
    }

    #[test]
    fn test_get_active_menu() {
        let mut parser = TuiMenuParser::new();

        assert!(parser.get_active_menu().is_none());

        parser.process(SIMPLE_MENU.as_bytes());
        assert!(parser.get_active_menu().is_some());

        let menu = parser.get_active_menu().unwrap();
        assert_eq!(menu.title, "Select option");
    }

    #[test]
    fn test_multiple_menus_sequentially() {
        let mut parser = TuiMenuParser::new();

        // Parse first menu
        let menu1 = parser.process(SIMPLE_MENU.as_bytes()).unwrap();
        assert_eq!(menu1.title, "Select option");

        // Dismiss first menu
        parser.dismiss_menu();

        // Parse second menu
        let menu2 = parser.process(MENU_NO_DESC.as_bytes()).unwrap();
        assert_eq!(menu2.title, "Choose mode");
    }

    #[test]
    fn test_option_indices_are_zero_based() {
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(SIMPLE_MENU.as_bytes()).unwrap();

        assert_eq!(menu.options[0].index, 0);
        assert_eq!(menu.options[1].index, 1);
        assert_eq!(menu.options[2].index, 2);
    }

    #[test]
    fn test_ignores_non_menu_output() {
        let mut parser = TuiMenuParser::new();

        let regular_output = "Hello, I'm Claude. How can I help you today?\n\nI can assist with coding tasks.";
        let result = parser.process(regular_output.as_bytes());

        assert!(result.is_none());
        assert!(!parser.is_accumulating());
    }

    #[test]
    fn test_handles_arrow_highlight_variants() {
        let with_arrow = r#"
Select
  1. First
> 2. Second
  3. Third
Enter to confirm
"#;
        let mut parser = TuiMenuParser::new();
        let menu = parser.process(with_arrow.as_bytes()).unwrap();

        assert_eq!(menu.highlighted_index, 1);
    }

    #[test]
    fn test_normalize_unicode_escapes() {
        // Arrow symbols
        assert_eq!(normalize_unicode_escapes(r"\u2191\u2193"), "↑↓");
        // Middle dot
        assert_eq!(normalize_unicode_escapes(r"\u00B7"), "·");
        // Triangle pointer
        assert_eq!(normalize_unicode_escapes(r"\u25B8"), "▸");
        // Mixed with regular text
        assert_eq!(
            normalize_unicode_escapes(r"\u2191\u2193 Navigate \u00B7 Enter Select"),
            "↑↓ Navigate · Enter Select"
        );
        // Text without escapes should remain unchanged
        assert_eq!(normalize_unicode_escapes("plain text"), "plain text");
    }

    #[test]
    fn test_strip_ansi_with_dec_private_sequences() {
        // DEC private sequences use ? (e.g., show/hide cursor)
        let with_dec = "\x1b[?2026Hsome text\x1b[?2026l";
        let stripped = strip_ansi_codes(with_dec);
        assert_eq!(stripped, "some text");
    }

    #[test]
    fn test_strip_ansi_charset_switching() {
        // Charset switching sequences
        let with_charset = "\x1b(Bsome text\x1b)0";
        let stripped = strip_ansi_codes(with_charset);
        assert_eq!(stripped, "some text");
    }

    #[test]
    fn test_detects_menu_with_literal_unicode_footer() {
        // This simulates what Claude Code actually outputs - literal \u sequences
        let menu_with_literal_escapes = r#"
Select model

  1. Default
  2. Sonnet
  3. Haiku

\u2191\u2193 Navigate \u00B7 Enter Select \u00B7 Esc Cancel
"#;
        let mut parser = TuiMenuParser::new();
        let result = parser.process(menu_with_literal_escapes.as_bytes());

        assert!(result.is_some(), "Should detect menu with literal unicode escapes in footer");
        let menu = result.unwrap();
        assert_eq!(menu.options.len(), 3);
    }

    #[test]
    fn test_detects_menu_with_simplified_footer() {
        // Footer with "Navigate" pattern
        let menu_with_navigate = r#"
Select option
  1. First
  2. Second
Navigate with arrows
"#;
        let mut parser = TuiMenuParser::new();
        let result = parser.process(menu_with_navigate.as_bytes());

        assert!(result.is_some(), "Should detect menu with 'Navigate' in footer");
    }

    #[test]
    fn test_full_claude_code_menu_format() {
        // Complete menu as Claude Code outputs it (with ANSI codes and literal escapes)
        let realistic_menu = concat!(
            "\x1b[2J\x1b[H",  // Clear screen
            "Select model\r\n\r\n",
            "  1. Default (recommended)   Opus 4.5\r\n",
            "  2. Sonnet                  Sonnet 4.5\r\n",
            "\x1b[32m▸\x1b[0m 3. Haiku \x1b[33m✓\x1b[0m               Haiku 4.5\r\n",
            "\r\n",
            r"\u2191\u2193 Navigate \u00B7 Enter Select \u00B7 Esc Cancel",
            "\r\n",
        );

        let mut parser = TuiMenuParser::new();
        let result = parser.process(realistic_menu.as_bytes());

        assert!(result.is_some(), "Should detect realistic Claude Code menu");
        let menu = result.unwrap();
        assert_eq!(menu.title, "Select model");
        assert_eq!(menu.options.len(), 3);
        assert_eq!(menu.highlighted_index, 2); // ▸ on option 3
        assert!(menu.options[2].is_selected);  // ✓ on option 3
    }
}
