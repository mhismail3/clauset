import { Show, createSignal, onMount, onCleanup, For } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { Spinner } from '../components/ui/Spinner';
import { ConnectionStatus } from '../components/ui/ConnectionStatus';
import { MessageBubble } from '../components/chat/MessageBubble';
import { InputBar } from '../components/chat/InputBar';
import { TerminalView } from '../components/terminal/TerminalView';
import { TimelineView } from '../components/interactions/TimelineView';
import { DiffViewer } from '../components/interactions/DiffViewer';
import { api, Session } from '../lib/api';
import { createWebSocketManager, WsMessage, SyncResponse, ConnectionState } from '../lib/ws';
import { useKeyboard } from '../lib/keyboard';
import { isIOS } from '../lib/fonts';
import {
  getMessagesForSession,
  appendToStreamingMessage,
  finalizeStreamingMessage,
  getStreamingContent,
  addToolCall,
  updateToolCallResult,
  handleChatEvent,
  handleChatHistory,
  handleSubagentStarted,
  handleSubagentStopped,
  handleToolError,
  handleContextCompacting,
  markPermissionResponded,
  type ChatEvent,
  type ChatMessage,
} from '../stores/messages';
import { appendTerminalOutput, clearTerminalHistory } from '../stores/terminal';
import {
  getInteractiveState,
  handleInteractiveEvent,
  clearInteractiveState,
  type InteractiveEvent,
} from '../stores/interactive';
import { InteractiveCarousel } from '../components/interactive/InteractiveCarousel';

// Maximum chunks to queue when terminal is not yet ready (prevents OOM)
const MAX_TERMINAL_QUEUE_CHUNKS = 100;

// Parse Claude's status line: "Model | $Cost | InputK/OutputK | ctx:X%"
interface StatusInfo {
  model: string;
  cost: number;
  inputTokens: number;
  outputTokens: number;
  contextPercent: number;
}

function parseStatusLine(text: string): StatusInfo | null {
  // Look for pattern like: "Opus 4.5 | $0.68 | 29.2K/22.5K | ctx:11%"
  // The status line may have ANSI escape codes, so we need to strip them
  const cleanText = text.replace(/\x1b\[[0-9;]*m/g, '');

  // Match the status line pattern
  const match = cleanText.match(
    /([A-Za-z0-9. ]+)\s*\|\s*\$([0-9.]+)\s*\|\s*([0-9.]+)K?\/([0-9.]+)K?\s*\|\s*ctx:(\d+)%/
  );

  if (!match) return null;

  const [, model, costStr, inputStr, outputStr, ctxStr] = match;

  // Parse token counts - they may be in K format (e.g., "29.2K") or raw numbers
  const parseTokens = (s: string): number => {
    const num = parseFloat(s);
    // If the original text had "K", multiply by 1000
    return Math.round(num * 1000);
  };

  return {
    model: model.trim(),
    cost: parseFloat(costStr),
    inputTokens: parseTokens(inputStr),
    outputTokens: parseTokens(outputStr),
    contextPercent: parseInt(ctxStr, 10),
  };
}

// Get status dot color based on session status and activity
// Green: active and ready, Orange: thinking/working, Gray: stopped
function getStatusDotColor(status: Session['status'], currentStep?: string): string {
  if (status === 'stopped' || status === 'error') {
    return 'var(--color-text-muted)'; // Gray
  }
  if (status === 'active' || status === 'starting' || status === 'waiting_input') {
    // Check if actively working (not just ready)
    if (currentStep && currentStep !== 'Ready' && currentStep.length > 0) {
      return '#c45b37'; // Orange - thinking/working
    }
    return '#2c8f7a'; // Green - ready
  }
  return 'var(--color-text-muted)'; // Gray default
}

// Status dot component
function StatusDot(props: { status: Session['status']; currentStep?: string }) {
  const color = () => getStatusDotColor(props.status, props.currentStep);
  const isActive = () => props.status === 'active' || props.status === 'starting' || props.status === 'waiting_input';
  const isWorking = () => isActive() && props.currentStep && props.currentStep !== 'Ready' && props.currentStep.length > 0;

  return (
    <span
      style={{
        width: '8px',
        height: '8px',
        'border-radius': '50%',
        background: color(),
        'flex-shrink': '0',
        'box-shadow': isWorking() ? `0 0 6px ${color()}` : 'none',
        animation: isWorking() ? 'pulse 1.5s ease-in-out infinite' : 'none',
      }}
    />
  );
}

export default function SessionPage() {
  const params = useParams<{ id: string }>();
  const navigate = useNavigate();

  const [session, setSession] = createSignal<Session | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [wsState, setWsState] = createSignal<ConnectionState>('initial');
  const [connectionInfo, setConnectionInfo] = createSignal({ reconnectAttempt: 0, maxReconnectAttempts: 5, queuedMessageCount: 0 });
  const [currentView, setCurrentView] = createSignal<'chat' | 'terminal' | 'history'>('chat');
  const [currentStreamingId, setCurrentStreamingId] = createSignal<string | null>(null);
  const [diffState, setDiffState] = createSignal<{ interactionId: string; file: string } | null>(null);
  const [terminalData, setTerminalData] = createSignal<Uint8Array[]>([]);
  const [resuming, setResuming] = createSignal(false);
  const [mode, setMode] = createSignal<'normal' | 'plan'>('normal');
  const [isProcessing, setIsProcessing] = createSignal(false);

  // iOS keyboard handling for chat view (follows visualViewport in real-time)
  // offsetTop counters iOS's automatic page scroll when keyboard appears
  const { viewportHeight, offsetTop } = useKeyboard();

  let wsManager: ReturnType<typeof createWebSocketManager> | null = null;
  let messagesEndRef: HTMLDivElement | undefined;
  let terminalWriteFn: ((data: Uint8Array) => void) | null = null;
  let outputBuffer = '';
  let lastStatus: StatusInfo | null = null;
  let statusUpdateTimer: number | null = null;
  let terminalDimensions: { cols: number; rows: number } | null = null;

  function scrollToBottom() {
    messagesEndRef?.scrollIntoView({ behavior: 'smooth' });
  }

  async function loadSession() {
    try {
      const data = await api.sessions.get(params.id);
      setSession(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load session');
    } finally {
      setLoading(false);
    }
  }

  // Handle terminal data from reliable streaming protocol
  function handleTerminalData(bytes: Uint8Array) {
    const sessionId = params.id;
    appendTerminalOutput(sessionId, bytes);

    if (terminalWriteFn) {
      terminalWriteFn(bytes);
    } else {
      // Queue data (capped to prevent OOM if terminal takes too long to mount)
      setTerminalData((prev) => {
        if (prev.length >= MAX_TERMINAL_QUEUE_CHUNKS) {
          return [...prev.slice(-MAX_TERMINAL_QUEUE_CHUNKS + 1), bytes];
        }
        return [...prev, bytes];
      });
    }

    // Try to parse status line from output
    const text = new TextDecoder().decode(bytes);
    outputBuffer += text;
    // Keep buffer size manageable (last 2KB)
    if (outputBuffer.length > 2000) {
      outputBuffer = outputBuffer.slice(-1500);
    }

    const status = parseStatusLine(outputBuffer);
    if (status) {
      // Check if status changed significantly
      const changed = !lastStatus ||
        lastStatus.model !== status.model ||
        Math.abs(lastStatus.cost - status.cost) > 0.001 ||
        lastStatus.inputTokens !== status.inputTokens ||
        lastStatus.outputTokens !== status.outputTokens ||
        lastStatus.contextPercent !== status.contextPercent;

      if (changed) {
        lastStatus = status;

        // Immediately update local session state for responsive UI
        const currentSession = session();
        if (currentSession) {
          setSession({
            ...currentSession,
            model: status.model,
            total_cost_usd: status.cost,
            input_tokens: status.inputTokens,
            output_tokens: status.outputTokens,
            context_percent: status.contextPercent,
          });
        }

        // Debounce backend updates (send at most every 2 seconds)
        if (statusUpdateTimer) {
          clearTimeout(statusUpdateTimer);
        }
        statusUpdateTimer = window.setTimeout(() => {
          if (wsManager && wsState() === 'connected') {
            wsManager.send({
              type: 'status_update',
              model: status.model,
              cost: status.cost,
              input_tokens: status.inputTokens,
              output_tokens: status.outputTokens,
              context_percent: status.contextPercent,
            });
          }
          statusUpdateTimer = null;
        }, 2000);
      }
    }
  }

  // Handle sync response from reliable streaming protocol
  function handleSyncResponse(response: SyncResponse) {
    // Clear localStorage for this session since server buffer is source of truth
    clearTerminalHistory(params.id);

    // The terminal data is already written by the ws manager's onTerminalData callback
    // Just update the output buffer for status parsing
    if (response.full_buffer && response.full_buffer.length > 0) {
      const text = new TextDecoder().decode(new Uint8Array(response.full_buffer));
      outputBuffer = text.slice(-2000); // Keep last 2KB for status parsing
      const status = parseStatusLine(outputBuffer);
      if (status) {
        lastStatus = status;
        const currentSession = session();
        if (currentSession) {
          setSession({
            ...currentSession,
            model: status.model,
            total_cost_usd: status.cost,
            input_tokens: status.inputTokens,
            output_tokens: status.outputTokens,
            context_percent: status.contextPercent,
          });
        }
      }
    }

    // Request chat history from the backend
    // This replaces any localStorage cached messages with the authoritative server data
    wsManager?.send({ type: 'request_chat_history' });
  }

  function handleWsMessage(msg: WsMessage) {
    const sessionId = params.id;

    switch (msg.type) {
      case 'text': {
        const { message_id, content, is_complete } = msg as unknown as { message_id: string; content: string; is_complete: boolean };
        setCurrentStreamingId(message_id);
        appendToStreamingMessage(sessionId, message_id, content);
        if (is_complete) {
          finalizeStreamingMessage(sessionId, message_id);
          setCurrentStreamingId(null);
        }
        scrollToBottom();
        break;
      }
      case 'message_complete': {
        const { message_id } = msg as unknown as { message_id: string };
        finalizeStreamingMessage(sessionId, message_id);
        setCurrentStreamingId(null);
        break;
      }
      case 'tool_use': {
        const { message_id, tool_use_id, tool_name, input } = msg as unknown as {
          message_id: string;
          tool_use_id: string;
          tool_name: string;
          input: unknown;
        };
        addToolCall(sessionId, message_id, {
          id: tool_use_id,
          name: tool_name,
          input,
        });
        scrollToBottom();
        break;
      }
      case 'tool_result': {
        const { tool_use_id, output, is_error } = msg as unknown as {
          tool_use_id: string;
          output: string;
          is_error: boolean;
        };
        updateToolCallResult(sessionId, tool_use_id, output, is_error);
        break;
      }
      case 'status_change': {
        loadSession();
        break;
      }
      case 'error': {
        const { message } = msg as unknown as { message: string };
        setError(message);
        break;
      }
      case 'terminal_buffer': {
        // DEPRECATED: Legacy message type. Server now uses sync_response via reliable streaming.
        // Kept for backward compatibility during transition.
        const { data } = msg as unknown as { data: number[] };
        const bytes = new Uint8Array(data);

        // Clear localStorage for this session since server is source of truth
        clearTerminalHistory(params.id);

        // Write buffer to terminal for display
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          // Queue data (capped to prevent OOM if terminal takes too long to mount)
          setTerminalData((prev) => {
            if (prev.length >= MAX_TERMINAL_QUEUE_CHUNKS) {
              // Drop oldest chunks to make room
              return [...prev.slice(-MAX_TERMINAL_QUEUE_CHUNKS + 1), bytes];
            }
            return [...prev, bytes];
          });
        }

        // Parse status from buffer
        const text = new TextDecoder().decode(bytes);
        outputBuffer = text.slice(-2000); // Keep last 2KB for status parsing
        const status = parseStatusLine(outputBuffer);
        if (status) {
          lastStatus = status;
          const currentSession = session();
          if (currentSession) {
            setSession({
              ...currentSession,
              model: status.model,
              total_cost_usd: status.cost,
              input_tokens: status.inputTokens,
              output_tokens: status.outputTokens,
              context_percent: status.contextPercent,
            });
          }
        }
        break;
      }
      case 'terminal_output': {
        // DEPRECATED: Legacy message type. Server now uses terminal_chunk via reliable streaming.
        // Kept for backward compatibility during transition.
        const { data } = msg as unknown as { data: number[] };
        const bytes = new Uint8Array(data);
        appendTerminalOutput(sessionId, bytes);
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          // Queue data (capped to prevent OOM if terminal takes too long to mount)
          setTerminalData((prev) => {
            if (prev.length >= MAX_TERMINAL_QUEUE_CHUNKS) {
              return [...prev.slice(-MAX_TERMINAL_QUEUE_CHUNKS + 1), bytes];
            }
            return [...prev, bytes];
          });
        }

        // Try to parse status line from output
        const text = new TextDecoder().decode(bytes);
        outputBuffer += text;
        // Keep buffer size manageable (last 2KB)
        if (outputBuffer.length > 2000) {
          outputBuffer = outputBuffer.slice(-1500);
        }

        const status = parseStatusLine(outputBuffer);
        if (status) {
          // Check if status changed significantly
          const changed = !lastStatus ||
            lastStatus.model !== status.model ||
            Math.abs(lastStatus.cost - status.cost) > 0.001 ||
            lastStatus.inputTokens !== status.inputTokens ||
            lastStatus.outputTokens !== status.outputTokens ||
            lastStatus.contextPercent !== status.contextPercent;

          if (changed) {
            lastStatus = status;

            // Immediately update local session state for responsive UI
            const currentSession = session();
            if (currentSession) {
              setSession({
                ...currentSession,
                model: status.model,
                total_cost_usd: status.cost,
                input_tokens: status.inputTokens,
                output_tokens: status.outputTokens,
                context_percent: status.contextPercent,
              });
            }

            // Debounce backend updates (send at most every 2 seconds)
            if (statusUpdateTimer) {
              clearTimeout(statusUpdateTimer);
            }
            statusUpdateTimer = window.setTimeout(() => {
              if (wsManager && wsState() === 'connected') {
                wsManager.send({
                  type: 'status_update',
                  model: status.model,
                  cost: status.cost,
                  input_tokens: status.inputTokens,
                  output_tokens: status.outputTokens,
                  context_percent: status.contextPercent,
                });
              }
              statusUpdateTimer = null;
            }, 2000);
          }
        }
        break;
      }
      case 'activity_update': {
        // Activity update from global broadcast - update local session state
        const { model, cost, input_tokens, output_tokens, context_percent, current_step, recent_actions } = msg as unknown as {
          model: string;
          cost: number;
          input_tokens: number;
          output_tokens: number;
          context_percent: number;
          current_step?: string;
          recent_actions?: Array<{ action_type: string; summary: string; detail?: string; timestamp: number }>;
        };
        const currentSession = session();
        if (currentSession) {
          setSession({
            ...currentSession,
            model: model || currentSession.model,
            total_cost_usd: cost,
            input_tokens,
            output_tokens,
            context_percent,
            current_step,
            recent_actions: recent_actions || currentSession.recent_actions || [],
          });
        }
        break;
      }
      case 'chat_history': {
        // Full chat history from backend (on connect)
        const chatMessages = (msg as unknown as { messages: ChatMessage[] }).messages;
        if (chatMessages && Array.isArray(chatMessages)) {
          handleChatHistory(params.id, chatMessages);
          scrollToBottom();
        }
        break;
      }
      case 'chat_event': {
        // Chat event from hook processing - update chat messages
        const chatEvent = (msg as unknown as { event: ChatEvent }).event;
        if (chatEvent) {
          handleChatEvent(chatEvent);
          scrollToBottom();
        }
        break;
      }
      case 'interactive': {
        // Interactive prompt from AskUserQuestion tool
        const interactiveEvent = (msg as unknown as { event: InteractiveEvent }).event;
        if (interactiveEvent) {
          handleInteractiveEvent(interactiveEvent);
          scrollToBottom();
        }
        break;
      }
      case 'subagent_started': {
        // Subagent (Task tool) started
        const data = msg as unknown as { session_id: string; agent_id: string; agent_type: string };
        handleSubagentStarted(params.id, data.agent_id, data.agent_type);
        scrollToBottom();
        break;
      }
      case 'subagent_stopped': {
        // Subagent (Task tool) completed
        const data = msg as unknown as { session_id: string; agent_id: string };
        handleSubagentStopped(params.id, data.agent_id);
        scrollToBottom();
        break;
      }
      case 'tool_error': {
        // Tool execution failed
        const data = msg as unknown as { session_id: string; tool_name: string; error: string; is_timeout: boolean };
        handleToolError(params.id, data.tool_name, data.error, data.is_timeout);
        scrollToBottom();
        break;
      }
      case 'context_compacting': {
        // Context being compacted
        const data = msg as unknown as { session_id: string; trigger: string };
        handleContextCompacting(params.id, data.trigger);
        scrollToBottom();
        break;
      }
      case 'permission_request': {
        // Permission request for tool
        const data = msg as unknown as { session_id: string; tool_name: string; tool_input: unknown };
        // Import handlePermissionRequest when needed
        import('../stores/messages').then(({ handlePermissionRequest }) => {
          handlePermissionRequest(params.id, data.tool_name, data.tool_input);
          scrollToBottom();
        });
        break;
      }
      case 'context_update': {
        // Context token update from hook data (accurate counts)
        const data = msg as unknown as {
          session_id: string;
          input_tokens: number;
          output_tokens: number;
          cache_read_tokens: number;
          cache_creation_tokens: number;
          context_window_size: number;
        };
        const currentSession = session();
        if (currentSession) {
          // Calculate context percent
          const contextPercent = data.context_window_size > 0
            ? Math.round((data.input_tokens / data.context_window_size) * 100)
            : 0;
          setSession({
            ...currentSession,
            input_tokens: data.input_tokens,
            output_tokens: data.output_tokens,
            context_percent: contextPercent,
            // Store cache tokens in extended session state
            cache_read_tokens: data.cache_read_tokens,
            cache_creation_tokens: data.cache_creation_tokens,
          });
        }
        break;
      }
      case 'mode_change': {
        // Plan mode indicator from hook detection
        const data = msg as unknown as { session_id: string; mode: 'normal' | 'plan' };
        setMode(data.mode);
        break;
      }
    }
  }

  function registerTerminalWrite(writeFn: (data: Uint8Array) => void) {
    terminalWriteFn = writeFn;

    // Don't replay from localStorage - server buffer is the source of truth
    // Server sends terminal_buffer on WebSocket connect with full history
    // localStorage is only used as backup for persistence, not for display

    // Flush queued data (this includes server buffer if received before terminal mounted)
    const queued = terminalData();
    if (queued.length > 0) {
      queued.forEach((data) => writeFn(data));
      setTerminalData([]);
    }
  }

  async function handleSendMessage(content: string) {
    const sessionId = params.id;

    // Don't add message locally - it will come back from the UserPromptSubmit hook
    // This ensures messages are only added once and stay in sync with the server

    scrollToBottom();

    if (wsManager && wsState() === 'connected') {
      wsManager.send({ type: 'input', content });
    } else {
      await api.sessions.sendInput(sessionId, content);
    }
  }

  function handleTerminalInput(data: Uint8Array) {
    if (wsManager && wsState() === 'connected') {
      wsManager.send({ type: 'terminal_input', data: Array.from(data) });
    }
  }

  function handleTerminalResize(cols: number, rows: number) {
    // Store dimensions for sync requests
    terminalDimensions = { cols, rows };

    // Update ws manager's terminal dimensions
    // setTerminalDimensions already sends sync request if dimensions changed
    // Don't call requestResync separately - it causes double syncs and flickering
    if (wsManager) {
      wsManager.setTerminalDimensions(cols, rows);
    }
  }

  async function handleResume() {
    setResuming(true);
    setError(null);
    try {
      const response = await api.sessions.resume(params.id);
      if (!response.ok) {
        const errorText = await response.text();
        // Check for specific error types
        if (errorText.includes('not resumable') || errorText.includes('SessionNotResumable')) {
          setError('This session cannot be resumed. It may have been created before session persistence was enabled. Try starting a new session instead.');
        } else if (errorText.includes('not found')) {
          setError('Session not found. It may have been deleted.');
        } else {
          setError(errorText || 'Failed to resume session');
        }
        return;
      }
      await loadSession();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to resume session');
    } finally {
      setResuming(false);
    }
  }

  const isSessionStopped = () => {
    const s = session();
    return s && (s.status === 'stopped' || s.status === 'error');
  };

  // Check if Claude is actively processing (has a current step or streaming)
  const isClaudeProcessing = () => {
    const s = session();
    if (!s) return false;
    if (s.status === 'stopped' || s.status === 'error') return false;
    // Processing if has current step or messages are streaming
    const hasCurrentStep = s.current_step && s.current_step !== 'Ready' && s.current_step.length > 0;
    const hasStreaming = currentStreamingId() !== null;
    return hasCurrentStep || hasStreaming;
  };

  // Handle permission response (Allow/Deny/Allow All)
  function handlePermissionResponse(messageId: string, response: 'y' | 'n' | 'a') {
    if (wsManager && wsState() === 'connected') {
      wsManager.send({ type: 'permission_response', response });
      markPermissionResponded(params.id, messageId);
    }
  }

  // Handle interrupt (Stop button)
  function handleInterrupt() {
    if (wsManager && wsState() === 'connected') {
      wsManager.send({ type: 'interrupt' });
    }
  }

  function handleConnectionRetry() {
    wsManager?.retry();
  }

  onMount(() => {
    loadSession();

    wsManager = createWebSocketManager({
      url: `/ws/sessions/${params.id}`,
      onMessage: handleWsMessage,
      onStateChange: (state) => {
        setWsState(state);
        // Update connection info when state changes
        if (wsManager) {
          setConnectionInfo(wsManager.getConnectionInfo());
        }
      },
      // Reliable streaming protocol callbacks
      onTerminalData: handleTerminalData,
      onSyncResponse: handleSyncResponse,
    });

    // Set initial terminal dimensions if known
    if (terminalDimensions) {
      wsManager.setTerminalDimensions(terminalDimensions.cols, terminalDimensions.rows);
    }

    wsManager.connect();
  });

  onCleanup(() => {
    wsManager?.disconnect();
  });

  const messages = () => getMessagesForSession(params.id);
  const streamingContent = () => {
    const id = currentStreamingId();
    return id ? getStreamingContent(params.id, id) : '';
  };

  return (
    <div
      class="flex flex-col"
      style={{
        // On iOS, follow visualViewport in real-time to sync with keyboard
        // Use position fixed + transform to counter iOS's automatic page scroll
        ...(isIOS() ? {
          position: 'fixed',
          top: '0',
          left: '0',
          right: '0',
          height: `${viewportHeight()}px`,
          // Counter iOS scroll by translating back to original position
          transform: `translateY(${offsetTop()}px)`,
        } : {
          height: '100%',
        }),
        width: "100%",
        "max-width": "100%",
        "min-width": "0",
        overflow: "hidden",
      }}
    >
      {/* Connection status banner */}
      <ConnectionStatus
        state={wsState()}
        reconnectAttempt={connectionInfo().reconnectAttempt}
        maxReconnectAttempts={connectionInfo().maxReconnectAttempts}
        queuedMessageCount={connectionInfo().queuedMessageCount}
        onRetry={handleConnectionRetry}
      />

      {/* Header */}
      <header class="flex-none glass safe-top" style={{ padding: '12px 16px' }}>
        <div
          style={{
            display: 'flex',
            "align-items": 'center',
            gap: '12px',
          }}
        >
          {/* Back button */}
          <button
            onClick={() => navigate('/')}
            class="pressable"
            style={{
              width: '36px',
              height: '36px',
              "flex-shrink": '0',
              display: 'flex',
              "align-items": 'center',
              "justify-content": 'center',
              color: 'var(--color-text-secondary)',
              background: 'none',
              border: 'none',
              "border-radius": '50%',
              cursor: 'pointer',
            }}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M15 18l-6-6 6-6" />
            </svg>
          </button>

          {/* Session info */}
          <div style={{ flex: '1', "min-width": '0', display: 'flex', "align-items": 'center', gap: '8px' }}>
            <Show when={session()} fallback={<Spinner size="sm" />}>
              {(s) => (
                <>
                  <StatusDot status={s().status} currentStep={s().current_step} />
                  <span
                    class="text-mono"
                    style={{
                      "font-size": '15px',
                      "font-weight": '600',
                      color: 'var(--color-text-primary)',
                      overflow: 'hidden',
                      "text-overflow": 'ellipsis',
                      "white-space": 'nowrap',
                    }}
                  >
                    {s().project_path.split('/').pop()}
                  </span>
                  {/* Plan Mode Badge */}
                  <Show when={mode() === 'plan'}>
                    <span
                      class="text-mono"
                      style={{
                        background: '#8b5cf6',
                        color: 'white',
                        padding: '2px 8px',
                        "border-radius": '4px',
                        "font-size": '10px',
                        "font-weight": '600',
                        "text-transform": 'uppercase',
                        "letter-spacing": '0.05em',
                        "flex-shrink": '0',
                      }}
                    >
                      Plan
                    </span>
                  </Show>
                </>
              )}
            </Show>
          </div>

          {/* View toggle */}
          <div
            style={{
              display: 'flex',
              "flex-shrink": '0',
              background: 'var(--color-bg-surface)',
              "border-radius": '10px',
              padding: '2px',
              border: '1px solid var(--color-bg-overlay)',
            }}
          >
            {(['chat', 'terminal', 'history'] as const).map((view) => (
              <button
                onClick={() => setCurrentView(view)}
                class="text-mono"
                style={{
                  padding: '5px 10px',
                  "font-size": '10px',
                  "font-weight": '500',
                  "border-radius": '8px',
                  border: 'none',
                  cursor: 'pointer',
                  transition: 'all 0.15s ease',
                  background: currentView() === view ? 'var(--color-bg-elevated)' : 'transparent',
                  color: currentView() === view ? 'var(--color-text-primary)' : 'var(--color-text-muted)',
                  "box-shadow": currentView() === view ? 'var(--shadow-retro-sm)' : 'none',
                }}
              >
                {view}
              </button>
            ))}
          </div>
        </div>
      </header>

      {/* Content */}
      <Show when={!loading()} fallback={
        <div class="flex-1 flex items-center justify-center">
          <Spinner size="lg" />
        </div>
      }>
        {/* Error banner */}
        <Show when={error()}>
          <div
            style={{
              margin: '16px',
              padding: '14px 16px',
              background: 'var(--color-accent-muted)',
              border: '1px solid var(--color-accent)',
              "border-radius": '12px',
              color: 'var(--color-accent)',
              "font-size": '14px',
            }}
          >
            {error()}
          </div>
        </Show>

        {/* Resume prompt for stopped sessions */}
        <Show when={isSessionStopped()}>
          <div style={{
            display: 'flex',
            "align-items": 'center',
            "justify-content": 'center',
            flex: '1',
            "min-height": '300px',
          }}>
            <div style={{ "text-align": 'center', padding: '24px' }}>
              <p class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '15px', "font-weight": '600', "margin-bottom": '8px' }}>
                Session ended
              </p>
              <p style={{
                color: 'var(--color-text-tertiary)',
                "font-family": 'var(--font-serif)',
                "font-size": '14px',
                "margin-bottom": '24px'
              }}>
                Resume to continue where you left off
              </p>
              <button
                onClick={handleResume}
                disabled={resuming()}
                style={{
                  display: 'inline-flex',
                  "align-items": 'center',
                  gap: '8px',
                  padding: '10px 18px',
                  "border-radius": '6px',
                  border: '1.5px solid var(--color-accent)',
                  background: 'transparent',
                  color: 'var(--color-accent)',
                  "font-family": 'var(--font-mono)',
                  "font-size": '13px',
                  "font-weight": '600',
                  cursor: resuming() ? 'default' : 'pointer',
                  opacity: resuming() ? 0.6 : 1,
                  transition: 'background 0.15s ease',
                }}
                onMouseEnter={(e) => !resuming() && (e.currentTarget.style.background = 'var(--color-accent-muted)')}
                onMouseLeave={(e) => e.currentTarget.style.background = 'transparent'}
              >
                <span style={{ "font-size": '14px' }}>▸</span>
                {resuming() ? 'Resuming...' : 'Resume'}
              </button>
            </div>
          </div>
        </Show>

        {/* Chat View */}
        <Show when={currentView() === 'chat'}>
          <div class="flex-1 flex flex-col" style={{ "min-height": '0' }}>
            <main class="flex-1 scrollable p-4 space-y-4" style={{ "min-height": '0' }}>
              {/* Empty state when no messages yet (only show when session is active) */}
              <Show when={messages().length === 0 && !streamingContent() && !isSessionStopped()}>
                <div style={{
                  display: 'flex',
                  "align-items": 'center',
                  "justify-content": 'center',
                  flex: '1',
                  "min-height": '200px',
                }}>
                  <div style={{ "text-align": 'center', padding: '24px' }}>
                    <p class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '15px', "font-weight": '600', "margin-bottom": '8px' }}>
                      No messages yet
                    </p>
                    <p style={{
                      color: 'var(--color-text-tertiary)',
                      "font-family": 'var(--font-serif)',
                      "font-size": '14px',
                    }}>
                      Messages from Claude will appear here
                    </p>
                  </div>
                </div>
              </Show>

              <For each={messages()}>
                {(message) => (
                  <MessageBubble
                    message={message}
                    onPermissionResponse={(response) => handlePermissionResponse(message.id, response)}
                  />
                )}
              </For>

              <Show when={streamingContent()}>
                <MessageBubble
                  message={{
                    id: 'streaming',
                    role: 'assistant',
                    content: streamingContent(),
                    timestamp: Date.now(),
                    isStreaming: true,
                  }}
                />
              </Show>

              <div ref={messagesEndRef} />
            </main>

            {/* Interactive Question Carousel */}
            <Show when={getInteractiveState(params.id).type === 'prompt'}>
              {() => {
                const state = getInteractiveState(params.id);
                if (state.type !== 'prompt') return null;
                return (
                  <InteractiveCarousel
                    sessionId={params.id}
                    session={state.session}
                    onSubmitAll={async (answers) => {
                      console.log('[interactive] onSubmitAll called with answers:', answers);
                      if (wsManager && wsState() === 'connected') {
                        // Send answers to terminal one by one with delay
                        // Each answer triggers Claude to process and show next question,
                        // so we need adequate delay between sends (500ms should work)
                        for (let i = 0; i < answers.length; i++) {
                          const answer = answers[i];
                          console.log('[interactive] Sending answer:', answer);
                          wsManager.send({
                            type: 'interactive_choice',
                            question_id: answer.questionId,
                            selected_indices: answer.selectedIndices,
                          });
                          // Wait for Claude to process and show next question
                          // First answer is immediate, subsequent ones need delay
                          if (i < answers.length - 1) {
                            await new Promise((r) => setTimeout(r, 500));
                          }
                        }
                        console.log('[interactive] All answers sent');
                      } else {
                        console.log('[interactive] WebSocket not connected, state:', wsState());
                      }
                    }}
                    onCancel={() => {
                      if (wsManager && wsState() === 'connected') {
                        wsManager.send({ type: 'interactive_cancel' });
                      }
                      clearInteractiveState(params.id);
                    }}
                  />
                );
              }}
            </Show>

            <InputBar
              onSend={handleSendMessage}
              disabled={wsState() !== 'connected' || getInteractiveState(params.id).type === 'prompt'}
              placeholder={session()?.mode === 'terminal' ? 'Type here (output in terminal)...' : 'Message Claude...'}
              isProcessing={isClaudeProcessing()}
              onInterrupt={handleInterrupt}
            />
          </div>
        </Show>

        {/* Terminal View - always render but hide to preserve state */}
        <div
          style={{
            display: currentView() === 'terminal' ? 'flex' : 'none',
            "flex-direction": 'column',
            flex: '1 1 0%',
            "min-height": '0',
            "min-width": '0',
            width: "100%",
            "max-width": '100%',
            overflow: 'hidden',
          }}
        >
          <TerminalView
            onInput={handleTerminalInput}
            onResize={handleTerminalResize}
            onClose={() => setCurrentView('chat')}
            onReady={registerTerminalWrite}
            isConnected={wsState() === 'connected'}
            isVisible={currentView() === 'terminal'}
          />
        </div>

        {/* History/Timeline View */}
        <Show when={currentView() === 'history'}>
          <div
            style={{
              display: 'flex',
              "flex-direction": 'column',
              flex: '1 1 0%',
              "min-height": '0',
              overflow: 'hidden',
              position: 'relative',
            }}
          >
            <Show when={diffState()}>
              {(diff) => (
                <div
                  style={{
                    position: 'absolute',
                    inset: '0',
                    background: 'var(--color-bg-base)',
                    "z-index": '10',
                    overflow: 'auto',
                  }}
                >
                  <DiffViewer
                    fromInteraction={diff().interactionId}
                    toInteraction={diff().interactionId}
                    filePath={diff().file}
                    onClose={() => setDiffState(null)}
                  />
                </div>
              )}
            </Show>
            <TimelineView
              sessionId={params.id}
              onViewDiff={(interactionId, file) => setDiffState({ interactionId, file })}
            />
          </div>
        </Show>
      </Show>
    </div>
  );
}
