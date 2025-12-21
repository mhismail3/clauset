//! WebSocket route handler.

use crate::state::AppState;
use crate::websocket::handle_websocket;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use std::sync::Arc;
use uuid::Uuid;

pub async fn upgrade(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_connection(socket, state, session_id))
}

async fn handle_connection(socket: WebSocket, state: Arc<AppState>, session_id: Uuid) {
    if let Err(e) = handle_websocket(socket, state, session_id).await {
        tracing::error!("WebSocket error for session {}: {}", session_id, e);
    }
}
