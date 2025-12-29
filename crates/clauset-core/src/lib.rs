//! Core session and process management for Clauset.

mod buffer;
mod chat_processor;
mod claude_sessions;
mod command_discovery;
mod db;
mod diff;
mod error;
mod history;
mod interaction_store;
mod parser;
mod process;
mod prompt_indexer;
mod session;
mod sizing;
mod transcript_watcher;
mod tui_menu_parser;

pub use buffer::{AppendResult, RecentAction, SequencedChunk, SessionActivity, SessionBuffers};
pub use chat_processor::ChatProcessor;
pub use command_discovery::CommandDiscovery;
pub use claude_sessions::{ClaudeSession, ClaudeSessionReader, TranscriptMessage};
pub use db::{SessionStore, TerminalBufferData};
pub use diff::{compute_diff, generate_unified_diff, DiffChangeType, DiffHunk, DiffLine, FileDiff};
pub use error::ClausetError;
pub use history::HistoryWatcher;
pub use interaction_store::{
    AnalyticsSummary, CleanupStats, DailyCostEntry, FileChangeWithDiff, FilePathMatch,
    GlobalSearchResults, InteractionStore, SearchField, SearchResult, SessionAnalytics,
    StorageStats, ToolCostEntry, DEFAULT_RETENTION_DAYS, MAX_SNAPSHOT_SIZE,
};
pub use parser::OutputParser;
pub use process::{ProcessEvent, ProcessManager, SpawnOptions};
pub use prompt_indexer::{BackfillStats, PromptIndexer};
pub use session::{CreateSessionOptions, SessionManager, SessionManagerConfig};
pub use sizing::{
    validate_dimensions, ConfidenceLevel, DeviceHint, DimensionError, DimensionSource,
    ValidatedDimensions,
};
pub use transcript_watcher::{
    transcript_event_to_chat_event, TranscriptEvent, TranscriptWatcher, TranscriptWatcherHandle,
};
pub use tui_menu_parser::TuiMenuParser;

/// Result type for Clauset operations.
pub type Result<T> = std::result::Result<T, ClausetError>;
