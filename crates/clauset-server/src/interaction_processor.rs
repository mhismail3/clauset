//! Interaction capture engine for persistent interaction tracking.
//!
//! This module captures Claude interactions (user prompts + tool invocations)
//! and persists them to the database for timeline, search, and analytics features.

use clauset_core::InteractionStore;
use clauset_types::{FileSnapshot, HookEvent, Interaction, SnapshotType, ToolInvocation};
use dashmap::DashMap;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum file size for snapshots (1 MB).
const MAX_SNAPSHOT_SIZE: u64 = 1_048_576;

/// Captures interactions and tool invocations from hook events.
pub struct InteractionProcessor {
    store: Arc<InteractionStore>,
    /// Maps session_id -> current active interaction_id
    active_interactions: DashMap<Uuid, Uuid>,
    /// Maps tool_use_id -> (tool_invocation_id, interaction_id, cwd)
    pending_tool_invocations: DashMap<String, (Uuid, Uuid, Option<String>)>,
}

impl InteractionProcessor {
    pub fn new(store: Arc<InteractionStore>) -> Self {
        Self {
            store,
            active_interactions: DashMap::new(),
            pending_tool_invocations: DashMap::new(),
        }
    }

    /// Process a hook event and update the interaction tracking state.
    pub async fn process_event(&self, event: &HookEvent) {
        if let Err(e) = self.process_event_inner(event).await {
            error!(target: "clauset::interactions", "Failed to process hook event: {}", e);
        }
    }

    async fn process_event_inner(
        &self,
        event: &HookEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            HookEvent::UserPromptSubmit {
                session_id, prompt, ..
            } => {
                self.handle_user_prompt(*session_id, prompt).await?;
            }

            HookEvent::PreToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_use_id,
                cwd,
                ..
            } => {
                self.handle_pre_tool_use(
                    *session_id,
                    tool_name,
                    tool_input,
                    tool_use_id,
                    cwd.clone(),
                )
                .await?;
            }

            HookEvent::PostToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_response,
                tool_use_id,
                ..
            } => {
                self.handle_post_tool_use(
                    *session_id,
                    tool_name,
                    tool_input,
                    tool_response,
                    tool_use_id,
                )
                .await?;
            }

            HookEvent::Stop {
                session_id,
                stop_hook_active,
                ..
            } => {
                if !stop_hook_active {
                    self.handle_stop(*session_id).await?;
                }
            }

            HookEvent::SessionEnd { session_id, .. } => {
                // Complete any active interaction when session ends
                self.handle_stop(*session_id).await?;
            }

            _ => {
                // Other events don't affect interaction tracking
            }
        }

        Ok(())
    }

    /// Handle UserPromptSubmit: Create a new interaction.
    async fn handle_user_prompt(
        &self,
        session_id: Uuid,
        prompt: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Complete any existing interaction first
        if let Some((_, existing_id)) = self.active_interactions.remove(&session_id) {
            debug!(target: "clauset::interactions",
                "Completing previous interaction {} before starting new one", existing_id);
            if let Err(e) = self.store.complete_interaction(existing_id) {
                warn!(target: "clauset::interactions",
                    "Failed to complete previous interaction: {}", e);
            }
        }

        // Get next sequence number
        let seq_num = self.store.next_sequence_number(session_id)?;

        // Create new interaction
        let interaction = Interaction::new(session_id, seq_num, prompt.to_string());
        let interaction_id = interaction.id;

        self.store.insert_interaction(&interaction)?;
        self.active_interactions.insert(session_id, interaction_id);

        info!(target: "clauset::interactions",
            "Started interaction {} (seq {}) for session {}",
            interaction_id, seq_num, session_id);

        Ok(())
    }

    /// Handle PreToolUse: Create tool invocation and capture before snapshot.
    async fn handle_pre_tool_use(
        &self,
        session_id: Uuid,
        tool_name: &str,
        tool_input: &Value,
        tool_use_id: &str,
        cwd: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get or create active interaction
        let interaction_id = match self.active_interactions.get(&session_id) {
            Some(id) => *id,
            None => {
                // Create a synthetic interaction if none exists
                // This can happen if we missed the UserPromptSubmit event
                debug!(target: "clauset::interactions",
                    "No active interaction for session {}, creating synthetic one", session_id);
                let seq_num = self.store.next_sequence_number(session_id)?;
                let interaction =
                    Interaction::new(session_id, seq_num, "(no prompt captured)".to_string());
                let id = interaction.id;
                self.store.insert_interaction(&interaction)?;
                self.active_interactions.insert(session_id, id);
                id
            }
        };

        // Get next tool sequence number
        let tool_seq = self
            .store
            .list_tool_invocations(interaction_id)?
            .len() as u32
            + 1;

        // Create tool invocation
        let invocation = ToolInvocation::new(
            interaction_id,
            tool_seq,
            tool_name.to_string(),
            tool_input.clone(),
            if tool_use_id.is_empty() {
                None
            } else {
                Some(tool_use_id.to_string())
            },
        );
        let invocation_id = invocation.id;

        // Extract file path for Write/Edit tools
        let file_path = self.extract_file_path(tool_input);

        // Store invocation with file_path
        let mut inv = invocation;
        inv.file_path = file_path.clone();
        self.store.insert_tool_invocation(&inv)?;

        // Store pending invocation for PostToolUse
        self.pending_tool_invocations.insert(
            tool_use_id.to_string(),
            (invocation_id, interaction_id, cwd.clone()),
        );

        // Capture before snapshot for Write/Edit tools
        if matches!(tool_name, "Write" | "Edit") {
            if let Some(ref rel_path) = file_path {
                let abs_path = self.resolve_path(rel_path, cwd.as_deref());
                self.capture_snapshot(
                    interaction_id,
                    Some(invocation_id),
                    &abs_path,
                    SnapshotType::Before,
                )
                .await;
            }
        }

        debug!(target: "clauset::interactions",
            "Started tool invocation {} ({}) for interaction {}",
            invocation_id, tool_name, interaction_id);

        Ok(())
    }

    /// Handle PostToolUse: Complete tool invocation and capture after snapshot.
    async fn handle_post_tool_use(
        &self,
        _session_id: Uuid,
        tool_name: &str,
        _tool_input: &Value,
        tool_response: &Value,
        tool_use_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Look up pending invocation
        let (invocation_id, interaction_id, cwd) =
            match self.pending_tool_invocations.remove(tool_use_id) {
                Some((_, data)) => data,
                None => {
                    // Try to find by tool_use_id in database
                    if let Some(inv) = self.store.get_tool_invocation_by_tool_use_id(tool_use_id)? {
                        // Get cwd from first invocation's context (not ideal but workable)
                        (inv.id, inv.interaction_id, None)
                    } else {
                        debug!(target: "clauset::interactions",
                            "No pending tool invocation for tool_use_id {}", tool_use_id);
                        return Ok(());
                    }
                }
            };

        // Check for error
        let is_error = tool_response.get("error").is_some()
            || tool_response
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        let error_message = if is_error {
            tool_response
                .get("error")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    tool_response
                        .get("message")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
        } else {
            None
        };

        // Extract preview from response
        let preview = self.extract_response_preview(tool_response);

        // Complete the tool invocation
        self.store
            .complete_tool_invocation(invocation_id, preview, is_error, error_message)?;

        // Capture after snapshot for Write/Edit tools
        if matches!(tool_name, "Write" | "Edit") {
            // Get the file path from the stored invocation
            if let Some(inv) = self.store.get_tool_invocation(invocation_id)? {
                if let Some(ref rel_path) = inv.file_path {
                    let abs_path = self.resolve_path(rel_path, cwd.as_deref());
                    self.capture_snapshot(
                        interaction_id,
                        Some(invocation_id),
                        &abs_path,
                        SnapshotType::After,
                    )
                    .await;
                }
            }
        }

        debug!(target: "clauset::interactions",
            "Completed tool invocation {} ({}) error={}",
            invocation_id, tool_name, is_error);

        Ok(())
    }

    /// Handle Stop: Complete the current interaction.
    async fn handle_stop(
        &self,
        session_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some((_, interaction_id)) = self.active_interactions.remove(&session_id) {
            self.store.complete_interaction(interaction_id)?;
            info!(target: "clauset::interactions",
                "Completed interaction {} for session {}", interaction_id, session_id);
        }

        Ok(())
    }

    /// Extract file path from tool input.
    fn extract_file_path(&self, tool_input: &Value) -> Option<PathBuf> {
        tool_input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
    }

    /// Resolve a relative path using the working directory.
    fn resolve_path(&self, file_path: &Path, cwd: Option<&str>) -> PathBuf {
        if file_path.is_absolute() {
            file_path.to_path_buf()
        } else if let Some(cwd) = cwd {
            Path::new(cwd).join(file_path)
        } else {
            file_path.to_path_buf()
        }
    }

    /// Capture a file snapshot (before or after modification).
    async fn capture_snapshot(
        &self,
        interaction_id: Uuid,
        tool_invocation_id: Option<Uuid>,
        file_path: &Path,
        snapshot_type: SnapshotType,
    ) {
        // Check if file exists and is readable
        let metadata = match tokio::fs::metadata(file_path).await {
            Ok(m) => m,
            Err(e) => {
                // File might not exist (for Write creating new file)
                debug!(target: "clauset::interactions",
                    "Cannot read file metadata for snapshot {:?}: {}", file_path, e);
                return;
            }
        };

        // Skip files that are too large
        if metadata.len() > MAX_SNAPSHOT_SIZE {
            debug!(target: "clauset::interactions",
                "Skipping snapshot for large file {:?} ({}  bytes)", file_path, metadata.len());
            return;
        }

        // Read file content
        let content = match tokio::fs::read(file_path).await {
            Ok(c) => c,
            Err(e) => {
                debug!(target: "clauset::interactions",
                    "Cannot read file for snapshot {:?}: {}", file_path, e);
                return;
            }
        };

        // Store the content (with deduplication)
        let (content_hash, _is_new) = match self.store.store_file_content(&content) {
            Ok(result) => result,
            Err(e) => {
                error!(target: "clauset::interactions",
                    "Failed to store file content: {}", e);
                return;
            }
        };

        // Create snapshot record
        let snapshot = FileSnapshot::new(
            interaction_id,
            tool_invocation_id,
            file_path.to_path_buf(),
            content_hash,
            snapshot_type,
            metadata.len(),
        );

        if let Err(e) = self.store.insert_file_snapshot(&snapshot) {
            error!(target: "clauset::interactions",
                "Failed to store file snapshot: {}", e);
        } else {
            debug!(target: "clauset::interactions",
                "Captured {:?} snapshot for {:?}", snapshot_type, file_path);
        }
    }

    /// Extract a preview from the tool response.
    fn extract_response_preview(&self, tool_response: &Value) -> Option<String> {
        // Try common response fields
        if let Some(s) = tool_response.as_str() {
            return Some(truncate(s, 500));
        }

        if let Some(content) = tool_response.get("content").and_then(|v| v.as_str()) {
            return Some(truncate(content, 500));
        }

        if let Some(output) = tool_response.get("output").and_then(|v| v.as_str()) {
            return Some(truncate(output, 500));
        }

        if let Some(result) = tool_response.get("result").and_then(|v| v.as_str()) {
            return Some(truncate(result, 500));
        }

        // For arrays or objects, just note the type
        if tool_response.is_array() {
            let len = tool_response.as_array().map(|a| a.len()).unwrap_or(0);
            return Some(format!("[array of {} items]", len));
        }

        if tool_response.is_object() {
            let keys: Vec<_> = tool_response
                .as_object()
                .map(|o| o.keys().take(5).cloned().collect())
                .unwrap_or_default();
            return Some(format!("{{{}...}}", keys.join(", ")));
        }

        None
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &Arc<InteractionStore> {
        &self.store
    }

    /// Get storage statistics.
    pub fn get_storage_stats(&self) -> Result<clauset_core::StorageStats, clauset_core::ClausetError> {
        self.store.get_storage_stats()
    }

    /// Clean up old data based on retention policy.
    pub fn cleanup_old_data(
        &self,
        retention_days: i64,
    ) -> Result<clauset_core::CleanupStats, clauset_core::ClausetError> {
        self.store.cleanup_old_data(retention_days)
    }
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
