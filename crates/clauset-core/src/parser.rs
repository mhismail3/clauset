//! Parser for Claude CLI stream-json output.

use crate::Result;
use clauset_types::ClaudeEvent;

/// Parser for Claude's stream-json output format.
#[derive(Debug, Default)]
pub struct OutputParser {
    /// Buffer for incomplete JSON.
    buffer: String,
}

impl OutputParser {
    /// Create a new parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a line of output into a Claude event.
    pub fn parse_line(&mut self, line: &str) -> Result<Option<ClaudeEvent>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        // Try to parse as JSON
        match serde_json::from_str::<ClaudeEvent>(trimmed) {
            Ok(event) => Ok(Some(event)),
            Err(e) => {
                // Log but don't fail on parse errors - Claude outputs non-JSON sometimes
                tracing::debug!("Failed to parse line as ClaudeEvent: {}: {}", e, trimmed);
                Ok(None)
            }
        }
    }

    /// Parse streaming data that may contain partial lines.
    pub fn parse_chunk(&mut self, chunk: &str) -> Vec<ClaudeEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if let Ok(Some(event)) = self.parse_line(&line) {
                events.push(event);
            }
        }

        events
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assistant_event() {
        let mut parser = OutputParser::new();
        // Test parsing a valid assistant event from Claude CLI stream-json format
        let line = r#"{"type":"assistant","session_id":"00000000-0000-0000-0000-000000000001","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        let event = parser.parse_line(line).unwrap();
        assert!(event.is_some());

        // Verify it parsed as an Assistant event
        if let Some(ClaudeEvent::Assistant(assistant)) = event {
            assert_eq!(assistant.message.id, "msg_1");
            assert_eq!(assistant.message.role, "assistant");
        } else {
            panic!("Expected Assistant event");
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let mut parser = OutputParser::new();
        // Invalid JSON should return None, not error
        let event = parser.parse_line("not json").unwrap();
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_chunk() {
        let mut parser = OutputParser::new();
        // Test parsing chunked data with partial lines
        let chunk1 = r#"{"type":"assistant","session_id":"00000000-0000-0000-0000-000000000001","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Hi"}]}}"#;
        let chunk2 = "\n";

        // First chunk has no newline, should return empty
        let events = parser.parse_chunk(chunk1);
        assert!(events.is_empty());

        // Second chunk completes the line
        let events = parser.parse_chunk(chunk2);
        assert_eq!(events.len(), 1);
    }
}
