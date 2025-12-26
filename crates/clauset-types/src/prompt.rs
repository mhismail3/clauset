//! Prompt library types.
//!
//! Types for indexing and displaying user prompts from Claude Code sessions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A user prompt indexed from Claude Code session transcripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    /// Unique identifier for this prompt.
    pub id: Uuid,
    /// Claude Code's session ID (for linking to transcript).
    pub claude_session_id: String,
    /// Project path where this prompt was sent.
    pub project_path: PathBuf,
    /// Full prompt content.
    pub content: String,
    /// First 200 chars for list preview.
    pub preview: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
    /// Number of words in the prompt.
    pub word_count: u32,
    /// Number of characters in the prompt.
    pub char_count: u32,
}

impl Prompt {
    /// Create a new prompt entry.
    pub fn new(
        claude_session_id: String,
        project_path: PathBuf,
        content: String,
        timestamp: u64,
    ) -> Self {
        let preview = truncate_preview(&content, 200);
        let word_count = content.split_whitespace().count() as u32;
        let char_count = content.len() as u32;

        Self {
            id: Uuid::new_v4(),
            claude_session_id,
            project_path,
            content,
            preview,
            timestamp,
            word_count,
            char_count,
        }
    }

    /// Compute a hash of the content for deduplication.
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Extract the project name from the path.
    pub fn project_name(&self) -> String {
        self.project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Summary of a prompt for list display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSummary {
    /// Prompt ID.
    pub id: Uuid,
    /// First 200 chars preview.
    pub preview: String,
    /// Project name (extracted from path).
    pub project_name: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
    /// Word count.
    pub word_count: u32,
}

impl From<&Prompt> for PromptSummary {
    fn from(prompt: &Prompt) -> Self {
        Self {
            id: prompt.id,
            preview: prompt.preview.clone(),
            project_name: prompt.project_name(),
            timestamp: prompt.timestamp,
            word_count: prompt.word_count,
        }
    }
}

impl From<Prompt> for PromptSummary {
    fn from(prompt: Prompt) -> Self {
        let project_name = prompt.project_name();
        Self {
            id: prompt.id,
            preview: prompt.preview,
            project_name,
            timestamp: prompt.timestamp,
            word_count: prompt.word_count,
        }
    }
}

/// Truncate text to a maximum number of characters, adding ellipsis if truncated.
fn truncate_preview(text: &str, max_chars: usize) -> String {
    // Normalize whitespace first
    let normalized: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let char_count = normalized.chars().count();
    if char_count <= max_chars {
        normalized
    } else {
        // Find byte index for the max_chars boundary (safe for UTF-8)
        let byte_index = normalized
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(normalized.len());
        let truncated = &normalized[..byte_index];
        // Find a good break point (word boundary)
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &truncated[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_creation() {
        let prompt = Prompt::new(
            "session-123".to_string(),
            PathBuf::from("/Users/test/projects/myapp"),
            "Help me fix the bug in the login function".to_string(),
            1703894400000,
        );

        assert_eq!(prompt.project_name(), "myapp");
        assert_eq!(prompt.word_count, 9);
        assert!(!prompt.content_hash().is_empty());
    }

    #[test]
    fn test_truncate_preview() {
        let short = "Short text";
        assert_eq!(truncate_preview(short, 200), "Short text");

        let long = "a ".repeat(150);
        let truncated = truncate_preview(&long, 50);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_preview_multibyte() {
        // Test with multi-byte Unicode characters (box-drawing, emoji, etc.)
        let with_unicode = "Status ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Done ✓";
        let truncated = truncate_preview(with_unicode, 20);
        // Should not panic and should produce valid UTF-8
        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 23); // 20 + "..."
    }

    #[test]
    fn test_prompt_summary_from() {
        let prompt = Prompt::new(
            "session-456".to_string(),
            PathBuf::from("/home/user/project"),
            "Test prompt content".to_string(),
            1703894400000,
        );

        let summary: PromptSummary = (&prompt).into();
        assert_eq!(summary.id, prompt.id);
        assert_eq!(summary.project_name, "project");
    }
}
