//! Prompt indexer for backfilling prompts from Claude's transcript files.
//!
//! This module handles the initial indexing of historical prompts from
//! `~/.claude/` on first run, as well as ongoing indexing from active sessions.

use crate::claude_sessions::ClaudeSessionReader;
use crate::interaction_store::InteractionStore;
use crate::Result;
use clauset_types::Prompt;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Statistics from a backfill operation.
#[derive(Debug, Clone, Default)]
pub struct BackfillStats {
    /// Number of sessions scanned.
    pub sessions_scanned: u32,
    /// Number of prompts indexed.
    pub prompts_indexed: u32,
    /// Number of prompts skipped (duplicates).
    pub prompts_skipped: u32,
    /// Number of errors encountered.
    pub errors: u32,
}

/// Indexes prompts from Claude Code transcript files.
pub struct PromptIndexer {
    claude_reader: ClaudeSessionReader,
    store: Arc<InteractionStore>,
}

impl PromptIndexer {
    /// Create a new prompt indexer.
    pub fn new(store: Arc<InteractionStore>) -> Self {
        Self {
            claude_reader: ClaudeSessionReader::new(),
            store,
        }
    }

    /// Check if backfill is needed (prompts table is empty).
    pub fn needs_backfill(&self) -> bool {
        match self.store.is_prompts_empty() {
            Ok(empty) => empty,
            Err(e) => {
                warn!(target: "clauset::prompt_indexer", "Failed to check prompts table: {}", e);
                false // Don't backfill on error
            }
        }
    }

    /// Backfill prompts from all Claude transcript files.
    /// This is called on server startup if the prompts table is empty.
    pub async fn backfill(&self) -> Result<BackfillStats> {
        let mut stats = BackfillStats::default();

        info!(target: "clauset::prompt_indexer", "Starting prompt backfill from ~/.claude");

        // Get all sessions from Claude's history
        let sessions = match self.claude_reader.list_all_sessions() {
            Ok(s) => s,
            Err(e) => {
                warn!(target: "clauset::prompt_indexer", "Failed to list sessions: {}", e);
                return Ok(stats);
            }
        };

        info!(
            target: "clauset::prompt_indexer",
            "Found {} sessions to scan",
            sessions.len()
        );

        for session in sessions {
            stats.sessions_scanned += 1;

            let project_path = session.project_path.clone();

            // Read transcript messages
            let messages = match self.claude_reader.read_transcript(&session.session_id, &project_path) {
                Ok(m) => m,
                Err(e) => {
                    debug!(
                        target: "clauset::prompt_indexer",
                        "Failed to read transcript for session {}: {}",
                        session.session_id, e
                    );
                    stats.errors += 1;
                    continue;
                }
            };

            // Extract and index user prompts
            for message in messages {
                if message.role != "user" {
                    continue;
                }

                // Skip empty or very short prompts
                if message.content.trim().len() < 2 {
                    continue;
                }

                let timestamp = message.timestamp.timestamp_millis() as u64;

                let prompt = Prompt::new(
                    session.session_id.clone(),
                    project_path.clone(),
                    message.content,
                    timestamp,
                );

                match self.store.insert_prompt(&prompt) {
                    Ok(_) => stats.prompts_indexed += 1,
                    Err(e) => {
                        // Duplicates are handled silently by the UPSERT
                        debug!(
                            target: "clauset::prompt_indexer",
                            "Failed to insert prompt: {}",
                            e
                        );
                        stats.prompts_skipped += 1;
                    }
                }
            }

            // Yield to allow other tasks to run
            if stats.sessions_scanned % 100 == 0 {
                tokio::task::yield_now().await;
            }
        }

        info!(
            target: "clauset::prompt_indexer",
            "Backfill complete: scanned {} sessions, indexed {} prompts, skipped {} duplicates, {} errors",
            stats.sessions_scanned,
            stats.prompts_indexed,
            stats.prompts_skipped,
            stats.errors
        );

        Ok(stats)
    }

    /// Index a single prompt from a hook event.
    /// This is called in real-time when UserPromptSubmit fires.
    pub fn index_prompt(
        &self,
        claude_session_id: &str,
        project_path: &str,
        content: &str,
    ) -> Result<()> {
        if content.trim().len() < 2 {
            return Ok(());
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let prompt = Prompt::new(
            claude_session_id.to_string(),
            PathBuf::from(project_path),
            content.to_string(),
            timestamp,
        );

        self.store.insert_prompt(&prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backfill_stats_default() {
        let stats = BackfillStats::default();
        assert_eq!(stats.sessions_scanned, 0);
        assert_eq!(stats.prompts_indexed, 0);
    }
}
