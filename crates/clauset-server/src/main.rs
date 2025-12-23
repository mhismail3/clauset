//! Clauset server - HTTP/WebSocket server for Claude Code session management.

mod config;
mod event_processor;
mod global_ws;
mod interaction_processor;
mod logging;
mod routes;
mod state;
mod websocket;

use anyhow::Result;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::{delete, get, post, put},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

use logging::{LogConfig, LogFormat};

/// Clauset server - Claude Code session management dashboard.
#[derive(Parser, Debug)]
#[command(name = "clauset-server")]
#[command(about = "HTTP/WebSocket server for Claude Code session management")]
#[command(version)]
struct Cli {
    /// Enable verbose logging (INFO level for most targets)
    #[arg(short, long)]
    verbose: bool,

    /// Enable debug logging (DEBUG level, excludes ping traces)
    #[arg(short, long)]
    debug: bool,

    /// Enable trace logging (TRACE level for everything)
    #[arg(long)]
    trace: bool,

    /// Quiet mode (WARN and ERROR only)
    #[arg(short, long)]
    quiet: bool,

    /// Set log level for specific targets (e.g., "activity=debug" or "ws::ping=trace")
    /// Can be specified multiple times. Targets are prefixed with "clauset::" automatically.
    #[arg(long = "log", value_name = "TARGET=LEVEL")]
    log_overrides: Vec<String>,

    /// Log output format
    #[arg(long = "log-format", value_name = "FORMAT", default_value = "text")]
    log_format: LogFormat,
}

use config::Config;
use state::AppState;

/// Handler for global events WebSocket upgrade.
async fn global_events_ws(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_global_events(socket, state))
}

async fn handle_global_events(socket: WebSocket, state: Arc<AppState>) {
    if let Err(e) = global_ws::handle_global_websocket(socket, state).await {
        tracing::error!(target: "clauset::ws", "Global WebSocket error: {}", e);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    let log_config = LogConfig::from_cli(
        cli.verbose,
        cli.debug,
        cli.trace,
        cli.quiet,
        cli.log_overrides,
        cli.log_format,
    );
    logging::init(&log_config);

    // Load configuration
    let config = Config::load()?;
    tracing::info!(target: "clauset::startup", "Loaded configuration");

    // Initialize application state
    let state = Arc::new(AppState::new(config.clone())?);
    tracing::info!(target: "clauset::startup", "Initialized application state");

    // Start background event processor for continuous terminal buffering
    event_processor::spawn_event_processor(state.clone());
    tracing::info!(target: "clauset::startup", "Started background event processor");

    // Build router
    let api_routes = Router::new()
        // Session management
        .route("/sessions", get(routes::sessions::list))
        .route("/sessions", post(routes::sessions::create))
        .route("/sessions/{id}", get(routes::sessions::get))
        .route("/sessions/{id}", delete(routes::sessions::terminate))
        .route("/sessions/{id}/delete", delete(routes::sessions::delete))
        .route("/sessions/{id}/name", put(routes::sessions::rename))
        .route("/sessions/{id}/start", post(routes::sessions::start))
        .route("/sessions/{id}/resume", post(routes::sessions::resume))
        .route("/sessions/{id}/input", post(routes::sessions::send_input))
        // Interaction timeline
        .route(
            "/sessions/{id}/interactions",
            get(routes::interactions::list_session_interactions),
        )
        .route(
            "/sessions/{id}/files-changed",
            get(routes::interactions::get_session_files_changed),
        )
        .route(
            "/interactions/{id}",
            get(routes::interactions::get_interaction),
        )
        // Diff computation
        .route("/diff", get(routes::interactions::get_diff))
        // Cross-session search
        .route("/search", get(routes::interactions::search))
        // Cost analytics
        .route("/analytics", get(routes::interactions::get_analytics))
        .route(
            "/analytics/expensive",
            get(routes::interactions::get_expensive_interactions),
        )
        .route(
            "/analytics/storage",
            get(routes::interactions::get_storage_stats),
        )
        // Other routes
        .route("/history", get(routes::history::list))
        .route("/projects", get(routes::projects::list).post(routes::projects::create))
        .route("/hooks", post(routes::hooks::receive))
        .route("/health", get(routes::health));

    let ws_routes = Router::new()
        .route("/sessions/{id}", get(routes::ws::upgrade))
        .route("/events", get(global_events_ws));

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
    tracing::info!(target: "clauset::startup", "Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
