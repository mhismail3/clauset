//! Diff computation for file snapshots.
//!
//! This module computes line-based diffs between before/after file snapshots,
//! providing structured output suitable for display in the frontend.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// A single line change in a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    /// Type of change: "add", "remove", or "context"
    pub change_type: DiffChangeType,
    /// Line number in the old file (None for additions)
    pub old_line_num: Option<u32>,
    /// Line number in the new file (None for deletions)
    pub new_line_num: Option<u32>,
    /// The actual line content
    pub content: String,
}

/// Type of change in a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffChangeType {
    /// Line was added
    Add,
    /// Line was removed
    Remove,
    /// Line is unchanged (context)
    Context,
}

/// A hunk (contiguous block of changes) in a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// Starting line number in old file
    pub old_start: u32,
    /// Number of lines in old file
    pub old_count: u32,
    /// Starting line number in new file
    pub new_start: u32,
    /// Number of lines in new file
    pub new_count: u32,
    /// The lines in this hunk
    pub lines: Vec<DiffLine>,
}

/// Complete diff result for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// Total lines added
    pub lines_added: u32,
    /// Total lines removed
    pub lines_removed: u32,
    /// The hunks (contiguous blocks of changes)
    pub hunks: Vec<DiffHunk>,
    /// Whether files are identical
    pub is_identical: bool,
    /// Whether either file is binary
    pub is_binary: bool,
}

impl FileDiff {
    /// Create a diff indicating files are identical.
    pub fn identical() -> Self {
        Self {
            lines_added: 0,
            lines_removed: 0,
            hunks: Vec::new(),
            is_identical: true,
            is_binary: false,
        }
    }

    /// Create a diff indicating binary content.
    pub fn binary() -> Self {
        Self {
            lines_added: 0,
            lines_removed: 0,
            hunks: Vec::new(),
            is_identical: false,
            is_binary: true,
        }
    }
}

/// Compute a diff between two file contents.
///
/// # Arguments
/// * `old_content` - The content before modification (may be None for new files)
/// * `new_content` - The content after modification (may be None for deleted files)
/// * `context_lines` - Number of context lines to include around changes (default: 3)
///
/// # Returns
/// A `FileDiff` containing the structured diff result.
pub fn compute_diff(
    old_content: Option<&[u8]>,
    new_content: Option<&[u8]>,
    context_lines: usize,
) -> FileDiff {
    // Handle edge cases
    match (old_content, new_content) {
        (None, None) => return FileDiff::identical(),
        (Some(old), Some(new)) if old == new => return FileDiff::identical(),
        _ => {}
    }

    // Convert to strings, checking for binary content
    let old_str = old_content.map(|c| String::from_utf8_lossy(c));
    let new_str = new_content.map(|c| String::from_utf8_lossy(c));

    // Check for binary content (null bytes indicate binary)
    let old_is_binary = old_content.map(|c| c.contains(&0)).unwrap_or(false);
    let new_is_binary = new_content.map(|c| c.contains(&0)).unwrap_or(false);

    if old_is_binary || new_is_binary {
        return FileDiff::binary();
    }

    let old_text = old_str.as_deref().unwrap_or("");
    let new_text = new_str.as_deref().unwrap_or("");

    // Compute the diff using similar
    let diff = TextDiff::from_lines(old_text, new_text);

    let mut hunks = Vec::new();
    let mut lines_added = 0u32;
    let mut lines_removed = 0u32;

    // Group changes into hunks with context
    for group in diff.grouped_ops(context_lines) {
        let mut hunk_lines = Vec::new();
        let mut old_start = 0u32;
        let mut new_start = 0u32;
        let mut old_count = 0u32;
        let mut new_count = 0u32;
        let mut first = true;

        for op in group {
            for change in diff.iter_changes(&op) {
                let (old_line, new_line) = match change.tag() {
                    ChangeTag::Delete => {
                        lines_removed += 1;
                        old_count += 1;
                        let old_idx = change.old_index().map(|i| i as u32 + 1);
                        if first {
                            old_start = old_idx.unwrap_or(1);
                            first = false;
                        }
                        (old_idx, None)
                    }
                    ChangeTag::Insert => {
                        lines_added += 1;
                        new_count += 1;
                        let new_idx = change.new_index().map(|i| i as u32 + 1);
                        if first {
                            new_start = new_idx.unwrap_or(1);
                            first = false;
                        }
                        (None, new_idx)
                    }
                    ChangeTag::Equal => {
                        old_count += 1;
                        new_count += 1;
                        let old_idx = change.old_index().map(|i| i as u32 + 1);
                        let new_idx = change.new_index().map(|i| i as u32 + 1);
                        if first {
                            old_start = old_idx.unwrap_or(1);
                            new_start = new_idx.unwrap_or(1);
                            first = false;
                        }
                        (old_idx, new_idx)
                    }
                };

                let change_type = match change.tag() {
                    ChangeTag::Delete => DiffChangeType::Remove,
                    ChangeTag::Insert => DiffChangeType::Add,
                    ChangeTag::Equal => DiffChangeType::Context,
                };

                // Remove trailing newline from content for cleaner display
                let content = change.value().trim_end_matches('\n').to_string();

                hunk_lines.push(DiffLine {
                    change_type,
                    old_line_num: old_line,
                    new_line_num: new_line,
                    content,
                });
            }
        }

        if !hunk_lines.is_empty() {
            hunks.push(DiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: hunk_lines,
            });
        }
    }

    FileDiff {
        lines_added,
        lines_removed,
        hunks,
        is_identical: lines_added == 0 && lines_removed == 0,
        is_binary: false,
    }
}

/// Generate a unified diff string (like `diff -u` output).
pub fn generate_unified_diff(
    old_content: Option<&[u8]>,
    new_content: Option<&[u8]>,
    old_path: &str,
    new_path: &str,
    context_lines: usize,
) -> String {
    let old_str = old_content.map(|c| String::from_utf8_lossy(c));
    let new_str = new_content.map(|c| String::from_utf8_lossy(c));

    let old_text = old_str.as_deref().unwrap_or("");
    let new_text = new_str.as_deref().unwrap_or("");

    let diff = TextDiff::from_lines(old_text, new_text);

    diff.unified_diff()
        .context_radius(context_lines)
        .header(old_path, new_path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_files() {
        let content = b"line1\nline2\nline3\n";
        let diff = compute_diff(Some(content), Some(content), 3);

        assert!(diff.is_identical);
        assert_eq!(diff.lines_added, 0);
        assert_eq!(diff.lines_removed, 0);
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn test_new_file() {
        let content = b"line1\nline2\n";
        let diff = compute_diff(None, Some(content), 3);

        assert!(!diff.is_identical);
        assert_eq!(diff.lines_added, 2);
        assert_eq!(diff.lines_removed, 0);
        assert_eq!(diff.hunks.len(), 1);
    }

    #[test]
    fn test_deleted_file() {
        let content = b"line1\nline2\n";
        let diff = compute_diff(Some(content), None, 3);

        assert!(!diff.is_identical);
        assert_eq!(diff.lines_added, 0);
        assert_eq!(diff.lines_removed, 2);
        assert_eq!(diff.hunks.len(), 1);
    }

    #[test]
    fn test_modified_file() {
        let old = b"line1\nline2\nline3\n";
        let new = b"line1\nmodified\nline3\n";
        let diff = compute_diff(Some(old), Some(new), 3);

        assert!(!diff.is_identical);
        assert_eq!(diff.lines_added, 1);
        assert_eq!(diff.lines_removed, 1);
        assert_eq!(diff.hunks.len(), 1);
    }

    #[test]
    fn test_binary_detection() {
        let binary = b"hello\x00world";
        let text = b"hello world";
        let diff = compute_diff(Some(binary), Some(text), 3);

        assert!(diff.is_binary);
    }

    #[test]
    fn test_unified_diff_output() {
        let old = b"line1\nline2\nline3\n";
        let new = b"line1\nmodified\nline3\n";

        let unified = generate_unified_diff(
            Some(old),
            Some(new),
            "a/file.txt",
            "b/file.txt",
            3,
        );

        assert!(unified.contains("--- a/file.txt"));
        assert!(unified.contains("+++ b/file.txt"));
        assert!(unified.contains("-line2"));
        assert!(unified.contains("+modified"));
    }
}
