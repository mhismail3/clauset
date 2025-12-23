// WebSocket connection manager with reliable streaming protocol

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface WsMessage {
  type: string;
  [key: string]: unknown;
}

// Terminal chunk with sequence number for reliable streaming
export interface TerminalChunk {
  seq: number;
  data: number[]; // Vec<u8> serialized as array of numbers
  timestamp: number;
}

// Sync response from server
export interface SyncResponse {
  buffer_start_seq: number;
  buffer_end_seq: number;
  cols: number;
  rows: number;
  full_buffer?: number[];
  full_buffer_start_seq?: number;
}

// Chunk batch for gap recovery
export interface ChunkBatch {
  start_seq: number;
  data: number[];
  chunk_count: number;
  is_complete: boolean;
}

// Buffer overflow notification
export interface BufferOverflow {
  new_start_seq: number;
  requires_resync: boolean;
}

export interface WebSocketManagerOptions {
  url: string;
  onMessage?: (data: WsMessage) => void;
  onStateChange?: (state: ConnectionState) => void;
  // New: callback for processed terminal data (after reordering)
  onTerminalData?: (data: Uint8Array) => void;
  // New: callback for sync response
  onSyncResponse?: (response: SyncResponse) => void;
  reconnectAttempts?: number;
  reconnectDelay?: number;
}

// Stream state for reliable delivery
interface StreamState {
  // Last contiguous sequence number received (all prior seqs are complete)
  lastContiguousSeq: number;
  // Pending out-of-order chunks waiting for gap fill
  pendingChunks: Map<number, Uint8Array>;
  // Last sequence number we acknowledged
  lastAckedSeq: number;
  // Timer for batched acks
  ackTimer: number | null;
  // Timer for gap recovery
  gapRecoveryTimer: number | null;
  // Terminal dimensions for sync request
  terminalCols: number;
  terminalRows: number;
}

// Maximum number of messages to queue when disconnected
const MAX_QUEUE_SIZE = 50;
// Maximum pending out-of-order chunks before forcing resync
const MAX_PENDING_CHUNKS = 100;
// Ack batch interval (ms)
const ACK_INTERVAL_MS = 100;
// Gap recovery timeout (ms) - wait this long for missing chunk before requesting
const GAP_RECOVERY_TIMEOUT_MS = 500;

export function createWebSocketManager(options: WebSocketManagerOptions) {
  let ws: WebSocket | null = null;
  let reconnectCount = 0;
  let reconnectTimer: number | null = null;
  let state: ConnectionState = 'disconnected';
  let messageQueue: unknown[] = [];

  // Stream state for reliable delivery
  const streamState: StreamState = {
    lastContiguousSeq: 0,
    pendingChunks: new Map(),
    lastAckedSeq: 0,
    ackTimer: null,
    gapRecoveryTimer: null,
    terminalCols: 80,
    terminalRows: 24,
  };

  const maxReconnectAttempts = options.reconnectAttempts ?? 10;
  const baseReconnectDelay = options.reconnectDelay ?? 1000;

  function setState(newState: ConnectionState) {
    state = newState;
    options.onStateChange?.(newState);
  }

  function connect() {
    if (ws?.readyState === WebSocket.OPEN) return;

    setState('connecting');

    try {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = options.url.startsWith('ws') ? options.url : `${protocol}//${window.location.host}${options.url}`;
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setState('connected');
        reconnectCount = 0;

        // Send SyncRequest to get current state and any missed chunks
        sendSyncRequest();

        // Flush queued messages
        if (messageQueue.length > 0) {
          console.log(`Flushing ${messageQueue.length} queued messages after reconnect`);
          const queue = messageQueue;
          messageQueue = [];
          for (const msg of queue) {
            ws!.send(JSON.stringify(msg));
          }
        }
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          handleMessage(data);
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      ws.onclose = (event) => {
        ws = null;
        clearTimers();
        if (!event.wasClean) {
          scheduleReconnect();
        } else {
          setState('disconnected');
        }
      };

      ws.onerror = () => {
        console.error('WebSocket error');
      };
    } catch (e) {
      console.error('WebSocket connection failed:', e);
      scheduleReconnect();
    }
  }

  function handleMessage(data: WsMessage) {
    switch (data.type) {
      case 'terminal_chunk':
        handleTerminalChunk(data as unknown as { type: string } & TerminalChunk);
        break;
      case 'sync_response':
        handleSyncResponse(data as unknown as { type: string } & SyncResponse);
        break;
      case 'chunk_batch':
        handleChunkBatch(data as unknown as { type: string } & ChunkBatch);
        break;
      case 'buffer_overflow':
        handleBufferOverflow(data as unknown as { type: string } & BufferOverflow);
        break;
      default:
        // Pass through other messages to the generic handler
        options.onMessage?.(data);
    }
  }

  function handleTerminalChunk(chunk: { type: string } & TerminalChunk) {
    const expectedSeq = streamState.lastContiguousSeq + 1;

    if (chunk.seq === expectedSeq) {
      // In order - process immediately
      const bytes = new Uint8Array(chunk.data);
      options.onTerminalData?.(bytes);
      streamState.lastContiguousSeq = chunk.seq;

      // Process any pending chunks that are now in order
      processPendingChunks();

      // Schedule ack
      scheduleAck();
    } else if (chunk.seq > expectedSeq) {
      // Out of order - buffer it
      console.debug(`Out of order chunk: got ${chunk.seq}, expected ${expectedSeq}`);
      streamState.pendingChunks.set(chunk.seq, new Uint8Array(chunk.data));

      // If we have too many pending chunks, request resync
      if (streamState.pendingChunks.size > MAX_PENDING_CHUNKS) {
        console.warn('Too many pending chunks, requesting resync');
        streamState.pendingChunks.clear();
        sendSyncRequest();
        return;
      }

      // Schedule gap recovery
      scheduleGapRecovery(expectedSeq, chunk.seq - 1);
    }
    // If chunk.seq <= lastContiguousSeq, it's a duplicate - ignore
  }

  function processPendingChunks() {
    // Keep processing pending chunks while we have the next expected one
    while (streamState.pendingChunks.has(streamState.lastContiguousSeq + 1)) {
      const nextSeq = streamState.lastContiguousSeq + 1;
      const data = streamState.pendingChunks.get(nextSeq)!;
      streamState.pendingChunks.delete(nextSeq);
      options.onTerminalData?.(data);
      streamState.lastContiguousSeq = nextSeq;
    }
  }

  function handleSyncResponse(response: { type: string } & SyncResponse) {
    console.log(`SyncResponse: buffer ${response.buffer_start_seq}..${response.buffer_end_seq}, dims ${response.cols}x${response.rows}`);

    // Clear any pending state
    streamState.pendingChunks.clear();
    clearTimers();

    if (response.full_buffer && response.full_buffer.length > 0) {
      // Server sent full buffer - apply it
      const bytes = new Uint8Array(response.full_buffer);
      options.onTerminalData?.(bytes);
      streamState.lastContiguousSeq = response.buffer_end_seq;
    } else {
      // We're caught up - just update our sequence position
      streamState.lastContiguousSeq = response.buffer_end_seq;
    }

    // Notify callback
    options.onSyncResponse?.(response);
  }

  function handleChunkBatch(batch: { type: string } & ChunkBatch) {
    console.debug(`ChunkBatch: ${batch.chunk_count} chunks from seq ${batch.start_seq}`);

    // Process the batch data - it's a concatenation of the requested chunks
    // Note: we treat the batch as a single blob since individual chunk boundaries
    // aren't preserved in the batch response
    const bytes = new Uint8Array(batch.data);
    options.onTerminalData?.(bytes);

    // Update our sequence position if this fills the gap
    if (batch.start_seq === streamState.lastContiguousSeq + 1) {
      // This batch fills our gap - estimate end sequence from start + count
      streamState.lastContiguousSeq = batch.start_seq + batch.chunk_count - 1;
      processPendingChunks();
    }

    // Clear gap recovery timer since we got the response
    if (streamState.gapRecoveryTimer) {
      clearTimeout(streamState.gapRecoveryTimer);
      streamState.gapRecoveryTimer = null;
    }
  }

  function handleBufferOverflow(overflow: { type: string } & BufferOverflow) {
    console.warn(`Buffer overflow: new start seq ${overflow.new_start_seq}, resync required: ${overflow.requires_resync}`);

    if (overflow.requires_resync) {
      // Server told us we're too far behind - request full resync
      streamState.pendingChunks.clear();
      streamState.lastContiguousSeq = 0;
      sendSyncRequest();
    }
  }

  function sendSyncRequest() {
    if (ws?.readyState === WebSocket.OPEN) {
      const syncRequest = {
        type: 'sync_request',
        last_seq: streamState.lastContiguousSeq,
        cols: streamState.terminalCols,
        rows: streamState.terminalRows,
      };
      ws.send(JSON.stringify(syncRequest));
      console.debug(`SyncRequest: last_seq=${streamState.lastContiguousSeq}, dims=${streamState.terminalCols}x${streamState.terminalRows}`);
    }
  }

  function scheduleAck() {
    // Batch acks to reduce message overhead
    if (streamState.ackTimer) return;

    streamState.ackTimer = window.setTimeout(() => {
      streamState.ackTimer = null;
      sendAck();
    }, ACK_INTERVAL_MS);
  }

  function sendAck() {
    if (ws?.readyState === WebSocket.OPEN && streamState.lastContiguousSeq > streamState.lastAckedSeq) {
      const ack = {
        type: 'ack',
        ack_seq: streamState.lastContiguousSeq,
      };
      ws.send(JSON.stringify(ack));
      streamState.lastAckedSeq = streamState.lastContiguousSeq;
    }
  }

  function scheduleGapRecovery(startSeq: number, endSeq: number) {
    // Only schedule if not already pending
    if (streamState.gapRecoveryTimer) return;

    streamState.gapRecoveryTimer = window.setTimeout(() => {
      streamState.gapRecoveryTimer = null;

      // Check if we still have the gap
      if (streamState.lastContiguousSeq < startSeq - 1) {
        sendRangeRequest(streamState.lastContiguousSeq + 1, endSeq);
      }
    }, GAP_RECOVERY_TIMEOUT_MS);
  }

  function sendRangeRequest(startSeq: number, endSeq: number) {
    if (ws?.readyState === WebSocket.OPEN) {
      const rangeRequest = {
        type: 'range_request',
        start_seq: startSeq,
        end_seq: endSeq,
      };
      ws.send(JSON.stringify(rangeRequest));
      console.debug(`RangeRequest: ${startSeq}..${endSeq}`);
    }
  }

  function clearTimers() {
    if (streamState.ackTimer) {
      clearTimeout(streamState.ackTimer);
      streamState.ackTimer = null;
    }
    if (streamState.gapRecoveryTimer) {
      clearTimeout(streamState.gapRecoveryTimer);
      streamState.gapRecoveryTimer = null;
    }
  }

  function scheduleReconnect() {
    if (reconnectCount >= maxReconnectAttempts) {
      setState('disconnected');
      return;
    }

    setState('reconnecting');
    reconnectCount++;

    // Exponential backoff with jitter
    const delay = Math.min(
      baseReconnectDelay * Math.pow(2, reconnectCount - 1) + Math.random() * 1000,
      30000
    );

    reconnectTimer = window.setTimeout(connect, delay);
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    clearTimers();
    // Clear queued messages on intentional disconnect
    messageQueue = [];
    ws?.close(1000, 'Client disconnect');
    ws = null;
    setState('disconnected');
  }

  function send(data: unknown): boolean {
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(data));
      return true;
    }

    // Queue message for later delivery (with cap to prevent unbounded growth)
    if (messageQueue.length < MAX_QUEUE_SIZE) {
      messageQueue.push(data);
      console.debug(`Queued message (${messageQueue.length}/${MAX_QUEUE_SIZE}), will send on reconnect`);
    } else {
      console.warn(`Message queue full (${MAX_QUEUE_SIZE}), dropping message`);
    }
    return false;
  }

  function getState(): ConnectionState {
    return state;
  }

  // Update terminal dimensions (call before connect or when resizing)
  function setTerminalDimensions(cols: number, rows: number) {
    streamState.terminalCols = cols;
    streamState.terminalRows = rows;
  }

  // Get current stream state for debugging
  function getStreamState() {
    return {
      lastContiguousSeq: streamState.lastContiguousSeq,
      pendingChunks: streamState.pendingChunks.size,
      lastAckedSeq: streamState.lastAckedSeq,
    };
  }

  // Force a resync (useful after terminal resize)
  function requestResync() {
    streamState.pendingChunks.clear();
    sendSyncRequest();
  }

  return {
    connect,
    disconnect,
    send,
    getState,
    setTerminalDimensions,
    getStreamState,
    requestResync,
  };
}
