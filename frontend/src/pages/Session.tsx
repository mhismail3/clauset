import { Show, createSignal, createEffect, onMount, onCleanup, For } from 'solid-js';
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
  Message,
} from '../stores/messages';
import { getStatusVariant, getStatusLabel } from '../stores/sessions';

export default function SessionPage() {
  const params = useParams<{ id: string }>();
  const navigate = useNavigate();

  const [session, setSession] = createSignal<Session | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [wsState, setWsState] = createSignal<'connecting' | 'connected' | 'disconnected' | 'reconnecting'>('disconnected');
  const [showTerminal, setShowTerminal] = createSignal(true); // Default to terminal view
  const [currentStreamingId, setCurrentStreamingId] = createSignal<string | null>(null);
  const [terminalData, setTerminalData] = createSignal<Uint8Array[]>([]);
  const [resuming, setResuming] = createSignal(false);

  let wsManager: ReturnType<typeof createWebSocketManager> | null = null;
  let messagesEndRef: HTMLDivElement | undefined;
  let terminalWriteFn: ((data: Uint8Array) => void) | null = null;

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
        loadSession(); // Refresh session data
        break;
      }
      case 'error': {
        const { message } = msg as { message: string };
        setError(message);
        break;
      }
      case 'terminal_output': {
        const { data } = msg as { data: number[] };
        const bytes = new Uint8Array(data);
        // Write to terminal if available
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          // Queue data until terminal is ready
          setTerminalData((prev) => [...prev, bytes]);
        }
        break;
      }
    }
  }

  // Register terminal write function
  function registerTerminalWrite(writeFn: (data: Uint8Array) => void) {
    terminalWriteFn = writeFn;
    // Flush any queued data
    const queued = terminalData();
    if (queued.length > 0) {
      queued.forEach((data) => writeFn(data));
      setTerminalData([]);
    }
  }

  async function handleSendMessage(content: string) {
    const sessionId = params.id;

    // Add user message
    addMessage(sessionId, {
      id: `user-${Date.now()}`,
      role: 'user',
      content,
      timestamp: Date.now(),
    });

    scrollToBottom();

    // Send via WebSocket or REST
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
      await loadSession(); // Refresh session status
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

    // Connect WebSocket
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
    <div class="flex flex-col h-screen">
      {/* Header */}
      <header class="flex-none bg-bg-base/80 backdrop-blur-sm border-b border-bg-elevated safe-top z-40">
        <div class="flex items-center gap-3 px-4 py-3">
          <button
            onClick={() => navigate('/')}
            class="p-2 -ml-2 text-text-secondary hover:text-text-primary"
          >
            ←
          </button>
          <div class="flex-1 min-w-0">
            <Show when={session()} fallback={<Spinner size="sm" />}>
              {(s) => (
                <>
                  <div class="flex items-center gap-2">
                    <span class="font-medium truncate">
                      {s().project_path.split('/').pop()}
                    </span>
                    <Badge variant={getStatusVariant(s().status)}>
                      {getStatusLabel(s().status)}
                    </Badge>
                  </div>
                  <div class="text-xs text-text-muted flex items-center gap-2">
                    <span>{s().model}</span>
                    <span class={`w-2 h-2 rounded-full ${wsState() === 'connected' ? 'bg-status-active' : wsState() === 'connecting' || wsState() === 'reconnecting' ? 'bg-status-idle' : 'bg-status-completed'}`} />
                  </div>
                </>
              )}
            </Show>
          </div>
          <Show when={session()?.mode === 'terminal' || true}>
            <Button
              variant={showTerminal() ? 'primary' : 'ghost'}
              size="sm"
              onClick={() => setShowTerminal(!showTerminal())}
            >
              {'>_'}
            </Button>
          </Show>
        </div>
      </header>

      {/* Content */}
      <Show when={!loading()} fallback={
        <div class="flex-1 flex items-center justify-center">
          <Spinner size="lg" />
        </div>
      }>
        <Show when={error()}>
          <div class="m-4 bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-400">
            {error()}
          </div>
        </Show>

        {/* Resume button for stopped sessions */}
        <Show when={isSessionStopped()}>
          <div class="m-4 bg-bg-surface rounded-lg p-4 text-center">
            <p class="text-text-secondary mb-3">
              This session has stopped. Resume to continue where you left off.
            </p>
            <Button
              onClick={handleResume}
              disabled={resuming()}
            >
              {resuming() ? 'Resuming...' : 'Resume Session'}
            </Button>
          </div>
        </Show>

        {/* Chat View - hidden when terminal is shown */}
        <div class={`flex-1 flex flex-col ${showTerminal() ? 'hidden' : ''}`}>
          <main class="flex-1 overflow-y-auto p-4 space-y-4">
            {/* Terminal mode notice */}
            <Show when={session()?.mode === 'terminal' && messages().length === 0}>
              <div class="bg-bg-surface rounded-lg p-4 text-center">
                <p class="text-text-secondary mb-2">
                  This session uses terminal mode for full Claude Max subscription access.
                </p>
                <p class="text-text-muted text-sm mb-3">
                  Claude's responses appear in the terminal view. Switch to terminal for the full experience.
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

            {/* Streaming message */}
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

        {/* Terminal View - hidden when chat is shown, but always mounted */}
        <div class={`flex-1 flex flex-col ${showTerminal() ? '' : 'hidden'}`}>
          <TerminalView
            onInput={handleTerminalInput}
            onResize={handleTerminalResize}
            onClose={() => setShowTerminal(false)}
            onReady={registerTerminalWrite}
          />
        </div>
      </Show>
    </div>
  );
}
