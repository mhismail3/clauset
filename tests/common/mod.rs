//! Common test utilities for integration tests.

use clauset_types::HookEventPayload;
use std::path::PathBuf;
use uuid::Uuid;

/// Load a hook event fixture from the fixtures directory.
pub fn load_hook_fixture(name: &str) -> HookEventPayload {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("hook_events")
        .join(format!("{}.json", name));

    let content = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", fixture_path.display(), e));

    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse fixture {}: {}", fixture_path.display(), e))
}

/// Load a hook fixture and override the clauset_session_id.
pub fn load_hook_fixture_with_session(name: &str, session_id: Uuid) -> HookEventPayload {
    let mut payload = load_hook_fixture(name);
    payload.clauset_session_id = session_id;
    payload
}

/// Generate a new test session UUID.
pub fn test_session_id() -> Uuid {
    Uuid::new_v4()
}

/// Assert that a hook event payload has the expected event type.
pub fn assert_hook_event_type(payload: &HookEventPayload, expected: &str) {
    assert_eq!(
        payload.hook_event_name, expected,
        "Expected hook event type '{}', got '{}'",
        expected, payload.hook_event_name
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_session_start_fixture() {
        let payload = load_hook_fixture("session_start");
        assert_eq!(payload.hook_event_name, "SessionStart");
    }
}
