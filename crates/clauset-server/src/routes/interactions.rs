//! Interaction tracking API routes.
//!
//! Provides endpoints for:
//! - Listing session interactions (timeline)
//! - Getting interaction details
//! - Computing file diffs
//! - Cross-session search
//! - Cost analytics

use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use clauset_core::{
    compute_diff, generate_unified_diff, AnalyticsSummary, DailyCostEntry, FileChangeWithDiff,
    FileDiff, GlobalSearchResults, SessionAnalytics, StorageStats, ToolCostEntry,
};
use clauset_types::{Interaction, ToolInvocation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Interaction Timeline Endpoints
// ============================================================================

/// Response for listing interactions in a session.
#[derive(Serialize)]
pub struct InteractionListResponse {
    pub interactions: Vec<InteractionSummary>,
    pub total_count: usize,
}

/// Summary of an interaction for timeline display.
#[derive(Serialize)]
pub struct InteractionSummary {
    pub id: Uuid,
    pub sequence_number: u32,
    pub user_prompt: String,
    pub user_prompt_preview: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub cost_delta_usd: f64,
    pub input_tokens_delta: u64,
    pub output_tokens_delta: u64,
    pub tool_count: u32,
    pub files_changed: Vec<String>,
}

#[derive(Deserialize)]
pub struct InteractionListQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// List all interactions for a session.
pub async fn list_session_interactions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<InteractionListQuery>,
) -> Result<Json<InteractionListResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let interactions = store
        .list_interactions(session_id, 1000, 0)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let limit = query.limit.unwrap_or(50) as usize;
    let offset = query.offset.unwrap_or(0) as usize;
    let total_count = interactions.len();

    let summaries: Vec<InteractionSummary> = interactions
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|i| {
            let tool_count = store
                .list_tool_invocations(i.id)
                .map(|tools| tools.len() as u32)
                .unwrap_or(0);

            let files_changed: Vec<String> = store
                .get_file_changes_with_diffs(i.id, 3)
                .map(|changes| {
                    changes
                        .into_iter()
                        .map(|c| c.file_path.display().to_string())
                        .collect()
                })
                .unwrap_or_default();

            let preview = if i.user_prompt.len() > 100 {
                format!("{}...", &i.user_prompt[..100])
            } else {
                i.user_prompt.clone()
            };

            InteractionSummary {
                id: i.id,
                sequence_number: i.sequence_number,
                user_prompt: i.user_prompt,
                user_prompt_preview: preview,
                started_at: i.started_at,
                ended_at: i.ended_at,
                cost_delta_usd: i.cost_usd_delta,
                input_tokens_delta: i.input_tokens_delta,
                output_tokens_delta: i.output_tokens_delta,
                tool_count,
                files_changed,
            }
        })
        .collect();

    Ok(Json(InteractionListResponse {
        interactions: summaries,
        total_count,
    }))
}

/// Full interaction detail response.
#[derive(Serialize)]
pub struct InteractionDetailResponse {
    pub interaction: Interaction,
    pub tool_invocations: Vec<ToolInvocation>,
    pub file_changes: Vec<FileChangeWithDiff>,
}

/// Get full details for a single interaction.
pub async fn get_interaction(
    State(state): State<Arc<AppState>>,
    Path(interaction_id): Path<Uuid>,
) -> Result<Json<InteractionDetailResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let interaction = store
        .get_interaction(interaction_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Interaction not found".to_string()))?;

    let tool_invocations = store
        .list_tool_invocations(interaction_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let file_changes = store
        .get_file_changes_with_diffs(interaction_id, 3)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(InteractionDetailResponse {
        interaction,
        tool_invocations,
        file_changes,
    }))
}

// ============================================================================
// Diff Endpoints
// ============================================================================

#[derive(Deserialize)]
pub struct DiffQuery {
    /// Interaction ID to diff FROM (uses 'after' snapshot)
    pub from: Uuid,
    /// Interaction ID to diff TO (uses 'after' snapshot)
    pub to: Uuid,
    /// File path to diff
    pub file: String,
    /// Number of context lines (default: 3)
    pub context: Option<usize>,
}

/// Response for diff computation.
#[derive(Serialize)]
pub struct DiffResponse {
    pub file_path: String,
    pub from_interaction: Uuid,
    pub to_interaction: Uuid,
    pub diff: FileDiff,
    pub unified_diff: String,
}

/// Compute diff between two interaction snapshots for a specific file.
pub async fn get_diff(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<DiffResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();
    let context_lines = query.context.unwrap_or(3);

    // Get 'after' snapshot from the 'from' interaction
    let from_content = store
        .get_snapshot_content(query.from, &query.file, clauset_types::SnapshotType::After)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get 'after' snapshot from the 'to' interaction
    let to_content = store
        .get_snapshot_content(query.to, &query.file, clauset_types::SnapshotType::After)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let diff = compute_diff(
        from_content.as_deref(),
        to_content.as_deref(),
        context_lines,
    );

    let unified_diff = generate_unified_diff(
        from_content.as_deref(),
        to_content.as_deref(),
        &format!("a/{}", query.file),
        &format!("b/{}", query.file),
        context_lines,
    );

    Ok(Json(DiffResponse {
        file_path: query.file,
        from_interaction: query.from,
        to_interaction: query.to,
        diff,
        unified_diff,
    }))
}

/// Response for files changed in a session.
#[derive(Serialize)]
pub struct FilesChangedResponse {
    pub files: Vec<FileChangeSummary>,
}

#[derive(Serialize)]
pub struct FileChangeSummary {
    pub file_path: String,
    pub change_count: u32,
    pub interactions: Vec<Uuid>,
}

/// List all files changed in a session with counts.
pub async fn get_session_files_changed(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<FilesChangedResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let interactions = store
        .list_interactions(session_id, 1000, 0)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Collect all file changes across interactions
    let mut file_map: std::collections::HashMap<String, (u32, Vec<Uuid>)> =
        std::collections::HashMap::new();

    for interaction in interactions {
        let changes = store.get_file_changes_with_diffs(interaction.id, 0).unwrap_or_default();

        for change in changes {
            let path_str = change.file_path.display().to_string();
            let entry = file_map.entry(path_str).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push(interaction.id);
        }
    }

    let files: Vec<FileChangeSummary> = file_map
        .into_iter()
        .map(|(path, (count, interactions))| FileChangeSummary {
            file_path: path,
            change_count: count,
            interactions,
        })
        .collect();

    Ok(Json(FilesChangedResponse { files }))
}

// ============================================================================
// Search Endpoints
// ============================================================================

#[derive(Deserialize)]
pub struct SearchQuery {
    /// Search query string
    pub q: String,
    /// Search scope: prompts, files, tools, all
    pub scope: Option<String>,
    /// Filter by session ID
    pub session_id: Option<Uuid>,
    /// Maximum results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Search across sessions.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<GlobalSearchResults>, (StatusCode, String)> {
    let store = state.interaction_processor.store();
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let scope = query.scope.as_deref().unwrap_or("all");

    let results = match scope {
        "prompts" => {
            let interactions = store
                .search_interactions(&query.q, query.session_id, limit, offset)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            GlobalSearchResults {
                interactions,
                tool_invocations: Vec::new(),
                file_matches: Vec::new(),
            }
        }
        "files" => {
            let file_matches = store
                .search_files_by_path(&query.q, limit)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            GlobalSearchResults {
                interactions: Vec::new(),
                tool_invocations: Vec::new(),
                file_matches,
            }
        }
        "tools" => {
            let tool_invocations = store
                .search_tool_invocations(&query.q, None, limit, offset)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            GlobalSearchResults {
                interactions: Vec::new(),
                tool_invocations,
                file_matches: Vec::new(),
            }
        }
        _ => {
            // "all" - combined search
            store
                .global_search(&query.q, limit)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
    };

    Ok(Json(results))
}

// ============================================================================
// Analytics Endpoints
// ============================================================================

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    /// Number of days to include in daily breakdown (default: 30)
    pub days: Option<u32>,
}

/// Combined analytics response.
#[derive(Serialize)]
pub struct AnalyticsResponse {
    pub summary: AnalyticsSummary,
    pub daily_costs: Vec<DailyCostEntry>,
    pub tool_costs: Vec<ToolCostEntry>,
    pub session_analytics: Vec<SessionAnalytics>,
}

/// Get analytics summary.
pub async fn get_analytics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<AnalyticsResponse>, (StatusCode, String)> {
    let store = state.interaction_processor.store();
    let days = query.days.unwrap_or(30);

    let summary = store
        .get_analytics_summary()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let daily_costs = store
        .get_daily_cost_breakdown(days)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tool_costs = store
        .get_tool_cost_breakdown(None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get session analytics for most recent sessions
    let all_sessions = store
        .get_all_session_ids()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let session_analytics: Vec<SessionAnalytics> = all_sessions
        .into_iter()
        .take(20)
        .filter_map(|session_id| store.get_session_analytics(session_id).ok())
        .collect();

    Ok(Json(AnalyticsResponse {
        summary,
        daily_costs,
        tool_costs,
        session_analytics,
    }))
}

/// Get most expensive interactions.
#[derive(Deserialize)]
pub struct ExpensiveInteractionsQuery {
    pub limit: Option<usize>,
}

pub async fn get_expensive_interactions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExpensiveInteractionsQuery>,
) -> Result<Json<Vec<Interaction>>, (StatusCode, String)> {
    let store = state.interaction_processor.store();
    let limit = query.limit.unwrap_or(10);

    let interactions = store
        .get_most_expensive_interactions(limit)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(interactions))
}

/// Get storage statistics.
pub async fn get_storage_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StorageStats>, (StatusCode, String)> {
    let store = state.interaction_processor.store();

    let stats = store
        .get_storage_stats()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(stats))
}
