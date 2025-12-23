//! Core session and process management for Clauset.

mod buffer;
mod db;
mod diff;
mod error;
mod history;
mod interaction_store;
mod parser;
mod process;
mod session;
mod sizing;

pub use buffer::{AppendResult, RecentAction, SequencedChunk, SessionActivity, SessionBuffers};
pub use db::SessionStore;
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
pub use session::{CreateSessionOptions, SessionManager, SessionManagerConfig};
pub use sizing::{
    validate_dimensions, ConfidenceLevel, DeviceHint, DimensionError, DimensionSource,
    ValidatedDimensions,
};

/// Result type for Clauset operations.
pub type Result<T> = std::result::Result<T, ClausetError>;
