//! Session management routes.

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use clauset_core::CreateSessionOptions;
use clauset_types::{SessionMode, SessionSummary};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};
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
    info!("Starting session {} with prompt: {}", id, prompt);

    state
        .session_manager
        .start_session(id, &prompt)
        .await
        .map_err(|e| {
            error!("Failed to start session {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    info!("Session {} started successfully", id);
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
