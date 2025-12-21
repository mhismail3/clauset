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
import { appendTerminalOutput, getTerminalHistory } from '../stores/terminal';

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
      case 'terminal_output': {
        const { data } = msg as { data: number[] };
        const bytes = new Uint8Array(data);
        appendTerminalOutput(sessionId, bytes);
        if (terminalWriteFn) {
          terminalWriteFn(bytes);
        } else {
          setTerminalData((prev) => [...prev, bytes]);
        }
        break;
      }
    }
  }

  function registerTerminalWrite(writeFn: (data: Uint8Array) => void) {
    terminalWriteFn = writeFn;

    // Replay history from persistent store
    const history = getTerminalHistory(params.id);
    if (history.length > 0) {
      history.forEach((data) => writeFn(data));
    }

    // Flush queued data
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

  const connectionStatus = () => {
    switch (wsState()) {
      case 'connected': return { text: 'Connected', class: 'status-dot-active' };
      case 'connecting': return { text: 'Connecting', class: 'status-dot-idle' };
      case 'reconnecting': return { text: 'Reconnecting', class: 'status-dot-idle' };
      default: return { text: 'Disconnected', class: 'status-dot-inactive' };
    }
  };

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <header class="flex-none glass border-b border-bg-overlay/50 safe-top">
        <div class="flex items-center gap-3 px-4 py-3">
          {/* Back button */}
          <button
            onClick={() => navigate('/')}
            class="w-10 h-10 flex items-center justify-center -ml-2 text-accent rounded-full hover:bg-bg-elevated transition-colors touch-target"
          >
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M15 18l-6-6 6-6" />
            </svg>
          </button>

          {/* Session info */}
          <div class="flex-1 min-w-0">
            <Show when={session()} fallback={<Spinner size="sm" />}>
              {(s) => (
                <>
                  <div class="flex items-center gap-2">
                    <span class="font-semibold truncate">
                      {s().project_path.split('/').pop()}
                    </span>
                    <Badge variant={getStatusVariant(s().status)}>
                      {getStatusLabel(s().status)}
                    </Badge>
                  </div>
                  <div class="flex items-center gap-2 mt-0.5">
                    <span class="text-caption">{s().model}</span>
                    <span class={`status-dot ${connectionStatus().class}`} />
                  </div>
                </>
              )}
            </Show>
          </div>

          {/* View toggle */}
          <div class="flex bg-bg-surface rounded-xl p-1">
            <button
              onClick={() => setShowTerminal(false)}
              class={`px-3 py-1.5 text-sm font-medium rounded-lg transition-colors ${
                !showTerminal() ? 'bg-bg-elevated text-text-primary' : 'text-text-muted'
              }`}
            >
              Chat
            </button>
            <button
              onClick={() => setShowTerminal(true)}
              class={`px-3 py-1.5 text-sm font-medium rounded-lg transition-colors ${
                showTerminal() ? 'bg-bg-elevated text-text-primary' : 'text-text-muted'
              }`}
            >
              Term
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
          <div class="mx-4 mt-4 bg-status-error/10 border border-status-error/20 rounded-xl p-4 text-status-error text-sm">
            {error()}
          </div>
        </Show>

        {/* Resume prompt for stopped sessions */}
        <Show when={isSessionStopped()}>
          <div class="mx-4 mt-4 bg-bg-surface rounded-xl p-5 text-center">
            <div class="w-12 h-12 mx-auto mb-3 rounded-full bg-bg-elevated flex items-center justify-center">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-text-muted">
                <polygon points="5 3 19 12 5 21 5 3" />
              </svg>
            </div>
            <p class="text-text-secondary mb-4">
              This session has ended. Resume to continue.
            </p>
            <Button
              onClick={handleResume}
              disabled={resuming()}
            >
              {resuming() ? 'Resuming...' : 'Resume Session'}
            </Button>
          </div>
        </Show>

        {/* Chat View */}
        <div class={`flex-1 flex flex-col ${showTerminal() ? 'hidden' : ''}`}>
          <main class="flex-1 scrollable p-4 space-y-4">
            {/* Terminal mode notice */}
            <Show when={session()?.mode === 'terminal' && messages().length === 0}>
              <div class="bg-bg-surface rounded-xl p-5 text-center">
                <div class="w-12 h-12 mx-auto mb-3 rounded-full bg-accent/10 flex items-center justify-center">
                  <span class="text-accent text-xl font-mono">&gt;_</span>
                </div>
                <p class="text-text-secondary mb-2">
                  Terminal mode is active
                </p>
                <p class="text-caption mb-4">
                  Claude's responses appear in the terminal view for full interaction.
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
