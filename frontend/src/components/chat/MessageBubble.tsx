import { Show, For, createSignal } from 'solid-js';
import { Message, ToolCall } from '../../stores/messages';

interface MessageBubbleProps {
  message: Message;
}

export function MessageBubble(props: MessageBubbleProps) {
  const isUser = () => props.message.role === 'user';
  const isThinking = () =>
    !isUser() &&
    props.message.isStreaming &&
    !props.message.content &&
    !props.message.toolCalls?.length;
  const hasContent = () => props.message.content || props.message.toolCalls?.length;

  return (
    <div class={`flex ${isUser() ? 'justify-end' : 'justify-start'}`}>
      <div
        style={{
          "max-width": '85%',
          "border-radius": '14px',
          padding: '12px 16px',
          // Retro styling
          background: isUser() ? 'var(--color-accent)' : 'var(--color-bg-elevated)',
          color: isUser() ? '#ffffff' : 'var(--color-text-primary)',
          border: isUser()
            ? '1px solid var(--color-accent-active)'
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
          <div style={{ display: 'flex', "flex-direction": 'column', gap: '8px', "margin-bottom": '12px' }}>
            <For each={props.message.toolCalls}>
              {(toolCall) => <ToolCallView toolCall={toolCall} />}
            </For>
          </div>
        </Show>

        {/* Content */}
        <Show when={props.message.content}>
          <div style={{ "white-space": 'pre-wrap', "word-break": 'break-word' }}>
            <MarkdownContent content={props.message.content} />
          </div>
        </Show>

        {/* Streaming indicator - shown while content is still being added */}
        <Show when={props.message.isStreaming && hasContent()}>
          <span
            style={{
              display: 'inline-block',
              width: '2px',
              height: '16px',
              background: isUser() ? '#ffffff' : 'var(--color-accent)',
              "margin-left": '2px',
              "vertical-align": 'text-bottom',
              animation: 'blink 1s step-end infinite',
            }}
          />
        </Show>
      </div>
    </div>
  );
}

function ToolCallView(props: { toolCall: ToolCall }) {
  const [expanded, setExpanded] = createSignal(false);

  const icon = () => {
    switch (props.toolCall.name) {
      case 'Read': return '📄';
      case 'Grep': return '🔍';
      case 'Edit': return '✏️';
      case 'Write': return '📝';
      case 'Bash': return '💻';
      case 'Glob': return '📁';
      case 'Task': return '🚀';
      case 'WebFetch': return '🌐';
      case 'WebSearch': return '🔎';
      default: return '🔧';
    }
  };

  // Color for left accent bar based on tool type
  const accentColor = () => {
    switch (props.toolCall.name) {
      case 'Bash': return 'var(--color-accent)'; // Orange for terminal
      case 'Edit':
      case 'Write': return 'var(--color-secondary)'; // Teal for file writes
      case 'Read':
      case 'Glob':
      case 'Grep': return 'var(--color-text-muted)'; // Gray for reads
      default: return 'var(--color-accent)';
    }
  };

  const summary = () => {
    const input = props.toolCall.input as Record<string, unknown>;
    if (props.toolCall.name === 'Read' && input.file_path) {
      return String(input.file_path).split('/').pop();
    }
    if (props.toolCall.name === 'Grep' && input.pattern) {
      return `"${input.pattern}"`;
    }
    if (props.toolCall.name === 'Bash' && input.command) {
      const cmd = String(input.command);
      return cmd.length > 30 ? cmd.slice(0, 30) + '...' : cmd;
    }
    if (props.toolCall.name === 'Edit' && input.file_path) {
      return String(input.file_path).split('/').pop();
    }
    if (props.toolCall.name === 'Write' && input.file_path) {
      return String(input.file_path).split('/').pop();
    }
    if (props.toolCall.name === 'Task' && input.description) {
      return String(input.description);
    }
    return '';
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
          padding: '8px 12px',
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
        <span class="text-mono" style={{ "font-weight": '600', color: 'var(--color-text-primary)' }}>
          {props.toolCall.name}
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
            padding: '10px 12px',
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
              <pre
                style={{
                  background: 'var(--color-code-bg)',
                  border: '1px solid var(--color-code-border)',
                  "border-radius": '8px',
                  padding: '10px',
                  "overflow-x": 'auto',
                  margin: '0',
                  "font-family": 'var(--font-mono)',
                  "font-size": '11px',
                  "line-height": '1.5',
                  color: 'var(--color-text-secondary)',
                }}
              >
                {JSON.stringify(props.toolCall.input, null, 2)}
              </pre>
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
              <pre
                style={{
                  background: 'var(--color-code-bg)',
                  border: props.toolCall.isError
                    ? '1px solid var(--color-accent)'
                    : '1px solid var(--color-code-border)',
                  "border-radius": '8px',
                  padding: '10px',
                  "overflow-x": 'auto',
                  "max-height": '200px',
                  "overflow-y": 'auto',
                  margin: '0',
                  "font-family": 'var(--font-mono)',
                  "font-size": '11px',
                  "line-height": '1.5',
                  color: props.toolCall.isError ? 'var(--color-accent)' : 'var(--color-text-secondary)',
                }}
              >
                {props.toolCall.output}
              </pre>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function MarkdownContent(props: { content: string }) {
  // Simple markdown rendering - code blocks and inline code
  const parts = () => {
    const content = props.content;
    const segments: Array<{ type: 'text' | 'code' | 'inline-code'; content: string; lang?: string }> = [];

    let remaining = content;

    while (remaining.length > 0) {
      // Check for code block
      const codeBlockMatch = remaining.match(/^```(\w*)\n([\s\S]*?)```/);
      if (codeBlockMatch) {
        segments.push({
          type: 'code',
          lang: codeBlockMatch[1] || 'text',
          content: codeBlockMatch[2],
        });
        remaining = remaining.slice(codeBlockMatch[0].length);
        continue;
      }

      // Check for inline code
      const inlineCodeMatch = remaining.match(/^`([^`]+)`/);
      if (inlineCodeMatch) {
        segments.push({ type: 'inline-code', content: inlineCodeMatch[1] });
        remaining = remaining.slice(inlineCodeMatch[0].length);
        continue;
      }

      // Find next special pattern
      const nextCode = remaining.indexOf('```');
      const nextInline = remaining.indexOf('`');
      const nextSpecial = Math.min(
        nextCode >= 0 ? nextCode : Infinity,
        nextInline >= 0 ? nextInline : Infinity
      );

      if (nextSpecial === Infinity) {
        segments.push({ type: 'text', content: remaining });
        break;
      }

      if (nextSpecial > 0) {
        segments.push({ type: 'text', content: remaining.slice(0, nextSpecial) });
      }
      remaining = remaining.slice(nextSpecial);
    }

    return segments;
  };

  return (
    <>
      <For each={parts()}>
        {(part) => {
          switch (part.type) {
            case 'code':
              return (
                <pre
                  style={{
                    background: 'var(--color-code-bg)',
                    border: '1px solid var(--color-code-border)',
                    "border-radius": '10px',
                    padding: '12px',
                    margin: '8px 0',
                    "overflow-x": 'auto',
                    "font-family": 'var(--font-mono)',
                    "font-size": '13px',
                    "line-height": '1.5',
                    "box-shadow": '1px 1px 0px rgba(0, 0, 0, 0.15)',
                  }}
                >
                  <code>{part.content}</code>
                </pre>
              );
            case 'inline-code':
              return (
                <code
                  style={{
                    background: 'var(--color-code-bg)',
                    border: '1px solid var(--color-code-border)',
                    padding: '2px 6px',
                    "border-radius": '4px',
                    "font-family": 'var(--font-mono)',
                    "font-size": '0.9em',
                  }}
                >
                  {part.content}
                </code>
              );
            default:
              return <span>{part.content}</span>;
          }
        }}
      </For>
    </>
  );
}
