//! History reader for ~/.claude/history.jsonl.

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// An entry from Claude's history.jsonl file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    #[serde(default)]
    pub pasted_contents: HashMap<String, serde_json::Value>,
    pub timestamp: i64,
    pub project: PathBuf,
    #[serde(default)]
    pub session_id: Option<Uuid>,
}

/// Watches and reads Claude's history file.
pub struct HistoryWatcher {
    entries: Arc<RwLock<Vec<HistoryEntry>>>,
    history_path: PathBuf,
}

impl HistoryWatcher {
    /// Create a new history watcher.
    pub fn new() -> Result<Self> {
        let history_path = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
            .join(".claude")
            .join("history.jsonl");

        let entries = Arc::new(RwLock::new(Vec::new()));

        let watcher = Self {
            entries,
            history_path,
        };

        // Initial load
        watcher.reload()?;

        Ok(watcher)
    }

    /// Reload history from disk.
    pub fn reload(&self) -> Result<()> {
        if !self.history_path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&self.history_path)?;
        let reader = BufReader::new(file);
        let mut new_entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                new_entries.push(entry);
            }
        }

        // Sort by timestamp descending
        new_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        *self.entries.write().unwrap() = new_entries;
        Ok(())
    }

    /// Get recent history entries.
    pub fn get_entries(&self, limit: Option<usize>) -> Vec<HistoryEntry> {
        let entries = self.entries.read().unwrap();
        match limit {
            Some(n) => entries.iter().take(n).cloned().collect(),
            None => entries.clone(),
        }
    }

    /// Get entries for a specific session.
    pub fn get_by_session(&self, session_id: Uuid) -> Vec<HistoryEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.session_id == Some(session_id))
            .cloned()
            .collect()
    }

    /// Get entries for a specific project.
    pub fn get_by_project(&self, project_path: &PathBuf) -> Vec<HistoryEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| &e.project == project_path)
            .cloned()
            .collect()
    }
}

impl Default for HistoryWatcher {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            history_path: PathBuf::new(),
        })
    }
}
