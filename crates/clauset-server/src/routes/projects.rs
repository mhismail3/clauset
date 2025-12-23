//! Project listing route handlers.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

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

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
}

/// Error type for project creation.
pub enum CreateProjectError {
    InvalidName(String),
    AlreadyExists,
    IoError(std::io::Error),
}

impl IntoResponse for CreateProjectError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            CreateProjectError::InvalidName(reason) => {
                (StatusCode::BAD_REQUEST, format!("Invalid project name: {}", reason))
            }
            CreateProjectError::AlreadyExists => {
                (StatusCode::CONFLICT, "A project with this name already exists".to_string())
            }
            CreateProjectError::IoError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create project: {}", e))
            }
        };
        (status, message).into_response()
    }
}

/// Create a new project directory.
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<Project>, CreateProjectError> {
    let name = req.name.trim();

    // Validate project name
    if name.is_empty() {
        return Err(CreateProjectError::InvalidName("name cannot be empty".to_string()));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(CreateProjectError::InvalidName("name cannot contain path separators".to_string()));
    }
    if name == "." || name == ".." {
        return Err(CreateProjectError::InvalidName("name cannot be '.' or '..'".to_string()));
    }
    if name.starts_with('.') {
        return Err(CreateProjectError::InvalidName("name cannot start with '.'".to_string()));
    }

    let project_path = state.config.projects_root.join(name);

    // Check if already exists
    if project_path.exists() {
        return Err(CreateProjectError::AlreadyExists);
    }

    // Create the directory
    std::fs::create_dir(&project_path).map_err(CreateProjectError::IoError)?;

    info!("Created new project directory: {:?}", project_path);

    Ok(Json(Project {
        name: name.to_string(),
        path: project_path.to_string_lossy().to_string(),
    }))
}
