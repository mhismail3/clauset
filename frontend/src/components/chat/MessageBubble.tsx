import { Show, For, createSignal, JSX } from 'solid-js';
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
          <div style={{ display: 'flex', "flex-direction": 'column', gap: '8px', "margin-bottom": '12px' }}>
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

    while (i < lines.length) {
      const line = lines[i];

      // Check for headers
      const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
      if (headerMatch) {
        result.push({ type: 'header', level: headerMatch[1].length, content: headerMatch[2] });
        i++;
        continue;
      }

      // Check for unordered list
      if (line.match(/^[-*]\s+/)) {
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
        const items: string[] = [];
        while (i < lines.length && lines[i].match(/^\d+\.\s+/)) {
          items.push(lines[i].replace(/^\d+\.\s+/, ''));
          i++;
        }
        result.push({ type: 'ol', items, content: '' });
        continue;
      }

      // Regular text (paragraph)
      if (line.trim()) {
        result.push({ type: 'text', content: line });
      }
      i++;
    }
  }

  const headerStyles = (level: number) => {
    const sizes: Record<number, string> = { 1: '1.5em', 2: '1.3em', 3: '1.15em', 4: '1em', 5: '0.95em', 6: '0.9em' };
    return {
      "font-size": sizes[level] || '1em',
      "font-weight": '600',
      "margin": '12px 0 6px 0',
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
                    padding: '12px',
                    margin: '8px 0',
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
                <ul style={{ margin: '8px 0', "padding-left": '20px', "list-style-type": 'disc' }}>
                  <For each={block.items}>
                    {(item) => (
                      <li style={{ margin: '4px 0' }}>
                        <InlineMarkdown text={item} />
                      </li>
                    )}
                  </For>
                </ul>
              );
            case 'ol':
              return (
                <ol style={{ margin: '8px 0', "padding-left": '20px', "list-style-type": 'decimal' }}>
                  <For each={block.items}>
                    {(item) => (
                      <li style={{ margin: '4px 0' }}>
                        <InlineMarkdown text={item} />
                      </li>
                    )}
                  </For>
                </ol>
              );
            default:
              return (
                <span style={{ "font-family": 'inherit' }}>
                  <InlineMarkdown text={block.content} />
                </span>
              );
          }
        }}
      </For>
    </>
  );
}
