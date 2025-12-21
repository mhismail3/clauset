//! History routes.

use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use clauset_core::HistoryWatcher;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub entries: Vec<HistoryEntryResponse>,
}

#[derive(Serialize)]
pub struct HistoryEntryResponse {
    pub display: String,
    pub timestamp: i64,
    pub project: String,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, (StatusCode, String)> {
    // Reload history to get latest
    state
        .history_watcher
        .reload()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries = state.history_watcher.get_entries(query.limit);

    let response_entries: Vec<HistoryEntryResponse> = entries
        .into_iter()
        .map(|e| HistoryEntryResponse {
            display: e.display,
            timestamp: e.timestamp,
            project: e.project.to_string_lossy().to_string(),
        })
        .collect();

    Ok(Json(HistoryResponse {
        entries: response_entries,
    }))
}
