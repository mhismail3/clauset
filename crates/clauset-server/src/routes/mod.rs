//! HTTP route handlers.

pub mod history;
pub mod hooks;
pub mod interactions;
pub mod projects;
pub mod prompts;
pub mod sessions;
pub mod ws;

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
