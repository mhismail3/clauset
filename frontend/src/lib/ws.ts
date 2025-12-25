// WebSocket connection manager with reliable streaming protocol
// Phase 1: Sequence numbers, ACKs, gap recovery
// Phase 2: Extended states, heartbeat, iOS lifecycle

export type ConnectionState =
  | 'initial'      // Never connected
  | 'connecting'   // Active connection attempt
  | 'connected'    // Healthy (recent pong received)
  | 'stale'        // Connected but no pong in STALE_THRESHOLD_MS
  | 'reconnecting' // Triggered reconnect
  | 'backoff'      // Waiting before retry
  | 'failed'       // Max retries exceeded
  | 'suspended';   // iOS background (intentional pause)

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

// Dimension negotiation response from server
export interface DimensionsConfirmed {
  cols: number;
  rows: number;
  adjusted: boolean;
  adjustment_reason?: string;
}

// Dimension negotiation rejection from server
export interface DimensionsRejected {
  reason: string;
  suggested_cols: number;
  suggested_rows: number;
}

export interface WebSocketManagerOptions {
  url: string;
  onMessage?: (data: WsMessage) => void;
  onStateChange?: (state: ConnectionState) => void;
  // Callback for processed terminal data (after reordering)
  onTerminalData?: (data: Uint8Array) => void;
  // Callback for sync response
  onSyncResponse?: (response: SyncResponse) => void;
  // Callback for dimension negotiation confirmation
  onDimensionsConfirmed?: (response: DimensionsConfirmed) => void;
  // Callback for dimension negotiation rejection
  onDimensionsRejected?: (response: DimensionsRejected) => void;
  // Callback for stale connection detection
  onStale?: () => void;
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

// Heartbeat state for stale detection
interface HeartbeatState {
  // Timer for sending pings
  pingTimer: number | null;
  // Timer for expecting pong
  pongTimeoutTimer: number | null;
  // Timestamp of last pong received
  lastPongTime: number;
  // Count of missed pongs
  missedPongs: number;
}

// === Constants ===

// Maximum number of messages to queue when disconnected
const MAX_QUEUE_SIZE = 50;
// Maximum pending out-of-order chunks before forcing resync
const MAX_PENDING_CHUNKS = 100;
// Ack batch interval (ms)
const ACK_INTERVAL_MS = 100;
// Gap recovery timeout (ms) - wait this long for missing chunk before requesting
const GAP_RECOVERY_TIMEOUT_MS = 500;

// Heartbeat constants
const PING_INTERVAL_MS = 15000;     // Send ping every 15s
const PONG_TIMEOUT_MS = 5000;       // Expect pong within 5s
const STALE_THRESHOLD_MS = 25000;   // Mark stale if no pong in 25s
const MAX_MISSED_PONGS = 2;         // Force reconnect after 2 missed pongs

// Safari-specific: Check for stale connection even when readyState === OPEN
const SAFARI_MESSAGE_RECENCY_MS = 45000;  // If no message in 45s, connection might be dead

// Detect Safari
const isSafari = /^((?!chrome|android).)*safari/i.test(navigator.userAgent);

// LocalStorage key for persisting message queue
const QUEUE_STORAGE_KEY = 'clauset_message_queue';
// Maximum age for queued messages (5 minutes)
const MAX_QUEUE_AGE_MS = 5 * 60 * 1000;

// Get device-appropriate default dimensions
// These are sent immediately on connect before terminal is visible
function getDeviceDefaultDimensions(): { cols: number; rows: number } {
  const isIOSDevice = /iPad|iPhone|iPod/.test(navigator.userAgent) ||
    (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

  if (isIOSDevice) {
    const screenWidth = window.screen.width;
    const pixelRatio = window.devicePixelRatio || 1;
    const logicalWidth = screenWidth / pixelRatio;

    // iPad vs iPhone based on logical screen width
    if (logicalWidth >= 768) {
      return { cols: 80, rows: 30 }; // iPad
    }
    return { cols: 45, rows: 25 }; // iPhone - slightly larger than min for safety
  }

  return { cols: 80, rows: 24 }; // Desktop default
}

export function createWebSocketManager(options: WebSocketManagerOptions) {
  let ws: WebSocket | null = null;
  let reconnectCount = 0;
  let reconnectTimer: number | null = null;
  let state: ConnectionState = 'initial';
  let messageQueue: Array<{ data: unknown; timestamp: number }> = [];
  let isSuspended = false;

  // Get device-appropriate defaults for initial sync
  const deviceDefaults = getDeviceDefaultDimensions();

  // Stream state for reliable delivery
  const streamState: StreamState = {
    lastContiguousSeq: 0,
    pendingChunks: new Map(),
    lastAckedSeq: 0,
    ackTimer: null,
    gapRecoveryTimer: null,
    // Use device-appropriate defaults so PTY is created with reasonable dimensions
    // These will be updated when terminal becomes visible
    terminalCols: deviceDefaults.cols,
    terminalRows: deviceDefaults.rows,
  };

  // Heartbeat state
  const heartbeatState: HeartbeatState = {
    pingTimer: null,
    pongTimeoutTimer: null,
    lastPongTime: Date.now(),
    missedPongs: 0,
  };

  // Track last message received (for Safari staleness detection)
  let lastMessageTime = Date.now();

  const maxReconnectAttempts = options.reconnectAttempts ?? 10;
  const baseReconnectDelay = options.reconnectDelay ?? 1000;

  // Load persisted message queue from localStorage
  function loadPersistedQueue() {
    try {
      const stored = localStorage.getItem(QUEUE_STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored) as Array<{ data: unknown; timestamp: number }>;
        const now = Date.now();
        // Filter out expired messages
        messageQueue = parsed.filter(msg => now - msg.timestamp < MAX_QUEUE_AGE_MS);
        localStorage.removeItem(QUEUE_STORAGE_KEY);
        if (messageQueue.length > 0) {
          console.log(`Loaded ${messageQueue.length} persisted messages from storage`);
        }
      }
    } catch (e) {
      console.warn('Failed to load persisted message queue:', e);
    }
  }

  // Persist message queue to localStorage
  function persistQueue() {
    try {
      if (messageQueue.length > 0) {
        localStorage.setItem(QUEUE_STORAGE_KEY, JSON.stringify(messageQueue));
        console.log(`Persisted ${messageQueue.length} messages to storage`);
      }
    } catch (e) {
      console.warn('Failed to persist message queue:', e);
    }
  }

  function setState(newState: ConnectionState) {
    if (state !== newState) {
      console.debug(`WS state: ${state} -> ${newState}`);
      state = newState;
      options.onStateChange?.(newState);
    }
  }

  function startHeartbeat() {
    stopHeartbeat();
    heartbeatState.lastPongTime = Date.now();
    heartbeatState.missedPongs = 0;

    // Send ping periodically
    heartbeatState.pingTimer = window.setInterval(() => {
      if (ws?.readyState === WebSocket.OPEN) {
        sendPing();
      }
    }, PING_INTERVAL_MS);
  }

  function stopHeartbeat() {
    if (heartbeatState.pingTimer) {
      clearInterval(heartbeatState.pingTimer);
      heartbeatState.pingTimer = null;
    }
    if (heartbeatState.pongTimeoutTimer) {
      clearTimeout(heartbeatState.pongTimeoutTimer);
      heartbeatState.pongTimeoutTimer = null;
    }
  }

  function sendPing() {
    if (ws?.readyState !== WebSocket.OPEN) return;

    const timestamp = Date.now();
    ws.send(JSON.stringify({ type: 'ping', timestamp }));

    // Set timeout for pong response
    heartbeatState.pongTimeoutTimer = window.setTimeout(() => {
      heartbeatState.missedPongs++;
      console.warn(`Missed pong (${heartbeatState.missedPongs}/${MAX_MISSED_PONGS})`);

      // Check if connection is stale
      const timeSinceLastPong = Date.now() - heartbeatState.lastPongTime;
      const timeSinceLastMessage = Date.now() - lastMessageTime;

      // Safari-specific: Also check message recency
      // Safari's WebSocket.readyState can be OPEN even when connection is dead
      const isStale = timeSinceLastPong > STALE_THRESHOLD_MS ||
        (isSafari && timeSinceLastMessage > SAFARI_MESSAGE_RECENCY_MS);

      if (isStale && state === 'connected') {
        if (isSafari && timeSinceLastMessage > SAFARI_MESSAGE_RECENCY_MS) {
          console.warn('Safari: No messages received recently, connection may be dead');
        }
        setState('stale');
        options.onStale?.();
      }

      // Force reconnect after too many missed pongs
      if (heartbeatState.missedPongs >= MAX_MISSED_PONGS) {
        console.warn('Too many missed pongs, forcing reconnect');
        forceReconnect();
      }
    }, PONG_TIMEOUT_MS);
  }

  function handlePong(_timestamp: number) {
    // Clear pong timeout
    if (heartbeatState.pongTimeoutTimer) {
      clearTimeout(heartbeatState.pongTimeoutTimer);
      heartbeatState.pongTimeoutTimer = null;
    }

    heartbeatState.lastPongTime = Date.now();
    heartbeatState.missedPongs = 0;

    // If we were stale, we're now healthy again
    if (state === 'stale') {
      setState('connected');
    }
  }

  function forceReconnect() {
    stopHeartbeat();
    if (ws) {
      ws.close(4000, 'Stale connection');
      ws = null;
    }
    scheduleReconnect();
  }

  function connect() {
    if (ws?.readyState === WebSocket.OPEN) return;
    if (isSuspended) return;

    // Load any persisted messages from previous session
    loadPersistedQueue();

    setState('connecting');

    try {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = options.url.startsWith('ws') ? options.url : `${protocol}//${window.location.host}${options.url}`;
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setState('connected');
        reconnectCount = 0;

        // Start heartbeat
        startHeartbeat();

        // Send SyncRequest immediately with device-appropriate default dimensions
        // This ensures PTY is created with reasonable dimensions (45 cols for iPhone, 80 for desktop)
        // Dimensions will be updated when terminal becomes visible
        sendSyncRequest();

        // Flush queued messages
        if (messageQueue.length > 0) {
          console.log(`Flushing ${messageQueue.length} queued messages after reconnect`);
          const queue = messageQueue;
          messageQueue = [];
          for (const msg of queue) {
            ws!.send(JSON.stringify(msg.data));
          }
        }
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          // Debug: log all incoming messages
          if (data.type !== 'terminal_chunk' && data.type !== 'pong') {
            console.log('[WS] Received:', data.type, data);
          }
          handleMessage(data);
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      ws.onclose = (event) => {
        ws = null;
        stopHeartbeat();
        clearStreamTimers();

        if (isSuspended) {
          // Don't reconnect if suspended
          setState('suspended');
        } else if (!event.wasClean) {
          scheduleReconnect();
        } else {
          setState('initial');
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
    // Track message recency for Safari staleness detection
    lastMessageTime = Date.now();

    switch (data.type) {
      case 'pong':
        handlePong(data.timestamp as number);
        break;
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
      case 'dimensions_confirmed':
        handleDimensionsConfirmed(data as unknown as { type: string } & DimensionsConfirmed);
        break;
      case 'dimensions_rejected':
        handleDimensionsRejected(data as unknown as { type: string } & DimensionsRejected);
        break;
      default:
        // Pass through other messages to the generic handler
        options.onMessage?.(data);
    }
  }

  function handleDimensionsConfirmed(response: { type: string } & DimensionsConfirmed) {
    console.log(`DimensionsConfirmed: ${response.cols}x${response.rows}, adjusted=${response.adjusted}`);
    if (response.adjusted && response.adjustment_reason) {
      console.warn(`Dimensions adjusted: ${response.adjustment_reason}`);
    }
    options.onDimensionsConfirmed?.(response);
  }

  function handleDimensionsRejected(response: { type: string } & DimensionsRejected) {
    console.warn(`DimensionsRejected: ${response.reason}, suggested ${response.suggested_cols}x${response.suggested_rows}`);
    options.onDimensionsRejected?.(response);
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
    clearStreamTimers();

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

    // Process the batch data
    const bytes = new Uint8Array(batch.data);
    options.onTerminalData?.(bytes);

    // Update our sequence position if this fills the gap
    if (batch.start_seq === streamState.lastContiguousSeq + 1) {
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
    if (streamState.gapRecoveryTimer) return;

    streamState.gapRecoveryTimer = window.setTimeout(() => {
      streamState.gapRecoveryTimer = null;

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

  function clearStreamTimers() {
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
      setState('failed');
      return;
    }

    setState('backoff');
    reconnectCount++;

    // Exponential backoff with jitter
    const delay = Math.min(
      baseReconnectDelay * Math.pow(2, reconnectCount - 1) + Math.random() * 1000,
      30000
    );

    console.log(`Reconnecting in ${Math.round(delay)}ms (attempt ${reconnectCount}/${maxReconnectAttempts})`);

    reconnectTimer = window.setTimeout(() => {
      setState('reconnecting');
      connect();
    }, delay);
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    stopHeartbeat();
    clearStreamTimers();
    messageQueue = [];
    ws?.close(1000, 'Client disconnect');
    ws = null;
    setState('initial');
  }

  function send(data: unknown): boolean {
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(data));
      return true;
    }

    // Queue message for later delivery (with cap to prevent unbounded growth)
    if (messageQueue.length < MAX_QUEUE_SIZE) {
      messageQueue.push({ data, timestamp: Date.now() });
      console.debug(`Queued message (${messageQueue.length}/${MAX_QUEUE_SIZE}), will send on reconnect`);
    } else {
      console.warn(`Message queue full (${MAX_QUEUE_SIZE}), dropping message`);
    }
    return false;
  }

  function getState(): ConnectionState {
    return state;
  }

  function setTerminalDimensions(cols: number, rows: number) {
    const changed = cols !== streamState.terminalCols || rows !== streamState.terminalRows;
    streamState.terminalCols = cols;
    streamState.terminalRows = rows;

    // If dimensions changed and we're connected, send updated dimensions via resync
    // This handles the case where terminal becomes visible and we have accurate measurements
    if (changed && ws?.readyState === WebSocket.OPEN && cols > 0 && rows > 0) {
      console.log(`Sending dimension update: ${cols}x${rows}`);
      sendSyncRequest();
    }
  }

  function getStreamState() {
    return {
      lastContiguousSeq: streamState.lastContiguousSeq,
      pendingChunks: streamState.pendingChunks.size,
      lastAckedSeq: streamState.lastAckedSeq,
    };
  }

  function getConnectionInfo() {
    return {
      reconnectAttempt: reconnectCount,
      maxReconnectAttempts: maxReconnectAttempts,
      queuedMessageCount: messageQueue.length,
    };
  }

  function retry() {
    if (state === 'failed' || state === 'stale') {
      reconnectCount = 0;
      connect();
    }
  }

  function requestResync() {
    streamState.pendingChunks.clear();
    sendSyncRequest();
  }

  /**
   * Negotiate terminal dimensions with server validation.
   * Call this before requestResync to ensure dimensions are validated.
   */
  function negotiateDimensions(params: {
    cols: number;
    rows: number;
    confidence: 'high' | 'medium' | 'low';
    source: 'fitaddon' | 'container' | 'estimation' | 'defaults';
    cellWidth?: number;
    fontLoaded: boolean;
    deviceHint: 'iphone' | 'ipad' | 'desktop';
  }) {
    if (ws?.readyState !== WebSocket.OPEN) {
      console.warn('Cannot negotiate dimensions: WebSocket not connected');
      return false;
    }

    const msg = {
      type: 'negotiate_dimensions',
      cols: params.cols,
      rows: params.rows,
      confidence: params.confidence,
      source: params.source,
      cell_width: params.cellWidth ?? null,
      font_loaded: params.fontLoaded,
      device_hint: params.deviceHint,
    };

    ws.send(JSON.stringify(msg));
    console.debug(`NegotiateDimensions: ${params.cols}x${params.rows}, confidence=${params.confidence}, device=${params.deviceHint}`);
    return true;
  }

  // === iOS PWA Lifecycle Handling ===

  function suspend() {
    if (isSuspended) return;
    isSuspended = true;
    console.log('WebSocket suspended (iOS background)');

    // Persist queue before suspension
    persistQueue();

    // Close connection gracefully
    stopHeartbeat();
    clearStreamTimers();
    ws?.close(1000, 'iOS suspend');
    ws = null;
    setState('suspended');
  }

  function resume() {
    if (!isSuspended) return;
    isSuspended = false;
    console.log('WebSocket resumed (iOS foreground)');

    // Reconnect
    connect();
  }

  // Set up visibility change listener for iOS PWA
  function setupVisibilityHandler() {
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') {
        // Page going to background - persist state
        persistQueue();
      } else if (document.visibilityState === 'visible') {
        // Page coming to foreground
        if (isSuspended) {
          resume();
        } else if (state === 'stale' || state === 'failed') {
          // Try to reconnect if we were in a bad state
          reconnectCount = 0;
          connect();
        }
      }
    });

    // Handle iOS-specific page lifecycle events
    // These may fire in addition to visibilitychange
    document.addEventListener('freeze', () => {
      console.log('Page freeze event');
      suspend();
    });

    document.addEventListener('resume', () => {
      console.log('Page resume event');
      resume();
    });

    // Handle network changes
    window.addEventListener('online', () => {
      console.log('Network online');
      if (state === 'failed' || state === 'suspended') {
        reconnectCount = 0;
        connect();
      }
    });

    window.addEventListener('offline', () => {
      console.log('Network offline');
      // Don't immediately disconnect - let heartbeat detect the issue
    });
  }

  // Initialize visibility handler
  setupVisibilityHandler();

  return {
    connect,
    disconnect,
    send,
    getState,
    setTerminalDimensions,
    getStreamState,
    getConnectionInfo,
    requestResync,
    negotiateDimensions,
    retry,
    suspend,
    resume,
  };
}
