import { Show, For, createSignal } from 'solid-js';
import type { InteractionSummary, ToolInvocation, FileChangeWithDiff } from '../../lib/api';
import { Badge } from '../ui/Badge';
import { formatCost, formatTokens, formatDuration } from '../../stores/interactions';
import { formatRelativeTime } from '../../stores/sessions';

interface InteractionCardProps {
  interaction: InteractionSummary;
  isExpanded?: boolean;
  onToggle?: () => void;
  onViewDiff?: (file: string) => void;
  toolInvocations?: ToolInvocation[];
  fileChanges?: FileChangeWithDiff[];
}

function ToolIcon(props: { name: string }) {
  const iconMap: Record<string, () => JSX.Element> = {
    Read: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
      </svg>
    ),
    Write: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
        <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
      </svg>
    ),
    Edit: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
      </svg>
    ),
    Bash: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="4 17 10 11 4 5" />
        <line x1="12" y1="19" x2="20" y2="19" />
      </svg>
    ),
    Grep: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
    ),
    Glob: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
        <polyline points="9 22 9 12 15 12 15 22" />
      </svg>
    ),
    Task: () => (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
        <line x1="9" y1="9" x2="15" y2="15" />
        <line x1="15" y1="9" x2="9" y2="15" />
      </svg>
    ),
  };

  const normalizedName = props.name.split('_').map(s =>
    s.charAt(0).toUpperCase() + s.slice(1).toLowerCase()
  ).join('');

  const Icon = iconMap[normalizedName] || iconMap.Read;
  return <Icon />;
}

export function InteractionCard(props: InteractionCardProps) {
  const [localExpanded, setLocalExpanded] = createSignal(false);
  const expanded = () => props.isExpanded ?? localExpanded();

  const handleToggle = () => {
    if (props.onToggle) {
      props.onToggle();
    } else {
      setLocalExpanded(!localExpanded());
    }
  };

  const getStatusColor = () => {
    if (props.fileChanges && props.fileChanges.length > 0) {
      return '#2c8f7a'; // Green for file changes
    }
    if (props.toolInvocations && props.toolInvocations.some(t => t.is_error)) {
      return '#c45b37'; // Red for errors
    }
    return 'var(--color-text-muted)';
  };

  return (
    <div
      class="card-retro"
      style={{
        overflow: 'hidden',
        'border-left': `3px solid ${getStatusColor()}`,
      }}
    >
      {/* Header - clickable to expand */}
      <button
        onClick={handleToggle}
        style={{
          width: '100%',
          padding: '12px 14px',
          background: 'transparent',
          border: 'none',
          cursor: 'pointer',
          'text-align': 'left',
          display: 'flex',
          'flex-direction': 'column',
          gap: '8px',
        }}
      >
        {/* Top row: sequence number + prompt preview */}
        <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '10px', width: '100%' }}>
          <span
            class="text-mono"
            style={{
              'font-size': '11px',
              color: 'var(--color-text-muted)',
              'flex-shrink': '0',
              'min-width': '24px',
            }}
          >
            #{props.interaction.sequence_number}
          </span>
          <p
            class="text-mono"
            style={{
              'font-size': '13px',
              color: 'var(--color-text-primary)',
              margin: '0',
              flex: '1',
              'line-height': '1.4',
              overflow: 'hidden',
              display: '-webkit-box',
              '-webkit-line-clamp': expanded() ? 'none' : '2',
              '-webkit-box-orient': 'vertical',
            }}
          >
            {props.interaction.user_prompt}
          </p>
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style={{
              'flex-shrink': '0',
              color: 'var(--color-text-muted)',
              transform: expanded() ? 'rotate(180deg)' : 'rotate(0deg)',
              transition: 'transform 0.2s ease',
            }}
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </div>

        {/* Stats row */}
        <div
          class="text-mono"
          style={{
            display: 'flex',
            'align-items': 'center',
            gap: '12px',
            'font-size': '11px',
            color: 'var(--color-text-muted)',
            'padding-left': '34px',
          }}
        >
          <span>{formatRelativeTime(props.interaction.started_at)}</span>
          <span>|</span>
          <span>{formatCost(props.interaction.cost_delta_usd)}</span>
          <span>|</span>
          <span>
            {formatTokens(props.interaction.input_tokens_delta)}/{formatTokens(props.interaction.output_tokens_delta)}
          </span>
          <Show when={props.interaction.tool_count > 0}>
            <span>|</span>
            <span>{props.interaction.tool_count} tools</span>
          </Show>
          <Show when={props.interaction.files_changed.length > 0}>
            <span>|</span>
            <span style={{ color: '#2c8f7a' }}>
              {props.interaction.files_changed.length} files
            </span>
          </Show>
        </div>
      </button>

      {/* Expanded content */}
      <Show when={expanded()}>
        <div
          style={{
            padding: '0 14px 14px 14px',
            'border-top': '1px solid var(--color-bg-overlay)',
            'margin-top': '4px',
          }}
        >
          {/* Tool invocations */}
          <Show when={props.toolInvocations && props.toolInvocations.length > 0}>
            <div style={{ 'margin-top': '12px' }}>
              <h4
                class="text-mono"
                style={{
                  'font-size': '11px',
                  'text-transform': 'uppercase',
                  'letter-spacing': '0.05em',
                  color: 'var(--color-text-muted)',
                  margin: '0 0 8px 0',
                }}
              >
                Tool Invocations
              </h4>
              <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
                <For each={props.toolInvocations}>
                  {(tool) => (
                    <div
                      style={{
                        display: 'flex',
                        'align-items': 'flex-start',
                        gap: '8px',
                        padding: '8px 10px',
                        background: 'var(--color-bg-base)',
                        'border-radius': '6px',
                        'border-left': `2px solid ${tool.is_error ? '#c45b37' : 'var(--color-text-muted)'}`,
                      }}
                    >
                      <span style={{ color: tool.is_error ? '#c45b37' : 'var(--color-text-secondary)', 'flex-shrink': '0' }}>
                        <ToolIcon name={tool.tool_name} />
                      </span>
                      <div style={{ flex: '1', 'min-width': '0' }}>
                        <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
                          <span
                            class="text-mono"
                            style={{
                              'font-size': '12px',
                              'font-weight': '500',
                              color: 'var(--color-text-primary)',
                            }}
                          >
                            {tool.tool_name}
                          </span>
                          <Show when={tool.is_error}>
                            <Badge variant="error">Error</Badge>
                          </Show>
                          <Show when={tool.duration_ms}>
                            <span
                              class="text-mono"
                              style={{ 'font-size': '10px', color: 'var(--color-text-muted)' }}
                            >
                              {formatDuration(tool.duration_ms)}
                            </span>
                          </Show>
                        </div>
                        <Show when={tool.file_path}>
                          <p
                            class="text-mono"
                            style={{
                              'font-size': '11px',
                              color: 'var(--color-text-secondary)',
                              margin: '4px 0 0 0',
                              'word-break': 'break-all',
                            }}
                          >
                            {tool.file_path}
                          </p>
                        </Show>
                        <Show when={tool.tool_output_preview}>
                          <p
                            class="text-mono"
                            style={{
                              'font-size': '11px',
                              color: 'var(--color-text-muted)',
                              margin: '4px 0 0 0',
                              overflow: 'hidden',
                              'text-overflow': 'ellipsis',
                              'white-space': 'nowrap',
                            }}
                          >
                            {tool.tool_output_preview}
                          </p>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* File changes */}
          <Show when={props.fileChanges && props.fileChanges.length > 0}>
            <div style={{ 'margin-top': '12px' }}>
              <h4
                class="text-mono"
                style={{
                  'font-size': '11px',
                  'text-transform': 'uppercase',
                  'letter-spacing': '0.05em',
                  color: 'var(--color-text-muted)',
                  margin: '0 0 8px 0',
                }}
              >
                File Changes
              </h4>
              <div style={{ display: 'flex', 'flex-direction': 'column', gap: '4px' }}>
                <For each={props.fileChanges}>
                  {(change) => (
                    <button
                      onClick={() => props.onViewDiff?.(change.file_path)}
                      style={{
                        display: 'flex',
                        'align-items': 'center',
                        gap: '8px',
                        padding: '6px 10px',
                        background: 'var(--color-bg-base)',
                        'border-radius': '6px',
                        border: 'none',
                        cursor: props.onViewDiff ? 'pointer' : 'default',
                        width: '100%',
                        'text-align': 'left',
                      }}
                    >
                      <span
                        class="text-mono"
                        style={{
                          'font-size': '10px',
                          'font-weight': '600',
                          padding: '2px 4px',
                          'border-radius': '3px',
                          background:
                            change.change_type === 'created'
                              ? 'rgba(44, 143, 122, 0.2)'
                              : change.change_type === 'deleted'
                              ? 'rgba(196, 91, 55, 0.2)'
                              : 'rgba(212, 166, 68, 0.2)',
                          color:
                            change.change_type === 'created'
                              ? '#2c8f7a'
                              : change.change_type === 'deleted'
                              ? '#c45b37'
                              : '#d4a644',
                        }}
                      >
                        {change.change_type === 'created' ? 'A' : change.change_type === 'deleted' ? 'D' : 'M'}
                      </span>
                      <span
                        class="text-mono"
                        style={{
                          'font-size': '12px',
                          color: 'var(--color-text-secondary)',
                          flex: '1',
                          overflow: 'hidden',
                          'text-overflow': 'ellipsis',
                          'white-space': 'nowrap',
                        }}
                      >
                        {change.file_path}
                      </span>
                      <Show when={!change.diff.is_identical}>
                        <span
                          class="text-mono"
                          style={{ 'font-size': '11px', color: '#2c8f7a' }}
                        >
                          +{change.diff.lines_added}
                        </span>
                        <span
                          class="text-mono"
                          style={{ 'font-size': '11px', color: '#c45b37' }}
                        >
                          -{change.diff.lines_removed}
                        </span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}
