//! WebSocket message protocol between client and server.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

use crate::{ResultUsage, SessionStatus};

/// Messages sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    /// Send text input to Claude.
    Input { content: String },
    /// Send raw terminal input (PTY mode).
    TerminalInput { data: Vec<u8> },
    /// Resize terminal and request buffer.
    /// Server will resize tmux first, then send the terminal buffer.
    Resize { rows: u16, cols: u16 },
    /// Ping for keepalive.
    Ping { timestamp: u64 },
    /// Request current session state.
    GetState,
    /// Request terminal buffer (after resize).
    /// DEPRECATED: Use SyncRequest instead for reliable streaming.
    RequestBuffer,
    /// Update session stats from parsed status line.
    StatusUpdate {
        model: String,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
    },

    // === Reliable Streaming Protocol Messages ===

    /// Request synchronization on connect/reconnect.
    /// Client sends this after connection to sync state and request missed data.
    SyncRequest {
        /// Last sequence number the client received (0 if fresh connection)
        last_seq: u64,
        /// Current terminal dimensions
        cols: u16,
        rows: u16,
    },
    /// Acknowledge receipt of terminal chunks.
    /// Client sends this to confirm it has received all data up to ack_seq.
    Ack {
        /// Highest contiguous sequence number received
        ack_seq: u64,
    },
    /// Request a specific range of chunks (for gap recovery).
    /// Client sends this when it detects missing sequence numbers.
    RangeRequest {
        /// First sequence number needed (inclusive)
        start_seq: u64,
        /// Last sequence number needed (inclusive)
        end_seq: u64,
    },

    // === Dimension Negotiation Protocol ===

    /// Negotiate terminal dimensions with server validation.
    /// Client sends this after calculating dimensions, before requesting buffer.
    NegotiateDimensions {
        /// Requested column count
        cols: u16,
        /// Requested row count
        rows: u16,
        /// Client's confidence in these dimensions
        confidence: String, // "high", "medium", "low"
        /// How dimensions were calculated
        source: String, // "fitaddon", "container", "estimation", "defaults"
        /// Character cell width (if known)
        cell_width: Option<f64>,
        /// Whether the font was loaded successfully
        font_loaded: bool,
        /// Device type hint
        device_hint: String, // "iphone", "ipad", "desktop"
    },

    // === Chat History Protocol ===

    /// Request chat history for the session.
    /// Client sends this on connect to load persisted chat messages.
    RequestChatHistory,

    // === Interactive Prompt Protocol ===

    /// User selected option(s) from an interactive question.
    /// For single-select: one index. For multi-select: multiple indices.
    InteractiveChoice {
        /// ID of the question being answered
        question_id: String,
        /// Selected option indices (1-based)
        selected_indices: Vec<usize>,
    },
    /// User provided text input for a text prompt.
    InteractiveText {
        /// The text response
        response: String,
    },
    /// User cancelled the interactive prompt.
    InteractiveCancel,
}

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    /// Session initialization info.
    SessionInit {
        session_id: Uuid,
        claude_session_id: Uuid,
        model: String,
        tools: Vec<String>,
        cwd: PathBuf,
    },
    /// Streaming text from Claude.
    Text {
        message_id: String,
        content: String,
        is_complete: bool,
    },
    /// Claude is using a tool.
    ToolUse {
        message_id: String,
        tool_use_id: String,
        tool_name: String,
        input: Value,
    },
    /// Tool execution result.
    ToolResult {
        tool_use_id: String,
        output: String,
        is_error: bool,
    },
    /// Message completed.
    MessageComplete { message_id: String },
    /// Claude is asking for input.
    InputRequired { prompt: String },
    /// Session result/summary.
    Result {
        success: bool,
        duration_ms: u64,
        total_cost_usd: f64,
        usage: Option<ResultUsage>,
    },
    /// Raw terminal output (PTY mode).
    /// DEPRECATED: Use TerminalChunk instead for reliable streaming.
    TerminalOutput { data: Vec<u8> },
    /// Terminal buffer for replay on reconnect.
    /// DEPRECATED: Use SyncResponse instead for reliable streaming.
    TerminalBuffer { data: Vec<u8> },

    // === Reliable Streaming Protocol Messages ===

    /// Sequenced terminal output chunk.
    /// Each chunk has a monotonically increasing sequence number for ordering and gap detection.
    TerminalChunk {
        /// Monotonically increasing sequence number (per session)
        seq: u64,
        /// Terminal data (raw bytes including ANSI codes)
        data: Vec<u8>,
        /// Timestamp when chunk was captured (ms since Unix epoch)
        timestamp: u64,
    },
    /// Response to client's SyncRequest on connect/reconnect.
    /// Tells client the server's buffer state and optionally includes full buffer.
    SyncResponse {
        /// Sequence number of oldest available chunk in server buffer
        buffer_start_seq: u64,
        /// Sequence number of most recent chunk
        buffer_end_seq: u64,
        /// Current terminal dimensions (confirmed after resize)
        cols: u16,
        rows: u16,
        /// If client is too far behind or fresh connect, contains full buffer data
        full_buffer: Option<Vec<u8>>,
        /// Starting sequence number of full_buffer (if provided)
        full_buffer_start_seq: Option<u64>,
    },
    /// Batch of chunks sent in response to RangeRequest (gap recovery).
    ChunkBatch {
        /// Starting sequence number of this batch
        start_seq: u64,
        /// Concatenated chunk data
        data: Vec<u8>,
        /// Number of chunks in this batch
        chunk_count: u32,
        /// True if this is the last batch for the RangeRequest
        is_complete: bool,
    },
    /// Notification that server buffer has overflowed.
    /// Client should request full resync if their state is too far behind.
    BufferOverflow {
        /// New oldest available sequence number
        new_start_seq: u64,
        /// True if client needs to resync (their ack_seq < new_start_seq)
        requires_resync: bool,
    },

    // === Dimension Negotiation Protocol ===

    /// Dimensions confirmed by server.
    /// Client can proceed with SyncRequest to get buffer.
    DimensionsConfirmed {
        /// Final columns (may differ from requested if adjusted)
        cols: u16,
        /// Final rows (may differ from requested if adjusted)
        rows: u16,
        /// Whether server adjusted the dimensions
        adjusted: bool,
        /// Reason for adjustment (if any)
        adjustment_reason: Option<String>,
    },
    /// Dimensions rejected by server.
    /// Client should use suggested dimensions and retry.
    DimensionsRejected {
        /// Reason for rejection
        reason: String,
        /// Suggested columns
        suggested_cols: u16,
        /// Suggested rows
        suggested_rows: u16,
    },

    /// Session status changed.
    StatusChange {
        session_id: Uuid,
        old_status: SessionStatus,
        new_status: SessionStatus,
    },
    /// Error occurred.
    Error { code: String, message: String },
    /// Pong response.
    Pong { timestamp: u64 },
    /// Current session state.
    State {
        session_id: Uuid,
        status: SessionStatus,
        messages: Vec<StoredMessage>,
    },
    /// Activity update (for real-time dashboard).
    ActivityUpdate {
        session_id: Uuid,
        model: String,
        cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        context_percent: u8,
        current_activity: String,
        /// Current tool/step being executed
        current_step: Option<String>,
        /// Recent actions with details for rich preview
        recent_actions: Vec<RecentAction>,
    },
    /// Chat event for chat mode view.
    /// Contains structured message updates from hook events.
    /// Note: Uses struct variant (not tuple) to avoid serde tag conflict with inner ChatEvent.
    ChatEvent { event: crate::ChatEvent },

    // === Chat History Protocol ===

    /// Full chat history for a session.
    /// Sent in response to RequestChatHistory.
    ChatHistory {
        /// All chat messages for the session (ordered by sequence)
        messages: Vec<crate::ChatMessage>,
    },
    /// A new prompt was indexed (for Prompt Library real-time updates).
    NewPrompt {
        prompt: crate::PromptSummary,
    },

    // === Interactive Prompt Protocol ===

    /// Interactive event for native UI rendering.
    /// Sent when Claude Code's AskUserQuestion tool is invoked.
    Interactive {
        event: crate::InteractiveEvent,
    },

    // === Subagent and Error Events ===

    /// Subagent (Task tool) started.
    /// Notifies frontend that Claude has spawned a subagent.
    SubagentStarted {
        session_id: Uuid,
        /// Unique identifier for this subagent
        agent_id: String,
        /// Type of agent (e.g., "Explore", "Plan", "general-purpose")
        agent_type: String,
    },
    /// Subagent (Task tool) stopped.
    /// Notifies frontend that a subagent has completed.
    SubagentStopped {
        session_id: Uuid,
        /// Identifier of the stopped subagent
        agent_id: String,
    },
    /// Tool execution failed.
    /// Notifies frontend of tool errors for display.
    ToolError {
        session_id: Uuid,
        tool_name: String,
        error: String,
        is_timeout: bool,
    },
    /// Context compaction starting.
    /// Notifies frontend that Claude is compacting context.
    ContextCompacting {
        session_id: Uuid,
        /// Trigger: "manual" or "auto"
        trigger: String,
    },
    /// Permission request dialog shown.
    /// Notifies frontend that Claude is waiting for permission.
    PermissionRequest {
        session_id: Uuid,
        tool_name: String,
        tool_input: Value,
    },
    /// Context token update from hook data.
    /// Provides accurate token counts (replaces regex parsing).
    ContextUpdate {
        session_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
        context_window_size: u64,
    },
}

/// A single action/step performed by Claude (for activity updates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentAction {
    /// Action type: "bash", "read", "write", "edit", "thinking", "searching", etc.
    pub action_type: String,
    /// Short summary of the action
    pub summary: String,
    /// Optional detail (file path, command, etc.)
    pub detail: Option<String>,
    /// Timestamp in milliseconds
    pub timestamp: u64,
}

/// A stored message for state recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<StoredToolCall>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolCall {
    pub tool_use_id: String,
    pub tool_name: String,
    pub input: Value,
    pub output: Option<String>,
    pub is_error: bool,
}

#[cfg(test)]
mod serialization_tests {
    use super::*;
    use uuid::Uuid;
    use serde_json::json;
    use std::path::PathBuf;

    // ========================================================================
    // WsServerMessage SERIALIZATION TESTS
    // ========================================================================

    #[test]
    fn test_session_init_serialization() {
        let msg = WsServerMessage::SessionInit {
            session_id: Uuid::nil(),
            claude_session_id: Uuid::nil(),
            model: "claude-sonnet-4".to_string(),
            tools: vec!["Read".to_string(), "Write".to_string()],
            cwd: PathBuf::from("/test"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"session_init""#));
        assert!(json.contains(r#""model":"claude-sonnet-4""#));
    }

    #[test]
    fn test_text_serialization() {
        let msg = WsServerMessage::Text {
            message_id: "msg_1".to_string(),
            content: "Hello world".to_string(),
            is_complete: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""is_complete":false"#));
    }

    #[test]
    fn test_tool_use_serialization() {
        let msg = WsServerMessage::ToolUse {
            message_id: "msg_1".to_string(),
            tool_use_id: "toolu_123".to_string(),
            tool_name: "Read".to_string(),
            input: json!({"file_path": "/test.rs"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"tool_use""#));
        assert!(json.contains(r#""tool_name":"Read""#));
    }

    #[test]
    fn test_tool_result_serialization() {
        let msg = WsServerMessage::ToolResult {
            tool_use_id: "toolu_123".to_string(),
            output: "File contents".to_string(),
            is_error: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"tool_result""#));
        assert!(json.contains(r#""is_error":false"#));
    }

    #[test]
    fn test_message_complete_serialization() {
        let msg = WsServerMessage::MessageComplete {
            message_id: "msg_1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"message_complete""#));
    }

    #[test]
    fn test_terminal_chunk_serialization() {
        let msg = WsServerMessage::TerminalChunk {
            seq: 42,
            data: vec![0x1b, 0x5b, 0x48], // ESC[H
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"terminal_chunk""#));
        assert!(json.contains(r#""seq":42"#));
    }

    #[test]
    fn test_sync_response_serialization() {
        let msg = WsServerMessage::SyncResponse {
            buffer_start_seq: 0,
            buffer_end_seq: 100,
            cols: 80,
            rows: 24,
            full_buffer: Some(vec![65, 66, 67]),
            full_buffer_start_seq: Some(0),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"sync_response""#));
        assert!(json.contains(r#""cols":80"#));
    }

    #[test]
    fn test_dimensions_confirmed_serialization() {
        let msg = WsServerMessage::DimensionsConfirmed {
            cols: 120,
            rows: 40,
            adjusted: true,
            adjustment_reason: Some("Minimum size enforced".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"dimensions_confirmed""#));
        assert!(json.contains(r#""adjusted":true"#));
    }

    #[test]
    fn test_status_change_serialization() {
        let msg = WsServerMessage::StatusChange {
            session_id: Uuid::nil(),
            old_status: SessionStatus::Active,
            new_status: SessionStatus::Stopped,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"status_change""#));
        assert!(json.contains(r#""old_status":"active""#));
        assert!(json.contains(r#""new_status":"stopped""#));
    }

    #[test]
    fn test_error_serialization() {
        let msg = WsServerMessage::Error {
            code: "INVALID_SESSION".to_string(),
            message: "Session not found".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains(r#""code":"INVALID_SESSION""#));
    }

    #[test]
    fn test_pong_serialization() {
        let msg = WsServerMessage::Pong { timestamp: 1234567890 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"pong""#));
        assert!(json.contains(r#""timestamp":1234567890"#));
    }

    #[test]
    fn test_activity_update_serialization() {
        let msg = WsServerMessage::ActivityUpdate {
            session_id: Uuid::nil(),
            model: "claude-sonnet-4".to_string(),
            cost: 0.05,
            input_tokens: 1000,
            output_tokens: 500,
            context_percent: 75,
            current_activity: "Running tests".to_string(),
            current_step: Some("cargo test".to_string()),
            recent_actions: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"activity_update""#));
        assert!(json.contains(r#""context_percent":75"#));
    }

    #[test]
    fn test_subagent_started_serialization() {
        let msg = WsServerMessage::SubagentStarted {
            session_id: Uuid::nil(),
            agent_id: "agent_123".to_string(),
            agent_type: "Explore".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"subagent_started""#));
        assert!(json.contains(r#""agent_type":"Explore""#));
    }

    #[test]
    fn test_subagent_stopped_serialization() {
        let msg = WsServerMessage::SubagentStopped {
            session_id: Uuid::nil(),
            agent_id: "agent_123".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"subagent_stopped""#));
    }

    #[test]
    fn test_tool_error_serialization() {
        let msg = WsServerMessage::ToolError {
            session_id: Uuid::nil(),
            tool_name: "Bash".to_string(),
            error: "Command timed out".to_string(),
            is_timeout: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"tool_error""#));
        assert!(json.contains(r#""is_timeout":true"#));
    }

    #[test]
    fn test_context_compacting_serialization() {
        let msg = WsServerMessage::ContextCompacting {
            session_id: Uuid::nil(),
            trigger: "auto".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"context_compacting""#));
        assert!(json.contains(r#""trigger":"auto""#));
    }

    #[test]
    fn test_permission_request_serialization() {
        let msg = WsServerMessage::PermissionRequest {
            session_id: Uuid::nil(),
            tool_name: "Write".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"permission_request""#));
        assert!(json.contains(r#""tool_name":"Write""#));
    }

    #[test]
    fn test_context_update_serialization() {
        let msg = WsServerMessage::ContextUpdate {
            session_id: Uuid::nil(),
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            context_window_size: 200000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"context_update""#));
        assert!(json.contains(r#""cache_read_tokens":200"#));
    }

    // ========================================================================
    // WsClientMessage SERIALIZATION TESTS
    // ========================================================================

    #[test]
    fn test_client_input_serialization() {
        let msg = WsClientMessage::Input {
            content: "Hello Claude".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"input""#));
    }

    #[test]
    fn test_client_terminal_input_serialization() {
        let msg = WsClientMessage::TerminalInput {
            data: vec![0x1b, 0x5b, 0x41], // ESC[A (up arrow)
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"terminal_input""#));
    }

    #[test]
    fn test_client_resize_serialization() {
        let msg = WsClientMessage::Resize { rows: 24, cols: 80 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"resize""#));
        assert!(json.contains(r#""rows":24"#));
        assert!(json.contains(r#""cols":80"#));
    }

    #[test]
    fn test_client_ping_serialization() {
        let msg = WsClientMessage::Ping { timestamp: 1234567890 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"ping""#));
    }

    #[test]
    fn test_client_sync_request_serialization() {
        let msg = WsClientMessage::SyncRequest {
            last_seq: 42,
            cols: 80,
            rows: 24,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"sync_request""#));
        assert!(json.contains(r#""last_seq":42"#));
    }

    #[test]
    fn test_client_ack_serialization() {
        let msg = WsClientMessage::Ack { ack_seq: 100 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"ack""#));
        assert!(json.contains(r#""ack_seq":100"#));
    }

    #[test]
    fn test_client_negotiate_dimensions_serialization() {
        let msg = WsClientMessage::NegotiateDimensions {
            cols: 120,
            rows: 40,
            confidence: "high".to_string(),
            source: "fitaddon".to_string(),
            cell_width: Some(9.5),
            font_loaded: true,
            device_hint: "desktop".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"negotiate_dimensions""#));
        assert!(json.contains(r#""confidence":"high""#));
    }

    #[test]
    fn test_client_interactive_choice_serialization() {
        let msg = WsClientMessage::InteractiveChoice {
            question_id: "q_1".to_string(),
            selected_indices: vec![1, 3],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"interactive_choice""#));
        assert!(json.contains(r#""selected_indices":[1,3]"#));
    }

    #[test]
    fn test_client_interactive_text_serialization() {
        let msg = WsClientMessage::InteractiveText {
            response: "Custom answer".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"interactive_text""#));
    }

    #[test]
    fn test_client_interactive_cancel_serialization() {
        let msg = WsClientMessage::InteractiveCancel;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"interactive_cancel""#));
    }

    // ========================================================================
    // ROUNDTRIP TESTS
    // ========================================================================

    #[test]
    fn test_client_message_roundtrip() {
        let original = WsClientMessage::SyncRequest {
            last_seq: 42,
            cols: 80,
            rows: 24,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: WsClientMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            WsClientMessage::SyncRequest { last_seq, cols, rows } => {
                assert_eq!(last_seq, 42);
                assert_eq!(cols, 80);
                assert_eq!(rows, 24);
            }
            _ => panic!("Expected SyncRequest"),
        }
    }

    #[test]
    fn test_server_message_roundtrip() {
        let original = WsServerMessage::TerminalChunk {
            seq: 100,
            data: vec![65, 66, 67, 68],
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: WsServerMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            WsServerMessage::TerminalChunk { seq, data, timestamp } => {
                assert_eq!(seq, 100);
                assert_eq!(data, vec![65, 66, 67, 68]);
                assert_eq!(timestamp, 1234567890);
            }
            _ => panic!("Expected TerminalChunk"),
        }
    }

    // ========================================================================
    // SNAKE_CASE TYPE TAG TESTS
    // ========================================================================

    #[test]
    fn test_all_server_messages_have_snake_case_type() {
        // Test that serde rename_all = snake_case is working
        let messages: Vec<(&str, WsServerMessage)> = vec![
            ("session_init", WsServerMessage::SessionInit {
                session_id: Uuid::nil(),
                claude_session_id: Uuid::nil(),
                model: "test".to_string(),
                tools: vec![],
                cwd: PathBuf::new(),
            }),
            ("terminal_chunk", WsServerMessage::TerminalChunk {
                seq: 0,
                data: vec![],
                timestamp: 0,
            }),
            ("sync_response", WsServerMessage::SyncResponse {
                buffer_start_seq: 0,
                buffer_end_seq: 0,
                cols: 80,
                rows: 24,
                full_buffer: None,
                full_buffer_start_seq: None,
            }),
            ("dimensions_confirmed", WsServerMessage::DimensionsConfirmed {
                cols: 80,
                rows: 24,
                adjusted: false,
                adjustment_reason: None,
            }),
            ("status_change", WsServerMessage::StatusChange {
                session_id: Uuid::nil(),
                old_status: SessionStatus::Created,
                new_status: SessionStatus::Active,
            }),
            ("activity_update", WsServerMessage::ActivityUpdate {
                session_id: Uuid::nil(),
                model: "test".to_string(),
                cost: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                context_percent: 0,
                current_activity: "".to_string(),
                current_step: None,
                recent_actions: vec![],
            }),
            ("message_complete", WsServerMessage::MessageComplete {
                message_id: "".to_string(),
            }),
            ("input_required", WsServerMessage::InputRequired {
                prompt: "".to_string(),
            }),
            ("tool_result", WsServerMessage::ToolResult {
                tool_use_id: "".to_string(),
                output: "".to_string(),
                is_error: false,
            }),
        ];

        for (expected_type, msg) in messages {
            let json = serde_json::to_string(&msg).unwrap();
            let type_pattern = format!(r#""type":"{}""#, expected_type);
            assert!(
                json.contains(&type_pattern),
                "Expected type '{}' in JSON: {}",
                expected_type,
                json
            );
        }
    }

    #[test]
    fn test_all_client_messages_have_snake_case_type() {
        let messages: Vec<(&str, WsClientMessage)> = vec![
            ("terminal_input", WsClientMessage::TerminalInput { data: vec![] }),
            ("get_state", WsClientMessage::GetState),
            ("request_buffer", WsClientMessage::RequestBuffer),
            ("status_update", WsClientMessage::StatusUpdate {
                model: "test".to_string(),
                cost: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                context_percent: 0,
            }),
            ("sync_request", WsClientMessage::SyncRequest {
                last_seq: 0,
                cols: 80,
                rows: 24,
            }),
            ("range_request", WsClientMessage::RangeRequest {
                start_seq: 0,
                end_seq: 10,
            }),
            ("negotiate_dimensions", WsClientMessage::NegotiateDimensions {
                cols: 80,
                rows: 24,
                confidence: "high".to_string(),
                source: "fitaddon".to_string(),
                cell_width: None,
                font_loaded: true,
                device_hint: "desktop".to_string(),
            }),
            ("request_chat_history", WsClientMessage::RequestChatHistory),
            ("interactive_choice", WsClientMessage::InteractiveChoice {
                question_id: "q1".to_string(),
                selected_indices: vec![1],
            }),
            ("interactive_text", WsClientMessage::InteractiveText {
                response: "test".to_string(),
            }),
            ("interactive_cancel", WsClientMessage::InteractiveCancel),
        ];

        for (expected_type, msg) in messages {
            let json = serde_json::to_string(&msg).unwrap();
            let type_pattern = format!(r#""type":"{}""#, expected_type);
            assert!(
                json.contains(&type_pattern),
                "Expected type '{}' in JSON: {}",
                expected_type,
                json
            );
        }
    }

    // ========================================================================
    // STORED MESSAGE TESTS
    // ========================================================================

    #[test]
    fn test_stored_message_serialization() {
        let msg = StoredMessage {
            id: "msg_1".to_string(),
            role: MessageRole::Assistant,
            content: "Hello!".to_string(),
            tool_calls: vec![],
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"assistant""#));
    }

    #[test]
    fn test_stored_tool_call_serialization() {
        let tc = StoredToolCall {
            tool_use_id: "toolu_123".to_string(),
            tool_name: "Read".to_string(),
            input: json!({"file_path": "/test.rs"}),
            output: Some("fn main() {}".to_string()),
            is_error: false,
        };
        let json = serde_json::to_string(&tc).unwrap();
        assert!(json.contains(r#""tool_name":"Read""#));
    }

    #[test]
    fn test_recent_action_serialization() {
        let action = RecentAction {
            action_type: "read".to_string(),
            summary: "Read config.rs".to_string(),
            detail: Some("/project/src/config.rs".to_string()),
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""action_type":"read""#));
    }
}
