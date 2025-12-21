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

export { messages, streamingMessage };
