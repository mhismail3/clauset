//! Core session and process management for Clauset.

mod buffer;
mod db;
mod error;
mod history;
mod parser;
mod process;
mod session;

pub use buffer::{RecentAction, SessionActivity, SessionBuffers};
pub use db::SessionStore;
pub use error::ClausetError;
pub use history::HistoryWatcher;
pub use parser::OutputParser;
pub use process::{ProcessEvent, ProcessManager, SpawnOptions};
pub use session::{CreateSessionOptions, SessionManager, SessionManagerConfig};

/// Result type for Clauset operations.
pub type Result<T> = std::result::Result<T, ClausetError>;
