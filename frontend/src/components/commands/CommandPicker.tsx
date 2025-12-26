import { Show, For, createEffect, createMemo } from 'solid-js';
import { Spinner } from '../ui/Spinner';
import { Command } from '../../lib/api';
import {
  commandsStore,
  loading,
  error,
  fetchCommands,
  filterCommands,
  resetFilter,
  getGroupedCommands,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
} from '../../stores/commands';

interface CommandPickerProps {
  isOpen: boolean;
  query: string;
  onSelect: (command: Command) => void;
  onClose: () => void;
  anchorBottom: number;
}

export function CommandPicker(props: CommandPickerProps) {
  let listRef: HTMLDivElement | undefined;

  // Fetch commands when picker opens
  createEffect(() => {
    if (props.isOpen) {
      fetchCommands();
    }
  });

  // Filter commands when query changes
  createEffect(() => {
    if (props.isOpen) {
      filterCommands(props.query);
    }
  });

  // Reset when closed
  createEffect(() => {
    if (!props.isOpen) {
      resetFilter();
    }
  });

  // Scroll selected item into view
  createEffect(() => {
    if (!props.isOpen || !listRef) return;
    const selectedIdx = commandsStore.selectedIndex;
    const selectedEl = listRef.querySelector(`[data-index="${selectedIdx}"]`);
    if (selectedEl) {
      selectedEl.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  });

  const grouped = createMemo(() => getGroupedCommands());
  const hasResults = () => commandsStore.filteredCommands.length > 0;

  // Get flat index for a command based on its position in filteredCommands
  const getFlatIndexForCommand = (cmd: Command): number => {
    return commandsStore.filteredCommands.findIndex(
      (c) => c.name === cmd.name && c.category === cmd.category
    );
  };

  return (
    <Show when={props.isOpen}>
      <div
        style={{
          position: 'fixed',
          left: '12px',
          right: '12px',
          bottom: `${props.anchorBottom}px`,
          'max-height': '280px',
          background: 'var(--color-bg-elevated)',
          border: '1.5px solid var(--color-bg-overlay)',
          'border-radius': '12px',
          'box-shadow': '0 -4px 24px rgba(0, 0, 0, 0.4)',
          overflow: 'hidden',
          'z-index': '100',
        }}
      >
        {/* Loading state */}
        <Show when={loading()}>
          <div
            style={{
              display: 'flex',
              'justify-content': 'center',
              padding: '24px',
            }}
          >
            <Spinner size="sm" />
          </div>
        </Show>

        {/* Error state */}
        <Show when={error()}>
          <div
            style={{
              padding: '16px',
              color: 'var(--color-accent)',
              'font-family': 'var(--font-mono)',
              'font-size': '13px',
              'text-align': 'center',
            }}
          >
            {error()}
          </div>
        </Show>

        {/* No results */}
        <Show when={!loading() && !error() && !hasResults()}>
          <div
            style={{
              padding: '24px 16px',
              color: 'var(--color-text-muted)',
              'font-family': 'var(--font-mono)',
              'font-size': '13px',
              'text-align': 'center',
            }}
          >
            No commands match "/{props.query}"
          </div>
        </Show>

        {/* Command list */}
        <Show when={!loading() && !error() && hasResults()}>
          <div
            ref={listRef}
            style={{
              'overflow-y': 'auto',
              'max-height': '280px',
              padding: '8px',
            }}
          >
            <For each={Array.from(grouped().entries())}>
              {([category, commands]) => (
                <div style={{ 'margin-bottom': '4px' }}>
                  {/* Category header */}
                  <div
                    style={{
                      padding: '4px 8px',
                      'font-family': 'var(--font-mono)',
                      'font-size': '10px',
                      'font-weight': '600',
                      'text-transform': 'uppercase',
                      'letter-spacing': '0.05em',
                      color: CATEGORY_COLORS[category],
                    }}
                  >
                    {CATEGORY_LABELS[category]}
                  </div>

                  {/* Commands */}
                  <For each={commands}>
                    {(cmd) => {
                      const idx = getFlatIndexForCommand(cmd);
                      const isSelected = () => commandsStore.selectedIndex === idx;

                      return (
                        <button
                          data-index={idx}
                          onClick={() => props.onSelect(cmd)}
                          style={{
                            width: '100%',
                            display: 'flex',
                            'align-items': 'center',
                            gap: '8px',
                            padding: '8px 10px',
                            background: isSelected()
                              ? 'var(--color-bg-overlay)'
                              : 'transparent',
                            border: 'none',
                            'border-radius': '6px',
                            cursor: 'pointer',
                            'text-align': 'left',
                            transition: 'background 0.1s ease',
                          }}
                          onMouseEnter={(e) => {
                            if (!isSelected()) {
                              e.currentTarget.style.background = 'var(--color-bg-base)';
                            }
                          }}
                          onMouseLeave={(e) => {
                            if (!isSelected()) {
                              e.currentTarget.style.background = 'transparent';
                            }
                          }}
                        >
                          {/* Command name */}
                          <span
                            style={{
                              'font-family': 'var(--font-mono)',
                              'font-size': '13px',
                              'font-weight': '500',
                              color: 'var(--color-accent)',
                              'flex-shrink': '0',
                            }}
                          >
                            {cmd.display_name}
                          </span>

                          {/* Argument hint */}
                          <Show when={cmd.argument_hint}>
                            <span
                              style={{
                                'font-family': 'var(--font-mono)',
                                'font-size': '11px',
                                color: 'var(--color-text-muted)',
                                'flex-shrink': '0',
                              }}
                            >
                              {cmd.argument_hint}
                            </span>
                          </Show>

                          {/* Description */}
                          <span
                            style={{
                              flex: '1',
                              'font-size': '12px',
                              color: 'var(--color-text-tertiary)',
                              overflow: 'hidden',
                              'text-overflow': 'ellipsis',
                              'white-space': 'nowrap',
                              'min-width': '0',
                            }}
                          >
                            {cmd.description}
                          </span>

                          {/* Enter hint for selected */}
                          <Show when={isSelected()}>
                            <span
                              style={{
                                'font-family': 'var(--font-mono)',
                                'font-size': '10px',
                                color: 'var(--color-text-muted)',
                                background: 'var(--color-bg-base)',
                                padding: '2px 6px',
                                'border-radius': '4px',
                                'flex-shrink': '0',
                              }}
                            >
                              enter
                            </span>
                          </Show>
                        </button>
                      );
                    }}
                  </For>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}
