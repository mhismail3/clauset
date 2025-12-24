import { createSignal } from 'solid-js';

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  toolCalls?: ToolCall[];
  timestamp: number;
  isStreaming?: boolean;
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

export function getMessagesForSession(sessionId: string): Message[] {
  return messages().get(sessionId) ?? [];
}

export function addMessage(sessionId: string, message: Message) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    const sessionMessages = [...(newMap.get(sessionId) ?? []), message];
    newMap.set(sessionId, sessionMessages);
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
      newMap.set(sessionId, [...sessionMessages.slice(0, -1), updatedMessage]);
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
    return newMap;
  });
}

export function clearSessionMessages(sessionId: string) {
  setMessages((prev) => {
    const newMap = new Map(prev);
    newMap.delete(sessionId);
    return newMap;
  });
}

// ChatEvent types matching backend
export interface ChatMessage {
  id: string;
  session_id: string;
  role: 'user' | 'assistant';
  content: string;
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
  | { type: 'tool_call_start'; session_id: string; message_id: string; tool_call: ChatToolCall }
  | { type: 'tool_call_complete'; session_id: string; message_id: string; tool_call_id: string; output: string; is_error: boolean }
  | { type: 'message_complete'; session_id: string; message_id: string };

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
      const message: Message = {
        id: msg.id,
        role: msg.role,
        content: msg.content,
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
        return newMap;
      });
      break;
    }
  }
}

export { messages, streamingMessage };
