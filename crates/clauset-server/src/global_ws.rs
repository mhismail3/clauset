//! Global WebSocket for dashboard real-time updates.

use crate::state::AppState;
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use clauset_core::ProcessEvent;
use clauset_types::{SessionStatus, WsServerMessage};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;

/// Handle global WebSocket connection for dashboard updates.
pub async fn handle_global_websocket(socket: WebSocket, state: Arc<AppState>) -> Result<()> {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to all session events
    let mut event_rx = state.session_manager.subscribe();

    tracing::info!(target: "clauset::ws", "Global WebSocket client connected");

    // Send initial activity state for all active sessions
    // This ensures the client gets current state even if they missed earlier updates
    if let Ok(sessions) = state.session_manager.list_sessions().await {
        for session in sessions {
            if matches!(session.status, SessionStatus::Active | SessionStatus::Starting) {
                // Send activity update for this session
                let msg = WsServerMessage::ActivityUpdate {
                    session_id: session.id,
                    model: session.model.clone(),
                    cost: session.total_cost_usd,
                    input_tokens: session.input_tokens,
                    output_tokens: session.output_tokens,
                    context_percent: session.context_percent,
                    current_activity: session.preview.clone(),
                    current_step: session.current_step.clone(),
                    recent_actions: session.recent_actions.clone(),
                };
                if let Ok(json) = serde_json::to_string(&msg) {
                    if ws_tx.send(Message::Text(json.into())).await.is_err() {
                        tracing::debug!(target: "clauset::ws", "Failed to send initial activity state");
                        return Ok(());
                    }
                }
            }
        }
    }

    // Spawn task to forward relevant events to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let msg = match &event {
                // Forward all activity updates to dashboard
                ProcessEvent::ActivityUpdate {
                    session_id,
                    model,
                    cost,
                    input_tokens,
                    output_tokens,
                    context_percent,
                    current_activity,
                    current_step,
                    recent_actions,
                } => {
                    Some(WsServerMessage::ActivityUpdate {
                        session_id: *session_id,
                        model: model.clone(),
                        cost: *cost,
                        input_tokens: *input_tokens,
                        output_tokens: *output_tokens,
                        context_percent: *context_percent,
                        current_activity: current_activity.clone(),
                        current_step: current_step.clone(),
                        recent_actions: recent_actions.iter().map(|a| clauset_types::RecentAction {
                            action_type: a.action_type.clone(),
                            summary: a.summary.clone(),
                            detail: a.detail.clone(),
                            timestamp: a.timestamp,
                        }).collect(),
                    })
                },

                // Forward session exits as status changes
                ProcessEvent::Exited { session_id, .. } => {
                    Some(WsServerMessage::StatusChange {
                        session_id: *session_id,
                        old_status: clauset_types::SessionStatus::Active,
                        new_status: clauset_types::SessionStatus::Stopped,
                    })
                }

                // Forward errors
                ProcessEvent::Error { session_id, message } => {
                    Some(WsServerMessage::Error {
                        code: format!("session_{}", session_id),
                        message: message.clone(),
                    })
                }

                _ => None,
            };

            if let Some(msg) = msg {
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    tracing::debug!(target: "clauset::ws", "Global WebSocket client disconnected");
                    break;
                }
            }
        }
    });

    // Handle incoming messages (keepalive pings)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Ping(data) => {
                    // Pong is handled automatically by axum
                    tracing::trace!(target: "clauset::ws::ping", "Received ping from global WebSocket client");
                }
                Message::Close(_) => {
                    tracing::debug!(target: "clauset::ws", "Global WebSocket client closed connection");
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    tracing::info!(target: "clauset::ws", "Global WebSocket client disconnected");
    Ok(())
}
