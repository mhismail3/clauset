//! Clauset server - HTTP/WebSocket server for Claude Code session management.

mod config;
mod routes;
mod state;
mod websocket;

use anyhow::Result;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "clauset_server=debug,clauset_core=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::load()?;
    tracing::info!("Loaded configuration");

    // Initialize application state
    let state = Arc::new(AppState::new(config.clone())?);
    tracing::info!("Initialized application state");

    // Build router
    let api_routes = Router::new()
        .route("/sessions", get(routes::sessions::list))
        .route("/sessions", post(routes::sessions::create))
        .route("/sessions/{id}", get(routes::sessions::get))
        .route("/sessions/{id}", delete(routes::sessions::terminate))
        .route("/sessions/{id}/delete", delete(routes::sessions::delete))
        .route("/sessions/{id}/name", put(routes::sessions::rename))
        .route("/sessions/{id}/start", post(routes::sessions::start))
        .route("/sessions/{id}/resume", post(routes::sessions::resume))
        .route("/sessions/{id}/input", post(routes::sessions::send_input))
        .route("/history", get(routes::history::list))
        .route("/health", get(routes::health));

    let ws_routes = Router::new()
        .route("/sessions/{id}", get(routes::ws::upgrade));

    let app = Router::new()
        .nest("/api", api_routes)
        .nest("/ws", ws_routes)
        .fallback_service(ServeDir::new(&config.static_dir))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
