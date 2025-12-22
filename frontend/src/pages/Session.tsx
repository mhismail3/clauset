import { Show, createSignal, onMount, onCleanup, For } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { Button } from '../components/ui/Button';
import { Badge } from '../components/ui/Badge';
import { Spinner } from '../components/ui/Spinner';
import { MessageBubble } from '../components/chat/MessageBubble';
import { InputBar } from '../components/chat/InputBar';
import { TerminalView } from '../components/terminal/TerminalView';
import { api, Session } from '../lib/api';
import { createWebSocketManager, WsMessage } from '../lib/ws';
import {
  getMessagesForSession,
  addMessage,
  appendToStreamingMessage,
  finalizeStreamingMessage,
  getStreamingContent,
  addToolCall,
  updateToolCallResult,
} from '../stores/messages';
import { getStatusVariant, getStatusLabel } from '../stores/sessions';
import { appendTerminalOutput, clearTerminalHistory } from '../stores/terminal';

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

export default function SessionPage() {
  const params = useParams<{ id: string }>();
  const navigate = useNavigate();

  const [session, setSession] = createSignal<Session | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [wsState, setWsState] = createSignal<'connecting' | 'connected' | 'disconnected' | 'reconnecting'>('disconnected');
  const [showTerminal, setShowTerminal] = createSignal(true);
  const [currentStreamingId, setCurrentStreamingId] = createSignal<string | null>(null);
  const [terminalData, setTerminalData] = createSignal<Uint8Array[]>([]);
  const [resuming, setResuming] = createSignal(false);

  let wsManager: ReturnType<typeof createWebSocketManager> | null = null;
  let messagesEndRef: HTMLDivElement | undefined;
  let terminalWriteFn: ((data: Uint8Array) => void) | null = null;
  let outputBuffer = '';
  let lastStatus: StatusInfo | null = null;
  let statusUpdateTimer: number | null = null;

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
        // Server sends buffered terminal output on reconnect for replay
        // This is the source of truth - replaces any localStorage data
        const { data } = msg as { data: number[] };
        const bytes = new Uint8Array(data);
        console.log(`Received terminal buffer: ${bytes.length} bytes`);

        // Clear localStorage for this session since server is source of truth
        clearTerminalHistory(params.id);

        // Write buffer to terminal for display
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          setTerminalData((prev) => [...prev, bytes]);
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
        const { data } = msg as { data: number[] };
        const bytes = new Uint8Array(data);
        appendTerminalOutput(sessionId, bytes);
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          setTerminalData((prev) => [...prev, bytes]);
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
    if (wsManager && wsState() === 'connected') {
      wsManager.send({ type: 'resize', cols, rows });
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

  onMount(() => {
    loadSession();

    wsManager = createWebSocketManager({
      url: `/ws/sessions/${params.id}`,
      onMessage: handleWsMessage,
      onStateChange: setWsState,
    });
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
          <div style={{ flex: '1', "min-width": '0', display: 'flex', "align-items": 'center', gap: '10px' }}>
            <Show when={session()} fallback={<Spinner size="sm" />}>
              {(s) => (
                <>
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
                  <Badge variant={getStatusVariant(s().status)}>
                    {getStatusLabel(s().status)}
                  </Badge>
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
            <button
              onClick={() => setShowTerminal(true)}
              class="text-mono"
              style={{
                padding: '6px 12px',
                "font-size": '11px',
                "font-weight": '500',
                "border-radius": '8px',
                border: 'none',
                cursor: 'pointer',
                transition: 'all 0.15s ease',
                background: showTerminal() ? 'var(--color-bg-elevated)' : 'transparent',
                color: showTerminal() ? 'var(--color-text-primary)' : 'var(--color-text-muted)',
                "box-shadow": showTerminal() ? 'var(--shadow-retro-sm)' : 'none',
              }}
            >
              term
            </button>
            <button
              onClick={() => setShowTerminal(false)}
              class="text-mono"
              style={{
                padding: '6px 12px',
                "font-size": '11px',
                "font-weight": '500',
                "border-radius": '8px',
                border: 'none',
                cursor: 'pointer',
                transition: 'all 0.15s ease',
                background: !showTerminal() ? 'var(--color-bg-elevated)' : 'transparent',
                color: !showTerminal() ? 'var(--color-text-primary)' : 'var(--color-text-muted)',
                "box-shadow": !showTerminal() ? 'var(--shadow-retro-sm)' : 'none',
              }}
            >
              chat
            </button>
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
        <div class={`flex-1 flex flex-col ${showTerminal() ? 'hidden' : ''}`}>
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
                  onClick={() => setShowTerminal(true)}
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

        {/* Terminal View */}
        <div
          class={showTerminal() ? '' : 'hidden'}
          style={{
            display: 'flex',
            "flex-direction": 'column',
            flex: '1 1 0%',
            "min-height": '0',
            width: "100%",
            overflow: 'hidden',
          }}
        >
          <TerminalView
            onInput={handleTerminalInput}
            onResize={handleTerminalResize}
            onClose={() => setShowTerminal(false)}
            onReady={registerTerminalWrite}
            isConnected={wsState() === 'connected'}
          />
        </div>
      </Show>
    </div>
  );
}
