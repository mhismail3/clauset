//! Shared application state.

use crate::config::Config;
use crate::interaction_processor::InteractionProcessor;
use clauset_core::{
    ChatProcessor, CommandDiscovery, HistoryWatcher, InteractionStore, SessionManager,
    SessionManagerConfig,
};
use std::sync::{Arc, Mutex};

/// Shared application state.
pub struct AppState {
    pub session_manager: Arc<SessionManager>,
    pub history_watcher: Arc<HistoryWatcher>,
    pub interaction_processor: Arc<InteractionProcessor>,
    pub chat_processor: Arc<ChatProcessor>,
    pub command_discovery: Mutex<CommandDiscovery>,
    pub config: Config,
}

impl AppState {
    pub fn new(config: Config) -> clauset_core::Result<Self> {
        // Build the URL that hooks should use to send events back to this server
        let clauset_url = format!("http://localhost:{}", config.port);

        let session_config = SessionManagerConfig {
            claude_path: config.claude_path.clone(),
            db_path: config.db_path.clone(),
            max_concurrent_sessions: config.max_concurrent_sessions,
            default_model: config.default_model.clone(),
            clauset_url,
        };

        let session_manager = Arc::new(SessionManager::new(session_config)?);
        let history_watcher = Arc::new(HistoryWatcher::default());
        let interaction_store = Arc::new(InteractionStore::open(&config.db_path)?);
        let interaction_processor = Arc::new(InteractionProcessor::new(interaction_store.clone()));
        let chat_processor = Arc::new(ChatProcessor::with_store(interaction_store));
        let command_discovery = Mutex::new(CommandDiscovery::new());

        Ok(Self {
            session_manager,
            history_watcher,
            interaction_processor,
            chat_processor,
            command_discovery,
            config,
        })
    }
}
