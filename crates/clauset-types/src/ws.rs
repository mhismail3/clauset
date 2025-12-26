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
