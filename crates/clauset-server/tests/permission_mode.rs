//! Integration tests for permission mode handling.
//!
//! These tests verify that permission mode changes from Claude Code hooks
//! are properly processed, stored, and broadcast to WebSocket clients.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use clauset_core::{CreateSessionOptions, ProcessEvent};
use clauset_server::{config::Config, routes, state::AppState};
use clauset_types::{HookEventPayload, PermissionMode, SessionMode};
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

// =============================================================================
// Permission Mode Parsing Tests
// =============================================================================

#[test]
fn test_permission_mode_from_hook_value_default() {
    assert_eq!(
        PermissionMode::from_hook_value("default"),
        Some(PermissionMode::Default)
    );
    assert_eq!(
        PermissionMode::from_hook_value("normal"),
        Some(PermissionMode::Default)
    );
    assert_eq!(
        PermissionMode::from_hook_value("Default"),
        Some(PermissionMode::Default)
    );
    assert_eq!(
        PermissionMode::from_hook_value("DEFAULT"),
        Some(PermissionMode::Default)
    );
}

#[test]
fn test_permission_mode_from_hook_value_plan() {
    assert_eq!(
        PermissionMode::from_hook_value("plan"),
        Some(PermissionMode::Plan)
    );
    assert_eq!(
        PermissionMode::from_hook_value("Plan"),
        Some(PermissionMode::Plan)
    );
    assert_eq!(
        PermissionMode::from_hook_value("plan mode"),
        Some(PermissionMode::Plan)
    );
}

#[test]
fn test_permission_mode_from_hook_value_accept_edits() {
    // Claude Code sends these camelCase values
    assert_eq!(
        PermissionMode::from_hook_value("acceptEdits"),
        Some(PermissionMode::AcceptEdits)
    );
    assert_eq!(
        PermissionMode::from_hook_value("accept_edits"),
        Some(PermissionMode::AcceptEdits)
    );
    assert_eq!(
        PermissionMode::from_hook_value("accept edits"),
        Some(PermissionMode::AcceptEdits)
    );
}

#[test]
fn test_permission_mode_from_hook_value_bypass_permissions() {
    // Claude Code sends these camelCase values
    assert_eq!(
        PermissionMode::from_hook_value("bypassPermissions"),
        Some(PermissionMode::BypassPermissions)
    );
    assert_eq!(
        PermissionMode::from_hook_value("bypass_permissions"),
        Some(PermissionMode::BypassPermissions)
    );
    assert_eq!(
        PermissionMode::from_hook_value("bypass permissions"),
        Some(PermissionMode::BypassPermissions)
    );
}

#[test]
fn test_permission_mode_from_hook_value_unknown() {
    assert_eq!(PermissionMode::from_hook_value("unknown"), None);
    assert_eq!(PermissionMode::from_hook_value(""), None);
    assert_eq!(PermissionMode::from_hook_value("invalid_mode"), None);
}

#[test]
fn test_permission_mode_from_hook_value_whitespace() {
    assert_eq!(
        PermissionMode::from_hook_value("  default  "),
        Some(PermissionMode::Default)
    );
    assert_eq!(
        PermissionMode::from_hook_value("\tplan\n"),
        Some(PermissionMode::Plan)
    );
}

// =============================================================================
// Hook Processing Tests
// =============================================================================

#[tokio::test]
async fn test_hook_with_permission_mode_updates_activity() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // Initially, no permission mode should be set
    let activity = state.session_manager.get_activity(session_id).await;
    assert!(activity.is_none() || activity.as_ref().unwrap().permission_mode.is_none());

    // Send a hook event with permission_mode = "plan"
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("plan".to_string());
    payload.source = Some("startup".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    // Verify mode was updated
    let activity = state.session_manager.get_activity(session_id).await;
    assert!(activity.is_some());
    assert_eq!(activity.unwrap().permission_mode, Some(PermissionMode::Plan));
}

#[tokio::test]
async fn test_hook_with_camelcase_permission_mode() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // Send hook with camelCase mode (as Claude Code sends it)
    let mut payload = create_hook_payload("UserPromptSubmit", session_id);
    payload.permission_mode = Some("acceptEdits".to_string());
    payload.prompt = Some("test".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    let activity = state.session_manager.get_activity(session_id).await;
    assert!(activity.is_some());
    assert_eq!(
        activity.unwrap().permission_mode,
        Some(PermissionMode::AcceptEdits)
    );
}

#[tokio::test]
async fn test_hook_with_bypass_permissions_mode() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let mut payload = create_hook_payload("PreToolUse", session_id);
    payload.permission_mode = Some("bypassPermissions".to_string());
    payload.tool_name = Some("Bash".to_string());
    payload.tool_use_id = Some("tool_1".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    let activity = state.session_manager.get_activity(session_id).await;
    assert!(activity.is_some());
    assert_eq!(
        activity.unwrap().permission_mode,
        Some(PermissionMode::BypassPermissions)
    );
}

#[tokio::test]
async fn test_mode_change_event_broadcast_on_change() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // Subscribe to events before sending
    let mut event_rx = state.session_manager.subscribe();

    // Send hook with mode = "plan"
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("plan".to_string());
    payload.source = Some("startup".to_string());

    let status = send_hook_event(&app, &payload).await;
    assert_eq!(status, StatusCode::OK);

    // Check for ModeChange event in the broadcast
    // We need to drain events looking for ModeChange
    let mut found_mode_change = false;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(100);

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(tokio::time::Duration::from_millis(10), event_rx.recv()).await {
            Ok(Ok(event)) => {
                if let ProcessEvent::ModeChange { session_id: sid, mode } = event {
                    if sid == session_id && mode == PermissionMode::Plan {
                        found_mode_change = true;
                        break;
                    }
                }
            }
            _ => break,
        }
    }

    assert!(found_mode_change, "Expected ModeChange event to be broadcast");
}

#[tokio::test]
async fn test_mode_change_event_not_broadcast_when_same() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // First, set the mode to "plan"
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("plan".to_string());
    payload.source = Some("startup".to_string());
    send_hook_event(&app, &payload).await;

    // Wait a bit for the first event to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Now subscribe (after first mode is set)
    let mut event_rx = state.session_manager.subscribe();

    // Send another hook with the SAME mode
    let mut payload2 = create_hook_payload("UserPromptSubmit", session_id);
    payload2.permission_mode = Some("plan".to_string());
    payload2.prompt = Some("test".to_string());

    send_hook_event(&app, &payload2).await;

    // Check that NO ModeChange event is broadcast
    let mut found_mode_change = false;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(100);

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(tokio::time::Duration::from_millis(10), event_rx.recv()).await {
            Ok(Ok(event)) => {
                if let ProcessEvent::ModeChange { session_id: sid, .. } = event {
                    if sid == session_id {
                        found_mode_change = true;
                    }
                }
            }
            _ => break,
        }
    }

    assert!(
        !found_mode_change,
        "ModeChange should NOT be broadcast when mode is unchanged"
    );
}

#[tokio::test]
async fn test_mode_change_broadcast_on_different_mode() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // First, set mode to "default"
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("default".to_string());
    payload.source = Some("startup".to_string());
    send_hook_event(&app, &payload).await;

    // Wait for first event
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Subscribe after first mode is set
    let mut event_rx = state.session_manager.subscribe();

    // Send hook with DIFFERENT mode
    let mut payload2 = create_hook_payload("UserPromptSubmit", session_id);
    payload2.permission_mode = Some("plan".to_string());
    payload2.prompt = Some("test".to_string());

    send_hook_event(&app, &payload2).await;

    // Should receive ModeChange event for the new mode
    let mut found_mode_change = false;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(100);

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(tokio::time::Duration::from_millis(10), event_rx.recv()).await {
            Ok(Ok(event)) => {
                if let ProcessEvent::ModeChange { session_id: sid, mode } = event {
                    if sid == session_id && mode == PermissionMode::Plan {
                        found_mode_change = true;
                        break;
                    }
                }
            }
            _ => break,
        }
    }

    assert!(
        found_mode_change,
        "ModeChange event should be broadcast when mode changes"
    );
}

#[tokio::test]
async fn test_hook_without_permission_mode_keeps_existing() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // First, set mode to "plan"
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("plan".to_string());
    payload.source = Some("startup".to_string());
    send_hook_event(&app, &payload).await;

    // Verify mode is set
    let activity = state.session_manager.get_activity(session_id).await;
    assert_eq!(activity.unwrap().permission_mode, Some(PermissionMode::Plan));

    // Send hook WITHOUT permission_mode
    let mut payload2 = create_hook_payload("UserPromptSubmit", session_id);
    payload2.prompt = Some("test".to_string());
    // permission_mode is None

    send_hook_event(&app, &payload2).await;

    // Mode should still be "plan"
    let activity = state.session_manager.get_activity(session_id).await;
    assert_eq!(activity.unwrap().permission_mode, Some(PermissionMode::Plan));
}

#[tokio::test]
async fn test_get_activity_returns_permission_mode() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // Set mode via hook
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("acceptEdits".to_string());
    payload.source = Some("startup".to_string());

    send_hook_event(&app, &payload).await;

    // get_activity should return the mode
    let activity = state.session_manager.get_activity(session_id).await;
    assert!(activity.is_some());
    let activity = activity.unwrap();
    assert_eq!(activity.permission_mode, Some(PermissionMode::AcceptEdits));
}

// =============================================================================
// Mode Cycling Tests (simulates Shift+Tab quick action)
// =============================================================================

#[tokio::test]
async fn test_mode_cycling_default_to_plan() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    // Start with default
    let mut payload = create_hook_payload("SessionStart", session_id);
    payload.permission_mode = Some("default".to_string());
    payload.source = Some("startup".to_string());
    send_hook_event(&app, &payload).await;

    // Cycle to plan
    let mut payload2 = create_hook_payload("UserPromptSubmit", session_id);
    payload2.permission_mode = Some("plan".to_string());
    payload2.prompt = Some("test".to_string());
    send_hook_event(&app, &payload2).await;

    let activity = state.session_manager.get_activity(session_id).await;
    assert_eq!(activity.unwrap().permission_mode, Some(PermissionMode::Plan));
}

#[tokio::test]
async fn test_mode_cycling_through_all_modes() {
    let (app, state, temp) = create_test_app().await;
    let session_id = create_test_session(&state, &temp).await;

    let modes = [
        ("default", PermissionMode::Default),
        ("plan", PermissionMode::Plan),
        ("acceptEdits", PermissionMode::AcceptEdits),
        ("bypassPermissions", PermissionMode::BypassPermissions),
    ];

    for (raw_mode, expected_mode) in modes {
        let mut payload = create_hook_payload("UserPromptSubmit", session_id);
        payload.permission_mode = Some(raw_mode.to_string());
        payload.prompt = Some("test".to_string());

        let status = send_hook_event(&app, &payload).await;
        assert_eq!(status, StatusCode::OK);

        let activity = state.session_manager.get_activity(session_id).await;
        assert_eq!(
            activity.unwrap().permission_mode,
            Some(expected_mode),
            "Mode should be {:?} after setting '{}'",
            expected_mode,
            raw_mode
        );
    }
}
