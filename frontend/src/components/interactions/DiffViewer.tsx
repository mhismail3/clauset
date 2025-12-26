import { Show, For, createSignal, createEffect } from 'solid-js';
import { Spinner } from '../ui/Spinner';
import { computeDiff, currentDiff, diffLoading } from '../../stores/interactions';
import type { DiffHunk, DiffLine, FileDiff } from '../../lib/api';

interface DiffViewerProps {
  fromInteraction: string;
  toInteraction: string;
  filePath: string;
  contextLines?: number;
  onClose?: () => void;
}

function DiffLineView(props: { line: DiffLine }) {
  const getBgColor = () => {
    switch (props.line.change_type) {
      case 'add':
        return 'rgba(44, 143, 122, 0.15)';
      case 'remove':
        return 'rgba(196, 91, 55, 0.15)';
      default:
        return 'transparent';
    }
  };

  const getTextColor = () => {
    switch (props.line.change_type) {
      case 'add':
        return '#2c8f7a';
      case 'remove':
        return '#c45b37';
      default:
        return 'var(--color-text-secondary)';
    }
  };

  const getPrefix = () => {
    switch (props.line.change_type) {
      case 'add':
        return '+';
      case 'remove':
        return '-';
      default:
        return ' ';
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        'font-family': 'var(--font-mono)',
        'font-size': '12px',
        'line-height': '1.5',
        background: getBgColor(),
        'border-left': props.line.change_type !== 'context' ? `3px solid ${getTextColor()}` : '3px solid transparent',
      }}
    >
      {/* Line numbers */}
      <div
        style={{
          display: 'flex',
          'flex-shrink': '0',
          'user-select': 'none',
          color: 'var(--color-text-muted)',
          'font-size': '11px',
        }}
      >
        <span
          style={{
            width: '48px',
            'text-align': 'right',
            padding: '0 8px',
            background: 'var(--color-bg-base)',
            'border-right': '1px solid var(--color-bg-overlay)',
          }}
        >
          {props.line.old_line_num ?? ''}
        </span>
        <span
          style={{
            width: '48px',
            'text-align': 'right',
            padding: '0 8px',
            background: 'var(--color-bg-base)',
            'border-right': '1px solid var(--color-bg-overlay)',
          }}
        >
          {props.line.new_line_num ?? ''}
        </span>
      </div>

      {/* Content */}
      <div
        style={{
          flex: '1',
          'min-width': '0',
          'white-space': 'pre',
          'overflow-x': 'auto',
          padding: '0 8px',
          color: getTextColor(),
        }}
      >
        <span style={{ 'user-select': 'none', color: getTextColor() }}>{getPrefix()}</span>
        {props.line.content}
      </div>
    </div>
  );
}

function DiffHunkView(props: { hunk: DiffHunk; index: number }) {
  return (
    <div style={{ 'margin-bottom': '16px' }}>
      {/* Hunk header */}
      <div
        style={{
          padding: '4px 12px',
          background: 'rgba(138, 134, 131, 0.1)',
          'border-radius': '4px 4px 0 0',
          color: 'var(--color-text-muted)',
          'font-family': 'var(--font-mono)',
          'font-size': '11px',
        }}
      >
        @@ -{props.hunk.old_start},{props.hunk.old_count} +{props.hunk.new_start},{props.hunk.new_count} @@
      </div>

      {/* Hunk lines */}
      <div
        style={{
          border: '1px solid var(--color-bg-overlay)',
          'border-top': 'none',
          'border-radius': '0 0 4px 4px',
          overflow: 'hidden',
        }}
      >
        <For each={props.hunk.lines}>
          {(line) => <DiffLineView line={line} />}
        </For>
      </div>
    </div>
  );
}

function DiffStats(props: { diff: FileDiff }) {
  const totalChanges = () => props.diff.lines_added + props.diff.lines_removed;
  const addedPercent = () => totalChanges() > 0 ? (props.diff.lines_added / totalChanges()) * 100 : 50;

  return (
    <div
      style={{
        display: 'flex',
        'align-items': 'center',
        gap: '12px',
        padding: '8px 12px',
        background: 'var(--color-bg-base)',
        'border-radius': '6px',
        'margin-bottom': '16px',
      }}
    >
      <span
        class="text-mono"
        style={{ 'font-size': '12px', color: '#2c8f7a', 'font-weight': '500' }}
      >
        +{props.diff.lines_added}
      </span>
      <span
        class="text-mono"
        style={{ 'font-size': '12px', color: '#c45b37', 'font-weight': '500' }}
      >
        -{props.diff.lines_removed}
      </span>

      {/* Visual bar */}
      <div
        style={{
          flex: '1',
          height: '6px',
          'border-radius': '3px',
          overflow: 'hidden',
          background: 'var(--color-bg-overlay)',
          display: 'flex',
        }}
      >
        <div
          style={{
            width: `${addedPercent()}%`,
            background: '#2c8f7a',
          }}
        />
        <div
          style={{
            flex: '1',
            background: '#c45b37',
          }}
        />
      </div>
    </div>
  );
}

export function DiffViewer(props: DiffViewerProps) {
  // View mode toggle planned for future - keeping unified for now
  const [_viewMode, _setViewMode] = createSignal<'split' | 'unified'>('unified');

  createEffect(() => {
    computeDiff(
      props.fromInteraction,
      props.toInteraction,
      props.filePath,
      props.contextLines
    );
  });

  const diff = () => currentDiff();
  const fileName = () => props.filePath.split('/').pop() || props.filePath;

  return (
    <div
      style={{
        display: 'flex',
        'flex-direction': 'column',
        height: '100%',
        background: 'var(--color-bg-elevated)',
        'border-radius': '8px',
        overflow: 'hidden',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          'align-items': 'center',
          gap: '12px',
          padding: '12px 16px',
          'border-bottom': '1px solid var(--color-bg-overlay)',
        }}
      >
        <div style={{ flex: '1', 'min-width': '0' }}>
          <h3
            class="text-mono"
            style={{
              'font-size': '13px',
              'font-weight': '600',
              color: 'var(--color-text-primary)',
              margin: '0',
              overflow: 'hidden',
              'text-overflow': 'ellipsis',
              'white-space': 'nowrap',
            }}
            title={props.filePath}
          >
            {fileName()}
          </h3>
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
            {props.filePath}
          </p>
        </div>

        <Show when={props.onClose}>
          <button
            onClick={props.onClose}
            class="pressable"
            style={{
              width: '32px',
              height: '32px',
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              background: 'var(--color-bg-overlay)',
              border: 'none',
              'border-radius': '6px',
              cursor: 'pointer',
              color: 'var(--color-text-muted)',
            }}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Content */}
      <div style={{ flex: '1', overflow: 'auto', padding: '16px' }}>
        <Show when={diffLoading()}>
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              padding: '48px',
            }}
          >
            <Spinner />
          </div>
        </Show>

        <Show when={!diffLoading() && diff()}>
          {(diffData) => (
            <>
              <Show when={diffData().diff.is_binary}>
                <div
                  style={{
                    padding: '24px',
                    'text-align': 'center',
                    background: 'var(--color-bg-base)',
                    'border-radius': '8px',
                  }}
                >
                  <p
                    class="text-mono"
                    style={{
                      'font-size': '13px',
                      color: 'var(--color-text-muted)',
                      margin: '0',
                    }}
                  >
                    Binary file - cannot display diff
                  </p>
                </div>
              </Show>

              <Show when={diffData().diff.is_identical}>
                <div
                  style={{
                    padding: '24px',
                    'text-align': 'center',
                    background: 'var(--color-bg-base)',
                    'border-radius': '8px',
                  }}
                >
                  <p
                    class="text-mono"
                    style={{
                      'font-size': '13px',
                      color: '#2c8f7a',
                      margin: '0',
                    }}
                  >
                    No changes - files are identical
                  </p>
                </div>
              </Show>

              <Show when={!diffData().diff.is_binary && !diffData().diff.is_identical}>
                <DiffStats diff={diffData().diff} />

                <For each={diffData().diff.hunks}>
                  {(hunk, index) => <DiffHunkView hunk={hunk} index={index()} />}
                </For>
              </Show>
            </>
          )}
        </Show>

        <Show when={!diffLoading() && !diff()}>
          <div
            style={{
              padding: '24px',
              'text-align': 'center',
              background: 'var(--color-bg-base)',
              'border-radius': '8px',
            }}
          >
            <p
              class="text-mono"
              style={{
                'font-size': '13px',
                color: 'var(--color-text-muted)',
                margin: '0',
              }}
            >
              Failed to load diff
            </p>
          </div>
        </Show>
      </div>
    </div>
  );
}
