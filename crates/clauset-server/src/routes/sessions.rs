//! Session management routes.

use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use clauset_core::{ClaudeSessionReader, CreateSessionOptions};
use clauset_types::{SessionMode, SessionStatus, SessionSummary};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionSummary>,
    pub active_count: usize,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionListResponse>, (StatusCode, String)> {
    let sessions = state
        .session_manager
        .list_sessions()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let active_count = sessions
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                clauset_types::SessionStatus::Active
                    | clauset_types::SessionStatus::Starting
                    | clauset_types::SessionStatus::WaitingInput
            )
        })
        .count();

    Ok(Json(SessionListResponse {
        sessions,
        active_count,
    }))
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub project_path: PathBuf,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub terminal_mode: bool,
    #[serde(default)]
    pub resume_session_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: Uuid,
    pub claude_session_id: Uuid,
    pub ws_url: String,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, (StatusCode, String)> {
    // ALWAYS use Terminal mode to use Claude Max subscription
    // StreamJson mode uses API credits which we want to avoid
    let mode = SessionMode::Terminal;

    let session = state
        .session_manager
        .create_session(CreateSessionOptions {
            project_path: req.project_path,
            prompt: req.prompt.unwrap_or_default(),
            model: req.model,
            mode,
            resume_session_id: req.resume_session_id,
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateSessionResponse {
        session_id: session.id,
        claude_session_id: session.claude_session_id,
        ws_url: format!("/ws/sessions/{}", session.id),
    }))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<clauset_types::Session>, (StatusCode, String)> {
    let session = state
        .session_manager
        .get_session(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    Ok(Json(session))
}

pub async fn terminate(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .session_manager
        .terminate_session(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct StartSessionRequest {
    #[serde(default)]
    pub prompt: Option<String>,
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<StartSessionRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let prompt = req.prompt.unwrap_or_default();
    info!(target: "clauset::session", "Starting session {} with prompt: {}", id, prompt);

    state
        .session_manager
        .start_session(id, &prompt)
        .await
        .map_err(|e| {
            error!(target: "clauset::session", "Failed to start session {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    info!(target: "clauset::session", "Session {} started successfully", id);
    Ok(StatusCode::OK)
}

pub async fn resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .session_manager
        .resume_session(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct SendInputRequest {
    pub content: String,
}

pub async fn send_input(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<SendInputRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .session_manager
        .send_input(id, &req.content)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

/// Delete a session permanently.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .session_manager
        .delete_session(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct RenameSessionRequest {
    pub name: String,
}

/// Rename a session.
pub async fn rename(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<RenameSessionRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .session_manager
        .rename_session(id, &req.name)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

// === Claude Sessions from ~/.claude ===

#[derive(Deserialize)]
pub struct ClaudeSessionsQuery {
    pub project_path: PathBuf,
}

/// A Claude session from ~/.claude storage.
#[derive(Serialize)]
pub struct ClaudeSessionResponse {
    pub session_id: String,
    pub project_path: PathBuf,
    pub timestamp: String,
    pub preview: String,
    /// Whether this session already exists in Clauset's database
    pub in_clauset: bool,
}

#[derive(Serialize)]
pub struct ClaudeSessionsListResponse {
    pub sessions: Vec<ClaudeSessionResponse>,
}

#[derive(Deserialize)]
pub struct ClaudeTranscriptQuery {
    pub project_path: PathBuf,
}

#[derive(Serialize)]
pub struct ClaudeTranscriptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct ClaudeTranscriptResponse {
    pub messages: Vec<ClaudeTranscriptMessage>,
}

/// List sessions from ~/.claude/history.jsonl for a specific project.
/// Returns sessions that exist in Claude's storage, indicating which ones
/// are already imported into Clauset.
pub async fn list_claude_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ClaudeSessionsQuery>,
) -> Result<Json<ClaudeSessionsListResponse>, (StatusCode, String)> {
    let reader = ClaudeSessionReader::new();

    let claude_sessions = reader
        .list_sessions_for_project(&query.project_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get existing Clauset sessions to check for duplicates
    let clauset_sessions = state
        .session_manager
        .list_sessions()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Build a set of claude_session_ids that are already in Clauset
    let existing_ids: std::collections::HashSet<String> = clauset_sessions
        .iter()
        .map(|s| s.claude_session_id.to_string())
        .collect();

    let sessions: Vec<ClaudeSessionResponse> = claude_sessions
        .into_iter()
        .map(|s| ClaudeSessionResponse {
            session_id: s.session_id.clone(),
            project_path: s.project_path,
            timestamp: s.timestamp.to_rfc3339(),
            preview: s.preview,
            in_clauset: existing_ids.contains(&s.session_id),
        })
        .collect();

    Ok(Json(ClaudeSessionsListResponse { sessions }))
}

/// Read the full transcript for a Claude session.
pub async fn get_claude_transcript(
    Path(session_id): Path<String>,
    Query(query): Query<ClaudeTranscriptQuery>,
) -> Result<Json<ClaudeTranscriptResponse>, (StatusCode, String)> {
    let reader = ClaudeSessionReader::new();
    let messages = reader
        .read_transcript(&session_id, &query.project_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = messages
        .into_iter()
        .map(|message| ClaudeTranscriptMessage {
            role: message.role,
            content: message.content,
            timestamp: message.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(ClaudeTranscriptResponse { messages: response }))
}

#[derive(Deserialize)]
pub struct ImportSessionRequest {
    pub claude_session_id: String,
    pub project_path: PathBuf,
}

#[derive(Serialize)]
pub struct ImportSessionResponse {
    pub session_id: Uuid,
    pub claude_session_id: String,
    pub ws_url: String,
}

/// Import a session from ~/.claude into Clauset.
/// Creates a new Clauset session that references the existing Claude session,
/// imports the chat history from the transcript, and sets status to Stopped.
pub async fn import_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportSessionRequest>,
) -> Result<Json<ImportSessionResponse>, (StatusCode, String)> {
    // Verify the session exists in Claude's storage
    let reader = ClaudeSessionReader::new();
    let claude_session = reader
        .get_session(&req.claude_session_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("Session {} not found in ~/.claude", req.claude_session_id),
        ))?;

    // Parse the claude_session_id as UUID
    let claude_uuid = Uuid::parse_str(&req.claude_session_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid session ID: {}", e)))?;

    // Create a Clauset session with the existing Claude session ID
    let session = state
        .session_manager
        .create_session(CreateSessionOptions {
            project_path: req.project_path.clone(),
            prompt: claude_session.preview.clone(),
            model: None, // Will use default model
            mode: SessionMode::Terminal,
            resume_session_id: Some(claude_uuid),
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Import chat history from the transcript
    let transcript_messages = reader
        .read_transcript(&req.claude_session_id, &req.project_path)
        .unwrap_or_else(|e| {
            warn!(
                target: "clauset::session",
                "Failed to read transcript for {}: {}",
                req.claude_session_id, e
            );
            Vec::new()
        });

    // Insert messages into chat_messages table
    let store = state.interaction_processor.store();
    for (seq, msg) in transcript_messages.iter().enumerate() {
        let chat_msg = clauset_types::ChatMessage {
            id: format!("imported-{}-{}", session.id, seq),
            session_id: session.id,
            role: if msg.role == "user" {
                clauset_types::ChatRole::User
            } else {
                clauset_types::ChatRole::Assistant
            },
            content: msg.content.clone(),
            thinking_content: None,
            tool_calls: Vec::new(),
            is_streaming: false,
            is_complete: true,
            timestamp: msg.timestamp.timestamp_millis() as u64,
        };

        if let Err(e) = store.save_chat_message(&chat_msg) {
            warn!(
                target: "clauset::session",
                "Failed to import message {} for session {}: {}",
                seq, session.id, e
            );
        }
    }

    info!(
        target: "clauset::session",
        "Imported {} messages from transcript for session {}",
        transcript_messages.len(),
        session.id
    );

    // Set status to Stopped (since this is an imported session, not a running one)
    state
        .session_manager
        .update_status(session.id, SessionStatus::Stopped)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!(
        target: "clauset::session",
        "Imported Claude session {} as Clauset session {} ({} messages)",
        req.claude_session_id,
        session.id,
        transcript_messages.len()
    );

    Ok(Json(ImportSessionResponse {
        session_id: session.id,
        claude_session_id: req.claude_session_id,
        ws_url: format!("/ws/sessions/{}", session.id),
    }))
}
