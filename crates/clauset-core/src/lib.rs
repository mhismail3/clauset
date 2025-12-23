//! Core session and process management for Clauset.

mod buffer;
mod db;
mod error;
mod history;
mod parser;
mod process;
mod session;
mod sizing;

pub use buffer::{AppendResult, RecentAction, SequencedChunk, SessionActivity, SessionBuffers};
pub use db::SessionStore;
pub use error::ClausetError;
pub use history::HistoryWatcher;
pub use parser::OutputParser;
pub use process::{ProcessEvent, ProcessManager, SpawnOptions};
pub use session::{CreateSessionOptions, SessionManager, SessionManagerConfig};
pub use sizing::{
    validate_dimensions, ConfidenceLevel, DeviceHint, DimensionError, DimensionSource,
    ValidatedDimensions,
};

/// Result type for Clauset operations.
pub type Result<T> = std::result::Result<T, ClausetError>;
