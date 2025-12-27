import { Show, For } from 'solid-js';

interface ShortcutItem {
  key: string;
  description: string;
}

interface ShortcutGroup {
  title: string;
  shortcuts: ShortcutItem[];
}

interface KeyboardShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const shortcutGroups: ShortcutGroup[] = [
  {
    title: 'Input',
    shortcuts: [
      { key: 'Enter', description: 'Send message' },
      { key: 'Shift + Enter', description: 'New line' },
      { key: '/', description: 'Open command picker' },
      { key: 'Esc', description: 'Cancel/close' },
    ],
  },
  {
    title: 'Navigation',
    shortcuts: [
      { key: '↑ / ↓', description: 'Navigate history' },
      { key: 'Tab', description: 'Accept autocomplete' },
    ],
  },
  {
    title: 'Views',
    shortcuts: [
      { key: '?', description: 'Show this help' },
    ],
  },
  {
    title: 'Terminal Mode',
    shortcuts: [
      { key: 'Ctrl + C', description: 'Interrupt/stop Claude' },
      { key: 'Ctrl + L', description: 'Clear terminal' },
      { key: 'Ctrl + D', description: 'Exit/close' },
    ],
  },
];

export function KeyboardShortcutsModal(props: KeyboardShortcutsModalProps) {
  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      props.onClose();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      props.onClose();
    }
  }

  return (
    <Show when={props.isOpen}>
      <div
        onClick={handleBackdropClick}
        onKeyDown={handleKeyDown}
        style={{
          position: 'fixed',
          inset: '0',
          background: 'rgba(0, 0, 0, 0.6)',
          display: 'flex',
          'align-items': 'center',
          'justify-content': 'center',
          'z-index': '1000',
          padding: '16px',
        }}
      >
        <div
          style={{
            background: 'var(--color-bg-surface)',
            border: '1px solid var(--color-bg-overlay)',
            'border-radius': '12px',
            'max-width': '420px',
            width: '100%',
            'max-height': '80vh',
            overflow: 'auto',
            'box-shadow': '0 20px 60px rgba(0, 0, 0, 0.4)',
          }}
        >
          {/* Header */}
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'space-between',
              padding: '16px 20px',
              'border-bottom': '1px solid var(--color-bg-overlay)',
            }}
          >
            <h2
              class="text-mono"
              style={{
                margin: '0',
                'font-size': '14px',
                'font-weight': '600',
                color: 'var(--color-text-primary)',
              }}
            >
              Keyboard Shortcuts
            </h2>
            <button
              onClick={() => props.onClose()}
              style={{
                background: 'none',
                border: 'none',
                color: 'var(--color-text-muted)',
                cursor: 'pointer',
                padding: '4px',
                display: 'flex',
                'align-items': 'center',
                'justify-content': 'center',
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Content */}
          <div style={{ padding: '16px 20px' }}>
            <For each={shortcutGroups}>
              {(group) => (
                <div style={{ 'margin-bottom': '16px' }}>
                  <h3
                    class="text-mono"
                    style={{
                      margin: '0 0 8px 0',
                      'font-size': '11px',
                      'font-weight': '600',
                      color: 'var(--color-text-muted)',
                      'text-transform': 'uppercase',
                      'letter-spacing': '0.05em',
                    }}
                  >
                    {group.title}
                  </h3>
                  <For each={group.shortcuts}>
                    {(shortcut) => (
                      <div
                        style={{
                          display: 'flex',
                          'align-items': 'center',
                          'justify-content': 'space-between',
                          padding: '6px 0',
                        }}
                      >
                        <span
                          style={{
                            'font-size': '13px',
                            color: 'var(--color-text-secondary)',
                          }}
                        >
                          {shortcut.description}
                        </span>
                        <kbd
                          class="text-mono"
                          style={{
                            background: 'var(--color-bg-overlay)',
                            padding: '3px 8px',
                            'border-radius': '4px',
                            'font-size': '11px',
                            color: 'var(--color-text-primary)',
                            'white-space': 'nowrap',
                          }}
                        >
                          {shortcut.key}
                        </kbd>
                      </div>
                    )}
                  </For>
                </div>
              )}
            </For>
          </div>

          {/* Footer */}
          <div
            style={{
              padding: '12px 20px',
              'border-top': '1px solid var(--color-bg-overlay)',
              'text-align': 'center',
            }}
          >
            <span
              style={{
                'font-size': '12px',
                color: 'var(--color-text-muted)',
              }}
            >
              Press <kbd class="text-mono" style={{ background: 'var(--color-bg-overlay)', padding: '2px 6px', 'border-radius': '3px', 'font-size': '11px' }}>Esc</kbd> to close
            </span>
          </div>
        </div>
      </div>
    </Show>
  );
}
