//! Shared application state.

use crate::config::Config;
use clauset_core::{HistoryWatcher, SessionManager, SessionManagerConfig};
use std::sync::Arc;

/// Shared application state.
pub struct AppState {
    pub session_manager: Arc<SessionManager>,
    pub history_watcher: Arc<HistoryWatcher>,
    pub config: Config,
}

impl AppState {
    pub fn new(config: Config) -> clauset_core::Result<Self> {
        let session_config = SessionManagerConfig {
            claude_path: config.claude_path.clone(),
            db_path: config.db_path.clone(),
            max_concurrent_sessions: config.max_concurrent_sessions,
            default_model: config.default_model.clone(),
        };

        let session_manager = Arc::new(SessionManager::new(session_config)?);
        let history_watcher = Arc::new(HistoryWatcher::default());

        Ok(Self {
            session_manager,
            history_watcher,
            config,
        })
    }
}
