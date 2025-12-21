//! Project listing route handlers.

use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::state::AppState;

#[derive(Serialize)]
pub struct Project {
    /// Directory name
    pub name: String,
    /// Full path to the project
    pub path: String,
}

#[derive(Serialize)]
pub struct ProjectsResponse {
    pub projects: Vec<Project>,
    pub projects_root: String,
}

/// List available projects in the projects root directory.
pub async fn list(State(state): State<Arc<AppState>>) -> Json<ProjectsResponse> {
    let projects_root = &state.config.projects_root;
    let mut projects = Vec::new();

    debug!("Scanning projects in {:?}", projects_root);

    if let Ok(entries) = std::fs::read_dir(projects_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') {
                        projects.push(Project {
                            name: name.to_string(),
                            path: path.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    } else {
        warn!("Failed to read projects directory: {:?}", projects_root);
    }

    // Sort by name
    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Json(ProjectsResponse {
        projects,
        projects_root: projects_root.to_string_lossy().to_string(),
    })
}
