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
    // Thinking/Planning states - orange/warning color to indicate active work
    Thinking: { bg: 'rgba(212, 166, 68, 0.15)', text: '#d4a644' },
    Planning: { bg: 'rgba(212, 166, 68, 0.15)', text: '#d4a644' },
    Working: { bg: 'rgba(212, 166, 68, 0.15)', text: '#d4a644' },
    // Ready state - green
    Ready: { bg: 'rgba(44, 143, 122, 0.15)', text: '#2c8f7a' },
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

// Status indicator when there are no recent actions
function StatusIndicator(props: { status: Session['status']; preview?: string; currentStep?: string }) {
  // Determine what to show based on session status
  const getStatusDisplay = () => {
    // Check if current step indicates thinking/processing (orange)
    const stepLower = props.currentStep?.toLowerCase();
    if (stepLower === 'thinking' || stepLower === 'planning') {
      return { icon: '●', text: props.currentStep!, color: 'var(--color-warning, #d4a644)' };
    }

    // Check if current step is "Ready" - show green
    const isReady = stepLower === 'ready' || props.preview?.toLowerCase() === 'ready';
    if (isReady) {
      return { icon: '✓', text: 'Ready', color: '#2c8f7a' };
    }

    // Check if current step is a tool name (active work, show in orange)
    const toolNames = ['read', 'edit', 'write', 'bash', 'grep', 'glob', 'task', 'search', 'webfetch', 'websearch'];
    if (stepLower && toolNames.includes(stepLower.toLowerCase())) {
      return { icon: '●', text: props.currentStep!, color: 'var(--color-warning, #d4a644)' };
    }

    switch (props.status) {
      case 'active':
        // If active but no explicit step, default to "Ready" (Claude Code starts at prompt)
        // Only show preview if it contains meaningful activity info
        if (props.preview && props.preview !== 'No preview available' &&
            !props.preview.toLowerCase().includes('ready')) {
          return { icon: '●', text: props.preview, color: 'var(--color-warning, #d4a644)' };
        }
        return { icon: '✓', text: 'Ready', color: '#2c8f7a' };
      case 'starting':
        return { icon: '◐', text: 'Starting session...', color: 'var(--color-text-secondary)' };
      case 'waiting_input':
        return { icon: '▸', text: 'Waiting for your input', color: 'var(--color-accent)' };
      case 'stopped':
        return { icon: '✓', text: 'Completed', color: '#2c8f7a' };
      case 'created':
        return { icon: '○', text: 'Ready to start', color: 'var(--color-text-muted)' };
      case 'error':
        return { icon: '✕', text: 'Error occurred', color: 'var(--color-accent)' };
      default:
        return { icon: '○', text: 'No activity', color: 'var(--color-text-muted)' };
    }
  };

  const status = getStatusDisplay();

  return (
    <div
      style={{
        display: 'flex',
        'align-items': 'center',
        gap: '8px',
        'margin-bottom': '12px',
        padding: '10px 12px',
        background: 'var(--color-bg-base)',
        'border-radius': '8px',
        'border-left': `2px solid ${status.color}`,
      }}
    >
      <span
        class="text-mono"
        style={{
          color: status.color,
          'font-size': '12px',
        }}
      >
        {status.icon}
      </span>
      <span
        class="text-mono"
        style={{
          'font-size': '12px',
          color: status.color === 'var(--color-accent)' || status.color === '#2c8f7a'
            ? status.color
            : 'var(--color-text-secondary)',
          'font-style': props.status === 'starting' ? 'italic' : 'normal',
        }}
      >
        {status.text}
      </span>
    </div>
  );
}

export function SessionCard(props: SessionCardProps) {
  // Memoize recent actions to limit to 3 most recent
  const displayActions = createMemo(() => {
    const actions = props.session.recent_actions || [];
    return actions.slice(-3).reverse(); // Most recent first, max 3
  });

  // Determine if we should show status indicator (without actions)
  // Only show StatusIndicator alone when there are no actions to display
  const showStatusIndicatorOnly = createMemo(() => {
    const status = props.session.status;
    const hasActions = displayActions().length > 0;

    // For terminal states without actions, show status indicator
    if ((status === 'stopped' || status === 'waiting_input' || status === 'error' || status === 'created') && !hasActions) {
      return true;
    }
    // For active/starting states without actions, show processing indicator
    if ((status === 'active' || status === 'starting') && !hasActions) {
      return true;
    }
    return false;
  });

  // Show actions whenever we have them, regardless of status
  const showActions = createMemo(() => {
    return displayActions().length > 0;
  });

  // Determine if session is actively working (thinking, running tools)
  const isActivelyWorking = createMemo(() => {
    const step = props.session.current_step?.toLowerCase();
    // Actively working if: thinking, planning, or executing a tool
    if (step === 'thinking' || step === 'planning') return true;
    const toolNames = ['read', 'edit', 'write', 'bash', 'grep', 'glob', 'task', 'search', 'webfetch', 'websearch'];
    if (step && toolNames.includes(step)) return true;
    // Also actively working if status is active but step is NOT ready
    if (props.session.status === 'active' && step && step !== 'ready') return true;
    return false;
  });

  // Get the status color for the actions container border
  const getActionsBorderColor = createMemo(() => {
    const step = props.session.current_step?.toLowerCase();

    // Orange for thinking/planning/tool execution
    if (step === 'thinking' || step === 'planning') {
      return 'var(--color-warning, #d4a644)';
    }

    // Orange for tool execution
    const toolNames = ['read', 'edit', 'write', 'bash', 'grep', 'glob', 'task', 'search', 'webfetch', 'websearch'];
    if (step && toolNames.includes(step)) {
      return 'var(--color-warning, #d4a644)';
    }

    // Green for ready state
    if (step === 'ready') {
      return '#2c8f7a';
    }

    switch (props.session.status) {
      case 'active':
      case 'starting':
        // If active with no specific step, default to ready (green)
        return '#2c8f7a';
      case 'stopped':
        return '#2c8f7a';
      case 'waiting_input':
        return '#2c8f7a';
      case 'error':
        return 'var(--color-accent)';
      default:
        return 'var(--color-text-muted)';
    }
  });

  return (
    <div
      class="card-retro"
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

        {/* Status indicator only when no actions to display */}
        <Show when={showStatusIndicatorOnly()}>
          <StatusIndicator
            status={props.session.status}
            preview={props.session.preview}
            currentStep={props.session.current_step}
          />
        </Show>

        {/* Recent actions - show for ALL states that have actions */}
        <Show when={showActions()}>
          <div
            style={{
              display: 'flex',
              'flex-direction': 'column',
              gap: '4px',
              'margin-bottom': '12px',
              padding: '8px 10px',
              background: 'var(--color-bg-base)',
              'border-radius': '8px',
              'border-left': `2px solid ${getActionsBorderColor()}`,
            }}
          >
            {/* Status header - show for all states with appropriate message */}
            <div
              style={{
                display: 'flex',
                'align-items': 'center',
                gap: '6px',
                'margin-bottom': '4px',
                'padding-bottom': '6px',
                'border-bottom': '1px solid var(--color-bg-overlay)',
              }}
            >
              <span
                class="text-mono"
                style={{
                  'font-size': '11px',
                  color: getActionsBorderColor(),
                }}
              >
                {props.session.status === 'stopped' ? '✓' :
                 props.session.status === 'waiting_input' ? '▸' :
                 props.session.status === 'error' ? '✕' :
                 isActivelyWorking() ? '●' :
                 props.session.current_step?.toLowerCase() === 'ready' ? '✓' : '✓'}
              </span>
              <span
                class="text-mono"
                style={{
                  'font-size': '11px',
                  color: getActionsBorderColor(),
                  'font-weight': '500',
                }}
              >
                {props.session.status === 'stopped' ? 'Completed' :
                 props.session.status === 'waiting_input' ? 'Waiting for input' :
                 props.session.status === 'error' ? 'Error' :
                 isActivelyWorking() ? (
                   props.session.current_step?.toLowerCase() === 'thinking' ? 'Thinking' :
                   props.session.current_step?.toLowerCase() === 'planning' ? 'Planning' :
                   props.session.current_step ? props.session.current_step : 'Working'
                 ) :
                 'Ready'}
              </span>
            </div>

            {/* Recent actions list */}
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
                      color: getActionsBorderColor(),
                      'flex-shrink': '0',
                      'margin-top': '4px',
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
