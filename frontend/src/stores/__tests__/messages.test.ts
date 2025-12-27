import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  getMessagesForSession,
  addMessage,
  addSystemMessage,
  setSessionMessages,
  appendToStreamingMessage,
  finalizeStreamingMessage,
  getStreamingContent,
  addToolCall,
  updateToolCallResult,
  clearSessionMessages,
  handleChatEvent,
  handleChatHistory,
  handleSubagentStarted,
  handleSubagentStopped,
  handleToolError,
  handleContextCompacting,
  handlePermissionRequest,
  type Message,
  type ChatEvent,
  type ChatMessage,
} from '../messages';

describe('Messages Store', () => {
  beforeEach(() => {
    // Clear localStorage and reset state
    localStorage.clear();
    // Clear any session messages - we use unique session IDs per test
  });

  // ==================== Basic Message Operations ====================

  describe('getMessagesForSession', () => {
    it('returns empty array for unknown session', () => {
      const sessionId = 'unknown-session-id';
      const messages = getMessagesForSession(sessionId);
      expect(messages).toEqual([]);
    });

    it('returns messages after adding them', () => {
      const sessionId = 'test-session-1';
      const message: Message = {
        id: 'msg-1',
        role: 'user',
        content: 'Hello',
        timestamp: Date.now(),
      };

      addMessage(sessionId, message);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].content).toBe('Hello');
    });

    it('loads from localStorage on first access', () => {
      const sessionId = 'storage-test-session';
      const storedMessages: Message[] = [
        { id: 'stored-1', role: 'user', content: 'Stored message', timestamp: 1234567890 },
      ];
      localStorage.setItem(`clauset_messages_${sessionId}`, JSON.stringify(storedMessages));

      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].content).toBe('Stored message');
    });
  });

  describe('addMessage', () => {
    it('adds a user message', () => {
      const sessionId = 'add-user-session';
      const message: Message = {
        id: 'user-msg-1',
        role: 'user',
        content: 'Test prompt',
        timestamp: Date.now(),
      };

      addMessage(sessionId, message);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].role).toBe('user');
    });

    it('adds an assistant message', () => {
      const sessionId = 'add-assistant-session';
      const message: Message = {
        id: 'assistant-msg-1',
        role: 'assistant',
        content: 'I can help with that.',
        timestamp: Date.now(),
        isStreaming: true,
      };

      addMessage(sessionId, message);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].role).toBe('assistant');
      expect(messages[0].isStreaming).toBe(true);
    });

    it('preserves message order', () => {
      const sessionId = 'order-session';
      addMessage(sessionId, { id: '1', role: 'user', content: 'First', timestamp: 1 });
      addMessage(sessionId, { id: '2', role: 'assistant', content: 'Second', timestamp: 2 });
      addMessage(sessionId, { id: '3', role: 'user', content: 'Third', timestamp: 3 });

      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(3);
      expect(messages[0].content).toBe('First');
      expect(messages[1].content).toBe('Second');
      expect(messages[2].content).toBe('Third');
    });

    it('includes tool calls when present', () => {
      const sessionId = 'tool-calls-session';
      const message: Message = {
        id: 'msg-with-tools',
        role: 'assistant',
        content: 'Let me read that file.',
        timestamp: Date.now(),
        toolCalls: [
          { id: 'tc-1', name: 'Read', input: { path: '/test.txt' } },
        ],
      };

      addMessage(sessionId, message);
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].toolCalls).toHaveLength(1);
      expect(messages[0].toolCalls![0].name).toBe('Read');
    });
  });

  // ==================== System Message Tests ====================

  describe('addSystemMessage', () => {
    it('creates system message with unique id', () => {
      const sessionId = 'system-msg-session';

      addSystemMessage(sessionId, 'subagent_started', 'Agent started', { agentId: 'a1' });
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].role).toBe('system');
      expect(messages[0].id).toMatch(/^system-/);
      expect(messages[0].systemType).toBe('subagent_started');
      expect(messages[0].metadata?.agentId).toBe('a1');
    });

    it('generates unique ids for multiple system messages', () => {
      const sessionId = 'multi-system-session';

      addSystemMessage(sessionId, 'tool_error', 'Error 1');
      addSystemMessage(sessionId, 'tool_error', 'Error 2');
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].id).not.toBe(messages[1].id);
    });
  });

  describe('handleSubagentStarted', () => {
    it('creates subagent started message', () => {
      const sessionId = 'subagent-start-session';

      handleSubagentStarted(sessionId, 'agent-123', 'general-purpose');
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].systemType).toBe('subagent_started');
      expect(messages[0].content).toBe('Agent started');
      expect(messages[0].metadata?.agentId).toBe('agent-123');
    });

    it('preserves custom agent type in content', () => {
      const sessionId = 'custom-agent-session';

      handleSubagentStarted(sessionId, 'agent-456', 'code-reviewer');
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].content).toBe('code-reviewer started');
    });
  });

  describe('handleSubagentStopped', () => {
    it('creates subagent stopped message', () => {
      const sessionId = 'subagent-stop-session';

      handleSubagentStopped(sessionId, 'agent-789');
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(1);
      expect(messages[0].systemType).toBe('subagent_stopped');
      expect(messages[0].content).toBe('Subagent completed');
    });
  });

  describe('handleToolError', () => {
    it('creates tool error message', () => {
      const sessionId = 'tool-error-session';

      handleToolError(sessionId, 'Read', 'File not found', false);
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].systemType).toBe('tool_error');
      expect(messages[0].content).toContain('Read failed');
      expect(messages[0].metadata?.isTimeout).toBe(false);
    });

    it('formats timeout errors differently', () => {
      const sessionId = 'timeout-session';

      handleToolError(sessionId, 'Bash', 'Timeout', true);
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].content).toBe('Bash timed out');
      expect(messages[0].metadata?.isTimeout).toBe(true);
    });
  });

  describe('handleContextCompacting', () => {
    it('creates auto-compact message', () => {
      const sessionId = 'auto-compact-session';

      handleContextCompacting(sessionId, 'auto');
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].systemType).toBe('context_compacting');
      expect(messages[0].content).toContain('automatically');
    });

    it('creates manual compact message', () => {
      const sessionId = 'manual-compact-session';

      handleContextCompacting(sessionId, 'manual');
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].content).toBe('Compacting context...');
    });
  });

  describe('handlePermissionRequest', () => {
    it('creates permission request message', () => {
      const sessionId = 'permission-session';

      handlePermissionRequest(sessionId, 'Bash', { command: 'rm -rf /' });
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].systemType).toBe('permission_request');
      expect(messages[0].content).toBe('Permission required: Bash');
      expect(messages[0].metadata?.toolName).toBe('Bash');
    });
  });

  // ==================== Session Management Tests ====================

  describe('setSessionMessages', () => {
    it('replaces all messages for session', () => {
      const sessionId = 'replace-session';

      // Add initial messages
      addMessage(sessionId, { id: '1', role: 'user', content: 'Old', timestamp: 1 });

      // Replace with new messages
      setSessionMessages(sessionId, [
        { id: '2', role: 'user', content: 'New 1', timestamp: 2 },
        { id: '3', role: 'assistant', content: 'New 2', timestamp: 3 },
      ]);

      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(2);
      expect(messages[0].content).toBe('New 1');
    });

    it('persists to localStorage', async () => {
      const sessionId = 'persist-session';
      setSessionMessages(sessionId, [
        { id: '1', role: 'user', content: 'Persisted', timestamp: 1 },
      ]);

      // Wait for debounced save
      await new Promise((resolve) => setTimeout(resolve, 600));

      const stored = localStorage.getItem(`clauset_messages_${sessionId}`);
      expect(stored).not.toBeNull();
      expect(stored).toContain('Persisted');
    });
  });

  describe('clearSessionMessages', () => {
    it('removes messages from memory', () => {
      const sessionId = 'clear-session';
      addMessage(sessionId, { id: '1', role: 'user', content: 'Test', timestamp: 1 });

      clearSessionMessages(sessionId);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toEqual([]);
    });

    it('removes messages from localStorage', async () => {
      const sessionId = 'clear-storage-session';
      setSessionMessages(sessionId, [{ id: '1', role: 'user', content: 'Test', timestamp: 1 }]);
      await new Promise((resolve) => setTimeout(resolve, 600));

      clearSessionMessages(sessionId);

      const stored = localStorage.getItem(`clauset_messages_${sessionId}`);
      expect(stored).toBeNull();
    });
  });

  // ==================== Streaming Message Tests ====================

  describe('streaming messages', () => {
    it('accumulates streaming content', () => {
      const sessionId = 'stream-session';
      const messageId = 'stream-msg-1';

      appendToStreamingMessage(sessionId, messageId, 'Hello ');
      appendToStreamingMessage(sessionId, messageId, 'World');

      const content = getStreamingContent(sessionId, messageId);
      expect(content).toBe('Hello World');
    });

    it('returns empty string for non-existent stream', () => {
      const content = getStreamingContent('nonexistent', 'nonexistent');
      expect(content).toBe('');
    });

    it('finalizes streaming message', () => {
      const sessionId = 'finalize-session';
      const messageId = 'finalize-msg';

      appendToStreamingMessage(sessionId, messageId, 'Complete message');
      finalizeStreamingMessage(sessionId, messageId);

      const messages = getMessagesForSession(sessionId);
      expect(messages).toHaveLength(1);
      expect(messages[0].content).toBe('Complete message');
      expect(messages[0].isStreaming).toBe(false);

      // Streaming content should be cleared
      const streamContent = getStreamingContent(sessionId, messageId);
      expect(streamContent).toBe('');
    });
  });

  // ==================== Tool Call Tests ====================

  describe('addToolCall', () => {
    it('adds tool call to last message', () => {
      const sessionId = 'add-tool-session';
      const messageId = 'tool-msg';

      addMessage(sessionId, {
        id: messageId,
        role: 'assistant',
        content: 'Let me help.',
        timestamp: Date.now(),
      });

      addToolCall(sessionId, messageId, {
        id: 'tc-1',
        name: 'Read',
        input: { path: '/test.txt' },
      });

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls).toHaveLength(1);
      expect(messages[0].toolCalls![0].name).toBe('Read');
    });

    it('adds multiple tool calls', () => {
      const sessionId = 'multi-tool-session';
      const messageId = 'multi-tool-msg';

      addMessage(sessionId, {
        id: messageId,
        role: 'assistant',
        content: 'I need to read and write.',
        timestamp: Date.now(),
      });

      addToolCall(sessionId, messageId, { id: 'tc-1', name: 'Read', input: {} });
      addToolCall(sessionId, messageId, { id: 'tc-2', name: 'Write', input: {} });

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls).toHaveLength(2);
    });

    it('ignores if message id does not match last message', () => {
      const sessionId = 'wrong-id-session';

      addMessage(sessionId, { id: 'msg-1', role: 'user', content: 'Test', timestamp: 1 });
      addMessage(sessionId, { id: 'msg-2', role: 'assistant', content: 'Response', timestamp: 2 });

      // Try to add tool call to wrong message
      addToolCall(sessionId, 'msg-1', { id: 'tc-1', name: 'Read', input: {} });

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls).toBeUndefined();
      expect(messages[1].toolCalls).toBeUndefined();
    });
  });

  describe('updateToolCallResult', () => {
    it('updates tool call with output', () => {
      const sessionId = 'update-tool-session';
      const messageId = 'update-tool-msg';

      addMessage(sessionId, {
        id: messageId,
        role: 'assistant',
        content: 'Reading file',
        timestamp: Date.now(),
        toolCalls: [{ id: 'tc-1', name: 'Read', input: {} }],
      });

      updateToolCallResult(sessionId, 'tc-1', 'File contents here', false);

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls![0].output).toBe('File contents here');
      expect(messages[0].toolCalls![0].isError).toBe(false);
    });

    it('marks tool call as error', () => {
      const sessionId = 'error-tool-session';

      addMessage(sessionId, {
        id: 'msg',
        role: 'assistant',
        content: 'Trying to read',
        timestamp: Date.now(),
        toolCalls: [{ id: 'tc-1', name: 'Read', input: {} }],
      });

      updateToolCallResult(sessionId, 'tc-1', 'File not found', true);

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls![0].isError).toBe(true);
    });

    it('updates correct tool call among multiple', () => {
      const sessionId = 'multi-update-session';

      addMessage(sessionId, {
        id: 'msg',
        role: 'assistant',
        content: 'Multiple tools',
        timestamp: Date.now(),
        toolCalls: [
          { id: 'tc-1', name: 'Read', input: {} },
          { id: 'tc-2', name: 'Write', input: {} },
        ],
      });

      updateToolCallResult(sessionId, 'tc-2', 'Write success', false);

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].toolCalls![0].output).toBeUndefined();
      expect(messages[0].toolCalls![1].output).toBe('Write success');
    });
  });

  // ==================== ChatEvent Handler Tests ====================

  describe('handleChatEvent', () => {
    describe('message event', () => {
      it('adds new message from event', () => {
        const sessionId = 'event-msg-session';
        const event: ChatEvent = {
          type: 'message',
          session_id: sessionId,
          message: {
            id: 'chat-msg-1',
            session_id: sessionId,
            role: 'user',
            content: 'Hello from event',
            tool_calls: [],
            is_streaming: false,
            is_complete: true,
            timestamp: Date.now(),
          },
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages).toHaveLength(1);
        expect(messages[0].content).toBe('Hello from event');
      });

      it('converts snake_case to camelCase', () => {
        const sessionId = 'snake-case-session';
        const event: ChatEvent = {
          type: 'message',
          session_id: sessionId,
          message: {
            id: 'msg',
            session_id: sessionId,
            role: 'assistant',
            content: 'Response',
            tool_calls: [
              {
                id: 'tc-1',
                name: 'Read',
                input: {},
                is_error: false,
                is_complete: true,
              },
            ],
            is_streaming: true,
            is_complete: false,
            timestamp: Date.now(),
          },
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages[0].isStreaming).toBe(true);
        expect(messages[0].toolCalls![0].isError).toBe(false);
      });
    });

    describe('content_delta event', () => {
      it('appends delta to existing message', () => {
        const sessionId = 'delta-session';
        const messageId = 'delta-msg';

        // First add the message
        addMessage(sessionId, {
          id: messageId,
          role: 'assistant',
          content: 'Initial',
          timestamp: Date.now(),
          isStreaming: true,
        });

        // Then send delta
        const event: ChatEvent = {
          type: 'content_delta',
          session_id: sessionId,
          message_id: messageId,
          delta: ' content',
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages[0].content).toBe('Initial content');
      });

      it('handles multiple deltas', () => {
        const sessionId = 'multi-delta-session';
        const messageId = 'multi-delta-msg';

        addMessage(sessionId, {
          id: messageId,
          role: 'assistant',
          content: '',
          timestamp: Date.now(),
          isStreaming: true,
        });

        handleChatEvent({ type: 'content_delta', session_id: sessionId, message_id: messageId, delta: 'Hello' });
        handleChatEvent({ type: 'content_delta', session_id: sessionId, message_id: messageId, delta: ' ' });
        handleChatEvent({ type: 'content_delta', session_id: sessionId, message_id: messageId, delta: 'World' });

        const messages = getMessagesForSession(sessionId);
        expect(messages[0].content).toBe('Hello World');
      });
    });

    describe('tool_call_start event', () => {
      it('adds tool call to message', () => {
        const sessionId = 'tool-start-session';
        const messageId = 'tool-start-msg';

        addMessage(sessionId, {
          id: messageId,
          role: 'assistant',
          content: 'I will read the file.',
          timestamp: Date.now(),
        });

        const event: ChatEvent = {
          type: 'tool_call_start',
          session_id: sessionId,
          message_id: messageId,
          tool_call: {
            id: 'tc-new',
            name: 'Read',
            input: { path: '/test.txt' },
            is_error: false,
            is_complete: false,
          },
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages[0].toolCalls).toHaveLength(1);
        expect(messages[0].toolCalls![0].name).toBe('Read');
      });
    });

    describe('tool_call_complete event', () => {
      it('updates tool call result', () => {
        const sessionId = 'tool-complete-session';
        const messageId = 'tool-complete-msg';

        addMessage(sessionId, {
          id: messageId,
          role: 'assistant',
          content: 'Reading...',
          timestamp: Date.now(),
          toolCalls: [{ id: 'tc-pending', name: 'Read', input: {} }],
        });

        const event: ChatEvent = {
          type: 'tool_call_complete',
          session_id: sessionId,
          message_id: messageId,
          tool_call_id: 'tc-pending',
          output: 'File contents',
          is_error: false,
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages[0].toolCalls![0].output).toBe('File contents');
      });
    });

    describe('message_complete event', () => {
      it('marks message as no longer streaming', () => {
        const sessionId = 'complete-session';
        const messageId = 'complete-msg';

        addMessage(sessionId, {
          id: messageId,
          role: 'assistant',
          content: 'Response',
          timestamp: Date.now(),
          isStreaming: true,
        });

        const event: ChatEvent = {
          type: 'message_complete',
          session_id: sessionId,
          message_id: messageId,
        };

        handleChatEvent(event);
        const messages = getMessagesForSession(sessionId);

        expect(messages[0].isStreaming).toBe(false);
      });
    });
  });

  describe('handleChatHistory', () => {
    it('replaces messages with backend history', () => {
      const sessionId = 'history-session';

      // Add some local messages
      addMessage(sessionId, { id: 'local-1', role: 'user', content: 'Local', timestamp: 1 });

      // Receive history from backend
      const chatMessages: ChatMessage[] = [
        {
          id: 'backend-1',
          session_id: sessionId,
          role: 'user',
          content: 'Backend message 1',
          tool_calls: [],
          is_streaming: false,
          is_complete: true,
          timestamp: 100,
        },
        {
          id: 'backend-2',
          session_id: sessionId,
          role: 'assistant',
          content: 'Backend response',
          tool_calls: [],
          is_streaming: false,
          is_complete: true,
          timestamp: 101,
        },
      ];

      handleChatHistory(sessionId, chatMessages);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toHaveLength(2);
      expect(messages[0].content).toBe('Backend message 1');
      expect(messages[1].content).toBe('Backend response');
    });

    it('handles empty history', () => {
      const sessionId = 'empty-history-session';

      handleChatHistory(sessionId, []);
      const messages = getMessagesForSession(sessionId);

      expect(messages).toEqual([]);
    });

    it('converts tool calls correctly', () => {
      const sessionId = 'history-tools-session';

      const chatMessages: ChatMessage[] = [
        {
          id: 'msg-with-tools',
          session_id: sessionId,
          role: 'assistant',
          content: 'Using tools',
          tool_calls: [
            {
              id: 'tc-1',
              name: 'Read',
              input: { path: '/file.txt' },
              output: 'contents',
              is_error: false,
              is_complete: true,
            },
          ],
          is_streaming: false,
          is_complete: true,
          timestamp: 100,
        },
      ];

      handleChatHistory(sessionId, chatMessages);
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].toolCalls).toHaveLength(1);
      expect(messages[0].toolCalls![0].name).toBe('Read');
      expect(messages[0].toolCalls![0].output).toBe('contents');
      expect(messages[0].toolCalls![0].isError).toBe(false);
    });
  });

  // ==================== Edge Cases ====================

  describe('edge cases', () => {
    it('handles malformed localStorage data', () => {
      const sessionId = 'malformed-session';
      localStorage.setItem(`clauset_messages_${sessionId}`, 'not valid json');

      const messages = getMessagesForSession(sessionId);
      expect(messages).toEqual([]);
    });

    it('handles session isolation', () => {
      const session1 = 'session-1';
      const session2 = 'session-2';

      addMessage(session1, { id: '1', role: 'user', content: 'Session 1', timestamp: 1 });
      addMessage(session2, { id: '2', role: 'user', content: 'Session 2', timestamp: 2 });

      expect(getMessagesForSession(session1)[0].content).toBe('Session 1');
      expect(getMessagesForSession(session2)[0].content).toBe('Session 2');

      clearSessionMessages(session1);
      expect(getMessagesForSession(session1)).toEqual([]);
      expect(getMessagesForSession(session2)).toHaveLength(1);
    });

    it('handles messages with all optional fields', () => {
      const sessionId = 'full-msg-session';
      const message: Message = {
        id: 'full-msg',
        role: 'assistant',
        content: 'Full message',
        timestamp: Date.now(),
        isStreaming: true,
        toolCalls: [
          { id: 'tc', name: 'Read', input: {}, output: 'output', isError: false },
        ],
        systemType: undefined, // Not a system message
        metadata: undefined,
      };

      addMessage(sessionId, message);
      const messages = getMessagesForSession(sessionId);

      expect(messages[0].toolCalls).toHaveLength(1);
      expect(messages[0].isStreaming).toBe(true);
    });

    it('handles empty content messages', () => {
      const sessionId = 'empty-content-session';

      addMessage(sessionId, {
        id: 'empty',
        role: 'assistant',
        content: '',
        timestamp: Date.now(),
        isStreaming: true,
      });

      const messages = getMessagesForSession(sessionId);
      expect(messages[0].content).toBe('');
    });
  });
});
