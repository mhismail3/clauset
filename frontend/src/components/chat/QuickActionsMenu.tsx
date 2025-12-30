import { createSignal, Show, For, onCleanup } from 'solid-js';

interface QuickAction {
  command: string;
  label: string;
  icon: string;
  description: string;
  kind: 'command' | 'terminal_input';
  terminalInput?: string;
}

const QUICK_ACTIONS: QuickAction[] = [
  { command: '/clear', label: 'Clear', icon: '🗑', description: 'Clear conversation', kind: 'command' },
  { command: 'Shift+Tab', label: 'Toggle Mode', icon: '↹', description: 'Cycle chat modes', kind: 'terminal_input', terminalInput: '\x1b[Z' },
];

interface QuickActionsMenuProps {
  onSelectCommand: (command: string) => void;
  onSendTerminalInput: (input: string) => void;
  buttonSize: number;
  disabled?: boolean;
}

export function QuickActionsMenu(props: QuickActionsMenuProps) {
  const [isOpen, setIsOpen] = createSignal(false);
  let menuRef: HTMLDivElement | undefined;

  // Close menu when clicking outside
  function handleClickOutside(e: MouseEvent) {
    if (menuRef && !menuRef.contains(e.target as Node)) {
      setIsOpen(false);
    }
  }

  // Add/remove event listener
  function toggleMenu() {
    if (props.disabled) return;
    const newState = !isOpen();
    setIsOpen(newState);
    if (newState) {
      setTimeout(() => document.addEventListener('click', handleClickOutside), 0);
    } else {
      document.removeEventListener('click', handleClickOutside);
    }
  }

  onCleanup(() => {
    document.removeEventListener('click', handleClickOutside);
  });

  function handleSelectAction(action: QuickAction) {
    setIsOpen(false);
    document.removeEventListener('click', handleClickOutside);
    if (action.kind === 'terminal_input' && action.terminalInput) {
      props.onSendTerminalInput(action.terminalInput);
      return;
    }
    props.onSelectCommand(action.command);
  }

  return (
    <div ref={menuRef} style={{ position: 'relative' }}>
      {/* Lightning bolt button */}
      <button
        type="button"
        onClick={toggleMenu}
        title="Quick actions"
        style={{
          width: `${props.buttonSize}px`,
          height: `${props.buttonSize}px`,
          "flex-shrink": '0',
          display: 'flex',
          "align-items": 'center',
          "justify-content": 'center',
          background: isOpen() ? 'var(--color-bg-overlay)' : 'transparent',
          color: isOpen() ? 'var(--color-accent)' : 'var(--color-text-muted)',
          border: 'none',
          "border-radius": '8px',
          cursor: props.disabled ? 'not-allowed' : 'pointer',
          opacity: props.disabled ? 0.5 : 1,
          transition: 'all 0.15s ease',
        }}
      >
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="currentColor"
          stroke="none"
        >
          <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
        </svg>
      </button>

      {/* Dropdown menu */}
      <Show when={isOpen()}>
        <div
          style={{
            position: 'absolute',
            bottom: '100%',
            left: '0',
            "margin-bottom": '8px',
            background: 'var(--color-bg-surface)',
            border: '1px solid var(--color-bg-overlay)',
            "border-radius": '12px',
            "min-width": '180px',
            "box-shadow": '0 8px 24px rgba(0, 0, 0, 0.3)',
            "z-index": '1000',
            overflow: 'hidden',
          }}
        >
          {/* Header */}
          <div
            style={{
              padding: '8px 12px',
              "border-bottom": '1px solid var(--color-bg-overlay)',
              background: 'var(--color-bg-overlay)',
            }}
          >
            <span
              class="text-mono"
              style={{
                "font-size": '10px',
                "font-weight": '600',
                color: 'var(--color-text-muted)',
                "text-transform": 'uppercase',
                "letter-spacing": '0.05em',
              }}
            >
              Quick Actions
            </span>
          </div>

          {/* Action items */}
          <div style={{ padding: '4px 0' }}>
            <For each={QUICK_ACTIONS}>
              {(action) => (
                <button
                  type="button"
                  onClick={() => handleSelectAction(action)}
                  style={{
                    width: '100%',
                    display: 'flex',
                    "align-items": 'center',
                    gap: '10px',
                    padding: '10px 12px',
                    background: 'transparent',
                    border: 'none',
                    cursor: 'pointer',
                    color: 'var(--color-text-secondary)',
                    transition: 'background 0.1s ease',
                  }}
                  class="pressable"
                  onMouseEnter={(e) => e.currentTarget.style.background = 'var(--color-bg-elevated)'}
                  onMouseLeave={(e) => e.currentTarget.style.background = 'transparent'}
                >
                  <span style={{ "font-size": '14px', width: '20px', "text-align": 'center' }}>
                    {action.icon}
                  </span>
                  <div style={{ flex: '1', "text-align": 'left' }}>
                    <div
                      class="text-mono"
                      style={{
                        "font-size": '13px',
                        "font-weight": '500',
                        color: 'var(--color-text-primary)',
                      }}
                    >
                      {action.label}
                    </div>
                    <div
                      style={{
                        "font-size": '11px',
                        color: 'var(--color-text-muted)',
                        "margin-top": '1px',
                      }}
                    >
                      {action.description}
                    </div>
                  </div>
                  <span
                    class="text-mono"
                    style={{
                      "font-size": '11px',
                      color: 'var(--color-text-muted)',
                    }}
                  >
                    {action.command}
                  </span>
                </button>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}
