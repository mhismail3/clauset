//! Prompt Library routes.

use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use clauset_types::{Prompt, PromptSummary};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Query parameters for listing prompts.
#[derive(Deserialize)]
pub struct ListPromptsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    50
}

/// Response for listing prompts.
#[derive(Serialize)]
pub struct PromptsListResponse {
    pub prompts: Vec<PromptSummary>,
    pub total_count: u64,
    pub has_more: bool,
}

/// GET /api/prompts - List prompts with pagination.
pub async fn list_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListPromptsQuery>,
) -> Result<Json<PromptsListResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let prompts = store
        .list_prompts(query.limit, query.offset)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_count = store
        .get_prompt_count()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let has_more = (query.offset as u64 + prompts.len() as u64) < total_count;

    Ok(Json(PromptsListResponse {
        prompts,
        total_count,
        has_more,
    }))
}

/// GET /api/prompts/{id} - Get a single prompt by ID.
pub async fn get_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Prompt>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let prompt = store
        .get_prompt(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Prompt not found".to_string()))?;

    Ok(Json(prompt))
}
