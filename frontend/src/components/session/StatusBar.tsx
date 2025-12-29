import { Show } from 'solid-js';
import { formatTokens, formatCost, shortenModel } from '../../lib/format';

interface StatusBarProps {
  model?: string;
  cost?: number;
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheCreationTokens?: number;
  contextPercent?: number;
}

export function StatusBar(props: StatusBarProps) {
  const hasData = () =>
    props.inputTokens !== undefined ||
    props.cost !== undefined ||
    props.model !== undefined;

  const contextColor = () => {
    const pct = props.contextPercent ?? 0;
    if (pct >= 80) return '#ef4444'; // Red - danger zone
    if (pct >= 60) return '#eab308'; // Yellow - warning
    return '#22c55e'; // Green - healthy
  };

  return (
    <Show when={hasData()}>
      <div
        style={{
          display: 'flex',
          'align-items': 'center',
          'justify-content': 'center',
          gap: '8px',
          padding: '4px 12px',
          background: 'var(--color-bg-surface)',
          'border-bottom': '1px solid var(--color-bg-overlay)',
          'font-family': 'var(--font-mono)',
          'font-size': '11px',
          color: 'var(--color-text-muted)',
          'flex-wrap': 'wrap',
        }}
      >
        {/* Model */}
        <Show when={props.model}>
          <span style={{ color: 'var(--color-text-secondary)' }}>
            {shortenModel(props.model)}
          </span>
          <span style={{ opacity: 0.4 }}>|</span>
        </Show>

        {/* Cost */}
        <Show when={props.cost !== undefined}>
          <span style={{ color: '#22c55e' }}>{formatCost(props.cost)}</span>
          <span style={{ opacity: 0.4 }}>|</span>
        </Show>

        {/* Tokens (input/output) */}
        <Show when={props.inputTokens !== undefined}>
          <span>
            <span style={{ color: 'var(--color-text-secondary)' }}>
              {formatTokens(props.inputTokens)}
            </span>
            <span style={{ opacity: 0.6 }}>/</span>
            <span style={{ color: 'var(--color-text-secondary)' }}>
              {formatTokens(props.outputTokens)}
            </span>
          </span>
        </Show>

        {/* Cache tokens - shows tokens served from cache (cost savings) */}
        <Show when={(props.cacheReadTokens ?? 0) > 0}>
          <span style={{ opacity: 0.4 }}>|</span>
          <span style={{ color: '#8b5cf6' }}>
            {formatTokens(props.cacheReadTokens)} cached
          </span>
        </Show>

        {/* Context percentage with mini progress bar */}
        <Show when={props.contextPercent !== undefined}>
          <span style={{ opacity: 0.4 }}>|</span>
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              gap: '4px',
            }}
          >
            <span style={{ color: contextColor() }}>{props.contextPercent}%</span>
            <div
              style={{
                width: '40px',
                height: '4px',
                background: 'var(--color-bg-overlay)',
                'border-radius': '2px',
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  width: `${Math.min(props.contextPercent ?? 0, 100)}%`,
                  height: '100%',
                  background: contextColor(),
                  transition: 'width 0.3s ease, background 0.3s ease',
                }}
              />
            </div>
          </div>
        </Show>
      </div>
    </Show>
  );
}
