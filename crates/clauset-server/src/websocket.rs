//! WebSocket connection handling.

use crate::state::AppState;
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use clauset_core::ProcessEvent;
use clauset_types::{WsClientMessage, WsServerMessage};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

/// Maximum size for text input messages (10KB)
const MAX_INPUT_SIZE: usize = 10 * 1024;

/// Maximum size for terminal input data (64KB - generous for paste operations)
const MAX_TERMINAL_INPUT_SIZE: usize = 64 * 1024;

pub async fn handle_websocket(
    socket: WebSocket,
    state: Arc<AppState>,
    session_id: Uuid,
) -> Result<()> {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to session events
    let mut event_rx = state.session_manager.subscribe();

    // Channel for recv_task to request buffer sends
    let (buffer_tx, mut buffer_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Get initial session state and send init message
    if let Ok(Some(session)) = state.session_manager.get_session(session_id) {
        let init_msg = WsServerMessage::SessionInit {
            session_id: session.id,
            claude_session_id: session.claude_session_id,
            model: session.model,
            tools: vec![],
            cwd: session.project_path,
        };
        let json = serde_json::to_string(&init_msg)?;
        ws_tx.send(Message::Text(json.into())).await?;
    }

    // NOTE: Terminal buffer is NOT sent here on connect.
    // The client must first send a Resize message so tmux can be resized to match.
    // Then the client sends RequestBuffer, and we send the buffer formatted for the correct size.
    // This prevents text from being formatted for 80 columns but displayed in a narrower terminal.

    // Spawn task to forward events to WebSocket
    let state_clone = state.clone();
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle buffer request from recv_task
                Some(()) = buffer_rx.recv() => {
                    // Send terminal buffer if available
                    if let Some(buffer) = state_clone.session_manager.get_terminal_buffer(session_id).await {
                        if !buffer.is_empty() {
                            debug!("Sending terminal buffer for session {}: {} bytes", session_id, buffer.len());
                            let buffer_msg = WsServerMessage::TerminalBuffer { data: buffer };
                            if let Ok(json) = serde_json::to_string(&buffer_msg) {
                                if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                                    debug!("WebSocket send failed for session {}: {}", session_id, e);
                                    break;
                                }
                            }
                        }
                    }
                }
                // Handle session events
                Ok(event) = event_rx.recv() => {
                    // Only forward events for this session
                    let msg = match &event {
                        ProcessEvent::Claude(claude_event) => {
                            // Convert Claude events to WsServerMessage
                            match claude_event {
                                clauset_types::ClaudeEvent::System(system) => {
                                    if system.subtype == "init" {
                                        Some(WsServerMessage::SessionInit {
                                            session_id,
                                            claude_session_id: system.session_id,
                                            model: system.model.clone(),
                                            tools: system.tools.clone(),
                                            cwd: system.cwd.clone().unwrap_or_default(),
                                        })
                                    } else {
                                        None
                                    }
                                }
                                clauset_types::ClaudeEvent::Assistant(assistant) => {
                                    // Extract text content from the message
                                    let mut messages = Vec::new();
                                    for block in assistant.message.content.iter() {
                                        match block {
                                            clauset_types::ContentBlock::Text { text } => {
                                                messages.push(WsServerMessage::Text {
                                                    message_id: assistant.message.id.clone(),
                                                    content: text.clone(),
                                                    is_complete: true,
                                                });
                                            }
                                            clauset_types::ContentBlock::ToolUse { id, name, input } => {
                                                messages.push(WsServerMessage::ToolUse {
                                                    message_id: assistant.message.id.clone(),
                                                    tool_use_id: id.clone(),
                                                    tool_name: name.clone(),
                                                    input: input.clone(),
                                                });
                                            }
                                            clauset_types::ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                                                messages.push(WsServerMessage::ToolResult {
                                                    tool_use_id: tool_use_id.clone(),
                                                    output: content.to_string(),
                                                    is_error: *is_error,
                                                });
                                            }
                                        }
                                    }
                                    // Check if there's an error
                                    if let Some(error) = &assistant.error {
                                        messages.push(WsServerMessage::Error {
                                            code: "claude_error".to_string(),
                                            message: error.clone(),
                                        });
                                    }
                                    // Return first message, queue the rest
                                    messages.into_iter().next()
                                }
                                clauset_types::ClaudeEvent::Result(result) => {
                                    Some(WsServerMessage::Result {
                                        success: !result.is_error,
                                        duration_ms: result.duration_ms,
                                        total_cost_usd: result.total_cost_usd,
                                        usage: result.usage.clone(),
                                    })
                                }
                                clauset_types::ClaudeEvent::User(_) => None,
                            }
                        }
                        ProcessEvent::TerminalOutput { session_id: sid, data } if *sid == session_id => {
                            // Just forward to client - buffering is done by background event processor
                            Some(WsServerMessage::TerminalOutput { data: data.clone() })
                        }
                        ProcessEvent::ActivityUpdate {
                            session_id: sid,
                            model,
                            cost,
                            input_tokens,
                            output_tokens,
                            context_percent,
                            current_activity,
                            current_step,
                            recent_actions,
                        } if *sid == session_id => {
                            Some(WsServerMessage::ActivityUpdate {
                                session_id: *sid,
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
                        }
                        ProcessEvent::Exited { session_id: sid, .. } if *sid == session_id => {
                            // Persist activity data before updating status
                            state_clone.session_manager.persist_session_activity(session_id).await;
                            // Update session status
                            let _ = state_clone
                                .session_manager
                                .update_status(session_id, clauset_types::SessionStatus::Stopped);
                            Some(WsServerMessage::StatusChange {
                                session_id,
                                old_status: clauset_types::SessionStatus::Active,
                                new_status: clauset_types::SessionStatus::Stopped,
                            })
                        }
                        ProcessEvent::Error { session_id: sid, message } if *sid == session_id => {
                            Some(WsServerMessage::Error {
                                code: "process_error".to_string(),
                                message: message.clone(),
                            })
                        }
                        _ => None,
                    };

                    if let Some(msg) = msg {
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!("Failed to serialize WebSocket message for session {}: {}", session_id, e);
                                continue;
                            }
                        };
                        if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                            debug!(
                                "WebSocket send failed for session {} (client likely disconnected): {}",
                                session_id, e
                            );
                            break;
                        }
                    }
                }
            }
        }
    });

    // Handle incoming messages
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(text) = msg {
                if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                    match client_msg {
                        WsClientMessage::Input { content } => {
                            // Validate input size
                            if content.len() > MAX_INPUT_SIZE {
                                warn!(
                                    "Input message too large ({} bytes) from session {}, max {} bytes",
                                    content.len(),
                                    session_id,
                                    MAX_INPUT_SIZE
                                );
                                continue;
                            }
                            // Mark session as busy before sending input
                            // This ensures status shows "Thinking" immediately
                            state_clone
                                .session_manager
                                .mark_session_busy(session_id)
                                .await;
                            let _ = state_clone
                                .session_manager
                                .send_input(session_id, &content)
                                .await;
                        }
                        WsClientMessage::TerminalInput { data } => {
                            // Validate terminal input size
                            if data.len() > MAX_TERMINAL_INPUT_SIZE {
                                warn!(
                                    "Terminal input too large ({} bytes) from session {}, max {} bytes",
                                    data.len(),
                                    session_id,
                                    MAX_TERMINAL_INPUT_SIZE
                                );
                                continue;
                            }
                            // Check if input contains Enter key (carriage return)
                            // If so, mark session as busy since user is submitting a command
                            if data.contains(&b'\r') || data.contains(&b'\n') {
                                state_clone
                                    .session_manager
                                    .mark_session_busy(session_id)
                                    .await;
                            }
                            let _ = state_clone
                                .session_manager
                                .send_terminal_input(session_id, &data)
                                .await;
                        }
                        WsClientMessage::Resize { rows, cols } => {
                            debug!("Resize for session {}: {}x{}", session_id, cols, rows);
                            let _ = state_clone
                                .session_manager
                                .resize_terminal(session_id, rows, cols)
                                .await;
                        }
                        WsClientMessage::RequestBuffer => {
                            // Signal send_task to send the buffer
                            let _ = buffer_tx.send(()).await;
                        }
                        WsClientMessage::Ping { timestamp } => {
                            // Pong is handled by the send task
                            tracing::debug!("Received ping: {}", timestamp);
                        }
                        WsClientMessage::GetState => {
                            // TODO: Send current state
                        }
                        WsClientMessage::StatusUpdate {
                            model,
                            cost,
                            input_tokens,
                            output_tokens,
                            context_percent,
                        } => {
                            let _ = state_clone.session_manager.update_session_stats(
                                session_id,
                                &model,
                                cost,
                                input_tokens,
                                output_tokens,
                                context_percent,
                            );
                        }
                    }
                }
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

    Ok(())
}
