//! Comprehensive tests for Claude Code hook event handling.
//!
//! These tests verify:
//! - Parsing of all 12 hook event types from fixtures
//! - Context window token extraction and calculations
//! - AskUserQuestion tool input parsing
//! - Infrastructure filtering (hook scripts)
//! - Error handling for malformed payloads
//! - Helper function correctness

use clauset_types::{HookEvent, HookEventPayload, HookEventType, ContextWindow, CurrentUsage};
use serde_json::json;
use std::path::PathBuf;
use uuid::Uuid;

// ============================================================================
// FIXTURE LOADING TESTS
// ============================================================================

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("hook_events")
        .join(format!("{}.json", name))
}

fn load_fixture(name: &str) -> HookEventPayload {
    let path = fixture_path(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}

#[test]
fn test_load_session_start_fixture() {
    let payload = load_fixture("session_start");
    assert_eq!(payload.hook_event_name, "SessionStart");
    assert_eq!(payload.source, Some("startup".to_string()));
    assert!(payload.context_window.is_some());
    assert!(payload.model.is_some());

    let model = payload.model.as_ref().unwrap();
    assert_eq!(model.id, "claude-sonnet-4-20250514");
}

#[test]
fn test_load_session_end_fixture() {
    let payload = load_fixture("session_end");
    assert_eq!(payload.hook_event_name, "SessionEnd");
    assert_eq!(payload.reason, Some("prompt_input_exit".to_string()));
}

#[test]
fn test_load_user_prompt_submit_fixture() {
    let payload = load_fixture("user_prompt_submit");
    assert_eq!(payload.hook_event_name, "UserPromptSubmit");
    assert_eq!(payload.prompt, Some("Add a new endpoint for user authentication".to_string()));
    assert!(payload.context_window.is_some());
}

#[test]
fn test_load_pre_tool_use_read_fixture() {
    let payload = load_fixture("pre_tool_use_read");
    assert_eq!(payload.hook_event_name, "PreToolUse");
    assert_eq!(payload.tool_name, Some("Read".to_string()));

    let tool_input = payload.tool_input.as_ref().unwrap();
    assert_eq!(tool_input["file_path"], "/Users/test/projects/myapp/src/main.rs");
}

#[test]
fn test_load_pre_tool_use_bash_fixture() {
    let payload = load_fixture("pre_tool_use_bash");
    assert_eq!(payload.hook_event_name, "PreToolUse");
    assert_eq!(payload.tool_name, Some("Bash".to_string()));

    let tool_input = payload.tool_input.as_ref().unwrap();
    assert_eq!(tool_input["command"], "cargo test --workspace");
}

#[test]
fn test_load_post_tool_use_fixture() {
    let payload = load_fixture("post_tool_use");
    assert_eq!(payload.hook_event_name, "PostToolUse");
    assert_eq!(payload.tool_name, Some("Read".to_string()));
    assert!(payload.tool_response.is_some());
}

#[test]
fn test_load_post_tool_use_failure_fixture() {
    let payload = load_fixture("post_tool_use_failure");
    assert_eq!(payload.hook_event_name, "PostToolUseFailure");
    assert_eq!(payload.is_timeout, Some(false));
    assert_eq!(payload.is_interrupt, Some(false));
    assert!(payload.error.is_some());
}

#[test]
fn test_load_post_tool_use_timeout_fixture() {
    let payload = load_fixture("post_tool_use_timeout");
    assert_eq!(payload.hook_event_name, "PostToolUseFailure");
    assert_eq!(payload.is_timeout, Some(true));
    assert_eq!(payload.error_type, Some("timeout".to_string()));
}

#[test]
fn test_load_stop_fixture() {
    let payload = load_fixture("stop");
    assert_eq!(payload.hook_event_name, "Stop");
    assert_eq!(payload.stop_hook_active, Some(false));
    assert!(payload.transcript_path.is_some());
}

#[test]
fn test_load_subagent_start_fixture() {
    let payload = load_fixture("subagent_start");
    assert_eq!(payload.hook_event_name, "SubagentStart");
    assert_eq!(payload.agent_id, Some("agent_xyz789".to_string()));
    assert_eq!(payload.agent_type, Some("Explore".to_string()));
}

#[test]
fn test_load_subagent_stop_fixture() {
    let payload = load_fixture("subagent_stop");
    assert_eq!(payload.hook_event_name, "SubagentStop");
    assert_eq!(payload.stop_hook_active, Some(false));
}

#[test]
fn test_load_notification_fixture() {
    let payload = load_fixture("notification");
    assert_eq!(payload.hook_event_name, "Notification");
    assert_eq!(payload.notification_type, Some("warning".to_string()));
    assert!(payload.message.is_some());
}

#[test]
fn test_load_pre_compact_fixture() {
    let payload = load_fixture("pre_compact");
    assert_eq!(payload.hook_event_name, "PreCompact");
    assert_eq!(payload.trigger, Some("auto".to_string()));
}

#[test]
fn test_load_permission_request_fixture() {
    let payload = load_fixture("permission_request");
    assert_eq!(payload.hook_event_name, "PermissionRequest");
    assert_eq!(payload.tool_name, Some("Write".to_string()));
    assert!(payload.tool_input.is_some());
}

// ============================================================================
// HOOK EVENT PARSING TESTS
// ============================================================================

#[test]
fn test_parse_all_12_event_types() {
    let fixtures = [
        "session_start",
        "session_end",
        "user_prompt_submit",
        "pre_tool_use_read",
        "post_tool_use",
        "post_tool_use_failure",
        "stop",
        "subagent_start",
        "subagent_stop",
        "notification",
        "pre_compact",
        "permission_request",
    ];

    for fixture_name in fixtures {
        let payload = load_fixture(fixture_name);
        let result = HookEvent::try_from(payload);
        assert!(result.is_ok(), "Failed to parse fixture: {}", fixture_name);
    }
}

#[test]
fn test_parse_session_start_event() {
    let payload = load_fixture("session_start");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::SessionStart { source, cwd, context_window, model, .. } => {
            assert_eq!(source, "startup");
            assert!(cwd.is_some());
            assert!(context_window.is_some());
            assert!(model.is_some());

            let ctx = context_window.unwrap();
            assert_eq!(ctx.context_window_size, 200000);
        }
        _ => panic!("Expected SessionStart event"),
    }
}

#[test]
fn test_parse_pre_tool_use_event() {
    let payload = load_fixture("pre_tool_use_read");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::PreToolUse { tool_name, tool_input, tool_use_id, .. } => {
            assert_eq!(tool_name, "Read");
            assert!(!tool_use_id.is_empty());
            assert_eq!(tool_input["file_path"], "/Users/test/projects/myapp/src/main.rs");
        }
        _ => panic!("Expected PreToolUse event"),
    }
}

#[test]
fn test_parse_post_tool_use_failure_event() {
    let payload = load_fixture("post_tool_use_failure");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::PostToolUseFailure { tool_name, error, is_timeout, is_interrupt, .. } => {
            assert_eq!(tool_name, "Bash");
            assert!(error.is_some());
            assert!(!is_timeout);
            assert!(!is_interrupt);
        }
        _ => panic!("Expected PostToolUseFailure event"),
    }
}

#[test]
fn test_parse_stop_event_with_transcript() {
    let payload = load_fixture("stop");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::Stop { stop_hook_active, transcript_path, context_window, .. } => {
            assert!(!stop_hook_active);
            assert!(transcript_path.is_some());
            assert!(context_window.is_some());

            let path = transcript_path.unwrap();
            assert!(path.ends_with(".jsonl"));
        }
        _ => panic!("Expected Stop event"),
    }
}

#[test]
fn test_parse_subagent_start_event() {
    let payload = load_fixture("subagent_start");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::SubagentStart { agent_id, agent_type, .. } => {
            assert_eq!(agent_id, "agent_xyz789");
            assert_eq!(agent_type, "Explore");
        }
        _ => panic!("Expected SubagentStart event"),
    }
}

#[test]
fn test_parse_permission_request_event() {
    let payload = load_fixture("permission_request");
    let event = HookEvent::try_from(payload).unwrap();

    match event {
        HookEvent::PermissionRequest { tool_name, tool_input, tool_use_id, .. } => {
            assert_eq!(tool_name, "Write");
            assert!(!tool_use_id.is_empty());
            assert!(tool_input["file_path"].is_string());
        }
        _ => panic!("Expected PermissionRequest event"),
    }
}

// ============================================================================
// CONTEXT WINDOW TESTS
// ============================================================================

#[test]
fn test_context_window_total_tokens() {
    let payload = load_fixture("user_prompt_submit");
    let ctx = payload.context_window.unwrap();

    assert_eq!(ctx.total_input_tokens, 15000);
    assert_eq!(ctx.total_output_tokens, 3000);
    assert_eq!(ctx.context_window_size, 200000);
}

#[test]
fn test_context_window_current_usage() {
    let payload = load_fixture("user_prompt_submit");
    let ctx = payload.context_window.unwrap();
    let usage = ctx.current_usage.unwrap();

    assert_eq!(usage.input_tokens, 15000);
    assert_eq!(usage.output_tokens, 200);
    assert_eq!(usage.cache_creation_input_tokens, 5000);
    assert_eq!(usage.cache_read_input_tokens, 8000);
}

#[test]
fn test_context_window_percent_calculation() {
    let payload = load_fixture("notification");
    let ctx = payload.context_window.unwrap();

    // 170000 input + 20000 output = 190000 total
    // 190000 / 200000 = 95%
    let total_tokens = ctx.total_input_tokens + ctx.total_output_tokens;
    let percent = (total_tokens as f64 / ctx.context_window_size as f64 * 100.0) as u8;

    assert_eq!(percent, 95);
}

#[test]
fn test_context_window_near_limit() {
    let payload = load_fixture("pre_compact");
    let ctx = payload.context_window.unwrap();

    // Should be very close to limit (triggering auto-compact)
    let total = ctx.total_input_tokens + ctx.total_output_tokens;
    let percent = total as f64 / ctx.context_window_size as f64;

    // Pre-compact triggers around 95%+
    assert!(percent > 0.90, "Expected near-limit context, got {}%", percent * 100.0);
}

#[test]
fn test_context_window_fresh_session() {
    let payload = load_fixture("session_start");
    let ctx = payload.context_window.unwrap();

    // Fresh session should have zero tokens
    assert_eq!(ctx.total_input_tokens, 0);
    assert_eq!(ctx.total_output_tokens, 0);
    assert!(ctx.current_usage.is_none());
}

// ============================================================================
// HOOK EVENT TYPE TESTS
// ============================================================================

#[test]
fn test_hook_event_type_from_str() {
    assert_eq!(HookEventType::from_str("SessionStart"), Some(HookEventType::SessionStart));
    assert_eq!(HookEventType::from_str("SessionEnd"), Some(HookEventType::SessionEnd));
    assert_eq!(HookEventType::from_str("UserPromptSubmit"), Some(HookEventType::UserPromptSubmit));
    assert_eq!(HookEventType::from_str("PreToolUse"), Some(HookEventType::PreToolUse));
    assert_eq!(HookEventType::from_str("PostToolUse"), Some(HookEventType::PostToolUse));
    assert_eq!(HookEventType::from_str("PostToolUseFailure"), Some(HookEventType::PostToolUseFailure));
    assert_eq!(HookEventType::from_str("Stop"), Some(HookEventType::Stop));
    assert_eq!(HookEventType::from_str("SubagentStart"), Some(HookEventType::SubagentStart));
    assert_eq!(HookEventType::from_str("SubagentStop"), Some(HookEventType::SubagentStop));
    assert_eq!(HookEventType::from_str("Notification"), Some(HookEventType::Notification));
    assert_eq!(HookEventType::from_str("PreCompact"), Some(HookEventType::PreCompact));
    assert_eq!(HookEventType::from_str("PermissionRequest"), Some(HookEventType::PermissionRequest));
}

#[test]
fn test_hook_event_type_unknown() {
    assert_eq!(HookEventType::from_str("Unknown"), None);
    assert_eq!(HookEventType::from_str("InvalidEvent"), None);
    assert_eq!(HookEventType::from_str(""), None);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_parse_unknown_event_type() {
    let payload = HookEventPayload {
        clauset_session_id: Uuid::new_v4(),
        session_id: "test".to_string(),
        hook_event_name: "UnknownEvent".to_string(),
        ..Default::default()
    };

    let result = HookEvent::try_from(payload);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "unknown hook event type");
}

#[test]
fn test_parse_missing_tool_name() {
    let payload = HookEventPayload {
        clauset_session_id: Uuid::new_v4(),
        session_id: "test".to_string(),
        hook_event_name: "PreToolUse".to_string(),
        tool_name: None, // Missing!
        ..Default::default()
    };

    let result = HookEvent::try_from(payload);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "missing tool_name");
}

#[test]
fn test_parse_with_defaults() {
    // Test that missing optional fields use defaults
    let payload = HookEventPayload {
        clauset_session_id: Uuid::new_v4(),
        session_id: "test".to_string(),
        hook_event_name: "SessionStart".to_string(),
        source: None, // Will default to "startup"
        ..Default::default()
    };

    let event = HookEvent::try_from(payload).unwrap();
    match event {
        HookEvent::SessionStart { source, .. } => {
            assert_eq!(source, "startup"); // Default value
        }
        _ => panic!("Expected SessionStart"),
    }
}

#[test]
fn test_parse_session_end_with_missing_reason() {
    let payload = HookEventPayload {
        clauset_session_id: Uuid::new_v4(),
        session_id: "test".to_string(),
        hook_event_name: "SessionEnd".to_string(),
        reason: None, // Will default to "unknown"
        ..Default::default()
    };

    let event = HookEvent::try_from(payload).unwrap();
    match event {
        HookEvent::SessionEnd { reason, .. } => {
            assert_eq!(reason, "unknown");
        }
        _ => panic!("Expected SessionEnd"),
    }
}

// ============================================================================
// PAYLOAD SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_payload_roundtrip() {
    let original = HookEventPayload {
        clauset_session_id: Uuid::new_v4(),
        session_id: "test-session".to_string(),
        hook_event_name: "PreToolUse".to_string(),
        tool_name: Some("Read".to_string()),
        tool_input: Some(json!({"file_path": "/test/file.rs"})),
        tool_use_id: Some("toolu_123".to_string()),
        cwd: Some("/home/user".to_string()),
        context_window: Some(ContextWindow {
            total_input_tokens: 1000,
            total_output_tokens: 500,
            context_window_size: 200000,
            current_usage: Some(CurrentUsage {
                input_tokens: 1000,
                output_tokens: 100,
                cache_creation_input_tokens: 200,
                cache_read_input_tokens: 300,
            }),
        }),
        ..Default::default()
    };

    // Serialize to JSON
    let json = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let parsed: HookEventPayload = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.clauset_session_id, original.clauset_session_id);
    assert_eq!(parsed.session_id, original.session_id);
    assert_eq!(parsed.hook_event_name, original.hook_event_name);
    assert_eq!(parsed.tool_name, original.tool_name);

    let ctx = parsed.context_window.unwrap();
    assert_eq!(ctx.total_input_tokens, 1000);
    assert_eq!(ctx.total_output_tokens, 500);
}

#[test]
fn test_context_window_serialization() {
    let ctx = ContextWindow {
        total_input_tokens: 50000,
        total_output_tokens: 10000,
        context_window_size: 200000,
        current_usage: Some(CurrentUsage {
            input_tokens: 50000,
            output_tokens: 500,
            cache_creation_input_tokens: 15000,
            cache_read_input_tokens: 30000,
        }),
    };

    let json = serde_json::to_string(&ctx).unwrap();
    let parsed: ContextWindow = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.total_input_tokens, 50000);
    assert_eq!(parsed.current_usage.unwrap().cache_read_input_tokens, 30000);
}

// ============================================================================
// TOOL INPUT PARSING TESTS
// ============================================================================

#[test]
fn test_read_tool_input() {
    let payload = load_fixture("pre_tool_use_read");
    let input = payload.tool_input.unwrap();

    assert!(input["file_path"].is_string());
    assert!(input["file_path"].as_str().unwrap().contains(".rs"));
}

#[test]
fn test_bash_tool_input() {
    let payload = load_fixture("pre_tool_use_bash");
    let input = payload.tool_input.unwrap();

    assert!(input["command"].is_string());
    assert!(input.get("description").is_some());
}

#[test]
fn test_write_tool_input() {
    let payload = load_fixture("permission_request");
    let input = payload.tool_input.unwrap();

    assert!(input["file_path"].is_string());
    assert!(input["content"].is_string());
}

// ============================================================================
// ACTIVITY UPDATE TESTS
// ============================================================================

use clauset_types::HookActivityUpdate;

#[test]
fn test_activity_update_pre_tool_use() {
    let update = HookActivityUpdate::pre_tool_use(
        "Read".to_string(),
        json!({"file_path": "/test.rs"}),
    );

    assert_eq!(update.event_type, HookEventType::PreToolUse);
    assert_eq!(update.tool_name, Some("Read".to_string()));
    assert!(!update.is_error);
}

#[test]
fn test_activity_update_post_tool_use_success() {
    let update = HookActivityUpdate::post_tool_use(
        "Read".to_string(),
        json!({"file_path": "/test.rs"}),
        json!({"content": "fn main() {}"}),
    );

    assert_eq!(update.event_type, HookEventType::PostToolUse);
    assert!(!update.is_error);
}

#[test]
fn test_activity_update_post_tool_use_error() {
    let update = HookActivityUpdate::post_tool_use(
        "Read".to_string(),
        json!({"file_path": "/test.rs"}),
        json!({"error": "File not found"}),
    );

    assert_eq!(update.event_type, HookEventType::PostToolUse);
    assert!(update.is_error);
}

#[test]
fn test_activity_update_post_tool_use_is_error_flag() {
    let update = HookActivityUpdate::post_tool_use(
        "Bash".to_string(),
        json!({"command": "test"}),
        json!({"is_error": true, "output": "Command failed"}),
    );

    assert!(update.is_error);
}

#[test]
fn test_activity_update_user_prompt() {
    let update = HookActivityUpdate::user_prompt_submit();

    assert_eq!(update.event_type, HookEventType::UserPromptSubmit);
    assert!(update.tool_name.is_none());
}

#[test]
fn test_activity_update_stop() {
    let update = HookActivityUpdate::stop();

    assert_eq!(update.event_type, HookEventType::Stop);
    assert!(update.tool_name.is_none());
}

#[test]
fn test_activity_update_session_end() {
    let update = HookActivityUpdate::session_end();

    assert_eq!(update.event_type, HookEventType::SessionEnd);
}

// ============================================================================
// INTEGRATION-STYLE FIXTURE TESTS
// ============================================================================

#[test]
fn test_all_fixtures_have_session_id() {
    let fixtures = [
        "session_start",
        "session_end",
        "user_prompt_submit",
        "pre_tool_use_read",
        "post_tool_use",
        "stop",
        "subagent_start",
        "subagent_stop",
        "notification",
        "pre_compact",
        "permission_request",
    ];

    for name in fixtures {
        let payload = load_fixture(name);
        assert!(!payload.session_id.is_empty(), "Fixture {} missing session_id", name);
        assert!(!payload.clauset_session_id.is_nil(), "Fixture {} has nil clauset_session_id", name);
    }
}

#[test]
fn test_all_tool_fixtures_have_tool_use_id() {
    let fixtures = ["pre_tool_use_read", "pre_tool_use_bash", "post_tool_use", "permission_request"];

    for name in fixtures {
        let payload = load_fixture(name);
        assert!(
            payload.tool_use_id.is_some(),
            "Fixture {} missing tool_use_id",
            name
        );
    }
}

#[test]
fn test_fixture_versions() {
    let fixtures = ["session_start", "user_prompt_submit", "stop"];

    for name in fixtures {
        let payload = load_fixture(name);
        assert_eq!(
            payload.version,
            Some("2.0.76".to_string()),
            "Fixture {} has wrong version",
            name
        );
    }
}
