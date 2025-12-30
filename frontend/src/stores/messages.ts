import { createSignal } from 'solid-js';
import { formatPermissionMode, type PermissionMode } from '../lib/format';

// localStorage constants
const STORAGE_KEY_PREFIX = 'clauset_messages_';
const MAX_STORAGE_SIZE = 500000; // 500KB per session

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  /** Claude's thinking/reasoning content (displayed separately from response) */
  thinkingContent?: string;
  toolCalls?: ToolCall[];
  timestamp: number;
  isStreaming?: boolean;
  /** System event type for system messages */
  systemType?: 'subagent_started' | 'subagent_stopped' | 'subagent_completed' | 'tool_error' | 'context_compacting' | 'permission_request' | 'mode_change';
  /** Additional metadata for system messages */
  metadata?: {
    agentId?: string;
    agentType?: string;
    description?: string;
    toolName?: string;
    toolInput?: unknown;
    error?: string;
    isTimeout?: boolean;
    trigger?: string;
    previousMode?: PermissionMode;
    newMode?: PermissionMode;
  };
  /** Whether a permission request has been responded to */
  responded?: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  input: unknown;
  output?: string;
  isError?: boolean;
}

const [messages, setMessages] = createSignal<Map<string, Message[]>>(new Map());
const [streamingMessage, setStreamingMessage] = createSignal<Map<string, string>>(new Map());

// Debounce localStorage saves
const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();

function saveToStorage(sessionId: string, msgs: Message[]) {
  // Clear any pending save for this session
  const existingTimeout = saveTimeouts.get(sessionId);
  if (existingTimeout) {
    clearTimeout(existingTimeout);
  }

  // Debounce saves by 500ms
  const timeout = setTimeout(() => {
    try {
      const json = JSON.stringify(msgs);
      if (json.length <= MAX_STORAGE_SIZE) {
        localStorage.setItem(STORAGE_KEY_PREFIX + sessionId, json);
      } else {
        // If too large, keep only the most recent messages
        const truncated = msgs.slice(-50);
        const truncatedJson = JSON.stringify(truncated);
        if (truncatedJson.length <= MAX_STORAGE_SIZE) {
          localStorage.setItem(STORAGE_KEY_PREFIX + sessionId, truncatedJson);
        }
      }
    } catch (e) {
      console.warn('[messages] Failed to save to localStorage:', e);
    }
    saveTimeouts.delete(sessionId);
  }, 500);

  saveTimeouts.set(sessionId, timeout);
}

function loadFromStorage(sessionId: string): Message[] | null {
  try {
    const json = localStorage.getItem(STORAGE_KEY_PREFIX + sessionId);
    if (json) {
      return JSON.parse(json) as Message[];
    }
  } catch (e) {
    console.warn('[messages] Failed to load from localStorage:', e);
  }
  return null;
}

function clearFromStorage(sessionId: string) {
  try {
    localStorage.removeItem(STORAGE_KEY_PREFIX + sessionId);
  } catch (e) {
    console.warn('[messages] Failed to clear from localStorage:', e);
  }
}

export function getMessagesForSession(sessionId: string): Message[] {
  // First check memory
  const inMemory = messages().get(sessionId);
  if (inMemory !== undefined) {
    return inMemory;
  }

  // Fall back to localStorage
  const fromStorage = loadFromStorage(sessionId);
  if (fromStorage) {
    // Load into memory
    setMessages((prev) => {
      const newMap = new Map(prev);
      newMap.set(sessionId, fromStorage);
      return newMap;
    });
    return fromStorage;
  }

  return [];
}

export function addMessage(sessionId: string, message: Message) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    const sessionMessages = [...(newMap.get(sessionId) ?? []), message];
    newMap.set(sessionId, sessionMessages);
    saveToStorage(sessionId, sessionMessages);
    return newMap;
  });
}

/**
 * Add a user message to the chat immediately.
 * This ensures user input (including slash commands) always appears in chat.
 */
export function addUserMessage(sessionId: string, content: string) {
  const message: Message = {
    id: `user-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    role: 'user',
    content,
    timestamp: Date.now(),
  };
  addMessage(sessionId, message);
}

/**
 * Add a system event message to the chat.
 */
export function addSystemMessage(
  sessionId: string,
  systemType: Message['systemType'],
  content: string,
  metadata?: Message['metadata']
) {
  const message: Message = {
    id: `system-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    role: 'system',
    content,
    timestamp: Date.now(),
    systemType,
    metadata,
  };
  addMessage(sessionId, message);
}

/**
 * Handle subagent started event.
 */
export function handleSubagentStarted(sessionId: string, agentId: string, agentType: string) {
  const typeLabel = agentType === 'general-purpose' ? 'Agent' : agentType;
  addSystemMessage(
    sessionId,
    'subagent_started',
    `${typeLabel} started`,
    { agentId, agentType }
  );
}

/**
 * Handle subagent stopped event.
 * Note: This is now mostly superseded by handleSubagentCompleted which provides more detail.
 */
export function handleSubagentStopped(_sessionId: string, _agentId: string) {
  // Don't add a message here - wait for the subagent_completed event with details
  // The subagent_completed event provides the actual output from the Task tool
}

/**
 * Handle subagent completed event with detailed output.
 * This creates an assistant-style message showing what the subagent accomplished.
 */
export function handleSubagentCompleted(
  sessionId: string,
  agentType: string,
  description: string,
  result: string
) {
  // Create an assistant message with subagent details
  // The content will be rendered in italics by MessageBubble
  const message: Message = {
    id: `subagent-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    role: 'assistant',
    content: result,
    timestamp: Date.now(),
    systemType: 'subagent_completed',
    metadata: {
      agentType,
      description,
    },
  };
  addMessage(sessionId, message);
}

/**
 * Handle tool error event.
 */
export function handleToolError(sessionId: string, toolName: string, error: string, isTimeout: boolean) {
  const content = isTimeout
    ? `${toolName} timed out`
    : `${toolName} failed: ${error}`;
  addSystemMessage(
    sessionId,
    'tool_error',
    content,
    { toolName, error, isTimeout }
  );
}

/**
 * Handle context compacting event.
 */
export function handleContextCompacting(sessionId: string, trigger: string) {
  const content = trigger === 'auto'
    ? 'Context automatically compacting...'
    : 'Compacting context...';
  addSystemMessage(
    sessionId,
    'context_compacting',
    content,
    { trigger }
  );
}

/**
 * Handle permission request event.
 */
export function handlePermissionRequest(sessionId: string, toolName: string, toolInput: unknown) {
  addSystemMessage(
    sessionId,
    'permission_request',
    `Permission required: ${toolName}`,
    { toolName, toolInput }
  );
}

/**
 * Handle permission mode changes.
 */
export function handleModeChange(
  sessionId: string,
  previousMode: PermissionMode,
  newMode: PermissionMode
) {
  if (previousMode === newMode) return;
  addSystemMessage(
    sessionId,
    'mode_change',
    `Mode changed: ${formatPermissionMode(previousMode)} -> ${formatPermissionMode(newMode)}`,
    { previousMode, newMode }
  );
}

/**
 * Mark a permission request message as responded.
 */
export function markPermissionResponded(sessionId: string, messageId: string) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    const sessionMessages = newMap.get(sessionId) ?? [];

    const updatedMessages = sessionMessages.map((msg) => {
      if (msg.id === messageId && msg.systemType === 'permission_request') {
        return { ...msg, responded: true };
      }
      return msg;
    });

    newMap.set(sessionId, updatedMessages);
    saveToStorage(sessionId, updatedMessages);
    return newMap;
  });
}

export function setSessionMessages(sessionId: string, msgs: Message[]) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    newMap.set(sessionId, msgs);
    saveToStorage(sessionId, msgs);
    return newMap;
  });
}

export function appendToStreamingMessage(sessionId: string, messageId: string, delta: string) {
  setStreamingMessage((prev) => {
    const newMap = new Map(prev);
    const key = `${sessionId}:${messageId}`;
    const current = newMap.get(key) ?? '';
    newMap.set(key, current + delta);
    return newMap;
  });
}

export function finalizeStreamingMessage(sessionId: string, messageId: string) {
  const key = `${sessionId}:${messageId}`;
  const content = streamingMessage().get(key) ?? '';

  if (content) {
    addMessage(sessionId, {
      id: messageId,
      role: 'assistant',
      content,
      timestamp: Date.now(),
      isStreaming: false,
    });

    setStreamingMessage((prev) => {
      const newMap = new Map(prev);
      newMap.delete(key);
      return newMap;
    });
  }
}

export function getStreamingContent(sessionId: string, messageId: string): string {
  return streamingMessage().get(`${sessionId}:${messageId}`) ?? '';
}

export function addToolCall(sessionId: string, messageId: string, toolCall: ToolCall) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    const sessionMessages = newMap.get(sessionId) ?? [];
    const lastMessage = sessionMessages[sessionMessages.length - 1];

    if (lastMessage && lastMessage.id === messageId) {
      const updatedMessage = {
        ...lastMessage,
        toolCalls: [...(lastMessage.toolCalls ?? []), toolCall],
      };
      const updatedMessages = [...sessionMessages.slice(0, -1), updatedMessage];
      newMap.set(sessionId, updatedMessages);
      saveToStorage(sessionId, updatedMessages);
    }

    return newMap;
  });
}

export function updateToolCallResult(sessionId: string, toolCallId: string, output: string, isError: boolean) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    const sessionMessages = newMap.get(sessionId) ?? [];

    const updatedMessages = sessionMessages.map((msg) => {
      if (!msg.toolCalls) return msg;

      const updatedToolCalls = msg.toolCalls.map((tc) =>
        tc.id === toolCallId ? { ...tc, output, isError } : tc
      );

      return { ...msg, toolCalls: updatedToolCalls };
    });

    newMap.set(sessionId, updatedMessages);
    saveToStorage(sessionId, updatedMessages);
    return newMap;
  });
}

export function clearSessionMessages(sessionId: string) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    newMap.delete(sessionId);
    return newMap;
  });
  clearFromStorage(sessionId);
}

// ChatEvent types matching backend
export interface ChatMessage {
  id: string;
  session_id: string;
  role: 'user' | 'assistant';
  content: string;
  thinking_content?: string;
  tool_calls: ChatToolCall[];
  is_streaming: boolean;
  is_complete: boolean;
  timestamp: number;
}

export interface ChatToolCall {
  id: string;
  name: string;
  input: unknown;
  output?: string;
  is_error: boolean;
  is_complete: boolean;
}

export type ChatEvent =
  | { type: 'message'; session_id: string; message: ChatMessage }
  | { type: 'content_delta'; session_id: string; message_id: string; delta: string }
  | { type: 'thinking_delta'; session_id: string; message_id: string; delta: string }
  | { type: 'tool_call_start'; session_id: string; message_id: string; tool_call: ChatToolCall }
  | { type: 'tool_call_complete'; session_id: string; message_id: string; tool_call_id: string; output: string; is_error: boolean }
  | { type: 'message_complete'; session_id: string; message_id: string };

/**
 * Convert a ChatMessage from the backend to our Message format.
 */
function convertChatMessage(msg: ChatMessage): Message {
  return {
    id: msg.id,
    role: msg.role,
    content: msg.content,
    thinkingContent: msg.thinking_content,
    toolCalls: msg.tool_calls.map((tc) => ({
      id: tc.id,
      name: tc.name,
      input: tc.input,
      output: tc.output,
      isError: tc.is_error,
    })),
    timestamp: msg.timestamp,
    isStreaming: msg.is_streaming,
  };
}

/**
 * Handle chat history from the backend.
 * This merges with existing messages to prevent data loss from race conditions.
 *
 * IMPORTANT: Don't blindly replace messages with server data because:
 * 1. Server may return empty array before hooks have been processed
 * 2. localStorage may have newer messages from real-time events
 * 3. Race condition between SyncResponse and hook events
 */
export function handleChatHistory(sessionId: string, chatMessages: ChatMessage[]) {
  console.log('[messages] Received chat history:', chatMessages.length, 'messages');

  // Get existing messages from memory or localStorage
  const existingInMemory = messages().get(sessionId);
  const existingFromStorage = existingInMemory === undefined ? loadFromStorage(sessionId) : null;
  const existingMessages = existingInMemory ?? existingFromStorage ?? [];

  // If server returns empty but we have existing messages, DON'T overwrite
  // This prevents the race condition where RequestChatHistory arrives before hooks are processed
  if (chatMessages.length === 0 && existingMessages.length > 0) {
    console.log('[messages] Server returned empty history but we have', existingMessages.length, 'existing messages - keeping existing');
    // Make sure in-memory state is populated from localStorage if needed
    if (existingInMemory === undefined && existingFromStorage) {
      setMessages((prev) => {
        const newMap = new Map(prev);
        newMap.set(sessionId, existingFromStorage);
        return newMap;
      });
    }
    return;
  }

  // If server has messages, use server as source of truth
  // But merge with any messages we've received via real-time events
  const serverMessages = chatMessages.map(convertChatMessage);

  // Build a set of server message IDs for deduplication
  const serverMessageIds = new Set(serverMessages.map(m => m.id));

  // Keep any real-time messages that aren't in server data (added since last save)
  // These are typically messages added via handleChatEvent during the current session
  const realtimeMessages = existingMessages.filter(m => {
    // Skip if ID matches server data
    if (serverMessageIds.has(m.id)) return false;

    // For user messages, also check content-based duplicates
    // User messages added locally may have different IDs than server messages
    if (m.role === 'user') {
      const isDuplicateContent = serverMessages.some(
        sm => sm.role === 'user' && sm.content === m.content
      );
      if (isDuplicateContent) {
        console.log('[messages] Filtering duplicate user message by content:', m.content.slice(0, 50));
        return false;
      }
    }

    // Only keep messages that were added recently (within last minute)
    // This preserves messages from real-time events that haven't been persisted to server yet
    const isRecent = Date.now() - m.timestamp < 60000; // 1 minute
    return isRecent;
  });

  // Merge: server messages first (historical), then recent real-time messages
  const mergedMessages = [...serverMessages, ...realtimeMessages];

  // Sort by timestamp to maintain chronological order
  mergedMessages.sort((a, b) => a.timestamp - b.timestamp);

  console.log('[messages] Merged:', serverMessages.length, 'server +', realtimeMessages.length, 'realtime =', mergedMessages.length, 'total');
  setSessionMessages(sessionId, mergedMessages);
}

/**
 * Handle a ChatEvent from the WebSocket.
 * Updates the messages store based on the event type.
 */
export function handleChatEvent(event: ChatEvent) {
  console.log('[ChatEvent]', event.type, event);
  switch (event.type) {
    case 'message': {
      const msg = event.message;
      console.log('[ChatEvent] Adding message:', msg.role, msg.id, 'streaming:', msg.is_streaming);

      // Deduplicate user messages - they may have been added locally already
      // Check if a recent message with same role and content exists
      if (msg.role === 'user') {
        const existing = messages().get(msg.session_id) ?? [];
        const recentDuplicate = existing.some(
          (m) =>
            m.role === 'user' &&
            m.content === msg.content &&
            Math.abs(m.timestamp - msg.timestamp) < 5000 // Within 5 seconds
        );
        if (recentDuplicate) {
          console.log('[ChatEvent] Skipping duplicate user message');
          break;
        }
      }

      const message = convertChatMessage(msg);
      addMessage(msg.session_id, message);
      break;
    }

    case 'content_delta': {
      // Update the message content with the delta
      setMessages((prev) => {
        const newMap = new Map(prev);
        const sessionMessages = newMap.get(event.session_id) ?? [];
        const updatedMessages = sessionMessages.map((msg) => {
          if (msg.id === event.message_id) {
            return { ...msg, content: msg.content + event.delta };
          }
          return msg;
        });
        newMap.set(event.session_id, updatedMessages);
        // Save after content delta (debounced)
        saveToStorage(event.session_id, updatedMessages);
        return newMap;
      });
      break;
    }

    case 'thinking_delta': {
      // Update the message thinking content with the delta
      setMessages((prev) => {
        const newMap = new Map(prev);
        const sessionMessages = newMap.get(event.session_id) ?? [];
        const updatedMessages = sessionMessages.map((msg) => {
          if (msg.id === event.message_id) {
            return { ...msg, thinkingContent: (msg.thinkingContent ?? '') + event.delta };
          }
          return msg;
        });
        newMap.set(event.session_id, updatedMessages);
        return newMap;
      });
      break;
    }

    case 'tool_call_start': {
      const tc = event.tool_call;
      addToolCall(event.session_id, event.message_id, {
        id: tc.id,
        name: tc.name,
        input: tc.input,
        output: tc.output,
        isError: tc.is_error,
      });
      break;
    }

    case 'tool_call_complete': {
      updateToolCallResult(event.session_id, event.tool_call_id, event.output, event.is_error);
      break;
    }

    case 'message_complete': {
      // Mark the message as no longer streaming
      setMessages((prev) => {
        const newMap = new Map(prev);
        const sessionMessages = newMap.get(event.session_id) ?? [];
        const updatedMessages = sessionMessages.map((msg) => {
          if (msg.id === event.message_id) {
            return { ...msg, isStreaming: false };
          }
          return msg;
        });
        newMap.set(event.session_id, updatedMessages);
        // Save when message completes
        saveToStorage(event.session_id, updatedMessages);
        return newMap;
      });
      break;
    }
  }
}

export { messages, streamingMessage };
