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

    // Channel for recv_task to request buffer sends (legacy)
    let (buffer_tx, mut buffer_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Channel for recv_task to send outgoing messages (for sync responses, chunk batches, etc.)
    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel::<WsServerMessage>(32);

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
                // Handle outgoing messages from recv_task (sync responses, chunk batches, etc.)
                Some(msg) = outgoing_rx.recv() => {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                            debug!(target: "clauset::ws", "WebSocket send failed for session {}: {}", session_id, e);
                            break;
                        }
                    }
                }
                // Handle buffer request from recv_task (legacy)
                Some(()) = buffer_rx.recv() => {
                    // Send terminal buffer if available
                    if let Some(buffer) = state_clone.session_manager.get_terminal_buffer(session_id).await {
                        if !buffer.is_empty() {
                            debug!(target: "clauset::ws", "Sending terminal buffer for session {}: {} bytes", session_id, buffer.len());
                            let buffer_msg = WsServerMessage::TerminalBuffer { data: buffer };
                            if let Ok(json) = serde_json::to_string(&buffer_msg) {
                                if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                                    debug!(target: "clauset::ws", "WebSocket send failed for session {}: {}", session_id, e);
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
                        ProcessEvent::TerminalOutput { .. } => {
                            // DEPRECATED: Raw TerminalOutput events are converted to SequencedTerminalOutput
                            // by the event processor. We handle SequencedTerminalOutput instead.
                            None
                        }
                        ProcessEvent::SequencedTerminalOutput { session_id: sid, seq, data, timestamp } if *sid == session_id => {
                            // Send sequenced chunk for reliable streaming protocol
                            Some(WsServerMessage::TerminalChunk {
                                seq: *seq,
                                data: data.clone(),
                                timestamp: *timestamp,
                            })
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
                        ProcessEvent::Chat(chat_event) => {
                            // Forward chat events for chat mode view
                            // Check if this event is for our session
                            let event_session_id = match &chat_event {
                                clauset_types::ChatEvent::Message { session_id, .. } => *session_id,
                                clauset_types::ChatEvent::ContentDelta { session_id, .. } => *session_id,
                                clauset_types::ChatEvent::ToolCallStart { session_id, .. } => *session_id,
                                clauset_types::ChatEvent::ToolCallComplete { session_id, .. } => *session_id,
                                clauset_types::ChatEvent::MessageComplete { session_id, .. } => *session_id,
                            };
                            if event_session_id == session_id {
                                Some(WsServerMessage::ChatEvent { event: chat_event.clone() })
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(msg) = msg {
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!(target: "clauset::ws", "Failed to serialize WebSocket message for session {}: {}", session_id, e);
                                continue;
                            }
                        };
                        if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                            debug!(
                                target: "clauset::ws",
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
    let outgoing_tx_clone = outgoing_tx;
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(text) = msg {
                if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                    match client_msg {
                        WsClientMessage::Input { content } => {
                            // Validate input size
                            if content.len() > MAX_INPUT_SIZE {
                                warn!(
                                    target: "clauset::ws",
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
                                    target: "clauset::ws",
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
                            debug!(target: "clauset::ws", "Resize for session {}: {}x{}", session_id, cols, rows);
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
                            // Send pong response
                            let pong = WsServerMessage::Pong { timestamp };
                            let _ = outgoing_tx_clone.send(pong).await;
                            tracing::trace!(target: "clauset::ws::ping", "Sent pong for timestamp: {}", timestamp);
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

                        // === Reliable Streaming Protocol (Phase 1.3) ===
                        WsClientMessage::SyncRequest { last_seq, cols, rows } => {
                            debug!(target: "clauset::ws", "SyncRequest: session={}, last_seq={}, cols={}, rows={}", session_id, last_seq, cols, rows);

                            // Resize terminal to match client dimensions
                            let _ = state_clone
                                .session_manager
                                .resize_terminal(session_id, rows, cols)
                                .await;

                            // Get buffer info to determine what the client needs
                            let buffers = state_clone.session_manager.buffers();
                            let (buffer_start_seq, buffer_end_seq, full_buffer) = if let Some((start, end, data)) = buffers.get_full_buffer(session_id).await {
                                // Client needs full buffer if:
                                // - Fresh connection (last_seq == 0)
                                // - Client is behind the buffer start (missed too many chunks)
                                let needs_full = last_seq == 0 || last_seq < start;
                                if needs_full {
                                    debug!(target: "clauset::ws", "SyncResponse: sending full buffer ({} bytes, seq {}..{})", data.len(), start, end);
                                    (start, end, Some(data))
                                } else {
                                    debug!(target: "clauset::ws", "SyncResponse: client up to date (last_seq={}, buffer {}..{})", last_seq, start, end);
                                    (start, end, None)
                                }
                            } else {
                                // No buffer yet - fresh session
                                debug!(target: "clauset::ws", "SyncResponse: no buffer yet for session {}", session_id);
                                (0, 0, None)
                            };

                            // Send SyncResponse
                            let response = WsServerMessage::SyncResponse {
                                buffer_start_seq,
                                buffer_end_seq,
                                cols,
                                rows,
                                full_buffer,
                                full_buffer_start_seq: if buffer_start_seq > 0 { Some(buffer_start_seq) } else { None },
                            };
                            let _ = outgoing_tx_clone.send(response).await;
                        }
                        WsClientMessage::Ack { ack_seq } => {
                            // Track client acknowledgment for flow control
                            // Future: pause sending if client falls too far behind
                            tracing::trace!(target: "clauset::ws", "Ack: session={}, seq={}", session_id, ack_seq);
                        }
                        WsClientMessage::RangeRequest { start_seq, end_seq } => {
                            debug!(target: "clauset::ws", "RangeRequest: session={}, range={}..{}", session_id, start_seq, end_seq);

                            // Fetch requested chunks from buffer
                            let buffers = state_clone.session_manager.buffers();
                            if let Some(chunks) = buffers.get_chunk_range(session_id, start_seq, end_seq).await {
                                if !chunks.is_empty() {
                                    // Concatenate chunk data for batch response
                                    let data: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
                                    let chunk_count = chunks.len() as u32;
                                    debug!(target: "clauset::ws", "ChunkBatch: sending {} chunks ({} bytes)", chunk_count, data.len());

                                    let batch = WsServerMessage::ChunkBatch {
                                        start_seq,
                                        data,
                                        chunk_count,
                                        is_complete: true,
                                    };
                                    let _ = outgoing_tx_clone.send(batch).await;
                                } else {
                                    // Requested range not available (buffer may have overflowed)
                                    debug!(target: "clauset::ws", "RangeRequest: no chunks in range {}..{}", start_seq, end_seq);
                                    // Notify client they need to resync
                                    if let Some((new_start, _)) = buffers.get_buffer_info(session_id).await {
                                        let overflow = WsServerMessage::BufferOverflow {
                                            new_start_seq: new_start,
                                            requires_resync: true,
                                        };
                                        let _ = outgoing_tx_clone.send(overflow).await;
                                    }
                                }
                            }
                        }
                        WsClientMessage::NegotiateDimensions {
                            cols,
                            rows,
                            confidence,
                            source,
                            cell_width: _,
                            font_loaded,
                            device_hint,
                        } => {
                            debug!(target: "clauset::ws", "NegotiateDimensions: session={}, {}x{}, conf={}, src={}, device={}",
                                session_id, cols, rows, confidence, source, device_hint);

                            // Convert string fields to enum types for validation
                            let confidence_level = match confidence.as_str() {
                                "high" => Some(clauset_core::ConfidenceLevel::High),
                                "medium" => Some(clauset_core::ConfidenceLevel::Medium),
                                _ => Some(clauset_core::ConfidenceLevel::Low),
                            };
                            let dim_source = match source.as_str() {
                                "fitaddon" => Some(clauset_core::DimensionSource::Fitaddon),
                                "container" => Some(clauset_core::DimensionSource::Container),
                                "estimation" => Some(clauset_core::DimensionSource::Estimation),
                                _ => Some(clauset_core::DimensionSource::Defaults),
                            };
                            let device = match device_hint.as_str() {
                                "iphone" => Some(clauset_core::DeviceHint::Iphone),
                                "ipad" => Some(clauset_core::DeviceHint::Ipad),
                                "desktop" => Some(clauset_core::DeviceHint::Desktop),
                                _ => Some(clauset_core::DeviceHint::Unknown),
                            };

                            // Validate dimensions
                            match clauset_core::validate_dimensions(cols, rows, device, confidence_level, dim_source) {
                                Ok(validated) => {
                                    // Apply the dimensions to the terminal
                                    let final_cols = validated.cols;
                                    let final_rows = validated.rows;

                                    // Resize terminal
                                    let _ = state_clone
                                        .session_manager
                                        .resize_terminal(session_id, final_rows, final_cols)
                                        .await;

                                    // Log font loading status for debugging
                                    if !font_loaded {
                                        debug!(target: "clauset::ws", "Client reports font not loaded for session {}", session_id);
                                    }

                                    // Send confirmation
                                    let response = WsServerMessage::DimensionsConfirmed {
                                        cols: final_cols,
                                        rows: final_rows,
                                        adjusted: validated.adjusted,
                                        adjustment_reason: validated.adjustment_reason,
                                    };
                                    let _ = outgoing_tx_clone.send(response).await;
                                }
                                Err(error) => {
                                    // Dimensions rejected - send suggested dimensions
                                    let response = WsServerMessage::DimensionsRejected {
                                        reason: error.reason,
                                        suggested_cols: error.suggested_cols,
                                        suggested_rows: error.suggested_rows,
                                    };
                                    let _ = outgoing_tx_clone.send(response).await;
                                }
                            }
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
