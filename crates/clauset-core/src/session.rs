//! Session manager orchestrating processes and persistence.

use crate::{ClausetError, ProcessEvent, ProcessManager, Result, SessionStore, SpawnOptions};
use clauset_types::{Session, SessionMode, SessionStatus, SessionSummary};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};
use uuid::Uuid;

/// Configuration for the session manager.
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    pub claude_path: PathBuf,
    pub db_path: PathBuf,
    pub max_concurrent_sessions: usize,
    pub default_model: String,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            claude_path: PathBuf::from("/opt/homebrew/bin/claude"),
            db_path: dirs::data_local_dir()
                .unwrap_or_default()
                .join("clauset")
                .join("sessions.db"),
            max_concurrent_sessions: 10,
            default_model: "sonnet".to_string(),
        }
    }
}

/// Options for creating a new session.
#[derive(Debug, Clone)]
pub struct CreateSessionOptions {
    pub project_path: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
    pub mode: SessionMode,
    pub resume_session_id: Option<Uuid>,
}

/// Manages Claude Code sessions.
pub struct SessionManager {
    config: SessionManagerConfig,
    db: Arc<SessionStore>,
    process_manager: Arc<ProcessManager>,
    event_tx: broadcast::Sender<ProcessEvent>,
    active_sessions: Arc<RwLock<Vec<Uuid>>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(config: SessionManagerConfig) -> Result<Self> {
        let db = Arc::new(SessionStore::open(&config.db_path)?);
        let process_manager = Arc::new(ProcessManager::new(config.claude_path.clone()));
        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            config,
            db,
            process_manager,
            event_tx,
            active_sessions: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Subscribe to process events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProcessEvent> {
        self.event_tx.subscribe()
    }

    /// Create a new session.
    pub async fn create_session(&self, opts: CreateSessionOptions) -> Result<Session> {
        // Check session limit
        let active_count = self.active_sessions.read().await.len();
        if active_count >= self.config.max_concurrent_sessions {
            return Err(ClausetError::SessionLimitExceeded(
                self.config.max_concurrent_sessions,
            ));
        }

        let session_id = Uuid::new_v4();
        let claude_session_id = opts.resume_session_id.unwrap_or_else(Uuid::new_v4);
        let now = chrono::Utc::now();

        let session = Session {
            id: session_id,
            claude_session_id,
            project_path: opts.project_path.clone(),
            model: opts.model.clone().unwrap_or_else(|| self.config.default_model.clone()),
            status: SessionStatus::Created,
            mode: opts.mode,
            created_at: now,
            last_activity_at: now,
            total_cost_usd: 0.0,
            preview: truncate_preview(&opts.prompt),
        };

        // Persist to database
        self.db.insert(&session)?;

        Ok(session)
    }

    /// Start a session (spawn Claude process).
    pub async fn start_session(&self, session_id: Uuid, prompt: &str) -> Result<()> {
        info!("Starting session {} with prompt: {}", session_id, prompt);

        let session = self
            .db
            .get(session_id)?
            .ok_or(ClausetError::SessionNotFound(session_id))?;

        // Update status to starting
        self.db.update_status(session_id, SessionStatus::Starting)?;

        // Spawn process
        let spawn_result = self
            .process_manager
            .spawn(
                SpawnOptions {
                    session_id,
                    claude_session_id: session.claude_session_id,
                    project_path: session.project_path,
                    prompt: prompt.to_string(),
                    model: Some(session.model),
                    mode: session.mode,
                    resume: false,
                },
                self.event_tx.clone(),
            )
            .await;

        // Handle spawn failure
        if let Err(e) = spawn_result {
            error!("Failed to spawn Claude process for session {}: {}", session_id, e);
            // Update status to Error
            let _ = self.db.update_status(session_id, SessionStatus::Error);
            return Err(e);
        }

        // Track as active
        self.active_sessions.write().await.push(session_id);

        // Update status to active
        self.db.update_status(session_id, SessionStatus::Active)?;

        info!("Session {} started successfully", session_id);
        Ok(())
    }

    /// Resume an existing session.
    pub async fn resume_session(&self, session_id: Uuid) -> Result<()> {
        let session = self
            .db
            .get(session_id)?
            .ok_or(ClausetError::SessionNotFound(session_id))?;

        // Update status
        self.db.update_status(session_id, SessionStatus::Starting)?;

        // Spawn process in resume mode
        self.process_manager
            .spawn(
                SpawnOptions {
                    session_id,
                    claude_session_id: session.claude_session_id,
                    project_path: session.project_path,
                    prompt: String::new(),
                    model: Some(session.model),
                    mode: session.mode,
                    resume: true,
                },
                self.event_tx.clone(),
            )
            .await?;

        // Track as active
        self.active_sessions.write().await.push(session_id);

        // Update status to active
        self.db.update_status(session_id, SessionStatus::Active)?;

        Ok(())
    }

    /// Send input to a session.
    pub async fn send_input(&self, session_id: Uuid, input: &str) -> Result<()> {
        self.process_manager.send_input(session_id, input).await
    }

    /// Send terminal input to a PTY session.
    pub async fn send_terminal_input(&self, session_id: Uuid, data: &[u8]) -> Result<()> {
        self.process_manager.send_terminal_input(session_id, data).await
    }

    /// Resize terminal for a PTY session.
    pub async fn resize_terminal(&self, session_id: Uuid, rows: u16, cols: u16) -> Result<()> {
        self.process_manager.resize_terminal(session_id, rows, cols).await
    }

    /// Terminate a session.
    pub async fn terminate_session(&self, session_id: Uuid) -> Result<()> {
        self.process_manager.terminate(session_id).await?;

        // Remove from active list
        self.active_sessions.write().await.retain(|&id| id != session_id);

        // Update status
        self.db.update_status(session_id, SessionStatus::Stopped)?;

        Ok(())
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: Uuid) -> Result<Option<Session>> {
        self.db.get(session_id)
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        self.db.list()
    }

    /// Update session status.
    pub fn update_status(&self, session_id: Uuid, status: SessionStatus) -> Result<()> {
        self.db.update_status(session_id, status)
    }

    /// Update session cost.
    pub fn update_cost(&self, session_id: Uuid, cost: f64) -> Result<()> {
        self.db.update_cost(session_id, cost)
    }

    /// Check if a session is active.
    pub async fn is_active(&self, session_id: Uuid) -> bool {
        self.process_manager.is_active(session_id).await
    }

    /// Get the event sender for external use.
    pub fn event_sender(&self) -> broadcast::Sender<ProcessEvent> {
        self.event_tx.clone()
    }
}

fn truncate_preview(s: &str) -> String {
    const MAX_LEN: usize = 100;
    if s.len() <= MAX_LEN {
        s.to_string()
    } else {
        format!("{}...", &s[..MAX_LEN - 3])
    }
}
