import { Show, For, createSignal } from 'solid-js';

interface AllowedToolsPanelProps {
  allowedTools?: string[];
}

// Categorize tools by type
function categorizeTool(tool: string): 'read' | 'write' | 'execute' | 'web' | 'other' {
  const toolLower = tool.toLowerCase();
  if (toolLower.includes('read') || toolLower.includes('glob') || toolLower.includes('grep')) {
    return 'read';
  }
  if (toolLower.includes('write') || toolLower.includes('edit')) {
    return 'write';
  }
  if (toolLower.includes('bash') || toolLower.includes('execute')) {
    return 'execute';
  }
  if (toolLower.includes('web') || toolLower.includes('fetch') || toolLower.includes('browser')) {
    return 'web';
  }
  return 'other';
}

const categoryLabels: Record<string, { label: string; color: string }> = {
  read: { label: 'Read', color: '#3b82f6' },
  write: { label: 'Write', color: '#22c55e' },
  execute: { label: 'Execute', color: '#eab308' },
  web: { label: 'Web', color: '#8b5cf6' },
  other: { label: 'Other', color: 'var(--color-text-muted)' },
};

export function AllowedToolsPanel(props: AllowedToolsPanelProps) {
  const [expanded, setExpanded] = createSignal(false);

  const hasTools = () => props.allowedTools && props.allowedTools.length > 0;

  const categorizedTools = () => {
    if (!props.allowedTools) return {};
    const result: Record<string, string[]> = {};
    for (const tool of props.allowedTools) {
      const category = categorizeTool(tool);
      if (!result[category]) result[category] = [];
      result[category].push(tool);
    }
    return result;
  };

  const categories = () => Object.entries(categorizedTools());

  return (
    <Show when={hasTools()}>
      <div
        style={{
          background: 'var(--color-bg-surface)',
          border: '1px solid var(--color-bg-overlay)',
          'border-radius': '8px',
          margin: '8px 16px',
          overflow: 'hidden',
          'box-shadow': '1px 1px 0px rgba(0, 0, 0, 0.15)',
        }}
      >
        {/* Header */}
        <button
          onClick={() => setExpanded(!expanded())}
          style={{
            width: '100%',
            display: 'flex',
            'align-items': 'center',
            'justify-content': 'space-between',
            padding: '8px 12px',
            background: 'transparent',
            border: 'none',
            cursor: 'pointer',
            'font-family': 'var(--font-mono)',
            'font-size': '12px',
            color: 'var(--color-text-primary)',
          }}
        >
          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
            <span>🔧</span>
            <span style={{ 'font-weight': '600' }}>Allowed Tools</span>
            <span style={{ color: 'var(--color-text-muted)', 'font-size': '11px' }}>
              {props.allowedTools?.length ?? 0} tools
            </span>
          </div>
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style={{
              color: 'var(--color-text-muted)',
              transform: expanded() ? 'rotate(180deg)' : 'rotate(0deg)',
              transition: 'transform 0.15s ease',
            }}
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </button>

        {/* Tools list by category */}
        <Show when={expanded()}>
          <div
            style={{
              padding: '0 12px 12px',
              'max-height': '250px',
              'overflow-y': 'auto',
            }}
          >
            <For each={categories()}>
              {([category, tools]) => (
                <div style={{ 'margin-bottom': '8px' }}>
                  <div
                    style={{
                      display: 'flex',
                      'align-items': 'center',
                      gap: '6px',
                      'margin-bottom': '4px',
                    }}
                  >
                    <span
                      style={{
                        width: '6px',
                        height: '6px',
                        'border-radius': '50%',
                        background: categoryLabels[category]?.color ?? 'var(--color-text-muted)',
                      }}
                    />
                    <span
                      class="text-mono"
                      style={{
                        'font-size': '10px',
                        'font-weight': '600',
                        color: 'var(--color-text-muted)',
                        'text-transform': 'uppercase',
                        'letter-spacing': '0.05em',
                      }}
                    >
                      {categoryLabels[category]?.label ?? category}
                    </span>
                  </div>
                  <div style={{ display: 'flex', 'flex-wrap': 'wrap', gap: '4px', 'padding-left': '12px' }}>
                    <For each={tools as string[]}>
                      {(tool) => (
                        <span
                          class="text-mono"
                          style={{
                            'font-size': '10px',
                            padding: '2px 6px',
                            background: 'var(--color-bg-overlay)',
                            'border-radius': '4px',
                            color: 'var(--color-text-secondary)',
                          }}
                        >
                          {tool}
                        </span>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}
