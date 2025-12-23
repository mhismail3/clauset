//! Interaction tracking types.
//!
//! These types represent the structured history of Claude interactions,
//! including prompts, tool invocations, and file snapshots for diffing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

/// Status of an interaction in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionStatus {
    /// Interaction started, Claude is processing.
    Active,
    /// Interaction completed successfully.
    Completed,
    /// Interaction failed or was interrupted.
    Failed,
}

impl Default for InteractionStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// A single prompt→response cycle within a session.
///
/// An interaction starts when the user submits a prompt (UserPromptSubmit hook)
/// and ends when Claude finishes responding (Stop hook with stop_hook_active=false).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    /// Unique identifier for this interaction.
    pub id: Uuid,
    /// Session this interaction belongs to.
    pub session_id: Uuid,
    /// Sequence number within the session (1, 2, 3, ...).
    pub sequence_number: u32,
    /// The user's prompt text.
    pub user_prompt: String,
    /// Generated summary of the assistant's response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_summary: Option<String>,
    /// When the interaction started.
    pub started_at: DateTime<Utc>,
    /// When the interaction completed (None if still active).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// Cost delta in USD for this interaction.
    pub cost_usd_delta: f64,
    /// Input tokens consumed in this interaction.
    pub input_tokens_delta: u64,
    /// Output tokens generated in this interaction.
    pub output_tokens_delta: u64,
    /// Current status.
    pub status: InteractionStatus,
    /// Error message if status is Failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl Interaction {
    /// Create a new interaction.
    pub fn new(session_id: Uuid, sequence_number: u32, user_prompt: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            sequence_number,
            user_prompt,
            assistant_summary: None,
            started_at: Utc::now(),
            ended_at: None,
            cost_usd_delta: 0.0,
            input_tokens_delta: 0,
            output_tokens_delta: 0,
            status: InteractionStatus::Active,
            error_message: None,
        }
    }

    /// Mark the interaction as completed.
    pub fn complete(&mut self) {
        self.status = InteractionStatus::Completed;
        self.ended_at = Some(Utc::now());
    }

    /// Mark the interaction as failed.
    pub fn fail(&mut self, error: String) {
        self.status = InteractionStatus::Failed;
        self.ended_at = Some(Utc::now());
        self.error_message = Some(error);
    }

    /// Duration of the interaction in milliseconds.
    pub fn duration_ms(&self) -> Option<i64> {
        self.ended_at.map(|end| (end - self.started_at).num_milliseconds())
    }
}

/// A single tool invocation within an interaction.
///
/// Created from PreToolUse hook, completed by PostToolUse hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// Unique identifier for this invocation.
    pub id: Uuid,
    /// Interaction this invocation belongs to.
    pub interaction_id: Uuid,
    /// Claude's tool_use_id for correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// Sequence number within the interaction (1, 2, 3, ...).
    pub sequence_number: u32,
    /// Name of the tool (Read, Write, Edit, Bash, etc.).
    pub tool_name: String,
    /// Tool input parameters as JSON.
    pub tool_input: Value,
    /// First 1KB of tool output (for preview).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output_preview: Option<String>,
    /// Extracted file path (for Read/Write/Edit tools).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<PathBuf>,
    /// Whether the tool returned an error.
    pub is_error: bool,
    /// Error message if is_error is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// When the tool started.
    pub started_at: DateTime<Utc>,
    /// When the tool completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

impl ToolInvocation {
    /// Create a new tool invocation from a PreToolUse event.
    pub fn new(
        interaction_id: Uuid,
        sequence_number: u32,
        tool_name: String,
        tool_input: Value,
        tool_use_id: Option<String>,
    ) -> Self {
        // Extract file_path from tool_input if present
        let file_path = tool_input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);

        Self {
            id: Uuid::new_v4(),
            interaction_id,
            tool_use_id,
            sequence_number,
            tool_name,
            tool_input,
            tool_output_preview: None,
            file_path,
            is_error: false,
            error_message: None,
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: None,
        }
    }

    /// Complete the invocation with a PostToolUse response.
    pub fn complete(&mut self, response: &Value) {
        let now = Utc::now();
        self.ended_at = Some(now);
        self.duration_ms = Some((now - self.started_at).num_milliseconds());

        // Check if response indicates an error
        self.is_error = response.get("error").is_some()
            || response
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        if self.is_error {
            self.error_message = response
                .get("error")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }

        // Store preview of output (first 1KB)
        if let Some(output) = response.get("output").or_else(|| response.get("content")) {
            let output_str = output.to_string();
            self.tool_output_preview = Some(if output_str.len() > 1024 {
                format!("{}...", &output_str[..1024])
            } else {
                output_str
            });
        }
    }
}

/// Type of file snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    /// File state before a Write/Edit operation.
    Before,
    /// File state after a Write/Edit operation.
    After,
}

/// Metadata about a file snapshot.
///
/// The actual content is stored in the file_contents table,
/// referenced by content_hash for deduplication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Unique identifier for this snapshot.
    pub id: Uuid,
    /// Interaction this snapshot belongs to.
    pub interaction_id: Uuid,
    /// Tool invocation that triggered this snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_invocation_id: Option<Uuid>,
    /// Absolute file path at time of snapshot.
    pub file_path: PathBuf,
    /// SHA256 hash of the file content (references file_contents table).
    pub content_hash: String,
    /// Type of snapshot (before/after).
    pub snapshot_type: SnapshotType,
    /// Original file size in bytes.
    pub file_size: u64,
    /// When the snapshot was taken.
    pub created_at: DateTime<Utc>,
}

impl FileSnapshot {
    /// Create a new file snapshot.
    pub fn new(
        interaction_id: Uuid,
        tool_invocation_id: Option<Uuid>,
        file_path: PathBuf,
        content_hash: String,
        snapshot_type: SnapshotType,
        file_size: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            interaction_id,
            tool_invocation_id,
            file_path,
            content_hash,
            snapshot_type,
            file_size,
            created_at: Utc::now(),
        }
    }
}

/// Stored file content with compression info.
///
/// Content is deduplicated by SHA256 hash - if two files have
/// the same content, they reference the same FileContent entry.
#[derive(Debug, Clone)]
pub struct FileContent {
    /// SHA256 hash of the uncompressed content.
    pub content_hash: String,
    /// zstd-compressed content.
    pub compressed_content: Vec<u8>,
    /// Original uncompressed size in bytes.
    pub original_size: u64,
    /// Compression ratio (original / compressed).
    pub compression_ratio: f64,
    /// When this content was first stored.
    pub created_at: DateTime<Utc>,
    /// Number of snapshots referencing this content.
    pub reference_count: u32,
}

/// Summary of an interaction for listing/display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionSummary {
    /// Interaction ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: Uuid,
    /// Sequence number.
    pub sequence_number: u32,
    /// Truncated prompt preview.
    pub prompt_preview: String,
    /// Number of tool invocations.
    pub tool_count: u32,
    /// Number of files changed.
    pub files_changed: u32,
    /// Cost in USD.
    pub cost_usd_delta: f64,
    /// Input tokens.
    pub input_tokens_delta: u64,
    /// Output tokens.
    pub output_tokens_delta: u64,
    /// Status.
    pub status: InteractionStatus,
    /// When started.
    pub started_at: DateTime<Utc>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

impl InteractionSummary {
    /// Create a summary from an interaction with aggregated counts.
    pub fn from_interaction(interaction: &Interaction, tool_count: u32, files_changed: u32) -> Self {
        // Create a truncated preview of the prompt
        let prompt_preview = if interaction.user_prompt.len() > 100 {
            format!("{}...", &interaction.user_prompt[..100])
        } else {
            interaction.user_prompt.clone()
        };

        Self {
            id: interaction.id,
            session_id: interaction.session_id,
            sequence_number: interaction.sequence_number,
            prompt_preview,
            tool_count,
            files_changed,
            cost_usd_delta: interaction.cost_usd_delta,
            input_tokens_delta: interaction.input_tokens_delta,
            output_tokens_delta: interaction.output_tokens_delta,
            status: interaction.status,
            started_at: interaction.started_at,
            duration_ms: interaction.duration_ms(),
        }
    }
}

/// A file change within an interaction (for display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path.
    pub file_path: PathBuf,
    /// Type of change.
    pub change_type: FileChangeType,
    /// Before snapshot ID (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_snapshot_id: Option<Uuid>,
    /// After snapshot ID (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_snapshot_id: Option<Uuid>,
}

/// Type of file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeType {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_lifecycle() {
        let session_id = Uuid::new_v4();
        let mut interaction = Interaction::new(session_id, 1, "Test prompt".to_string());

        assert_eq!(interaction.status, InteractionStatus::Active);
        assert!(interaction.ended_at.is_none());

        interaction.complete();

        assert_eq!(interaction.status, InteractionStatus::Completed);
        assert!(interaction.ended_at.is_some());
        assert!(interaction.duration_ms().unwrap() >= 0);
    }

    #[test]
    fn test_tool_invocation_complete() {
        let interaction_id = Uuid::new_v4();
        let input = serde_json::json!({
            "file_path": "/test/file.rs",
            "content": "test content"
        });

        let mut invocation = ToolInvocation::new(
            interaction_id,
            1,
            "Write".to_string(),
            input,
            Some("toolu_123".to_string()),
        );

        assert_eq!(invocation.file_path, Some(PathBuf::from("/test/file.rs")));
        assert!(invocation.ended_at.is_none());

        let response = serde_json::json!({
            "output": "File written successfully"
        });
        invocation.complete(&response);

        assert!(invocation.ended_at.is_some());
        assert!(!invocation.is_error);
        assert!(invocation.tool_output_preview.is_some());
    }

    #[test]
    fn test_tool_invocation_error() {
        let interaction_id = Uuid::new_v4();
        let input = serde_json::json!({"file_path": "/nonexistent"});

        let mut invocation = ToolInvocation::new(
            interaction_id,
            1,
            "Read".to_string(),
            input,
            None,
        );

        let response = serde_json::json!({
            "is_error": true,
            "error": "File not found"
        });
        invocation.complete(&response);

        assert!(invocation.is_error);
        assert_eq!(invocation.error_message, Some("File not found".to_string()));
    }
}
