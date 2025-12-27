import { Show, For, createSignal } from 'solid-js';

export interface ActiveSubagent {
  agentId: string;
  agentType: string;
  startedAt: number;
}

interface SubagentPanelProps {
  subagents: ActiveSubagent[];
}

// Format agent type for display
function formatAgentType(type: string): string {
  if (type === 'general-purpose') return 'Agent';
  // Capitalize first letter
  return type.charAt(0).toUpperCase() + type.slice(1);
}

// Calculate elapsed time
function formatElapsed(startedAt: number): string {
  const elapsed = Math.floor((Date.now() - startedAt) / 1000);
  if (elapsed < 60) return `${elapsed}s`;
  const mins = Math.floor(elapsed / 60);
  const secs = elapsed % 60;
  return `${mins}m ${secs}s`;
}

export function SubagentPanel(props: SubagentPanelProps) {
  const [expanded, setExpanded] = createSignal(true);

  const hasSubagents = () => props.subagents.length > 0;

  return (
    <Show when={hasSubagents()}>
      <div
        style={{
          background: 'rgba(59, 130, 246, 0.1)',
          border: '1px solid rgba(59, 130, 246, 0.3)',
          'border-radius': '8px',
          margin: '8px 16px',
          overflow: 'hidden',
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
            color: '#3b82f6',
          }}
        >
          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
            <span style={{ animation: 'pulse 1.5s ease-in-out infinite' }}>🚀</span>
            <span style={{ 'font-weight': '600' }}>
              {props.subagents.length} Active Agent{props.subagents.length !== 1 ? 's' : ''}
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
              transform: expanded() ? 'rotate(180deg)' : 'rotate(0deg)',
              transition: 'transform 0.15s ease',
            }}
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </button>

        {/* Subagent list */}
        <Show when={expanded()}>
          <div style={{ padding: '0 12px 8px' }}>
            <For each={props.subagents}>
              {(subagent) => (
                <div
                  style={{
                    display: 'flex',
                    'align-items': 'center',
                    'justify-content': 'space-between',
                    padding: '6px 8px',
                    background: 'rgba(59, 130, 246, 0.1)',
                    'border-radius': '6px',
                    'margin-bottom': '4px',
                    'font-size': '12px',
                    'font-family': 'var(--font-mono)',
                  }}
                >
                  <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
                    <span
                      style={{
                        width: '6px',
                        height: '6px',
                        'border-radius': '50%',
                        background: '#3b82f6',
                        animation: 'pulse 1s ease-in-out infinite',
                      }}
                    />
                    <span style={{ color: 'var(--color-text-primary)' }}>
                      {formatAgentType(subagent.agentType)}
                    </span>
                  </div>
                  <span style={{ color: 'var(--color-text-muted)', 'font-size': '11px' }}>
                    {formatElapsed(subagent.startedAt)}
                  </span>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}
