//! Session manager orchestrating processes and persistence.

use crate::{AppendResult, ClausetError, ProcessEvent, ProcessManager, Result, SessionActivity, SessionBuffers, SessionStore, SpawnOptions};
use clauset_types::{Session, SessionMode, SessionStatus, SessionSummary};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the session manager.
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    pub claude_path: PathBuf,
    pub db_path: PathBuf,
    pub max_concurrent_sessions: usize,
    pub default_model: String,
    /// URL for hooks to send events back to (e.g., "http://localhost:8080")
    pub clauset_url: String,
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
            default_model: "haiku".to_string(),
            clauset_url: "http://localhost:8080".to_string(),
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
    buffers: Arc<SessionBuffers>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(config: SessionManagerConfig) -> Result<Self> {
        let db = Arc::new(SessionStore::open(&config.db_path)?);
        let process_manager = Arc::new(ProcessManager::new(config.claude_path.clone()));
        let (event_tx, _) = broadcast::channel(256);
        let buffers = Arc::new(SessionBuffers::new());

        let manager = Self {
            config,
            db,
            process_manager,
            event_tx,
            active_sessions: Arc::new(RwLock::new(Vec::new())),
            buffers,
        };

        // Clean up orphaned sessions from previous runs
        if let Err(e) = manager.cleanup_orphaned_sessions() {
            error!(target: "clauset::session", "Failed to cleanup orphaned sessions: {}", e);
        }

        Ok(manager)
    }

    /// Cleanup sessions that were marked as active but the server has restarted.
    /// These sessions are no longer running, so mark them as stopped.
    fn cleanup_orphaned_sessions(&self) -> Result<()> {
        let orphaned = self.db.list_active()?;
        let count = orphaned.len();

        for session in orphaned {
            info!(target: "clauset::session", "Marking orphaned session {} as stopped", session.id);
            self.db.update_status(session.id, SessionStatus::Stopped)?;
        }

        if count > 0 {
            info!(target: "clauset::session", "Cleaned up {} orphaned sessions", count);
        }

        Ok(())
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
        // Use provided resume ID or nil (will be populated when Claude sends init event)
        let claude_session_id = opts.resume_session_id.unwrap_or(Uuid::nil());
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
            input_tokens: 0,
            output_tokens: 0,
            context_percent: 0,
            preview: truncate_preview(&opts.prompt),
        };

        // Persist to database
        self.db.insert(&session)?;

        Ok(session)
    }

    /// Start a session (spawn Claude process).
    pub async fn start_session(&self, session_id: Uuid, prompt: &str) -> Result<()> {
        debug!(target: "clauset::session", "Starting session {} with prompt: {}", session_id, prompt);

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
                    clauset_url: self.config.clauset_url.clone(),
                },
                self.event_tx.clone(),
            )
            .await;

        // Handle spawn failure
        if let Err(e) = spawn_result {
            error!(target: "clauset::session", "Failed to spawn Claude process for session {}: {}", session_id, e);
            // Update status to Error
            if let Err(db_err) = self.db.update_status(session_id, SessionStatus::Error) {
                warn!(target: "clauset::session", "Failed to update session {} status to Error in DB: {}", session_id, db_err);
            }
            return Err(e);
        }

        // Track as active
        self.active_sessions.write().await.push(session_id);

        // Update status to active
        self.db.update_status(session_id, SessionStatus::Active)?;

        // Initialize activity buffer with "Ready" state and broadcast
        self.initialize_session_activity(session_id).await;

        // Note: Claude's session ID is captured from hook events (SessionStart, UserPromptSubmit, etc.)
        // See hooks.rs - extract_claude_session_id() captures it on first hook

        info!(target: "clauset::session", "Session {} started successfully", session_id);
        Ok(())
    }

    /// Resume an existing session.
    pub async fn resume_session(&self, session_id: Uuid) -> Result<()> {
        let session = self
            .db
            .get(session_id)?
            .ok_or(ClausetError::SessionNotFound(session_id))?;

        // Check if we have a valid Claude session ID (not nil)
        if session.claude_session_id.is_nil() {
            warn!(
                target: "clauset::session",
                "Cannot resume session {} - Claude session ID not captured",
                session_id
            );
            return Err(ClausetError::SessionNotResumable(session_id));
        }

        // Update status
        self.db.update_status(session_id, SessionStatus::Starting)?;

        // Load persisted terminal buffer before spawning so it's ready for clients
        if let Ok(Some(buffer_data)) = self.db.get_terminal_buffer(session_id) {
            info!(
                target: "clauset::session",
                "Restoring terminal buffer for session {}: {} bytes",
                session_id,
                buffer_data.data.len()
            );
            self.buffers
                .restore_buffer(session_id, buffer_data.data, buffer_data.start_seq, buffer_data.end_seq)
                .await;
        }

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
                    clauset_url: self.config.clauset_url.clone(),
                },
                self.event_tx.clone(),
            )
            .await?;

        // Track as active
        self.active_sessions.write().await.push(session_id);

        // Update status to active
        self.db.update_status(session_id, SessionStatus::Active)?;

        // Initialize activity buffer with "Ready" state and broadcast
        // Note: If we restored a buffer, initialize_session won't clear it
        self.initialize_session_activity(session_id).await;

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

        // Persist activity data before updating status
        self.persist_session_activity(session_id).await;

        // Update status
        self.db.update_status(session_id, SessionStatus::Stopped)?;

        Ok(())
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: Uuid) -> Result<Option<Session>> {
        self.db.get(session_id)
    }

    /// List all sessions with current activity data.
    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut sessions = self.db.list()?;

        // Enrich active sessions with current activity data from buffers
        for session in &mut sessions {
            if matches!(
                session.status,
                SessionStatus::Active | SessionStatus::Starting
            ) {
                if let Some(activity) = self.buffers.get_activity(session.id).await {
                    session.current_step = activity.current_step;
                    session.recent_actions = activity
                        .recent_actions
                        .into_iter()
                        .map(|a| clauset_types::RecentAction {
                            action_type: a.action_type,
                            summary: a.summary,
                            detail: a.detail,
                            timestamp: a.timestamp,
                        })
                        .collect();

                    // Also enrich stats from buffer if available
                    // Buffer stats are more up-to-date than database (parsed from terminal in real-time)
                    if !activity.model.is_empty() {
                        session.model = activity.model;
                    }
                    if activity.cost > 0.0 {
                        session.total_cost_usd = activity.cost;
                    }
                    if activity.input_tokens > 0 {
                        session.input_tokens = activity.input_tokens;
                    }
                    if activity.output_tokens > 0 {
                        session.output_tokens = activity.output_tokens;
                    }
                    if activity.context_percent > 0 {
                        session.context_percent = activity.context_percent;
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Update session status.
    pub fn update_status(&self, session_id: Uuid, status: SessionStatus) -> Result<()> {
        self.db.update_status(session_id, status)
    }

    /// Store Claude's real session ID (captured from System init event).
    /// This is the ID that Claude uses internally and is required for resume.
    pub fn set_claude_session_id(&self, session_id: Uuid, claude_id: &str) -> Result<()> {
        info!(target: "clauset::session", "Storing Claude session ID {} for session {}", claude_id, session_id);
        self.db.update_claude_session_id(session_id, claude_id)
    }

    /// Persist session activity data to database (call before stopping a session).
    pub async fn persist_session_activity(&self, session_id: Uuid) {
        // Persist activity (current step, recent actions)
        if let Some(activity) = self.buffers.get_activity(session_id).await {
            let recent_actions: Vec<clauset_types::RecentAction> = activity
                .recent_actions
                .iter()
                .map(|a| clauset_types::RecentAction {
                    action_type: a.action_type.clone(),
                    summary: a.summary.clone(),
                    detail: a.detail.clone(),
                    timestamp: a.timestamp,
                })
                .collect();

            if let Err(e) = self.db.update_activity(
                session_id,
                activity.current_step.as_deref(),
                &recent_actions,
            ) {
                warn!(target: "clauset::session", "Failed to persist session {} activity: {}", session_id, e);
            } else {
                debug!(target: "clauset::session", "Persisted {} recent actions for session {}", recent_actions.len(), session_id);
            }
        }

        // Persist terminal buffer for resume
        if let Some((data, start_seq, end_seq)) = self.buffers.get_buffer_for_persistence(session_id).await {
            if let Err(e) = self.db.save_terminal_buffer(session_id, &data, start_seq, end_seq) {
                warn!(target: "clauset::session", "Failed to persist session {} terminal buffer: {}", session_id, e);
            } else {
                info!(
                    target: "clauset::session",
                    "Persisted terminal buffer for session {}: {} bytes",
                    session_id,
                    data.len()
                );
            }
        }
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

    /// Broadcast an event to all subscribers.
    pub fn broadcast_event(&self, event: ProcessEvent) -> std::result::Result<usize, broadcast::error::SendError<ProcessEvent>> {
        self.event_tx.send(event)
    }

    /// Delete a session permanently.
    pub async fn delete_session(&self, session_id: Uuid) -> Result<()> {
        // Terminate the process if it's running
        if self.process_manager.is_active(session_id).await {
            self.process_manager.terminate(session_id).await?;
        }

        // Remove from active list
        self.active_sessions.write().await.retain(|&id| id != session_id);

        // Delete from database
        self.db.delete(session_id)?;

        info!(target: "clauset::session", "Session {} deleted", session_id);
        Ok(())
    }

    /// Rename a session (update its preview/name).
    pub fn rename_session(&self, session_id: Uuid, name: &str) -> Result<()> {
        self.db.update_preview(session_id, name)?;
        info!(target: "clauset::session", "Session {} renamed to: {}", session_id, name);
        Ok(())
    }

    /// Update session stats from Claude's status line.
    pub fn update_session_stats(
        &self,
        session_id: Uuid,
        model: &str,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
    ) -> Result<()> {
        self.db.update_stats(
            session_id,
            model,
            cost,
            input_tokens,
            output_tokens,
            context_percent,
        )?;
        debug!(
            target: "clauset::activity::stats",
            "Session {} stats updated: {} ${:.2} {}K/{}K ctx:{}%",
            session_id, model, cost, input_tokens / 1000, output_tokens / 1000, context_percent
        );
        Ok(())
    }

    /// Append terminal output to session buffer and parse for activity.
    /// Returns (AppendResult, Option<SessionActivity>, Option<TuiMenu>) where:
    /// - activity is Some if it changed
    /// - tui_menu is Some if a new TUI menu was detected
    pub async fn append_terminal_output(&self, session_id: Uuid, data: &[u8]) -> (AppendResult, Option<SessionActivity>, Option<clauset_types::TuiMenu>) {
        let (append_result, activity, tui_menu) = self.buffers.append(session_id, data).await;

        // If activity changed, update the database with new stats
        if let Some(ref act) = activity {
            if !act.model.is_empty() {
                if let Err(e) = self.db.update_stats(
                    session_id,
                    &act.model,
                    act.cost,
                    act.input_tokens,
                    act.output_tokens,
                    act.context_percent,
                ) {
                    warn!(target: "clauset::session", "Failed to update session {} stats in DB: {}", session_id, e);
                }
            }
            // Update preview with current activity if meaningful
            if !act.current_activity.is_empty() {
                if let Err(e) = self.db.update_preview(session_id, &act.current_activity) {
                    warn!(target: "clauset::session", "Failed to update session {} preview in DB: {}", session_id, e);
                }
            }
        }

        (append_result, activity, tui_menu)
    }

    /// Get the terminal buffer for a session (for replay on reconnect).
    pub async fn get_terminal_buffer(&self, session_id: Uuid) -> Option<Vec<u8>> {
        self.buffers.get_buffer(session_id).await
    }

    /// Get current activity for a session.
    pub async fn get_activity(&self, session_id: Uuid) -> Option<SessionActivity> {
        self.buffers.get_activity(session_id).await
    }

    /// Get the session buffers for external use.
    pub fn buffers(&self) -> Arc<SessionBuffers> {
        self.buffers.clone()
    }

    /// Clear terminal buffer for a session.
    pub async fn clear_terminal_buffer(&self, session_id: Uuid) {
        self.buffers.clear(session_id).await;
    }

    /// Mark a session as busy (user sent input, waiting for Claude's response).
    /// This ensures the status stays "Thinking" until Claude reliably finishes.
    /// Also broadcasts an ActivityUpdate event so the dashboard updates immediately.
    pub async fn mark_session_busy(&self, session_id: Uuid) {
        self.buffers.mark_busy(session_id).await;

        // Broadcast activity update so dashboard shows "Thinking" immediately
        if let Some(activity) = self.buffers.get_activity(session_id).await {
            let _ = self.event_tx.send(ProcessEvent::ActivityUpdate {
                session_id,
                model: activity.model,
                cost: activity.cost,
                input_tokens: activity.input_tokens,
                output_tokens: activity.output_tokens,
                context_percent: activity.context_percent,
                current_activity: activity.current_activity,
                current_step: activity.current_step,
                recent_actions: activity.recent_actions,
            });
        }
    }

    /// Mark a session as ready (Claude finished responding).
    pub async fn mark_session_ready(&self, session_id: Uuid) {
        self.buffers.mark_ready(session_id).await;
    }

    /// Initialize a session's activity buffer and broadcast initial "Ready" state.
    /// Should be called when a session starts to ensure the dashboard shows "Ready".
    pub async fn initialize_session_activity(&self, session_id: Uuid) {
        let activity = self.buffers.initialize_session(session_id).await;

        // Broadcast initial activity so dashboard shows "Ready" immediately
        let _ = self.event_tx.send(ProcessEvent::ActivityUpdate {
            session_id,
            model: activity.model,
            cost: activity.cost,
            input_tokens: activity.input_tokens,
            output_tokens: activity.output_tokens,
            context_percent: activity.context_percent,
            current_activity: activity.current_activity,
            current_step: activity.current_step,
            recent_actions: activity.recent_actions,
        });
    }

    /// Update session activity from hook event data.
    /// This updates the internal state and broadcasts to WebSocket clients.
    pub async fn update_activity_from_hook(
        &self,
        session_id: Uuid,
        current_activity: String,
        current_step: Option<String>,
        new_action: Option<crate::RecentAction>,
        is_busy: bool,
    ) {
        if let Some(activity) = self.buffers.update_from_hook(
            session_id,
            current_activity,
            current_step,
            new_action,
            is_busy,
        ).await {
            tracing::debug!(
                target: "clauset::hooks",
                "Broadcasting ActivityUpdate from hook: session={}, model='{}', cost=${:.4}, tokens={}K/{}K, ctx={}%",
                session_id,
                activity.model,
                activity.cost,
                activity.input_tokens / 1000,
                activity.output_tokens / 1000,
                activity.context_percent
            );

            // Broadcast the updated activity
            let _ = self.event_tx.send(ProcessEvent::ActivityUpdate {
                session_id,
                model: activity.model,
                cost: activity.cost,
                input_tokens: activity.input_tokens,
                output_tokens: activity.output_tokens,
                context_percent: activity.context_percent,
                current_activity: activity.current_activity,
                current_step: activity.current_step,
                recent_actions: activity.recent_actions,
            });
        }
    }

    /// Update context window information from hook data.
    ///
    /// This uses the accurate context_window data from Claude's hook input,
    /// replacing the fragile regex parsing of terminal output.
    pub async fn update_context_from_hook(
        &self,
        session_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
        context_window_size: u64,
        model: Option<String>,
    ) {
        if let Some(activity) = self.buffers.update_context_from_hook(
            session_id,
            input_tokens,
            output_tokens,
            context_window_size,
            model,
        ).await {
            tracing::debug!(
                target: "clauset::hooks",
                "Broadcasting context update from hook: session={}, model='{}', tokens={}K/{}K, ctx={}%",
                session_id,
                activity.model,
                activity.input_tokens / 1000,
                activity.output_tokens / 1000,
                activity.context_percent
            );

            // Broadcast the updated activity
            let _ = self.event_tx.send(ProcessEvent::ActivityUpdate {
                session_id,
                model: activity.model,
                cost: activity.cost,
                input_tokens: activity.input_tokens,
                output_tokens: activity.output_tokens,
                context_percent: activity.context_percent,
                current_activity: activity.current_activity,
                current_step: activity.current_step,
                recent_actions: activity.recent_actions,
            });
        }
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
