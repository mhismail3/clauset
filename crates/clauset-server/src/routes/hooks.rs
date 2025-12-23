//! Claude Code hook event receiver.
//!
//! This module handles HTTP POST requests from the Claude Code hooks,
//! providing real-time activity tracking for the Clauset dashboard.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use clauset_core::RecentAction;
use clauset_types::{HookActivityUpdate, HookEvent, HookEventPayload, HookEventType, SessionStatus};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Response for hook events.
#[derive(Serialize)]
pub struct HookResponse {
    pub status: &'static str,
}

/// POST /api/hooks - Receive Claude Code hook events.
///
/// This endpoint is called by the clauset-hook.sh script whenever
/// Claude Code fires a hook event (PreToolUse, Stop, etc.).
pub async fn receive(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<HookEventPayload>,
) -> Result<Json<HookResponse>, (StatusCode, String)> {
    debug!(
        target: "clauset::hooks",
        "Received hook event: {} for session {}",
        payload.hook_event_name, payload.clauset_session_id
    );

    // Parse into typed event
    let event = match HookEvent::try_from(payload) {
        Ok(e) => e,
        Err(err) => {
            warn!(target: "clauset::hooks", "Failed to parse hook event: {}", err);
            return Err((StatusCode::BAD_REQUEST, err.to_string()));
        }
    };

    // Process the event
    if let Err(e) = process_hook_event(&state, event).await {
        warn!(target: "clauset::hooks", "Failed to process hook event: {}", e);
        // Return OK anyway - we don't want to block Claude
        // Errors are logged but not propagated
    }

    Ok(Json(HookResponse { status: "ok" }))
}

/// Process a parsed hook event and update session state.
async fn process_hook_event(state: &AppState, event: HookEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event {
        HookEvent::SessionStart {
            session_id, source, ..
        } => {
            info!(target: "clauset::hooks", "Session {} started (source: {})", session_id, source);
            // Confirm Ready state - session is initialized when spawned but this reinforces it
            // This ensures the dashboard shows Ready after Claude fully starts
            let update = HookActivityUpdate::stop(); // "stop" = Ready state
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::SessionEnd {
            session_id, reason, ..
        } => {
            info!(target: "clauset::hooks", "Session {} ended (reason: {})", session_id, reason);
            // Persist activity data before updating status
            state.session_manager.persist_session_activity(session_id).await;
            let _ = state.session_manager.update_status(session_id, SessionStatus::Stopped);
        }

        HookEvent::UserPromptSubmit { session_id, .. } => {
            debug!(target: "clauset::hooks", "User submitted prompt for session {}", session_id);

            // Mark session as busy (user sent input)
            state.session_manager.mark_session_busy(session_id).await;

            // Update activity to "Thinking"
            let update = HookActivityUpdate::user_prompt_submit();
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::PreToolUse {
            session_id,
            tool_name,
            tool_input,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Pre-tool use {} for session {}",
                tool_name, session_id
            );

            let update = HookActivityUpdate::pre_tool_use(tool_name, tool_input);
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::PostToolUse {
            session_id,
            tool_name,
            tool_input,
            tool_response,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Post-tool use {} for session {}",
                tool_name, session_id
            );

            let update = HookActivityUpdate::post_tool_use(tool_name, tool_input, tool_response);
            if update.is_error {
                warn!(target: "clauset::hooks", "Tool {} failed for session {}", update.tool_name.as_deref().unwrap_or("unknown"), session_id);
            }
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::Stop {
            session_id,
            stop_hook_active,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Claude stopped for session {} (continuing: {})",
                session_id, stop_hook_active
            );

            if !stop_hook_active {
                // Claude finished responding - mark as ready
                let update = HookActivityUpdate::stop();
                update_activity_from_hook(&state, session_id, update).await;
            }
        }

        HookEvent::SubagentStop {
            session_id,
            stop_hook_active,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Subagent stopped for session {} (continuing: {})",
                session_id, stop_hook_active
            );
            // Could track subagent completion separately if needed
        }

        HookEvent::Notification {
            session_id,
            message,
            notification_type,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Notification for session {}: {} ({})",
                session_id, message, notification_type
            );

            // Forward notifications that might need user attention
            // e.g., permission prompts, idle warnings
        }

        HookEvent::PreCompact {
            session_id, ..
        } => {
            debug!(target: "clauset::hooks", "Pre-compact for session {}", session_id);
            // Could show "Compacting context..." in UI
        }
    }

    Ok(())
}

/// Update session activity from a hook event and broadcast to WebSocket clients.
async fn update_activity_from_hook(
    state: &AppState,
    session_id: uuid::Uuid,
    update: HookActivityUpdate,
) {
    // Determine new activity state based on hook event type
    let (current_activity, current_step, new_action, is_busy) = match update.event_type {
        HookEventType::UserPromptSubmit => {
            ("Thinking...".to_string(), Some("Thinking".to_string()), None, true)
        }

        HookEventType::PreToolUse => {
            let tool_name = update.tool_name.clone().unwrap_or_else(|| "Unknown".to_string());

            // Create action for the tool use (may be None if filtered out)
            let summary = create_action_summary(&tool_name, &update.tool_input);

            // If the action should be filtered out (e.g., hook infrastructure), skip it
            if summary.is_none() {
                return;
            }

            let detail = extract_tool_detail(&tool_name, &update.tool_input);

            let action = RecentAction {
                action_type: tool_name_to_action_type(&tool_name),
                summary: summary.unwrap(),
                detail,
                timestamp: now_ms(),
            };

            (format!("Running {}...", tool_name), Some(tool_name), Some(action), true)
        }

        HookEventType::PostToolUse => {
            let tool_name = update.tool_name.clone().unwrap_or_else(|| "Unknown".to_string());

            // Filter out hook infrastructure (same as PreToolUse)
            if tool_name == "Bash" {
                if let Some(ref input) = update.tool_input {
                    if should_filter_bash_command(input) {
                        return;
                    }
                }
            }

            // Post-tool doesn't add new action, just confirms completion
            (format!("{} completed", tool_name), Some(tool_name), None, true)
        }

        HookEventType::Stop => {
            ("Ready".to_string(), Some("Ready".to_string()), None, false)
        }

        HookEventType::SessionEnd => {
            ("Session ended".to_string(), Some("Stopped".to_string()), None, false)
        }

        _ => {
            // For other events, don't update state
            return;
        }
    };

    // Update the session activity state and broadcast
    state.session_manager.update_activity_from_hook(
        session_id,
        current_activity,
        current_step,
        new_action,
        is_busy,
    ).await;
}

/// Convert tool name to action type for display.
fn tool_name_to_action_type(tool_name: &str) -> String {
    match tool_name {
        "Read" => "read",
        "Write" | "Edit" => "edit",
        "Bash" => "bash",
        "Grep" | "Glob" => "search",
        "Task" => "task",
        "WebFetch" | "WebSearch" => "web",
        "TodoWrite" => "task",
        "NotebookEdit" => "edit",
        _ => "task",
    }
    .to_string()
}

/// Check if text is hook-related infrastructure (should be filtered out).
/// Checks for Clauset hook script, Claude Code hooks, and related patterns.
fn is_hook_infrastructure(text: &str) -> bool {
    let text_lower = text.to_lowercase();

    // Clauset hook infrastructure
    text_lower.contains("clauset-hook")
        || text_lower.contains("clauset_hook")
        || text_lower.contains("api/hooks")
        || text_lower.contains("clauset_session_id")
        || text_lower.contains("clauset_url")
        || text_lower.contains("hook-debug.log")
        // Claude Code hooks directory
        || text_lower.contains("/.claude/hooks/")
        || text_lower.contains("\\.claude\\hooks\\")
        // Hook-related descriptions
        || text_lower.contains("stop hook")
        || text_lower.contains("pre hook")
        || text_lower.contains("post hook")
        || text_lower.contains("session hook")
        || text_lower.contains("hook event")
        || text_lower.contains("hook script")
}

/// Check if a Bash tool input should be filtered out (hook infrastructure).
fn should_filter_bash_command(input: &Value) -> bool {
    // Check command
    if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
        if is_hook_infrastructure(cmd) {
            return true;
        }
    }

    // Check description
    if let Some(desc) = input.get("description").and_then(|v| v.as_str()) {
        if is_hook_infrastructure(desc) {
            return true;
        }
    }

    false
}

/// Create a human-readable summary of the tool action.
/// Returns None if the action should be filtered out (e.g., hook infrastructure).
fn create_action_summary(tool_name: &str, input: &Option<Value>) -> Option<String> {
    if let Some(input) = input {
        match tool_name {
            "Read" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let filename = path.rsplit('/').next().unwrap_or(path);
                    // Check if reading with offset/limit (partial read)
                    let has_offset = input.get("offset").is_some();
                    let has_limit = input.get("limit").is_some();
                    if has_offset || has_limit {
                        return Some(format!("Read lines from {}", filename));
                    }
                    return Some(format!("Read {}", filename));
                }
            }
            "Write" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let filename = path.rsplit('/').next().unwrap_or(path);
                    return Some(format!("Write {}", filename));
                }
            }
            "Edit" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let filename = path.rsplit('/').next().unwrap_or(path);
                    return Some(format!("Edit {}", filename));
                }
            }
            "Bash" => {
                // Filter out hook infrastructure (check both command and description)
                if should_filter_bash_command(input) {
                    return None;
                }

                // Prefer description if available, otherwise use command
                if let Some(desc) = input.get("description").and_then(|v| v.as_str()) {
                    return Some(format!("$ {}", truncate_str(desc, 50)));
                }
                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                    let short = truncate_str(cmd, 50);
                    return Some(format!("$ {}", short));
                }
            }
            "Grep" => {
                if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                    return Some(format!("Grep: {}", truncate_str(pattern, 40)));
                }
            }
            "Glob" => {
                if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                    return Some(format!("Glob: {}", pattern));
                }
            }
            "Task" => {
                if let Some(desc) = input.get("description").and_then(|v| v.as_str()) {
                    return Some(format!("Task: {}", truncate_str(desc, 40)));
                }
            }
            "WebFetch" | "WebSearch" => {
                if let Some(url) = input.get("url").and_then(|v| v.as_str()) {
                    return Some(format!("Fetch: {}", truncate_str(url, 40)));
                }
                if let Some(query) = input.get("query").and_then(|v| v.as_str()) {
                    return Some(format!("Search: {}", truncate_str(query, 40)));
                }
            }
            _ => {}
        }
    }
    Some(tool_name.to_string())
}

/// Extract detail information from tool input.
fn extract_tool_detail(tool_name: &str, input: &Option<Value>) -> Option<String> {
    let input = input.as_ref()?;

    match tool_name {
        "Read" | "Write" | "Edit" => input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),

        "Bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 80)),

        "Grep" | "Glob" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),

        _ => None,
    }
}

/// Truncate a string to a maximum length.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Get current time in milliseconds since UNIX epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
