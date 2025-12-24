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
        class={`max-w-[85%] rounded-2xl px-4 py-3 ${
          isUser()
            ? 'bg-accent text-white'
            : 'bg-bg-elevated text-text-primary'
        }`}
      >
        {/* Thinking indicator - shown when streaming but no content yet */}
        <Show when={isThinking()}>
          <div class="flex items-center gap-2 text-text-muted">
            <div class="flex gap-1">
              <span class="w-2 h-2 rounded-full bg-accent animate-bounce" style={{ "animation-delay": "0ms" }} />
              <span class="w-2 h-2 rounded-full bg-accent animate-bounce" style={{ "animation-delay": "150ms" }} />
              <span class="w-2 h-2 rounded-full bg-accent animate-bounce" style={{ "animation-delay": "300ms" }} />
            </div>
            <span class="text-sm">Thinking...</span>
          </div>
        </Show>

        {/* Tool Calls */}
        <Show when={props.message.toolCalls?.length}>
          <div class="space-y-2 mb-3">
            <For each={props.message.toolCalls}>
              {(toolCall) => <ToolCallView toolCall={toolCall} />}
            </For>
          </div>
        </Show>

        {/* Content */}
        <Show when={props.message.content}>
          <div class="whitespace-pre-wrap break-words">
            <MarkdownContent content={props.message.content} />
          </div>
        </Show>

        {/* Streaming indicator - shown while content is still being added */}
        <Show when={props.message.isStreaming && hasContent()}>
          <span class="inline-block w-2 h-4 bg-accent ml-1 animate-pulse" />
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
      default: return '🔧';
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
    return '';
  };

  return (
    <div class="bg-bg-overlay rounded-lg overflow-hidden">
      <button
        class="w-full flex items-center gap-2 px-3 py-2 text-left text-sm hover:bg-bg-base/50"
        onClick={() => setExpanded(!expanded())}
      >
        <span>{icon()}</span>
        <span class="font-medium">{props.toolCall.name}</span>
        <span class="text-text-muted truncate flex-1">{summary()}</span>
        <span class="text-text-muted">{expanded() ? '▼' : '▶'}</span>
      </button>

      <Show when={expanded()}>
        <div class="px-3 py-2 border-t border-bg-base text-xs">
          <Show when={props.toolCall.input}>
            <div class="mb-2">
              <div class="text-text-muted mb-1">Input:</div>
              <pre class="bg-bg-base rounded p-2 overflow-x-auto">
                {JSON.stringify(props.toolCall.input, null, 2)}
              </pre>
            </div>
          </Show>
          <Show when={props.toolCall.output}>
            <div>
              <div class="text-text-muted mb-1">
                Output{props.toolCall.isError ? ' (error)' : ''}:
              </div>
              <pre class={`bg-bg-base rounded p-2 overflow-x-auto max-h-48 ${props.toolCall.isError ? 'text-red-400' : ''}`}>
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
                <pre class="bg-code-bg border border-code-border rounded-lg p-3 my-2 overflow-x-auto font-mono text-sm">
                  <code>{part.content}</code>
                </pre>
              );
            case 'inline-code':
              return (
                <code class="bg-code-bg px-1.5 py-0.5 rounded font-mono text-sm">
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
