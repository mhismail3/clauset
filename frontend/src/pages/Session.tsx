import { Show, createSignal, onMount, onCleanup, For } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { Button } from '../components/ui/Button';
import { Spinner } from '../components/ui/Spinner';
import { ConnectionStatus } from '../components/ui/ConnectionStatus';
import { MessageBubble } from '../components/chat/MessageBubble';
import { InputBar } from '../components/chat/InputBar';
import { TerminalView } from '../components/terminal/TerminalView';
import { TimelineView } from '../components/interactions/TimelineView';
import { DiffViewer } from '../components/interactions/DiffViewer';
import { api, Session } from '../lib/api';
import { createWebSocketManager, WsMessage, SyncResponse, ConnectionState } from '../lib/ws';
import {
  getMessagesForSession,
  addMessage,
  appendToStreamingMessage,
  finalizeStreamingMessage,
  getStreamingContent,
  addToolCall,
  updateToolCallResult,
} from '../stores/messages';
import { appendTerminalOutput, clearTerminalHistory } from '../stores/terminal';

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
  const [currentView, setCurrentView] = createSignal<'term' | 'chat' | 'history'>('term');
  const [currentStreamingId, setCurrentStreamingId] = createSignal<string | null>(null);
  const [diffState, setDiffState] = createSignal<{ interactionId: string; file: string } | null>(null);
  const [terminalData, setTerminalData] = createSignal<Uint8Array[]>([]);
  const [resuming, setResuming] = createSignal(false);

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
  }

  function handleWsMessage(msg: WsMessage) {
    const sessionId = params.id;

    switch (msg.type) {
      case 'text': {
        const { message_id, content, is_complete } = msg as { message_id: string; content: string; is_complete: boolean };
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
        const { message_id } = msg as { message_id: string };
        finalizeStreamingMessage(sessionId, message_id);
        setCurrentStreamingId(null);
        break;
      }
      case 'tool_use': {
        const { message_id, tool_use_id, tool_name, input } = msg as {
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
        const { tool_use_id, output, is_error } = msg as {
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
        const { message } = msg as { message: string };
        setError(message);
        break;
      }
      case 'terminal_buffer': {
        // DEPRECATED: Legacy message type. Server now uses sync_response via reliable streaming.
        // Kept for backward compatibility during transition.
        const { data } = msg as { data: number[] };
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
        const { data } = msg as { data: number[] };
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
        const { model, cost, input_tokens, output_tokens, context_percent, current_step, recent_actions } = msg as {
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

    addMessage(sessionId, {
      id: `user-${Date.now()}`,
      role: 'user',
      content,
      timestamp: Date.now(),
    });

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
    if (wsManager) {
      wsManager.setTerminalDimensions(cols, rows);
    }

    // If connected, request resync with new dimensions
    // The reliable streaming protocol handles buffer replay via SyncRequest
    if (wsManager && wsState() === 'connected') {
      wsManager.requestResync();
    }
  }

  async function handleResume() {
    setResuming(true);
    setError(null);
    try {
      await api.sessions.resume(params.id);
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
    <div class="flex flex-col h-full" style={{ width: "100%", "max-width": "100%", "min-width": "0", overflow: "hidden" }}>
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
              padding: '3px',
              border: '1px solid var(--color-bg-overlay)',
            }}
          >
            {(['term', 'chat', 'history'] as const).map((view) => (
              <button
                onClick={() => setCurrentView(view)}
                class="text-mono"
                style={{
                  padding: '6px 12px',
                  "font-size": '11px',
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
          <div
            class="card-bordered"
            style={{
              margin: '16px',
              padding: '24px',
              "text-align": 'center',
            }}
          >
            <div
              style={{
                width: '48px',
                height: '48px',
                margin: '0 auto 12px',
                "border-radius": '50%',
                background: 'var(--color-bg-elevated)',
                display: 'flex',
                "align-items": 'center',
                "justify-content": 'center',
              }}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-text-muted">
                <polygon points="5 3 19 12 5 21 5 3" />
              </svg>
            </div>
            <p style={{ color: 'var(--color-text-secondary)', "margin-bottom": '16px' }}>
              Session ended. Resume to continue.
            </p>
            <Button
              onClick={handleResume}
              disabled={resuming()}
            >
              {resuming() ? 'Resuming...' : 'Resume'}
            </Button>
          </div>
        </Show>

        {/* Chat View */}
        <Show when={currentView() === 'chat'}>
          <div class="flex-1 flex flex-col">
            <main class="flex-1 scrollable p-4 space-y-4">
              {/* Terminal mode notice */}
              <Show when={session()?.mode === 'terminal' && messages().length === 0}>
                <div class="card-bordered" style={{ padding: '24px', "text-align": 'center' }}>
                  <div
                    style={{
                      width: '48px',
                      height: '48px',
                      margin: '0 auto 12px',
                      "border-radius": '50%',
                      background: 'var(--color-accent-muted)',
                      display: 'flex',
                      "align-items": 'center',
                      "justify-content": 'center',
                    }}
                  >
                    <span class="text-mono" style={{ color: 'var(--color-accent)', "font-size": '18px' }}>&gt;_</span>
                  </div>
                  <p style={{ color: 'var(--color-text-secondary)', "margin-bottom": '8px' }}>
                    Terminal mode active
                  </p>
                  <p class="text-caption" style={{ "margin-bottom": '16px' }}>
                    Output appears in the terminal view.
                  </p>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setCurrentView('term')}
                  >
                    Switch to Terminal
                  </Button>
                </div>
              </Show>

              <For each={messages()}>
                {(message) => <MessageBubble message={message} />}
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

            <InputBar
              onSend={handleSendMessage}
              disabled={wsState() !== 'connected'}
              placeholder={session()?.mode === 'terminal' ? 'Type here (output in terminal)...' : 'Message Claude...'}
            />
          </div>
        </Show>

        {/* Terminal View - always render but hide to preserve state */}
        <div
          style={{
            display: currentView() === 'term' ? 'flex' : 'none',
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
