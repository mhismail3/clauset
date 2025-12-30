//! Types for Claude Code hook events.
//!
//! These types represent the structured data sent by Claude Code hooks
//! to the Clauset dashboard for real-time activity tracking.
//!
//! Based on reverse-engineering of Claude Code CLI v2.0.76 (cli.js).
//! The base hook input is created by the `aF()` function in cli.js.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Context window token usage information.
///
/// This comes directly from Claude Code's hook input and provides
/// accurate token counts (unlike regex parsing from terminal output).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ContextWindow {
    /// Total input tokens used in session (cumulative)
    #[serde(default, alias = "totalInputTokens")]
    pub total_input_tokens: u64,
    /// Total output tokens used in session (cumulative)
    #[serde(default, alias = "totalOutputTokens")]
    pub total_output_tokens: u64,
    /// Context window size for current model (e.g., 200000)
    #[serde(default, alias = "contextWindowSize")]
    pub context_window_size: u64,
    /// Token usage from last API call (null if no messages yet)
    #[serde(default, alias = "currentUsage")]
    pub current_usage: Option<CurrentUsage>,
}

/// Token usage from the last API call.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CurrentUsage {
    /// Input tokens for current context
    #[serde(default, alias = "inputTokens")]
    pub input_tokens: u64,
    /// Output tokens generated
    #[serde(default, alias = "outputTokens")]
    pub output_tokens: u64,
    /// Tokens written to cache
    #[serde(default, alias = "cacheCreationInputTokens")]
    pub cache_creation_input_tokens: u64,
    /// Tokens read from cache
    #[serde(default, alias = "cacheReadInputTokens")]
    pub cache_read_input_tokens: u64,
}

/// Model information from the hook input.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ModelInfo {
    /// Model ID (e.g., "claude-3-5-sonnet-20241022")
    #[serde(default, alias = "modelId")]
    pub id: String,
    /// Display name (e.g., "Claude 3.5 Sonnet")
    #[serde(default, alias = "displayName")]
    pub display_name: String,
}

/// Workspace information from the hook input.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WorkspaceInfo {
    /// Current working directory path
    #[serde(default)]
    pub current_dir: String,
    /// Project root directory path
    #[serde(default)]
    pub project_dir: String,
}

/// Output style information.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OutputStyle {
    /// Style name (e.g., "default", "Explanatory", "Learning")
    #[serde(default)]
    pub name: String,
}

/// Payload received from Claude Code hooks via HTTP POST.
///
/// The hook script adds `clauset_session_id` to the original Claude payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookEventPayload {
    /// Clauset's internal session UUID (added by hook script)
    pub clauset_session_id: Uuid,

    /// Claude's session ID (from the hook event)
    pub session_id: String,

    /// The type of hook event
    #[serde(alias = "hookEventName")]
    pub hook_event_name: String,

    /// Current working directory
    #[serde(default)]
    pub cwd: Option<String>,

    /// Path to the transcript file
    #[serde(default, alias = "transcriptPath")]
    pub transcript_path: Option<String>,

    /// Permission mode (default, plan, acceptEdits, bypassPermissions)
    #[serde(default, alias = "permissionMode")]
    pub permission_mode: Option<String>,

    // Tool-related fields (PreToolUse, PostToolUse)

    /// Name of the tool being used (Read, Write, Bash, etc.)
    #[serde(default)]
    pub tool_name: Option<String>,

    /// Tool input parameters (file_path, command, etc.)
    #[serde(default)]
    pub tool_input: Option<Value>,

    /// Tool response/output (PostToolUse only)
    #[serde(default)]
    pub tool_response: Option<Value>,

    /// Unique identifier for this tool use
    #[serde(default)]
    pub tool_use_id: Option<String>,

    // UserPromptSubmit fields

    /// The user's prompt text
    #[serde(default)]
    pub prompt: Option<String>,

    // SessionStart fields

    /// Session start source (startup, resume, clear, compact)
    #[serde(default)]
    pub source: Option<String>,

    // SessionEnd fields

    /// Session end reason (clear, logout, prompt_input_exit, other)
    #[serde(default)]
    pub reason: Option<String>,

    // Stop/SubagentStop fields

    /// Whether the stop hook is continuing (for chained hooks)
    #[serde(default)]
    pub stop_hook_active: Option<bool>,

    // Notification fields

    /// Notification message content
    #[serde(default)]
    pub message: Option<String>,

    /// Type of notification
    #[serde(default)]
    pub notification_type: Option<String>,

    // NEW: Context and metadata from cli.js aF() function

    /// Context window token usage (accurate source for token counts)
    #[serde(default, alias = "contextWindow")]
    pub context_window: Option<ContextWindow>,

    /// Model information
    #[serde(default)]
    pub model: Option<ModelInfo>,

    /// Workspace information
    #[serde(default)]
    pub workspace: Option<WorkspaceInfo>,

    /// Output style
    #[serde(default, alias = "outputStyle")]
    pub output_style: Option<OutputStyle>,

    /// Claude Code version (e.g., "2.0.76")
    #[serde(default)]
    pub version: Option<String>,

    // NEW: SubagentStart/SubagentStop fields

    /// Agent ID for Task tool subagents
    #[serde(default, alias = "agentId")]
    pub agent_id: Option<String>,

    /// Agent type (e.g., "Explore", "Plan", "general-purpose")
    #[serde(default, alias = "agentType")]
    pub agent_type: Option<String>,

    // NEW: PostToolUseFailure fields

    /// Error message when tool execution fails
    #[serde(default)]
    pub error: Option<String>,

    /// Error type classification
    #[serde(default)]
    pub error_type: Option<String>,

    /// Whether the tool timed out
    #[serde(default)]
    pub is_timeout: Option<bool>,

    /// Whether the tool was interrupted
    #[serde(default, alias = "isInterrupt")]
    pub is_interrupt: Option<bool>,

    // NEW: PreCompact fields

    /// Compaction trigger (manual, auto)
    #[serde(default)]
    pub trigger: Option<String>,
}

/// Enumeration of Claude Code hook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEventType {
    /// Session started (new or resumed)
    SessionStart,
    /// Session ended
    SessionEnd,
    /// User submitted a prompt
    UserPromptSubmit,
    /// Before a tool is executed
    PreToolUse,
    /// After a tool completes successfully
    PostToolUse,
    /// After a tool execution fails
    PostToolUseFailure,
    /// Claude finished responding
    Stop,
    /// Subagent (Task tool) started
    SubagentStart,
    /// Subagent (Task tool) finished
    SubagentStop,
    /// System notification
    Notification,
    /// Before context compaction
    PreCompact,
    /// Permission dialog shown
    PermissionRequest,
}

impl HookEventType {
    /// Parse event type from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "SessionStart" => Some(Self::SessionStart),
            "SessionEnd" => Some(Self::SessionEnd),
            "UserPromptSubmit" => Some(Self::UserPromptSubmit),
            "PreToolUse" => Some(Self::PreToolUse),
            "PostToolUse" => Some(Self::PostToolUse),
            "PostToolUseFailure" => Some(Self::PostToolUseFailure),
            "Stop" => Some(Self::Stop),
            "SubagentStart" => Some(Self::SubagentStart),
            "SubagentStop" => Some(Self::SubagentStop),
            "Notification" => Some(Self::Notification),
            "PreCompact" => Some(Self::PreCompact),
            "PermissionRequest" => Some(Self::PermissionRequest),
            _ => None,
        }
    }
}

/// Processed hook event for internal use.
///
/// This is a more structured representation after parsing the raw payload.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Session started
    SessionStart {
        session_id: Uuid,
        claude_session_id: String,
        source: String,
        cwd: Option<String>,
        context_window: Option<ContextWindow>,
        model: Option<ModelInfo>,
        /// Path to the JSONL transcript file (for real-time content streaming)
        transcript_path: Option<String>,
    },

    /// Session ended
    SessionEnd {
        session_id: Uuid,
        claude_session_id: String,
        reason: String,
    },

    /// User submitted a prompt
    UserPromptSubmit {
        session_id: Uuid,
        claude_session_id: String,
        prompt: String,
        cwd: Option<String>,
        context_window: Option<ContextWindow>,
    },

    /// Tool is about to execute
    PreToolUse {
        session_id: Uuid,
        claude_session_id: String,
        tool_name: String,
        tool_input: Value,
        tool_use_id: String,
        cwd: Option<String>,
        context_window: Option<ContextWindow>,
    },

    /// Tool finished executing successfully
    PostToolUse {
        session_id: Uuid,
        claude_session_id: String,
        tool_name: String,
        tool_input: Value,
        tool_response: Value,
        tool_use_id: String,
        context_window: Option<ContextWindow>,
    },

    /// Tool execution failed
    PostToolUseFailure {
        session_id: Uuid,
        claude_session_id: String,
        tool_name: String,
        tool_input: Value,
        tool_use_id: String,
        error: Option<String>,
        error_type: Option<String>,
        is_timeout: bool,
        is_interrupt: bool,
        context_window: Option<ContextWindow>,
    },

    /// Claude finished responding
    Stop {
        session_id: Uuid,
        claude_session_id: String,
        stop_hook_active: bool,
        transcript_path: Option<String>,
        context_window: Option<ContextWindow>,
    },

    /// Subagent (Task tool) started
    SubagentStart {
        session_id: Uuid,
        claude_session_id: String,
        agent_id: String,
        agent_type: String,
    },

    /// Subagent finished
    SubagentStop {
        session_id: Uuid,
        claude_session_id: String,
        stop_hook_active: bool,
    },

    /// System notification
    Notification {
        session_id: Uuid,
        claude_session_id: String,
        message: String,
        notification_type: String,
    },

    /// Context compaction starting
    PreCompact {
        session_id: Uuid,
        claude_session_id: String,
        trigger: String,
    },

    /// Permission dialog shown
    PermissionRequest {
        session_id: Uuid,
        claude_session_id: String,
        tool_name: String,
        tool_input: Value,
        tool_use_id: String,
    },
}

impl TryFrom<HookEventPayload> for HookEvent {
    type Error = &'static str;

    fn try_from(p: HookEventPayload) -> Result<Self, Self::Error> {
        let session_id = p.clauset_session_id;
        let claude_session_id = p.session_id.clone();

        match p.hook_event_name.as_str() {
            "SessionStart" => Ok(HookEvent::SessionStart {
                session_id,
                claude_session_id,
                source: p.source.unwrap_or_else(|| "startup".to_string()),
                cwd: p.cwd,
                context_window: p.context_window,
                model: p.model,
                transcript_path: p.transcript_path,
            }),

            "SessionEnd" => Ok(HookEvent::SessionEnd {
                session_id,
                claude_session_id,
                reason: p.reason.unwrap_or_else(|| "unknown".to_string()),
            }),

            "UserPromptSubmit" => Ok(HookEvent::UserPromptSubmit {
                session_id,
                claude_session_id,
                prompt: p.prompt.unwrap_or_default(),
                cwd: p.cwd,
                context_window: p.context_window,
            }),

            "PreToolUse" => Ok(HookEvent::PreToolUse {
                session_id,
                claude_session_id,
                tool_name: p.tool_name.ok_or("missing tool_name")?,
                tool_input: p.tool_input.unwrap_or(Value::Null),
                tool_use_id: p.tool_use_id.unwrap_or_default(),
                cwd: p.cwd,
                context_window: p.context_window,
            }),

            "PostToolUse" => Ok(HookEvent::PostToolUse {
                session_id,
                claude_session_id,
                tool_name: p.tool_name.ok_or("missing tool_name")?,
                tool_input: p.tool_input.unwrap_or(Value::Null),
                tool_response: p.tool_response.unwrap_or(Value::Null),
                tool_use_id: p.tool_use_id.unwrap_or_default(),
                context_window: p.context_window,
            }),

            "PostToolUseFailure" => Ok(HookEvent::PostToolUseFailure {
                session_id,
                claude_session_id,
                tool_name: p.tool_name.ok_or("missing tool_name")?,
                tool_input: p.tool_input.unwrap_or(Value::Null),
                tool_use_id: p.tool_use_id.unwrap_or_default(),
                error: p.error,
                error_type: p.error_type,
                is_timeout: p.is_timeout.unwrap_or(false),
                is_interrupt: p.is_interrupt.unwrap_or(false),
                context_window: p.context_window,
            }),

            "Stop" => Ok(HookEvent::Stop {
                session_id,
                claude_session_id,
                stop_hook_active: p.stop_hook_active.unwrap_or(false),
                transcript_path: p.transcript_path,
                context_window: p.context_window,
            }),

            "SubagentStart" => Ok(HookEvent::SubagentStart {
                session_id,
                claude_session_id,
                agent_id: p.agent_id.unwrap_or_default(),
                agent_type: p.agent_type.unwrap_or_else(|| "unknown".to_string()),
            }),

            "SubagentStop" => Ok(HookEvent::SubagentStop {
                session_id,
                claude_session_id,
                stop_hook_active: p.stop_hook_active.unwrap_or(false),
            }),

            "Notification" => Ok(HookEvent::Notification {
                session_id,
                claude_session_id,
                message: p.message.unwrap_or_default(),
                notification_type: p.notification_type.unwrap_or_default(),
            }),

            "PreCompact" => Ok(HookEvent::PreCompact {
                session_id,
                claude_session_id,
                trigger: p.trigger.unwrap_or_else(|| "unknown".to_string()),
            }),

            "PermissionRequest" => Ok(HookEvent::PermissionRequest {
                session_id,
                claude_session_id,
                tool_name: p.tool_name.ok_or("missing tool_name")?,
                tool_input: p.tool_input.unwrap_or(Value::Null),
                tool_use_id: p.tool_use_id.unwrap_or_default(),
            }),

            _ => Err("unknown hook event type"),
        }
    }
}

/// Activity update derived from a hook event.
///
/// This is what gets passed to the session buffer for updating activity state.
#[derive(Debug, Clone)]
pub struct HookActivityUpdate {
    /// The type of event
    pub event_type: HookEventType,
    /// Tool name (for PreToolUse/PostToolUse)
    pub tool_name: Option<String>,
    /// Tool input (for PreToolUse/PostToolUse)
    pub tool_input: Option<Value>,
    /// Tool response (for PostToolUse)
    pub tool_response: Option<Value>,
    /// Whether this is an error (from tool_response)
    pub is_error: bool,
}

impl HookActivityUpdate {
    /// Create an update from a PreToolUse event.
    pub fn pre_tool_use(tool_name: String, tool_input: Value) -> Self {
        Self {
            event_type: HookEventType::PreToolUse,
            tool_name: Some(tool_name),
            tool_input: Some(tool_input),
            tool_response: None,
            is_error: false,
        }
    }

    /// Create an update from a PostToolUse event.
    pub fn post_tool_use(tool_name: String, tool_input: Value, tool_response: Value) -> Self {
        // Check if response indicates an error
        let is_error = tool_response.get("error").is_some()
            || tool_response
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        Self {
            event_type: HookEventType::PostToolUse,
            tool_name: Some(tool_name),
            tool_input: Some(tool_input),
            tool_response: Some(tool_response),
            is_error,
        }
    }

    /// Create an update for UserPromptSubmit (user sent input, Claude thinking).
    pub fn user_prompt_submit() -> Self {
        Self {
            event_type: HookEventType::UserPromptSubmit,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            is_error: false,
        }
    }

    /// Create an update for Stop (Claude finished responding).
    pub fn stop() -> Self {
        Self {
            event_type: HookEventType::Stop,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            is_error: false,
        }
    }

    /// Create an update for SessionEnd.
    pub fn session_end() -> Self {
        Self {
            event_type: HookEventType::SessionEnd,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            is_error: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pre_tool_use() {
        let payload = HookEventPayload {
            clauset_session_id: Uuid::new_v4(),
            session_id: "test-session".to_string(),
            hook_event_name: "PreToolUse".to_string(),
            tool_name: Some("Read".to_string()),
            tool_input: Some(serde_json::json!({"file_path": "/test/file.rs"})),
            tool_use_id: Some("toolu_123".to_string()),
            cwd: Some("/home/user/project".to_string()),
            ..Default::default()
        };

        let event = HookEvent::try_from(payload).unwrap();
        match event {
            HookEvent::PreToolUse { tool_name, cwd, .. } => {
                assert_eq!(tool_name, "Read");
                assert_eq!(cwd, Some("/home/user/project".to_string()));
            }
            _ => panic!("Expected PreToolUse event"),
        }
    }

    #[test]
    fn test_parse_stop() {
        let payload = HookEventPayload {
            clauset_session_id: Uuid::new_v4(),
            session_id: "test-session".to_string(),
            hook_event_name: "Stop".to_string(),
            stop_hook_active: Some(false),
            ..Default::default()
        };

        let event = HookEvent::try_from(payload).unwrap();
        match event {
            HookEvent::Stop {
                stop_hook_active, ..
            } => {
                assert!(!stop_hook_active);
            }
            _ => panic!("Expected Stop event"),
        }
    }

    #[test]
    fn test_parse_camel_case_payload() {
        let json = r#"
        {
          "clauset_session_id": "550e8400-e29b-41d4-a716-446655440000",
          "session_id": "abc123def456",
          "hookEventName": "UserPromptSubmit",
          "permissionMode": "acceptEdits",
          "contextWindow": {
            "totalInputTokens": 12000,
            "totalOutputTokens": 3400,
            "contextWindowSize": 200000,
            "currentUsage": {
              "inputTokens": 12000,
              "outputTokens": 200,
              "cacheCreationInputTokens": 5000,
              "cacheReadInputTokens": 8000
            }
          },
          "model": {
            "modelId": "claude-sonnet-4-5-20250929",
            "displayName": "Claude Sonnet 4.5"
          }
        }"#;

        let payload: HookEventPayload = serde_json::from_str(json).expect("payload parse");
        assert_eq!(payload.hook_event_name, "UserPromptSubmit");
        assert_eq!(payload.permission_mode.as_deref(), Some("acceptEdits"));

        let ctx = payload.context_window.expect("context_window");
        assert_eq!(ctx.total_input_tokens, 12000);
        assert_eq!(ctx.total_output_tokens, 3400);
        assert_eq!(ctx.context_window_size, 200000);
        let usage = ctx.current_usage.expect("current_usage");
        assert_eq!(usage.input_tokens, 12000);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_creation_input_tokens, 5000);
        assert_eq!(usage.cache_read_input_tokens, 8000);

        let model = payload.model.expect("model");
        assert_eq!(model.id, "claude-sonnet-4-5-20250929");
        assert_eq!(model.display_name, "Claude Sonnet 4.5");
    }
}

impl Default for HookEventPayload {
    fn default() -> Self {
        Self {
            clauset_session_id: Uuid::nil(),
            session_id: String::new(),
            hook_event_name: String::new(),
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
            // NEW fields from cli.js aF() function
            context_window: None,
            model: None,
            workspace: None,
            output_style: None,
            version: None,
            // SubagentStart/Stop fields
            agent_id: None,
            agent_type: None,
            // PostToolUseFailure fields
            error: None,
            error_type: None,
            is_timeout: None,
            is_interrupt: None,
            // PreCompact fields
            trigger: None,
        }
    }
}
