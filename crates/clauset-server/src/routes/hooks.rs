//! Claude Code hook event receiver.
//!
//! This module handles HTTP POST requests from the Claude Code hooks,
//! providing real-time activity tracking for the Clauset dashboard.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use clauset_core::{
    transcript_event_to_chat_event, ChatProcessor, ProcessEvent, RecentAction, TranscriptEvent,
};
use clauset_types::{
    ChatEvent, HookActivityUpdate, HookEvent, HookEventPayload, HookEventType,
    InteractiveEvent, InteractivePrompt, InteractiveQuestion, PermissionMode, QuestionOption,
    SessionStatus,
};
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
    let session_id = payload.clauset_session_id;
    debug!(
        target: "clauset::hooks",
        "Received hook event: {} for session {}",
        payload.hook_event_name, session_id
    );

    let permission_mode = payload.permission_mode.clone();
    let model_display = payload.model.as_ref().and_then(|model| {
        if !model.display_name.is_empty() {
            Some(model.display_name.clone())
        } else if !model.id.is_empty() {
            Some(model.id.clone())
        } else {
            None
        }
    });

    // Parse into typed event
    let event = match HookEvent::try_from(payload) {
        Ok(e) => e,
        Err(err) => {
            warn!(target: "clauset::hooks", "Failed to parse hook event: {}", err);
            return Err((StatusCode::BAD_REQUEST, err.to_string()));
        }
    };

    // Capture Claude's session ID from the hook event (first hook that fires)
    // This is needed for resume to work - Terminal mode doesn't emit JSON events
    let claude_session_id = extract_claude_session_id(&event);
    if let Err(e) = state.session_manager.set_claude_session_id(session_id, &claude_session_id) {
        // This will fail if already set (which is expected) - only log on real errors
        debug!(target: "clauset::hooks", "Could not set Claude session ID: {}", e);
    }

    if let Some(raw_mode) = permission_mode {
        if let Some(mode) = PermissionMode::from_hook_value(&raw_mode) {
            state.session_manager.update_permission_mode(session_id, mode).await;
        }
    }

    // Get current session costs for interaction delta calculation
    let (cost_usd, input_tokens, output_tokens) =
        if let Some(activity) = state.session_manager.get_activity(session_id).await {
            (activity.cost, activity.input_tokens, activity.output_tokens)
        } else {
            (0.0, 0, 0)
        };

    // Capture interaction data for persistence (runs concurrently with activity update)
    state
        .interaction_processor
        .process_event(&event, cost_usd, input_tokens, output_tokens)
        .await;

    // Process the event for chat mode messages
    let chat_events = state.chat_processor.process_hook_event(&event).await;
    for chat_event in chat_events {
        // Broadcast chat events to WebSocket clients
        let _ = state.session_manager.broadcast_event(ProcessEvent::Chat(chat_event));
    }

    // Intercept AskUserQuestion tool calls for native UI rendering
    if let HookEvent::PreToolUse { session_id, tool_name, tool_input, .. } = &event {
        if tool_name == "AskUserQuestion" {
            if let Some(questions) = parse_ask_user_question(tool_input) {
                debug!(
                    target: "clauset::hooks",
                    "Broadcasting {} interactive questions for session {}",
                    questions.len(), session_id
                );
                // Batch all questions into a single prompt event
                let prompt = InteractivePrompt::new(questions);
                let interactive_event = InteractiveEvent::PromptPresented {
                    session_id: *session_id,
                    prompt,
                };
                let _ = state.session_manager.broadcast_event(
                    ProcessEvent::Interactive(interactive_event)
                );
            }
        }
    }

    // Process the event for real-time activity updates
    if let Err(e) = process_hook_event(&state, event, model_display).await {
        warn!(target: "clauset::hooks", "Failed to process hook event: {}", e);
        // Return OK anyway - we don't want to block Claude
        // Errors are logged but not propagated
    }

    Ok(Json(HookResponse { status: "ok" }))
}

/// Process a parsed hook event and update session state.
async fn process_hook_event(
    state: &AppState,
    event: HookEvent,
    model_display: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event {
        HookEvent::SessionStart {
            session_id, source, transcript_path, context_window, ..
        } => {
            info!(target: "clauset::hooks", "Session {} started (source: {})", session_id, source);

            // Seed context/model data from hook payload when available.
            if let Some(ref ctx) = context_window {
                state.session_manager.update_context_from_hook(
                    session_id,
                    ctx.total_input_tokens,
                    ctx.total_output_tokens,
                    ctx.context_window_size,
                    ctx.current_usage.clone(),
                    model_display.clone(),
                ).await;
            }

            // Start transcript watcher for real-time content streaming
            if let Some(path) = transcript_path {
                info!(target: "clauset::hooks", "Starting transcript watcher for session {} at {}", session_id, path);
                match state.chat_processor.start_transcript_watcher(session_id, &path) {
                    Ok(mut event_rx) => {
                        // Spawn task to process transcript events
                        let session_manager = state.session_manager.clone();
                        let chat_processor = state.chat_processor.clone();
                        tokio::spawn(async move {
                            while let Some(event) = event_rx.recv().await {
                                match event {
                                    // Usage events update session activity (authoritative token source)
                                    TranscriptEvent::Usage { usage, .. } => {
                                        session_manager.update_usage_from_transcript(
                                            session_id,
                                            usage.input_tokens,
                                            usage.output_tokens,
                                            usage.cache_read_input_tokens,
                                            usage.cache_creation_input_tokens,
                                            &usage.model,
                                        ).await;
                                    }
                                    // Other events convert to chat events for broadcast
                                    _ => {
                                        if let Some(chat_event) = transcript_event_to_chat_event(session_id, event) {
                                            // Remap message IDs from Claude's IDs to our internal IDs
                                            let remapped_event = remap_chat_event_message_id(
                                                chat_event,
                                                session_id,
                                                &chat_processor,
                                            ).await;
                                            let _ = session_manager.broadcast_event(ProcessEvent::Chat(remapped_event));
                                        }
                                    }
                                }
                            }
                            debug!(target: "clauset::hooks", "Transcript watcher event loop ended for session {}", session_id);
                        });
                    }
                    Err(e) => {
                        warn!(target: "clauset::hooks", "Failed to start transcript watcher for session {}: {}", session_id, e);
                    }
                }
            }

            // NOTE: We intentionally do NOT set activity to Ready here.
            // The session is already initialized to Ready when created in session.rs.
            // Setting it here would cause a race condition: if UserPromptSubmit arrives
            // before SessionStart (which can happen), this would overwrite the
            // "Thinking" state with "Ready", causing the dashboard to show Ready
            // even while Claude is actively processing.
        }

        HookEvent::SessionEnd {
            session_id, reason, ..
        } => {
            info!(target: "clauset::hooks", "Session {} ended (reason: {})", session_id, reason);

            // Stop transcript watcher
            state.chat_processor.stop_transcript_watcher(session_id).await;

            // Persist activity data before updating status
            state.session_manager.persist_session_activity(session_id).await;
            let _ = state.session_manager.update_status(session_id, SessionStatus::Stopped);
        }

        HookEvent::UserPromptSubmit {
            session_id,
            claude_session_id,
            prompt,
            cwd,
            context_window,
        } => {
            debug!(target: "clauset::hooks", "User submitted prompt for session {}", session_id);

            // Mark session as busy (user sent input)
            state.session_manager.mark_session_busy(session_id).await;

            // Update activity to "Thinking"
            let update = HookActivityUpdate::user_prompt_submit();
            update_activity_from_hook(&state, session_id, update).await;

            // Update context window size from hook data (transcript handles token counts)
            if let Some(ref ctx) = context_window {
                state.session_manager.update_context_from_hook(
                    session_id,
                    ctx.total_input_tokens,
                    ctx.total_output_tokens,
                    ctx.context_window_size,
                    ctx.current_usage.clone(),
                    model_display.clone(),
                ).await;
            }

            // Index the prompt for Prompt Library
            if let Some(cwd) = cwd {
                let prompt_entry = clauset_types::Prompt::new(
                    claude_session_id.clone(),
                    std::path::PathBuf::from(&cwd),
                    prompt.clone(),
                    now_ms(),
                );

                if let Err(e) = state.interaction_processor.store().insert_prompt(&prompt_entry) {
                    warn!(target: "clauset::hooks", "Failed to index prompt: {}", e);
                }

                // Broadcast for real-time UI update
                let summary: clauset_types::PromptSummary = (&prompt_entry).into();
                let _ = state.session_manager.broadcast_event(ProcessEvent::NewPrompt(summary));
            }
        }

        HookEvent::PreToolUse {
            session_id,
            tool_name,
            tool_input,
            cwd: _,
            context_window,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Pre-tool use {} for session {}",
                tool_name, session_id
            );

            // Update context window from accurate hook data
            if let Some(ref ctx) = context_window {
                state.session_manager.update_context_from_hook(
                    session_id,
                    ctx.total_input_tokens,
                    ctx.total_output_tokens,
                    ctx.context_window_size,
                    ctx.current_usage.clone(),
                    model_display.clone(),
                ).await;
            }

            let update = HookActivityUpdate::pre_tool_use(tool_name, tool_input);
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::PostToolUse {
            session_id,
            tool_name,
            tool_input,
            tool_response,
            context_window,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Post-tool use {} for session {}",
                tool_name, session_id
            );

            // Detect Task tool (subagent) completion and broadcast detailed info
            if tool_name == "Task" {
                // Extract agent type and description from tool_input
                let agent_type = tool_input
                    .get("subagent_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general-purpose")
                    .to_string();

                let description = tool_input
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Task completed")
                    .to_string();

                // Extract result from tool_response (truncate if too long)
                let result = tool_response
                    .as_str()
                    .map(|s| {
                        if s.len() > 500 {
                            format!("{}...", &s[..500])
                        } else {
                            s.to_string()
                        }
                    })
                    .unwrap_or_else(|| "No result".to_string());

                info!(
                    target: "clauset::hooks",
                    "Subagent completed for session {}: type={}, desc={}",
                    session_id, agent_type, description
                );

                let _ = state.session_manager.broadcast_event(ProcessEvent::SubagentCompleted {
                    session_id,
                    agent_type,
                    description,
                    result,
                });
            }

            // Update context window from accurate hook data
            if let Some(ref ctx) = context_window {
                state.session_manager.update_context_from_hook(
                    session_id,
                    ctx.total_input_tokens,
                    ctx.total_output_tokens,
                    ctx.context_window_size,
                    ctx.current_usage.clone(),
                    model_display.clone(),
                ).await;
            }

            let update = HookActivityUpdate::post_tool_use(tool_name, tool_input, tool_response);
            if update.is_error {
                warn!(target: "clauset::hooks", "Tool {} failed for session {}", update.tool_name.as_deref().unwrap_or("unknown"), session_id);
            }
            update_activity_from_hook(&state, session_id, update).await;
        }

        HookEvent::Stop {
            session_id,
            stop_hook_active,
            transcript_path,
            context_window,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Claude stopped for session {} (continuing: {}, transcript: {:?})",
                session_id, stop_hook_active, transcript_path
            );

            // Update context window from accurate hook data (replaces regex parsing)
            if let Some(ref ctx) = context_window {
                state.session_manager.update_context_from_hook(
                    session_id,
                    ctx.total_input_tokens,
                    ctx.total_output_tokens,
                    ctx.context_window_size,
                    ctx.current_usage.clone(),
                    model_display.clone(),
                ).await;
            }

            if !stop_hook_active {
                // Claude finished responding - broadcast Ready state
                // HookActivityUpdate::stop() returns is_busy=false, which update_activity_from_hook
                // uses to clear the busy flag atomically with the state update
                let update = HookActivityUpdate::stop();
                update_activity_from_hook(&state, session_id, update).await;
                debug!(target: "clauset::hooks", "Session {} marked as ready (Stop hook)", session_id);
            } else {
                debug!(target: "clauset::hooks", "Session {} Stop hook active, not marking as ready", session_id);
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

            // Broadcast subagent stop to frontend
            // Note: SubagentStop doesn't include agent_id, using empty string
            let _ = state.session_manager.broadcast_event(ProcessEvent::SubagentStopped {
                session_id,
                agent_id: String::new(), // Not provided in SubagentStop hook
            });
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
            session_id,
            trigger,
            ..
        } => {
            debug!(target: "clauset::hooks", "Pre-compact for session {} (trigger: {})", session_id, trigger);

            // Broadcast context compaction to frontend
            let _ = state.session_manager.broadcast_event(ProcessEvent::ContextCompacting {
                session_id,
                trigger,
            });
        }

        HookEvent::PostToolUseFailure {
            session_id,
            tool_name,
            error,
            is_timeout,
            is_interrupt,
            ..
        } => {
            warn!(
                target: "clauset::hooks",
                "Tool {} failed for session {}: {:?} (timeout: {}, interrupt: {})",
                tool_name, session_id, error, is_timeout, is_interrupt
            );

            // Broadcast tool error to frontend for display
            let _ = state.session_manager.broadcast_event(ProcessEvent::ToolError {
                session_id,
                tool_name,
                error: error.unwrap_or_else(|| "Unknown error".to_string()),
                is_timeout,
            });
        }

        HookEvent::SubagentStart {
            session_id,
            agent_id,
            agent_type,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Subagent started for session {}: {} (type: {})",
                session_id, agent_id, agent_type
            );

            // Broadcast subagent start to frontend
            let _ = state.session_manager.broadcast_event(ProcessEvent::SubagentStarted {
                session_id,
                agent_id,
                agent_type,
            });
            // TODO: Track subagent in session state
        }

        HookEvent::PermissionRequest {
            session_id,
            tool_name,
            tool_input,
            tool_use_id,
            ..
        } => {
            debug!(
                target: "clauset::hooks",
                "Permission request for session {}: {} ({})",
                session_id, tool_name, tool_use_id
            );

            // Broadcast permission request to frontend for display
            let _ = state.session_manager.broadcast_event(ProcessEvent::PermissionRequest {
                session_id,
                tool_name,
                tool_input,
            });
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

/// Extract Claude's session ID from any hook event.
fn extract_claude_session_id(event: &HookEvent) -> String {
    match event {
        HookEvent::SessionStart { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::SessionEnd { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::UserPromptSubmit { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::PreToolUse { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::PostToolUse { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::Stop { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::SubagentStop { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::Notification { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::PreCompact { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::PostToolUseFailure { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::SubagentStart { claude_session_id, .. } => claude_session_id.clone(),
        HookEvent::PermissionRequest { claude_session_id, .. } => claude_session_id.clone(),
    }
}

/// Parse AskUserQuestion tool input into structured questions.
///
/// The tool_input format from Claude Code is:
/// ```json
/// {
///   "questions": [
///     {
///       "header": "Model",
///       "question": "Which model should be used?",
///       "multiSelect": false,
///       "options": [
///         { "label": "Claude Sonnet", "description": "Balanced performance" },
///         { "label": "Claude Opus", "description": "Maximum capability" }
///       ]
///     }
///   ]
/// }
/// ```
fn parse_ask_user_question(input: &Value) -> Option<Vec<InteractiveQuestion>> {
    let questions = input.get("questions")?.as_array()?;

    let parsed: Vec<InteractiveQuestion> = questions
        .iter()
        .filter_map(|q| {
            let header = q.get("header")?.as_str()?.to_string();
            let question = q.get("question")?.as_str()?.to_string();
            let multi_select = q.get("multiSelect").and_then(|v| v.as_bool()).unwrap_or(false);

            let options: Vec<QuestionOption> = q
                .get("options")?
                .as_array()?
                .iter()
                .enumerate()
                .filter_map(|(i, opt)| {
                    Some(QuestionOption {
                        index: i + 1, // 1-based for PTY response
                        label: opt.get("label")?.as_str()?.to_string(),
                        description: opt.get("description").and_then(|v| v.as_str()).map(String::from),
                    })
                })
                .collect();

            if options.is_empty() {
                return None;
            }

            Some(InteractiveQuestion::new(header, question, options, multi_select))
        })
        .collect();

    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

/// Remap message IDs in chat events from Claude's IDs to our internal IDs.
///
/// Transcript events use Claude's message IDs (like `msg_015LzU...`), but our
/// message lifecycle uses internal IDs (like `assistant-{uuid}`). This function
/// looks up the current assistant message ID for the session and remaps the
/// event's message_id to match.
async fn remap_chat_event_message_id(
    event: ChatEvent,
    session_id: uuid::Uuid,
    chat_processor: &Arc<ChatProcessor>,
) -> ChatEvent {
    // Get the current assistant message ID for this session
    let internal_id = match chat_processor.get_current_assistant_message_id(session_id).await {
        Some(id) => id,
        None => {
            // No current message - return event unchanged
            // This can happen if the message hasn't been created yet
            debug!(
                target: "clauset::hooks",
                "No current assistant message for session {}, using original ID",
                session_id
            );
            return event;
        }
    };

    // Remap the message_id in content delta events
    match event {
        ChatEvent::ContentDelta { session_id, delta, .. } => {
            ChatEvent::ContentDelta {
                session_id,
                message_id: internal_id,
                delta,
            }
        }
        ChatEvent::ThinkingDelta { session_id, delta, .. } => {
            ChatEvent::ThinkingDelta {
                session_id,
                message_id: internal_id,
                delta,
            }
        }
        ChatEvent::ToolCallStart { session_id, tool_call, .. } => {
            ChatEvent::ToolCallStart {
                session_id,
                message_id: internal_id,
                tool_call,
            }
        }
        ChatEvent::ToolCallComplete { session_id, tool_call_id, output, is_error, .. } => {
            ChatEvent::ToolCallComplete {
                session_id,
                message_id: internal_id,
                tool_call_id,
                output,
                is_error,
            }
        }
        // Other event types pass through unchanged
        other => other,
    }
}
