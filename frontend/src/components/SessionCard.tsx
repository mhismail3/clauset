import { Show, For, createMemo } from 'solid-js';
import { A } from '@solidjs/router';
import { Badge } from './ui/Badge';
import { Session, RecentAction } from '../lib/api';
import { getStatusVariant, getStatusLabel, formatRelativeTime } from '../stores/sessions';

interface SessionCardProps {
  session: Session;
  onMenuOpen: (e: Event, session: Session) => void;
}

// Icon components for different action types
function ActionIcon(props: { type: string }) {
  const iconMap: Record<string, () => JSX.Element> = {
    read: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
      </svg>
    ),
    write: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
      </svg>
    ),
    edit: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
      </svg>
    ),
    bash: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="4 17 10 11 4 5" />
        <line x1="12" y1="19" x2="20" y2="19" />
      </svg>
    ),
    search: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
    ),
    task: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
        <line x1="9" y1="9" x2="15" y2="15" />
        <line x1="15" y1="9" x2="9" y2="15" />
      </svg>
    ),
    web: () => (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10" />
        <line x1="2" y1="12" x2="22" y2="12" />
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
      </svg>
    ),
  };

  const Icon = iconMap[props.type] || iconMap.task;
  return <Icon />;
}

// Step badge with appropriate styling
function StepBadge(props: { step?: string }) {
  const stepColors: Record<string, { bg: string; text: string }> = {
    Read: { bg: 'rgba(91, 138, 154, 0.15)', text: '#5b8a9a' },
    Write: { bg: 'rgba(44, 143, 122, 0.15)', text: '#2c8f7a' },
    Edit: { bg: 'rgba(212, 166, 68, 0.15)', text: '#d4a644' },
    Bash: { bg: 'rgba(196, 91, 55, 0.15)', text: '#c45b37' },
    Grep: { bg: 'rgba(138, 107, 148, 0.15)', text: '#8a6b94' },
    Glob: { bg: 'rgba(138, 107, 148, 0.15)', text: '#8a6b94' },
    Task: { bg: 'rgba(91, 154, 138, 0.15)', text: '#5b9a8a' },
    Web: { bg: 'rgba(91, 138, 154, 0.15)', text: '#5b8a9a' },
    Thinking: { bg: 'rgba(240, 235, 227, 0.1)', text: 'var(--color-text-secondary)' },
    Planning: { bg: 'rgba(240, 235, 227, 0.1)', text: 'var(--color-text-secondary)' },
  };

  const colors = () => stepColors[props.step || ''] || { bg: 'rgba(240, 235, 227, 0.1)', text: 'var(--color-text-muted)' };

  return (
    <Show when={props.step}>
      <span
        class="text-mono"
        style={{
          display: 'inline-flex',
          'align-items': 'center',
          padding: '2px 6px',
          'font-size': '10px',
          'font-weight': '500',
          'border-radius': '4px',
          background: colors().bg,
          color: colors().text,
          'white-space': 'nowrap',
        }}
      >
        {props.step}
      </span>
    </Show>
  );
}

export function SessionCard(props: SessionCardProps) {
  // Memoize recent actions to limit to 3 most recent
  const displayActions = createMemo(() => {
    const actions = props.session.recent_actions || [];
    return actions.slice(-3).reverse(); // Most recent first, max 3
  });

  const hasActivity = createMemo(() => {
    return props.session.preview && props.session.preview !== 'No preview available';
  });

  return (
    <div
      class="card-retro card-pressable"
      style={{ overflow: 'hidden', position: 'relative' }}
    >
      <A
        href={`/session/${props.session.id}`}
        style={{
          display: 'block',
          padding: '16px',
          'text-decoration': 'none',
          color: 'inherit',
        }}
      >
        {/* Top row: Project name + Badge + Menu */}
        <div style={{ display: 'flex', 'align-items': 'center', gap: '12px', 'margin-bottom': '10px' }}>
          <div style={{ flex: '1', 'min-width': '0', display: 'flex', 'align-items': 'center', gap: '8px' }}>
            <span
              class="text-mono"
              style={{
                'font-size': '14px',
                'font-weight': '600',
                color: 'var(--color-text-primary)',
                overflow: 'hidden',
                'text-overflow': 'ellipsis',
                'white-space': 'nowrap',
              }}
            >
              {props.session.project_path.split('/').pop() || 'Unknown'}
            </span>
            <Badge variant={getStatusVariant(props.session.status)}>
              {getStatusLabel(props.session.status)}
            </Badge>
            <StepBadge step={props.session.current_step} />
          </div>
          <button
            onClick={(e) => props.onMenuOpen(e, props.session)}
            class="pressable"
            style={{
              width: '32px',
              height: '32px',
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              color: 'var(--color-text-muted)',
              background: 'var(--color-bg-elevated)',
              border: '1px solid var(--color-bg-overlay)',
              'border-radius': '8px',
              cursor: 'pointer',
              'flex-shrink': '0',
            }}
          >
            <svg width="16" height="16" viewBox="0 0 20 20" fill="currentColor">
              <circle cx="4" cy="10" r="1.5" />
              <circle cx="10" cy="10" r="1.5" />
              <circle cx="16" cy="10" r="1.5" />
            </svg>
          </button>
        </div>

        {/* Current activity - main preview line */}
        <div
          style={{
            'font-size': '14px',
            color: hasActivity() ? 'var(--color-text-primary)' : 'var(--color-text-muted)',
            'margin-bottom': '8px',
            overflow: 'hidden',
            'text-overflow': 'ellipsis',
            'white-space': 'nowrap',
            'font-style': hasActivity() ? 'normal' : 'italic',
          }}
        >
          {props.session.preview || 'Waiting for activity...'}
        </div>

        {/* Recent actions - detailed sub-lines */}
        <Show when={displayActions().length > 0}>
          <div
            style={{
              display: 'flex',
              'flex-direction': 'column',
              gap: '4px',
              'margin-bottom': '12px',
              padding: '8px 10px',
              background: 'var(--color-bg-base)',
              'border-radius': '8px',
              'border-left': '2px solid var(--color-accent)',
            }}
          >
            <For each={displayActions()}>
              {(action, index) => (
                <div
                  style={{
                    display: 'flex',
                    'align-items': 'flex-start',
                    gap: '8px',
                    opacity: index() === 0 ? '1' : '0.7',
                  }}
                >
                  <span
                    style={{
                      color: 'var(--color-accent)',
                      'flex-shrink': '0',
                      'margin-top': '2px',
                    }}
                  >
                    <ActionIcon type={action.action_type} />
                  </span>
                  <div style={{ 'min-width': '0', flex: '1' }}>
                    <span
                      class="text-mono"
                      style={{
                        'font-size': '12px',
                        color: 'var(--color-text-secondary)',
                      }}
                    >
                      {action.summary}
                    </span>
                    <Show when={action.detail}>
                      <div
                        class="text-mono"
                        style={{
                          'font-size': '11px',
                          color: 'var(--color-text-muted)',
                          overflow: 'hidden',
                          'text-overflow': 'ellipsis',
                          'white-space': 'nowrap',
                        }}
                      >
                        {action.detail}
                      </div>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>

        {/* Time info */}
        <div
          class="text-mono"
          style={{
            display: 'flex',
            'align-items': 'center',
            gap: '6px',
            'font-size': '11px',
            color: 'var(--color-text-muted)',
            'margin-bottom': '10px',
          }}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <polyline points="12 6 12 12 16 14" />
          </svg>
          <span>{formatRelativeTime(props.session.last_activity_at)}</span>
        </div>

        {/* Status line - Claude Code style */}
        <div
          class="text-mono"
          style={{
            display: 'flex',
            'align-items': 'center',
            gap: '8px',
            'font-size': '11px',
            color: 'var(--color-text-tertiary)',
            padding: '8px 10px',
            background: 'var(--color-bg-base)',
            'border-radius': '8px',
            margin: '0 -4px',
            'flex-wrap': 'wrap',
          }}
        >
          <span>{props.session.model}</span>
          <span style={{ color: 'var(--color-text-muted)' }}>|</span>
          <span>${props.session.total_cost_usd.toFixed(2)}</span>
          <Show when={props.session.input_tokens > 0 || props.session.output_tokens > 0}>
            <span style={{ color: 'var(--color-text-muted)' }}>|</span>
            <span>
              {(props.session.input_tokens / 1000).toFixed(1)}K/{(props.session.output_tokens / 1000).toFixed(1)}K
            </span>
          </Show>
          <Show when={props.session.context_percent > 0}>
            <span style={{ color: 'var(--color-text-muted)' }}>|</span>
            <span>ctx:{props.session.context_percent}%</span>
          </Show>
        </div>
      </A>
    </div>
  );
}
