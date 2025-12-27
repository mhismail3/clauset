import { Show, For, createSignal, onCleanup, JSX } from 'solid-js';
import { Message, ToolCall } from '../../stores/messages';

const THINKING_PHRASES = [
  'Thinking',
  'Evaluating',
  'Noodling',
  'Considering',
  'Processing',
  'Pondering',
];

const DOT_CYCLE = ['●', '○', '◐', '◑'];

function StreamingIndicator() {
  const [phraseIndex, setPhraseIndex] = createSignal(0);
  const [dotIndex, setDotIndex] = createSignal(0);

  const interval = setInterval(() => {
    setDotIndex((i) => (i + 1) % DOT_CYCLE.length);
    if (dotIndex() === DOT_CYCLE.length - 1) {
      setPhraseIndex((i) => (i + 1) % THINKING_PHRASES.length);
    }
  }, 300);

  onCleanup(() => clearInterval(interval));

  return (
    <div
      style={{
        display: 'flex',
        'align-items': 'center',
        gap: '8px',
        'margin-top': '8px',
        color: 'var(--color-text-muted)',
      }}
    >
      <span style={{ color: 'var(--color-accent)', 'font-size': '12px' }}>
        {DOT_CYCLE[dotIndex()]}
      </span>
      <span class="text-mono" style={{ 'font-size': '13px' }}>
        {THINKING_PHRASES[phraseIndex()]}...
      </span>
    </div>
  );
}

interface MessageBubbleProps {
  message: Message;
  /** Callback to send permission response (for permission_request system messages) */
  onPermissionResponse?: (response: 'y' | 'n' | 'a') => void;
}

export function MessageBubble(props: MessageBubbleProps) {
  const isUser = () => props.message.role === 'user';
  const isSystem = () => props.message.role === 'system';
  const isThinking = () =>
    !isUser() &&
    !isSystem() &&
    props.message.isStreaming &&
    !props.message.content &&
    !props.message.toolCalls?.length;
  const hasContent = () => props.message.content || props.message.toolCalls?.length;

  // System message styling based on event type
  const getSystemStyles = () => {
    const systemType = props.message.systemType;
    switch (systemType) {
      case 'tool_error':
        return {
          background: 'rgba(220, 38, 38, 0.15)',
          border: '1px solid rgba(220, 38, 38, 0.4)',
          color: '#ef4444',
          icon: '⚠',
        };
      case 'context_compacting':
        return {
          background: 'rgba(234, 179, 8, 0.15)',
          border: '1px solid rgba(234, 179, 8, 0.4)',
          color: '#eab308',
          icon: '⟳',
        };
      case 'subagent_started':
        return {
          background: 'rgba(59, 130, 246, 0.15)',
          border: '1px solid rgba(59, 130, 246, 0.4)',
          color: '#3b82f6',
          icon: '▶',
        };
      case 'subagent_stopped':
        return {
          background: 'rgba(34, 197, 94, 0.15)',
          border: '1px solid rgba(34, 197, 94, 0.4)',
          color: '#22c55e',
          icon: '✓',
        };
      case 'permission_request':
        return {
          background: 'rgba(168, 85, 247, 0.15)',
          border: '1px solid rgba(168, 85, 247, 0.4)',
          color: '#a855f7',
          icon: '🔐',
        };
      default:
        return {
          background: 'rgba(100, 116, 139, 0.15)',
          border: '1px solid rgba(100, 116, 139, 0.4)',
          color: 'var(--color-text-muted)',
          icon: '•',
        };
    }
  };

  // Render system message
  if (isSystem()) {
    const styles = getSystemStyles();
    const isPermissionRequest = () => props.message.systemType === 'permission_request';
    const isResponded = () => props.message.responded;

    return (
      <div class="flex justify-center">
        <div
          style={{
            display: 'flex',
            'flex-direction': isPermissionRequest() ? 'column' : 'row',
            'align-items': isPermissionRequest() ? 'stretch' : 'center',
            gap: '6px',
            padding: isPermissionRequest() ? '8px 12px' : '4px 12px',
            'border-radius': '16px',
            background: styles.background,
            border: styles.border,
            'font-size': '12px',
            'font-family': 'var(--font-mono)',
            color: styles.color,
          }}
        >
          <div style={{ display: 'flex', 'align-items': 'center', gap: '6px' }}>
            <span>{styles.icon}</span>
            <span>{props.message.content}</span>
          </div>

          {/* Permission approval buttons */}
          <Show when={isPermissionRequest() && !isResponded() && props.onPermissionResponse}>
            <div style={{
              display: 'flex',
              gap: '8px',
              'margin-top': '6px',
              'justify-content': 'center',
            }}>
              <button
                onClick={() => props.onPermissionResponse?.('y')}
                style={{
                  padding: '4px 12px',
                  'border-radius': '6px',
                  border: 'none',
                  background: '#22c55e',
                  color: 'white',
                  'font-size': '11px',
                  'font-weight': '600',
                  'font-family': 'var(--font-mono)',
                  cursor: 'pointer',
                  transition: 'opacity 0.15s ease',
                }}
                class="pressable"
              >
                Allow
              </button>
              <button
                onClick={() => props.onPermissionResponse?.('n')}
                style={{
                  padding: '4px 12px',
                  'border-radius': '6px',
                  border: 'none',
                  background: '#ef4444',
                  color: 'white',
                  'font-size': '11px',
                  'font-weight': '600',
                  'font-family': 'var(--font-mono)',
                  cursor: 'pointer',
                  transition: 'opacity 0.15s ease',
                }}
                class="pressable"
              >
                Deny
              </button>
              <button
                onClick={() => props.onPermissionResponse?.('a')}
                style={{
                  padding: '4px 12px',
                  'border-radius': '6px',
                  border: 'none',
                  background: '#3b82f6',
                  color: 'white',
                  'font-size': '11px',
                  'font-weight': '600',
                  'font-family': 'var(--font-mono)',
                  cursor: 'pointer',
                  transition: 'opacity 0.15s ease',
                }}
                class="pressable"
              >
                Allow All
              </button>
            </div>
          </Show>

          {/* Show responded state */}
          <Show when={isPermissionRequest() && isResponded()}>
            <div style={{
              'margin-top': '4px',
              'font-size': '10px',
              color: 'var(--color-text-muted)',
              'text-align': 'center',
            }}>
              ✓ Response sent
            </div>
          </Show>
        </div>
      </div>
    );
  }

  return (
    <div class={`flex ${isUser() ? 'justify-end' : 'justify-start'}`}>
      <div
        style={{
          "max-width": '85%',
          "border-radius": '14px',
          padding: '6px 10px',
          // Retro styling - user bubbles use muted orange, assistant uses elevated bg
          background: isUser() ? '#9a4a2e' : 'var(--color-bg-elevated)',
          color: isUser() ? '#f0ebe3' : 'var(--color-text-primary)',
          border: isUser()
            ? '1.5px solid #7a3a22'
            : '1.5px solid var(--color-bg-overlay)',
          "box-shadow": '2px 2px 0px rgba(0, 0, 0, 0.3)',
        }}
      >
        {/* Thinking indicator - shown when streaming but no content yet */}
        <Show when={isThinking()}>
          <div
            style={{
              display: 'flex',
              "align-items": 'center',
              gap: '10px',
              color: 'var(--color-text-muted)',
            }}
          >
            <div style={{ display: 'flex', gap: '4px' }}>
              <span
                style={{
                  width: '6px',
                  height: '6px',
                  "border-radius": '50%',
                  background: 'var(--color-accent)',
                  animation: 'bounce 1s ease-in-out infinite',
                }}
              />
              <span
                style={{
                  width: '6px',
                  height: '6px',
                  "border-radius": '50%',
                  background: 'var(--color-accent)',
                  animation: 'bounce 1s ease-in-out infinite',
                  "animation-delay": '150ms',
                }}
              />
              <span
                style={{
                  width: '6px',
                  height: '6px',
                  "border-radius": '50%',
                  background: 'var(--color-accent)',
                  animation: 'bounce 1s ease-in-out infinite',
                  "animation-delay": '300ms',
                }}
              />
            </div>
            <span class="text-mono" style={{ "font-size": '13px' }}>Thinking...</span>
          </div>
        </Show>

        {/* Tool Calls */}
        <Show when={props.message.toolCalls?.length}>
          <div style={{ display: 'flex', "flex-direction": 'column', gap: '6px', "margin-bottom": '8px' }}>
            <For each={props.message.toolCalls}>
              {(toolCall) => <ToolCallView toolCall={toolCall} />}
            </For>
          </div>
        </Show>

        {/* Content */}
        <Show when={props.message.content}>
          <div style={{
            "white-space": 'pre-wrap',
            "word-break": 'break-word',
            "font-family": 'var(--font-serif)',
            "font-size": '15px',
            "line-height": '1.6',
          }}>
            <MarkdownContent content={props.message.content} />
          </div>
        </Show>

        {/* Streaming indicator - shown while content is still being added */}
        <Show when={props.message.isStreaming && hasContent()}>
          <StreamingIndicator />
        </Show>
      </div>
    </div>
  );
}

// Helper to extract filename from path
function getFilename(path: string): string {
  return path.split('/').pop() || path;
}

// Helper to truncate text
function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + '...';
}

// Format tool input for display
function formatToolInput(name: string, input: Record<string, unknown>): JSX.Element {
  switch (name) {
    case 'Read': {
      const filePath = String(input.file_path || '');
      const offset = input.offset as number | undefined;
      const limit = input.limit as number | undefined;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>File:</span>
            <span class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>
              {getFilename(filePath)}
            </span>
          </div>
          <Show when={offset !== undefined || limit !== undefined}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              {offset !== undefined && `Lines ${offset}+`}
              {limit !== undefined && `, limit ${limit}`}
            </div>
          </Show>
        </div>
      );
    }
    case 'Grep': {
      const pattern = String(input.pattern || '');
      const path = input.path as string | undefined;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>Pattern:</span>
            <code style={{ color: 'var(--color-accent)', "font-size": '12px', background: 'var(--color-code-bg)', padding: '2px 6px', "border-radius": '4px' }}>
              {pattern}
            </code>
          </div>
          <Show when={path}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              in {getFilename(path!)}
            </div>
          </Show>
        </div>
      );
    }
    case 'Bash': {
      const command = String(input.command || '');
      return (
        <div class="text-mono" style={{
          background: 'var(--color-code-bg)',
          padding: '8px 10px',
          "border-radius": '6px',
          "font-size": '12px',
          color: 'var(--color-text-primary)',
          "white-space": 'pre-wrap',
          "word-break": 'break-all',
        }}>
          <span style={{ color: 'var(--color-text-muted)' }}>$ </span>{command}
        </div>
      );
    }
    case 'Edit': {
      const filePath = String(input.file_path || '');
      const oldStr = String(input.old_string || '').slice(0, 50);
      const newStr = String(input.new_string || '').slice(0, 50);
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '6px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>File:</span>
            <span class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>
              {getFilename(filePath)}
            </span>
          </div>
          <div style={{ "font-size": '11px' }}>
            <div style={{ color: '#c45b37', background: 'rgba(196, 91, 55, 0.1)', padding: '4px 8px', "border-radius": '4px', "margin-bottom": '4px' }}>
              − {truncate(oldStr, 40)}
            </div>
            <div style={{ color: '#2c8f7a', background: 'rgba(44, 143, 122, 0.1)', padding: '4px 8px', "border-radius": '4px' }}>
              + {truncate(newStr, 40)}
            </div>
          </div>
        </div>
      );
    }
    case 'Write': {
      const filePath = String(input.file_path || '');
      const content = String(input.content || '');
      const lines = content.split('\n').length;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>File:</span>
            <span class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>
              {getFilename(filePath)}
            </span>
          </div>
          <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
            {lines} lines
          </div>
        </div>
      );
    }
    case 'Glob': {
      const pattern = String(input.pattern || '');
      const path = input.path as string | undefined;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>Pattern:</span>
            <code style={{ color: 'var(--color-accent)', "font-size": '12px', background: 'var(--color-code-bg)', padding: '2px 6px', "border-radius": '4px' }}>
              {pattern}
            </code>
          </div>
          <Show when={path}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              in {path}
            </div>
          </Show>
        </div>
      );
    }
    case 'Task': {
      const description = String(input.description || '');
      const subagentType = input.subagent_type as string | undefined;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>
            {description}
          </div>
          <Show when={subagentType}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              Agent: {subagentType}
            </div>
          </Show>
        </div>
      );
    }
    case 'TodoWrite': {
      const todos = input.todos as Array<{ content: string; status: string; activeForm?: string }> | undefined;
      if (!todos?.length) {
        return <span style={{ color: 'var(--color-text-muted)', "font-size": '12px' }}>No todos</span>;
      }
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <For each={todos}>
            {(todo) => {
              const statusIcon = () => {
                switch (todo.status) {
                  case 'completed': return '✓';
                  case 'in_progress': return '◐';
                  default: return '○';
                }
              };
              const statusColor = () => {
                switch (todo.status) {
                  case 'completed': return '#22c55e';
                  case 'in_progress': return '#3b82f6';
                  default: return 'var(--color-text-muted)';
                }
              };
              return (
                <div style={{ display: 'flex', "align-items": 'flex-start', gap: '6px', "font-size": '12px' }}>
                  <span style={{ color: statusColor(), "flex-shrink": '0' }}>{statusIcon()}</span>
                  <span style={{ color: todo.status === 'completed' ? 'var(--color-text-muted)' : 'var(--color-text-primary)' }}>
                    {todo.status === 'in_progress' ? (todo.activeForm || todo.content) : todo.content}
                  </span>
                </div>
              );
            }}
          </For>
        </div>
      );
    }
    case 'EnterPlanMode': {
      return (
        <div style={{ display: 'flex', "align-items": 'center', gap: '6px', color: 'var(--color-accent)', "font-size": '12px' }}>
          <span>📋</span>
          <span>Entering plan mode...</span>
        </div>
      );
    }
    case 'ExitPlanMode': {
      return (
        <div style={{ display: 'flex', "align-items": 'center', gap: '6px', color: 'var(--color-secondary)', "font-size": '12px' }}>
          <span>✓</span>
          <span>Exiting plan mode</span>
        </div>
      );
    }
    case 'Skill': {
      const skill = String(input.skill || '');
      const args = input.args as string | undefined;
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>Skill:</span>
            <code style={{ color: 'var(--color-accent)', "font-size": '12px', background: 'var(--color-code-bg)', padding: '2px 6px', "border-radius": '4px' }}>
              /{skill}
            </code>
          </div>
          <Show when={args}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              Args: {args}
            </div>
          </Show>
        </div>
      );
    }
    case 'AskUserQuestion': {
      const questions = input.questions as Array<{ question: string; header?: string; options?: Array<{ label: string }> }> | undefined;
      if (!questions?.length) {
        return <span style={{ color: 'var(--color-text-muted)', "font-size": '12px' }}>Asking question...</span>;
      }
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '6px' }}>
          <For each={questions}>
            {(q) => (
              <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
                <Show when={q.header}>
                  <span style={{ color: 'var(--color-text-muted)', "font-size": '10px', "text-transform": 'uppercase', "letter-spacing": '0.05em' }}>
                    {q.header}
                  </span>
                </Show>
                <span style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>{q.question}</span>
                <Show when={q.options?.length}>
                  <div style={{ display: 'flex', "flex-wrap": 'wrap', gap: '4px' }}>
                    <For each={q.options}>
                      {(opt) => (
                        <span style={{
                          background: 'var(--color-bg-overlay)',
                          padding: '2px 8px',
                          "border-radius": '12px',
                          "font-size": '11px',
                          color: 'var(--color-text-secondary)',
                        }}>
                          {opt.label}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </div>
      );
    }
    case 'WebFetch': {
      const url = String(input.url || '');
      return (
        <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
          <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>URL:</span>
          <span class="text-mono" style={{ color: 'var(--color-text-primary)', "font-size": '12px', "word-break": 'break-all' }}>
            {truncate(url, 50)}
          </span>
        </div>
      );
    }
    case 'WebSearch': {
      const query = String(input.query || '');
      return (
        <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
          <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>Query:</span>
          <span style={{ color: 'var(--color-text-primary)', "font-size": '12px' }}>
            "{query}"
          </span>
        </div>
      );
    }
    default:
      return (
        <pre class="text-mono" style={{
          "font-size": '11px',
          color: 'var(--color-text-secondary)',
          margin: 0,
          "white-space": 'pre-wrap',
          "word-break": 'break-all',
        }}>
          {JSON.stringify(input, null, 2)}
        </pre>
      );
  }
}

// Format tool output for display
function formatToolOutput(name: string, output: string, isError: boolean): JSX.Element {
  if (isError) {
    return (
      <div style={{ color: 'var(--color-accent)', "font-size": '12px' }}>
        {output}
      </div>
    );
  }

  // Try to parse JSON output
  let parsed: unknown = null;
  try {
    parsed = JSON.parse(output);
  } catch {
    // Not JSON, use as-is
  }

  switch (name) {
    case 'Read': {
      if (parsed && typeof parsed === 'object' && 'file' in (parsed as Record<string, unknown>)) {
        const file = (parsed as { file: { content?: string; lineCount?: number } }).file;
        const content = file.content || '';
        const lines = content.split('\n');
        const preview = lines.slice(0, 5).join('\n');
        const hasMore = lines.length > 5;
        return (
          <div style={{ display: 'flex', "flex-direction": 'column', gap: '6px' }}>
            <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
              {lines.length} lines
            </div>
            <pre class="text-mono" style={{
              "font-size": '11px',
              color: 'var(--color-text-secondary)',
              margin: 0,
              "white-space": 'pre-wrap',
              background: 'var(--color-code-bg)',
              padding: '8px',
              "border-radius": '6px',
              "max-height": '120px',
              overflow: 'auto',
            }}>
              {preview}{hasMore && '\n...'}
            </pre>
          </div>
        );
      }
      return <span style={{ color: 'var(--color-text-muted)', "font-size": '12px' }}>{truncate(output, 100)}</span>;
    }
    case 'Grep': {
      // Grep output is often a list of files or matches
      const lines = output.split('\n').filter(Boolean);
      if (lines.length === 0) {
        return <span style={{ color: 'var(--color-text-muted)', "font-size": '12px' }}>No matches</span>;
      }
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
            {lines.length} match{lines.length !== 1 ? 'es' : ''}
          </div>
          <div style={{ "max-height": '100px', overflow: 'auto' }}>
            {lines.slice(0, 8).map((line) => (
              <div class="text-mono" style={{ "font-size": '11px', color: 'var(--color-text-secondary)', padding: '2px 0' }}>
                {getFilename(line)}
              </div>
            ))}
            {lines.length > 8 && (
              <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
                +{lines.length - 8} more
              </div>
            )}
          </div>
        </div>
      );
    }
    case 'Bash': {
      const lines = output.split('\n');
      const preview = lines.slice(0, 8).join('\n');
      return (
        <pre class="text-mono" style={{
          "font-size": '11px',
          color: 'var(--color-text-secondary)',
          margin: 0,
          "white-space": 'pre-wrap',
          background: 'var(--color-code-bg)',
          padding: '8px',
          "border-radius": '6px',
          "max-height": '120px',
          overflow: 'auto',
        }}>
          {preview}{lines.length > 8 && '\n...'}
        </pre>
      );
    }
    case 'Edit':
    case 'Write': {
      // Success message
      if (output.includes('success') || output.includes('updated') || output.includes('written')) {
        return (
          <div style={{ display: 'flex', "align-items": 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-secondary)' }}>✓</span>
            <span style={{ color: 'var(--color-text-secondary)', "font-size": '12px' }}>
              {name === 'Edit' ? 'File updated' : 'File written'}
            </span>
          </div>
        );
      }
      return <span style={{ color: 'var(--color-text-secondary)', "font-size": '12px' }}>{truncate(output, 80)}</span>;
    }
    case 'Glob': {
      const lines = output.split('\n').filter(Boolean);
      if (lines.length === 0) {
        return <span style={{ color: 'var(--color-text-muted)', "font-size": '12px' }}>No files found</span>;
      }
      return (
        <div style={{ display: 'flex', "flex-direction": 'column', gap: '4px' }}>
          <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
            {lines.length} file{lines.length !== 1 ? 's' : ''}
          </div>
          <div style={{ "max-height": '100px', overflow: 'auto' }}>
            {lines.slice(0, 8).map((line) => (
              <div class="text-mono" style={{ "font-size": '11px', color: 'var(--color-text-secondary)', padding: '2px 0' }}>
                {getFilename(line)}
              </div>
            ))}
            {lines.length > 8 && (
              <div style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
                +{lines.length - 8} more
              </div>
            )}
          </div>
        </div>
      );
    }
    case 'Task': {
      return (
        <div style={{ color: 'var(--color-text-secondary)', "font-size": '12px', "max-height": '150px', overflow: 'auto' }}>
          {truncate(output, 300)}
        </div>
      );
    }
    default:
      return (
        <pre class="text-mono" style={{
          "font-size": '11px',
          color: 'var(--color-text-secondary)',
          margin: 0,
          "white-space": 'pre-wrap',
          "word-break": 'break-all',
          "max-height": '150px',
          overflow: 'auto',
        }}>
          {truncate(output, 500)}
        </pre>
      );
  }
}

function ToolCallView(props: { toolCall: ToolCall }) {
  const [expanded, setExpanded] = createSignal(false);

  // Parse MCP tool names: mcp__{server}__{tool}
  const parseMcpTool = (name: string): { server: string; tool: string } | null => {
    const match = name.match(/^mcp__(.+?)__(.+)$/);
    if (match) {
      return { server: match[1], tool: match[2] };
    }
    return null;
  };

  const mcpInfo = () => parseMcpTool(props.toolCall.name);
  const isMcp = () => mcpInfo() !== null;
  const displayName = () => {
    const mcp = mcpInfo();
    if (mcp) {
      // Shorten common MCP server names
      const serverShort = mcp.server.replace('plugin_', '').replace('claude-in-chrome', 'chrome');
      return `${serverShort}:${mcp.tool}`;
    }
    return props.toolCall.name;
  };

  const icon = () => {
    const name = props.toolCall.name;

    // MCP tools get special icons based on tool type
    if (isMcp()) {
      const mcp = mcpInfo()!;
      if (mcp.tool.includes('browser') || mcp.tool.includes('navigate')) return '🌐';
      if (mcp.tool.includes('screenshot')) return '📸';
      if (mcp.tool.includes('click') || mcp.tool.includes('type')) return '👆';
      return '🔌';
    }

    switch (name) {
      case 'Read': return '📄';
      case 'Grep': return '🔍';
      case 'Edit': return '✏️';
      case 'Write': return '📝';
      case 'Bash': return '💻';
      case 'Glob': return '📁';
      case 'Task': return '🚀';
      case 'WebFetch': return '🌐';
      case 'WebSearch': return '🔎';
      case 'TodoWrite': return '☑';
      case 'EnterPlanMode': return '📋';
      case 'ExitPlanMode': return '✅';
      case 'Skill': return '⚡';
      case 'AskUserQuestion': return '❓';
      case 'NotebookEdit': return '📓';
      case 'KillShell': return '🛑';
      case 'TaskOutput': return '📤';
      default: return '🔧';
    }
  };

  // Color for left accent bar based on tool type
  const accentColor = () => {
    const name = props.toolCall.name;

    // MCP tools get a purple accent
    if (isMcp()) return '#a855f7';

    switch (name) {
      case 'Bash': return 'var(--color-accent)'; // Orange for terminal
      case 'Edit':
      case 'Write': return 'var(--color-secondary)'; // Teal for file writes
      case 'Read':
      case 'Glob':
      case 'Grep': return 'var(--color-text-muted)'; // Gray for reads
      case 'TodoWrite': return '#3b82f6'; // Blue for todos
      case 'EnterPlanMode':
      case 'ExitPlanMode': return '#eab308'; // Yellow for plan mode
      case 'Skill': return '#f97316'; // Orange for skills
      case 'AskUserQuestion': return '#8b5cf6'; // Purple for questions
      default: return 'var(--color-accent)';
    }
  };

  const summary = () => {
    const input = props.toolCall.input as Record<string, unknown>;
    const name = props.toolCall.name;

    // Handle MCP tools
    if (isMcp()) {
      const mcp = mcpInfo()!;
      // For browser tools, show URL or element
      if (input.url) return truncate(String(input.url), 30);
      if (input.element) return truncate(String(input.element), 30);
      if (input.text) return truncate(String(input.text), 30);
      return mcp.tool;
    }

    switch (name) {
      case 'Read':
        return input.file_path ? String(input.file_path).split('/').pop() : '';
      case 'Grep':
        return input.pattern ? `"${input.pattern}"` : '';
      case 'Bash': {
        const cmd = String(input.command || '');
        return cmd.length > 30 ? cmd.slice(0, 30) + '...' : cmd;
      }
      case 'Edit':
      case 'Write':
        return input.file_path ? String(input.file_path).split('/').pop() : '';
      case 'Task':
        return input.description ? String(input.description) : '';
      case 'TodoWrite': {
        const todos = input.todos as Array<{ status: string }> | undefined;
        if (todos) {
          const inProgress = todos.filter(t => t.status === 'in_progress').length;
          const completed = todos.filter(t => t.status === 'completed').length;
          return `${completed}/${todos.length} done${inProgress ? `, ${inProgress} active` : ''}`;
        }
        return '';
      }
      case 'Skill':
        return input.skill ? `/${input.skill}` : '';
      case 'AskUserQuestion': {
        const questions = input.questions as Array<{ header?: string }> | undefined;
        if (questions?.[0]?.header) return questions[0].header;
        return questions ? `${questions.length} question(s)` : '';
      }
      default:
        return '';
    }
  };

  return (
    <div
      style={{
        "border-radius": '10px',
        border: '1px solid var(--color-bg-overlay)',
        background: 'var(--color-bg-surface)',
        "box-shadow": '1px 1px 0px rgba(0, 0, 0, 0.2)',
        overflow: 'hidden',
        // Left accent bar
        "border-left": `3px solid ${accentColor()}`,
      }}
    >
      <button
        onClick={() => setExpanded(!expanded())}
        style={{
          width: '100%',
          display: 'flex',
          "align-items": 'center',
          gap: '8px',
          padding: '6px 10px',
          "text-align": 'left',
          "font-size": '13px',
          background: 'transparent',
          border: 'none',
          cursor: 'pointer',
          transition: 'background 0.15s ease',
        }}
        class="pressable"
        onMouseEnter={(e) => e.currentTarget.style.background = 'var(--color-bg-elevated)'}
        onMouseLeave={(e) => e.currentTarget.style.background = 'transparent'}
      >
        <span style={{ "flex-shrink": '0' }}>{icon()}</span>
        <span class="text-mono" style={{ "font-weight": '600', color: isMcp() ? '#a855f7' : 'var(--color-text-primary)', "font-size": isMcp() ? '11px' : '13px' }}>
          {displayName()}
        </span>
        <span
          class="text-mono"
          style={{
            flex: '1',
            overflow: 'hidden',
            "text-overflow": 'ellipsis',
            "white-space": 'nowrap',
            color: 'var(--color-text-muted)',
            "font-size": '12px',
          }}
        >
          {summary()}
        </span>
        {/* Chevron icon */}
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style={{
            color: 'var(--color-text-muted)',
            "flex-shrink": '0',
            transform: expanded() ? 'rotate(180deg)' : 'rotate(0deg)',
            transition: 'transform 0.15s ease',
          }}
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>

      <Show when={expanded()}>
        <div
          style={{
            padding: '8px 10px',
            "border-top": '1px solid var(--color-bg-overlay)',
            "font-size": '12px',
          }}
        >
          <Show when={props.toolCall.input}>
            <div style={{ "margin-bottom": '10px' }}>
              <div
                class="text-mono"
                style={{
                  "font-size": '10px',
                  "font-weight": '500',
                  "text-transform": 'uppercase',
                  "letter-spacing": '0.05em',
                  color: 'var(--color-text-muted)',
                  "margin-bottom": '6px',
                }}
              >
                Input
              </div>
              {formatToolInput(props.toolCall.name, props.toolCall.input as Record<string, unknown>)}
            </div>
          </Show>
          <Show when={props.toolCall.output}>
            <div>
              <div
                class="text-mono"
                style={{
                  "font-size": '10px',
                  "font-weight": '500',
                  "text-transform": 'uppercase',
                  "letter-spacing": '0.05em',
                  color: props.toolCall.isError ? 'var(--color-accent)' : 'var(--color-text-muted)',
                  "margin-bottom": '6px',
                }}
              >
                {props.toolCall.isError ? 'Error' : 'Output'}
              </div>
              {formatToolOutput(props.toolCall.name, props.toolCall.output ?? '', !!props.toolCall.isError)}
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}

// Parse inline markdown (bold, italic, code, links) within a text segment
function InlineMarkdown(props: { text: string }) {
  const parseInline = (text: string): JSX.Element[] => {
    const elements: JSX.Element[] = [];
    let remaining = text;
    let key = 0;

    while (remaining.length > 0) {
      // Check for inline code first (highest priority)
      const codeMatch = remaining.match(/^`([^`]+)`/);
      if (codeMatch) {
        elements.push(
          <code
            style={{
              background: 'var(--color-code-bg)',
              border: '1px solid var(--color-code-border)',
              padding: '2px 6px',
              "border-radius": '4px',
              "font-family": 'var(--font-mono)',
              "font-size": '0.85em',
            }}
          >
            {codeMatch[1]}
          </code>
        );
        remaining = remaining.slice(codeMatch[0].length);
        key++;
        continue;
      }

      // Check for bold (**text** or __text__)
      const boldMatch = remaining.match(/^\*\*([^*]+)\*\*/) || remaining.match(/^__([^_]+)__/);
      if (boldMatch) {
        elements.push(<strong style={{ "font-weight": '600' }}>{boldMatch[1]}</strong>);
        remaining = remaining.slice(boldMatch[0].length);
        key++;
        continue;
      }

      // Check for italic (*text* or _text_) - but not if it's part of **
      const italicMatch = remaining.match(/^\*([^*]+)\*/) || remaining.match(/^_([^_]+)_/);
      if (italicMatch) {
        elements.push(<em style={{ "font-style": 'italic' }}>{italicMatch[1]}</em>);
        remaining = remaining.slice(italicMatch[0].length);
        key++;
        continue;
      }

      // Check for links [text](url)
      const linkMatch = remaining.match(/^\[([^\]]+)\]\(([^)]+)\)/);
      if (linkMatch) {
        elements.push(
          <a
            href={linkMatch[2]}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              color: 'var(--color-accent)',
              "text-decoration": 'underline',
              "text-underline-offset": '2px',
            }}
          >
            {linkMatch[1]}
          </a>
        );
        remaining = remaining.slice(linkMatch[0].length);
        key++;
        continue;
      }

      // Find next special character
      const nextSpecial = Math.min(
        ...[
          remaining.indexOf('`'),
          remaining.indexOf('**'),
          remaining.indexOf('__'),
          remaining.indexOf('*'),
          remaining.indexOf('_'),
          remaining.indexOf('['),
        ].filter(i => i >= 0).concat([Infinity])
      );

      if (nextSpecial === Infinity || nextSpecial === remaining.length) {
        elements.push(<span>{remaining}</span>);
        break;
      }

      if (nextSpecial > 0) {
        elements.push(<span>{remaining.slice(0, nextSpecial)}</span>);
      }
      remaining = remaining.slice(nextSpecial);

      // If we're stuck on a special char that didn't match a pattern, consume it
      if (remaining.length > 0 && nextSpecial === 0) {
        elements.push(<span>{remaining[0]}</span>);
        remaining = remaining.slice(1);
      }
      key++;
    }

    return elements;
  };

  return <>{parseInline(props.text)}</>;
}

function MarkdownContent(props: { content: string }) {
  // Parse markdown into block and inline elements
  const blocks = () => {
    const content = props.content;
    const result: Array<{ type: string; content: string; lang?: string; level?: number; items?: string[] }> = [];

    // First, split by code blocks
    const codeBlockRegex = /```(\w*)\n([\s\S]*?)```/g;
    let lastIndex = 0;
    let match;

    while ((match = codeBlockRegex.exec(content)) !== null) {
      // Add text before code block
      if (match.index > lastIndex) {
        const textBefore = content.slice(lastIndex, match.index);
        parseTextBlocks(textBefore, result);
      }
      // Add code block
      result.push({ type: 'code', lang: match[1] || 'text', content: match[2] });
      lastIndex = match.index + match[0].length;
    }

    // Add remaining text after last code block
    if (lastIndex < content.length) {
      parseTextBlocks(content.slice(lastIndex), result);
    }

    return result;
  };

  // Parse text into headers, lists, paragraphs
  function parseTextBlocks(text: string, result: Array<{ type: string; content: string; level?: number; items?: string[] }>) {
    const lines = text.split('\n');
    let i = 0;
    let paragraphLines: string[] = [];

    // Flush accumulated paragraph lines as a single paragraph block
    const flushParagraph = () => {
      if (paragraphLines.length > 0) {
        result.push({ type: 'paragraph', content: paragraphLines.join('\n') });
        paragraphLines = [];
      }
    };

    while (i < lines.length) {
      const line = lines[i];

      // Check for headers
      const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
      if (headerMatch) {
        flushParagraph();
        result.push({ type: 'header', level: headerMatch[1].length, content: headerMatch[2] });
        i++;
        continue;
      }

      // Check for unordered list
      if (line.match(/^[-*]\s+/)) {
        flushParagraph();
        const items: string[] = [];
        while (i < lines.length && lines[i].match(/^[-*]\s+/)) {
          items.push(lines[i].replace(/^[-*]\s+/, ''));
          i++;
        }
        result.push({ type: 'ul', items, content: '' });
        continue;
      }

      // Check for ordered list
      if (line.match(/^\d+\.\s+/)) {
        flushParagraph();
        const items: string[] = [];
        while (i < lines.length && lines[i].match(/^\d+\.\s+/)) {
          items.push(lines[i].replace(/^\d+\.\s+/, ''));
          i++;
        }
        result.push({ type: 'ol', items, content: '' });
        continue;
      }

      // Empty line ends current paragraph
      if (!line.trim()) {
        flushParagraph();
        i++;
        continue;
      }

      // Accumulate text into current paragraph
      paragraphLines.push(line);
      i++;
    }

    // Flush any remaining paragraph
    flushParagraph();
  }

  const headerStyles = (level: number) => {
    const sizes: Record<number, string> = { 1: '1.5em', 2: '1.3em', 3: '1.15em', 4: '1em', 5: '0.95em', 6: '0.9em' };
    return {
      "font-size": sizes[level] || '1em',
      "font-weight": '600',
      "margin": '8px 0 4px 0',
      "line-height": '1.3',
    };
  };

  return (
    <>
      <For each={blocks()}>
        {(block) => {
          switch (block.type) {
            case 'code':
              return (
                <pre
                  style={{
                    background: 'var(--color-code-bg)',
                    border: '1px solid var(--color-code-border)',
                    "border-radius": '10px',
                    padding: '10px',
                    margin: '6px 0',
                    "overflow-x": 'auto',
                    "font-family": 'var(--font-mono)',
                    "font-size": '13px',
                    "line-height": '1.5',
                    "box-shadow": '1px 1px 0px rgba(0, 0, 0, 0.15)',
                  }}
                >
                  <code>{block.content}</code>
                </pre>
              );
            case 'header':
              return (
                <div style={headerStyles(block.level || 1)}>
                  <InlineMarkdown text={block.content} />
                </div>
              );
            case 'ul':
              return (
                <ul style={{ margin: '4px 0', "padding-left": '20px', "list-style-type": 'disc' }}>
                  <For each={block.items}>
                    {(item) => (
                      <li style={{ margin: '2px 0' }}>
                        <InlineMarkdown text={item} />
                      </li>
                    )}
                  </For>
                </ul>
              );
            case 'ol':
              return (
                <ol style={{ margin: '4px 0', "padding-left": '20px', "list-style-type": 'decimal' }}>
                  <For each={block.items}>
                    {(item) => (
                      <li style={{ margin: '2px 0' }}>
                        <InlineMarkdown text={item} />
                      </li>
                    )}
                  </For>
                </ol>
              );
            case 'paragraph':
              return (
                <p style={{ margin: '4px 0', "white-space": 'pre-wrap' }}>
                  <InlineMarkdown text={block.content} />
                </p>
              );
            default:
              return (
                <span style={{ "font-family": 'inherit', "white-space": 'pre-wrap' }}>
                  <InlineMarkdown text={block.content} />
                </span>
              );
          }
        }}
      </For>
    </>
  );
}
