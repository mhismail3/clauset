//! Command discovery routes.

use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use clauset_types::{CommandCategory, CommandsResponse};
use serde::Deserialize;
use std::sync::Arc;

/// Query parameters for listing commands.
#[derive(Deserialize)]
pub struct ListCommandsQuery {
    /// Filter by category (built_in, user, skill, plugin)
    pub category: Option<String>,
}

/// GET /api/commands - List all discovered commands.
pub async fn list_commands(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListCommandsQuery>,
) -> Result<Json<CommandsResponse>, (StatusCode, String)> {
    let mut discovery = state.command_discovery.lock().unwrap();

    let mut response = discovery
        .discover_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Apply category filter if specified
    if let Some(ref category_str) = query.category {
        let category = match category_str.as_str() {
            "built_in" => Some(CommandCategory::BuiltIn),
            "user" => Some(CommandCategory::User),
            "skill" => Some(CommandCategory::Skill),
            "plugin" => Some(CommandCategory::Plugin),
            _ => None,
        };

        if let Some(cat) = category {
            response.commands.retain(|c| c.category == cat);
            // Recalculate counts for filtered response
            response.counts.total = response.commands.len();
        }
    }

    Ok(Json(response))
}
