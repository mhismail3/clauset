import { Show, createSignal, onCleanup } from 'solid-js';

interface SessionSettingsMenuProps {
  model?: string;
  mode?: 'normal' | 'plan';
  onShowShortcuts: () => void;
}

export function SessionSettingsMenu(props: SessionSettingsMenuProps) {
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

  return (
    <div ref={menuRef} style={{ position: 'relative' }}>
      {/* Gear button */}
      <button
        onClick={toggleMenu}
        title="Session settings"
        style={{
          width: '32px',
          height: '32px',
          display: 'flex',
          'align-items': 'center',
          'justify-content': 'center',
          background: isOpen() ? 'var(--color-bg-overlay)' : 'transparent',
          border: 'none',
          'border-radius': '6px',
          cursor: 'pointer',
          color: 'var(--color-text-muted)',
          transition: 'all 0.15s ease',
        }}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style={{
            transition: 'transform 0.2s ease',
            transform: isOpen() ? 'rotate(45deg)' : 'rotate(0deg)',
          }}
        >
          <circle cx="12" cy="12" r="3" />
          <path d="M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
        </svg>
      </button>

      {/* Dropdown menu */}
      <Show when={isOpen()}>
        <div
          style={{
            position: 'absolute',
            top: '100%',
            right: '0',
            'margin-top': '4px',
            background: 'var(--color-bg-surface)',
            border: '1px solid var(--color-bg-overlay)',
            'border-radius': '8px',
            'min-width': '180px',
            'box-shadow': '0 8px 24px rgba(0, 0, 0, 0.3)',
            'z-index': '1000',
            overflow: 'hidden',
          }}
        >
          {/* Current model info */}
          <Show when={props.model}>
            <div
              style={{
                padding: '10px 12px',
                'border-bottom': '1px solid var(--color-bg-overlay)',
              }}
            >
              <div
                class="text-mono"
                style={{
                  'font-size': '10px',
                  color: 'var(--color-text-muted)',
                  'text-transform': 'uppercase',
                  'letter-spacing': '0.05em',
                  'margin-bottom': '4px',
                }}
              >
                Model
              </div>
              <div
                class="text-mono"
                style={{
                  'font-size': '12px',
                  color: 'var(--color-text-primary)',
                  'font-weight': '500',
                }}
              >
                {props.model}
              </div>
            </div>
          </Show>

          {/* Current mode */}
          <Show when={props.mode}>
            <div
              style={{
                padding: '10px 12px',
                'border-bottom': '1px solid var(--color-bg-overlay)',
              }}
            >
              <div
                class="text-mono"
                style={{
                  'font-size': '10px',
                  color: 'var(--color-text-muted)',
                  'text-transform': 'uppercase',
                  'letter-spacing': '0.05em',
                  'margin-bottom': '4px',
                }}
              >
                Mode
              </div>
              <div
                style={{
                  display: 'flex',
                  'align-items': 'center',
                  gap: '6px',
                }}
              >
                <span
                  style={{
                    width: '6px',
                    height: '6px',
                    'border-radius': '50%',
                    background: props.mode === 'plan' ? '#8b5cf6' : '#22c55e',
                  }}
                />
                <span
                  class="text-mono"
                  style={{
                    'font-size': '12px',
                    color: 'var(--color-text-primary)',
                    'font-weight': '500',
                    'text-transform': 'capitalize',
                  }}
                >
                  {props.mode === 'plan' ? 'Plan Mode' : 'Normal'}
                </span>
              </div>
            </div>
          </Show>

          {/* Menu items */}
          <div style={{ padding: '4px 0' }}>
            <button
              onClick={() => {
                setIsOpen(false);
                props.onShowShortcuts();
              }}
              style={{
                width: '100%',
                display: 'flex',
                'align-items': 'center',
                gap: '8px',
                padding: '8px 12px',
                background: 'transparent',
                border: 'none',
                cursor: 'pointer',
                color: 'var(--color-text-secondary)',
                'font-size': '13px',
                'text-align': 'left',
              }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="10" />
                <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
                <circle cx="12" cy="17" r=".5" />
              </svg>
              Keyboard Shortcuts
            </button>
          </div>

          {/* Help text */}
          <div
            style={{
              padding: '8px 12px',
              'border-top': '1px solid var(--color-bg-overlay)',
              background: 'var(--color-bg-overlay)',
            }}
          >
            <span
              style={{
                'font-size': '11px',
                color: 'var(--color-text-muted)',
              }}
            >
              Tip: Use <kbd class="text-mono" style={{ background: 'var(--color-bg-surface)', padding: '1px 4px', 'border-radius': '2px', 'font-size': '10px' }}>/</kbd> for commands
            </span>
          </div>
        </div>
      </Show>
    </div>
  );
}
