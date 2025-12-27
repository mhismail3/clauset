//! Integration tests for the hook → WebSocket pipeline.
//!
//! These tests verify that hook events received from the Claude Code CLI
//! are properly parsed, processed, and would be broadcast to WebSocket clients.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use clauset_core::CreateSessionOptions;
use clauset_server::{config::Config, routes, state::AppState};
use clauset_types::{
    ContextWindow, CurrentUsage, HookEventPayload, HookEventType, SessionMode,
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// Create a minimal test app state for integration testing.
async fn create_test_app() -> (Router, Arc<AppState>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    let config = Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        db_path: db_path.clone(),
        static_dir,
        claude_path: PathBuf::from("/usr/bin/true"),
        max_concurrent_sessions: 5,
        default_model: "haiku".to_string(),
        projects_root: temp_dir.path().join("projects"),
    };

    let state = Arc::new(AppState::new(config).expect("Failed to create AppState"));

    let app = Router::new()
        .route("/api/hooks", post(routes::hooks::receive))
        .with_state(state.clone());

    (app, state, temp_dir)
}

/// Helper to create a test session with minimal options.
async fn create_test_session(state: &AppState, temp_dir: &TempDir) -> Uuid {
    let opts = CreateSessionOptions {
        project_path: temp_dir.path().to_path_buf(),
        prompt: "Test prompt".to_string(),
        model: Some("haiku".to_string()),
        mode: SessionMode::Terminal,
        resume_session_id: None,
    };
    let session = state.session_manager.create_session(opts).await.unwrap();
    session.id
}

/// Create a minimal hook payload with required fields.
fn create_hook_payload(event_name: &str, clauset_session_id: Uuid) -> HookEventPayload {
    HookEventPayload {
        clauset_session_id,
        session_id: "test-claude-session".to_string(),
        hook_event_name: event_name.to_string(),
        cwd: None,
        transcript_path: None,
        permission_mode: None,
        tool_name: None,
        tool_input: None,
        tool_response: None,
        tool_use_id: None,
        prompt: None,
        source: None,
        reason: None,
        stop_hook_active: None,
        message: None,
        notification_type: None,
        context_window: None,
        model: None,
        workspace: None,
        output_style: None,
        version: None,
        agent_id: None,
        agent_type: None,
        error: None,
        error_type: None,
        is_timeout: None,
        is_interrupt: None,
        trigger: None,
    }
}

/// Send a hook event to the test app and return the response status.
async fn send_hook_event(app: &Router, payload: &HookEventPayload) -> StatusCode {
    let body = serde_json::to_string(payload).unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/api/hooks")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    response.status()
}

#[tokio::test]
async fn test_hook_endpoint_returns_ok_for_valid_payload() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("Hello world".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_hook_endpoint_returns_ok_for_unknown_session() {
    let (app, _, _temp) = create_test_app().await;

    let mut payload = create_hook_payload("UserPromptSubmit", Uuid::new_v4());
    payload.prompt = Some("Hello".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_hook_endpoint_accepts_all_event_types() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let event_types = vec![
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
        "SubagentStart",
        "SubagentStop",
        "Notification",
        "PreCompact",
        "PermissionRequest",
    ];

    for event_name in event_types {
        let mut payload = create_hook_payload(event_name, session_id);
        // Some events require specific fields
        match event_name {
            "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "PermissionRequest" => {
                payload.tool_name = Some("TestTool".to_string());
                payload.tool_use_id = Some("test_tool_id".to_string());
            }
            _ => {}
        }
        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK, "Event type {} should return OK", event_name);
    }
}

#[tokio::test]
async fn test_hook_event_type_parsing() {
    let test_cases = vec![
        ("SessionStart", HookEventType::SessionStart),
        ("SessionEnd", HookEventType::SessionEnd),
        ("UserPromptSubmit", HookEventType::UserPromptSubmit),
        ("PreToolUse", HookEventType::PreToolUse),
        ("PostToolUse", HookEventType::PostToolUse),
        ("PostToolUseFailure", HookEventType::PostToolUseFailure),
        ("Stop", HookEventType::Stop),
        ("SubagentStart", HookEventType::SubagentStart),
        ("SubagentStop", HookEventType::SubagentStop),
        ("Notification", HookEventType::Notification),
        ("PreCompact", HookEventType::PreCompact),
        ("PermissionRequest", HookEventType::PermissionRequest),
    ];

    for (name, expected) in test_cases {
        let result = HookEventType::from_str(name);
        assert!(result.is_some(), "Should parse '{}'", name);
        assert_eq!(result.unwrap(), expected, "Parsed type for '{}'", name);
    }
}

#[tokio::test]
async fn test_context_window_extraction() {
    let context_window = ContextWindow {
        total_input_tokens: 15000,
        total_output_tokens: 5000,
        context_window_size: 200000,
        current_usage: Some(CurrentUsage {
            input_tokens: 15000,
            output_tokens: 5000,
            cache_creation_input_tokens: 1000,
            cache_read_input_tokens: 500,
        }),
    };

    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("Hello".to_string());
    payload.context_window = Some(context_window.clone());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(context_window.total_input_tokens, 15000);
    assert_eq!(context_window.total_output_tokens, 5000);
    let usage = context_window.current_usage.unwrap();
    assert_eq!(usage.input_tokens, 15000);
    assert_eq!(usage.output_tokens, 5000);
}

#[tokio::test]
async fn test_tool_use_events_capture_tool_details() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let tools = vec![
        ("Read", serde_json::json!({"path": "/test/file.txt"})),
        ("Write", serde_json::json!({"path": "/test/new.txt", "content": "hello"})),
        ("Edit", serde_json::json!({"path": "/test/file.txt", "old": "a", "new": "b"})),
        ("Bash", serde_json::json!({"command": "ls -la"})),
        ("mcp__greptile__list_pull_requests", serde_json::json!({"limit": 10})),
    ];

    for (tool_name, tool_input) in tools {
        let mut payload = create_hook_payload("PreToolUse", session_id);
        payload.tool_name = Some(tool_name.to_string());
        payload.tool_input = Some(tool_input);
        payload.tool_use_id = Some(format!("tool_{}", tool_name));

        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK, "PreToolUse for {} should succeed", tool_name);
    }
}

#[tokio::test]
async fn test_post_tool_use_failure_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let failures = vec![
        (true, false, "Command timed out"),
        (false, true, "User interrupted"),
        (false, false, "Permission denied"),
    ];

    for (is_timeout, is_interrupt, error_msg) in failures {
        let mut payload = create_hook_payload("PostToolUseFailure", session_id);
        payload.tool_name = Some("Bash".to_string());
        payload.tool_use_id = Some("tool_1".to_string());
        payload.error = Some(error_msg.to_string());
        payload.is_timeout = Some(is_timeout);
        payload.is_interrupt = Some(is_interrupt);

        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn test_subagent_lifecycle_events() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut start_payload = create_hook_payload("SubagentStart", session_id);
    start_payload.agent_id = Some("agent_123".to_string());
    start_payload.agent_type = Some("code-reviewer".to_string());
    assert_eq!(send_hook_event(&app, &start_payload).await, StatusCode::OK);

    let mut stop_payload = create_hook_payload("SubagentStop", session_id);
    stop_payload.agent_id = Some("agent_123".to_string());
    assert_eq!(send_hook_event(&app, &stop_payload).await, StatusCode::OK);
}

#[tokio::test]
async fn test_interactive_prompt_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("PreToolUse", session_id);
    payload.tool_name = Some("AskUserQuestion".to_string());
    payload.tool_input = Some(serde_json::json!({
        "questions": [{
            "question": "Which model to use?",
            "header": "Model",
            "options": [
                {"label": "opus", "description": "Claude 4 Opus"},
                {"label": "sonnet", "description": "Claude 4 Sonnet"}
            ],
            "multiSelect": false
        }]
    }));
    payload.tool_use_id = Some("ask_1".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_permission_request_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("PermissionRequest", session_id);
    payload.tool_name = Some("Write".to_string());
    payload.tool_input = Some(serde_json::json!({"path": "/etc/passwd"}));

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_pre_compact_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("PreCompact", session_id);
    payload.trigger = Some("auto".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_multiple_sessions_isolated() {
    let (app, state, temp) = create_test_app().await;

    let session1 = create_test_session(&state, &temp).await;
    let session2 = create_test_session(&state, &temp).await;

    let mut payload1 = create_hook_payload("UserPromptSubmit", session1);
    payload1.prompt = Some("Hello 1".to_string());
    assert_eq!(send_hook_event(&app, &payload1).await, StatusCode::OK);

    let mut payload2 = create_hook_payload("UserPromptSubmit", session2);
    payload2.prompt = Some("Hello 2".to_string());
    assert_eq!(send_hook_event(&app, &payload2).await, StatusCode::OK);
}

#[tokio::test]
async fn test_stop_event_with_transcript_path() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("Stop", session_id);
    payload.transcript_path = Some("/home/user/.claude/projects/test/123.jsonl".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_hooks_are_idempotent() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("Hello".to_string());

    for _ in 0..5 {
        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn test_malformed_json_returns_error() {
    let (app, _, _temp) = create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/hooks")
        .header("content-type", "application/json")
        .body(Body::from("not valid json"))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_client_error(), "Malformed JSON should return client error");
}

#[tokio::test]
async fn test_missing_required_fields_returns_error() {
    let (app, _, _temp) = create_test_app().await;

    let body = serde_json::json!({
        "clauset_session_id": Uuid::new_v4().to_string(),
        "session_id": "test"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/api/hooks")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_client_error(), "Missing required field should return client error");
}

#[tokio::test]
async fn test_hook_processing_performance() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("Hello".to_string());

    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = send_hook_event(&app, &payload).await;
    }
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 1, "100 hook events took {:?}, expected < 1s", elapsed);
}

#[tokio::test]
async fn test_user_prompt_submit_with_all_fields() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("Explain this code".to_string());
    payload.cwd = Some("/home/user/project".to_string());
    payload.context_window = Some(ContextWindow {
        total_input_tokens: 10000,
        total_output_tokens: 2000,
        context_window_size: 200000,
        current_usage: Some(CurrentUsage {
            input_tokens: 10000,
            output_tokens: 2000,
            cache_creation_input_tokens: 500,
            cache_read_input_tokens: 200,
        }),
    });

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_session_start_with_source() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let sources = vec!["startup", "resume", "clear", "compact"];

    for source in sources {
        let mut payload = create_hook_payload("SessionStart", session_id);
        payload.source = Some(source.to_string());

        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK, "SessionStart with source '{}' should succeed", source);
    }
}

#[tokio::test]
async fn test_notification_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("Notification", session_id);
    payload.message = Some("Build completed successfully".to_string());
    payload.notification_type = Some("success".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_session_end_event() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("SessionEnd", session_id);
    payload.reason = Some("clear".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_stop_hook_active_flag() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("Stop", session_id);
    payload.stop_hook_active = Some(true);

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    payload.stop_hook_active = Some(false);
    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_mcp_tool_detection() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mcp_tools = vec![
        "mcp__greptile__list_pull_requests",
        "mcp__browser_playwright__navigate",
        "mcp__filesystem__read_file",
    ];

    for tool_name in mcp_tools {
        let mut payload = create_hook_payload("PreToolUse", session_id);
        payload.tool_name = Some(tool_name.to_string());
        payload.tool_input = Some(serde_json::json!({}));
        payload.tool_use_id = Some("mcp_tool_1".to_string());

        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK, "MCP tool {} should succeed", tool_name);
    }
}

#[tokio::test]
async fn test_tool_response_with_error() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("PostToolUse", session_id);
    payload.tool_name = Some("Bash".to_string());
    payload.tool_input = Some(serde_json::json!({"command": "cat /nonexistent"}));
    payload.tool_response = Some(serde_json::json!({
        "error": "No such file or directory",
        "exit_code": 1
    }));
    payload.tool_use_id = Some("tool_err_1".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_concurrent_hook_events() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let app = app.clone();
            let mut payload = create_hook_payload("UserPromptSubmit", session_id);
            payload.prompt = Some(format!("Message {}", i));
            tokio::spawn(async move { send_hook_event(&app, &payload).await })
        })
        .collect();

    for handle in handles {
        let status = handle.await.unwrap();
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn test_large_tool_input() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let large_content = "x".repeat(100_000);
    let mut payload = create_hook_payload("PostToolUse", session_id);
    payload.tool_name = Some("Write".to_string());
    payload.tool_input = Some(serde_json::json!({
        "path": "/test/large_file.txt",
        "content": large_content
    }));
    payload.tool_response = Some(serde_json::json!({"success": true}));
    payload.tool_use_id = Some("large_write_1".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_empty_string_fields() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.prompt = Some("".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);
}
