import { For, Show, onMount, createSignal, onCleanup } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button } from '../components/ui/Button';
import { Spinner } from '../components/ui/Spinner';
import { SessionCard } from '../components/SessionCard';
import {
  sessions,
  activeCount,
  loading,
  error,
  fetchSessions,
} from '../stores/sessions';
import { NewSessionModal } from '../components/chat/NewSessionModal';
import { api, Session } from '../lib/api';

export default function Sessions() {
  const [showNewSession, setShowNewSession] = createSignal(false);
  const [menuState, setMenuState] = createSignal<{
    session: Session;
    position: { top: number; right: number };
    view: 'menu' | 'rename' | 'delete';
  } | null>(null);
  const [renameValue, setRenameValue] = createSignal('');
  const [deleting, setDeleting] = createSignal(false);
  const navigate = useNavigate();

  let menuRef: HTMLDivElement | undefined;

  onMount(() => {
    fetchSessions();
    const interval = setInterval(fetchSessions, 30000);

    // Close menu on outside click
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef && !menuRef.contains(e.target as Node)) {
        setMenuState(null);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);

    onCleanup(() => {
      clearInterval(interval);
      document.removeEventListener('mousedown', handleClickOutside);
    });
  });

  async function handleDelete() {
    const state = menuState();
    if (!state) return;

    setDeleting(true);
    try {
      await api.sessions.delete(state.session.id);
      fetchSessions();
      setMenuState(null);
    } catch (e) {
      console.error('Failed to delete session:', e);
    } finally {
      setDeleting(false);
    }
  }

  async function handleRename() {
    const state = menuState();
    if (!state || !renameValue().trim()) return;

    try {
      await api.sessions.rename(state.session.id, renameValue().trim());
      fetchSessions();
      setMenuState(null);
    } catch (e) {
      console.error('Failed to rename session:', e);
    }
  }

  function openMenu(e: Event, session: Session) {
    e.preventDefault();
    e.stopPropagation();

    const button = e.currentTarget as HTMLElement;
    const rect = button.getBoundingClientRect();

    setRenameValue(session.preview || session.project_path.split('/').pop() || '');
    setMenuState({
      session,
      position: {
        top: rect.bottom + 8,
        right: window.innerWidth - rect.right,
      },
      view: 'menu',
    });
  }

  function closeMenu() {
    setMenuState(null);
  }

  function showRename() {
    const state = menuState();
    if (state) {
      setMenuState({ ...state, view: 'rename' });
    }
  }

  function showDelete() {
    const state = menuState();
    if (state) {
      setMenuState({ ...state, view: 'delete' });
    }
  }

  function backToMenu() {
    const state = menuState();
    if (state) {
      setMenuState({ ...state, view: 'menu' });
    }
  }

  return (
    <div class="flex flex-col h-full">
      {/* Header - Clean centered design */}
      <header class="flex-none glass safe-top">
        <div
          style={{
            display: 'flex',
            "align-items": 'center',
            "justify-content": 'space-between',
            padding: '14px 20px',
          }}
        >
          {/* Left: New Session Button with glow effect */}
          <button
            onClick={() => setShowNewSession(true)}
            style={{
              display: 'flex',
              "align-items": 'center',
              "justify-content": 'center',
              gap: '6px',
              padding: '8px 14px',
              background: 'var(--color-accent)',
              border: 'none',
              "border-radius": '10px',
              color: '#ffffff',
              "font-size": '13px',
              "font-weight": '600',
              cursor: 'pointer',
              "box-shadow": '0 0 16px rgba(196, 91, 55, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2)',
              transition: 'all 0.2s ease',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.boxShadow = '0 0 24px rgba(196, 91, 55, 0.6), 0 4px 12px rgba(0, 0, 0, 0.3)';
              e.currentTarget.style.transform = 'translateY(-1px)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.boxShadow = '0 0 16px rgba(196, 91, 55, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2)';
              e.currentTarget.style.transform = 'translateY(0)';
            }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            <span>New</span>
          </button>

          {/* Center: Logo + Title */}
          <div
            style={{
              display: 'flex',
              "align-items": 'center',
              gap: '10px',
              position: 'absolute',
              left: '50%',
              transform: 'translateX(-50%)',
            }}
          >
            <img
              src="/logo.svg"
              alt="Clauset logo"
              style={{ width: '28px', height: '28px' }}
            />
            <h1
              class="text-brand"
              style={{
                color: 'var(--color-accent)',
                "font-size": '20px',
                "font-weight": '600',
                margin: '0',
                "letter-spacing": '0.5px',
              }}
            >
              Clauset
            </h1>
          </div>

          {/* Right: Active session count */}
          <div
            class="text-mono"
            style={{
              display: 'flex',
              "align-items": 'center',
              gap: '6px',
              padding: '6px 12px',
              background: activeCount() > 0 ? 'rgba(44, 143, 122, 0.15)' : 'var(--color-bg-surface)',
              "border-radius": '8px',
              "font-size": '12px',
              color: activeCount() > 0 ? '#2c8f7a' : 'var(--color-text-muted)',
            }}
          >
            <Show when={activeCount() > 0} fallback={
              <span>idle</span>
            }>
              <span
                style={{
                  width: '6px',
                  height: '6px',
                  background: '#2c8f7a',
                  "border-radius": '50%',
                  animation: 'pulse 2s infinite',
                }}
              />
              <span>{activeCount()} active</span>
            </Show>
          </div>
        </div>
      </header>

      {/* Content */}
      <main class="flex-1 scrollable">
        <div style={{ padding: '16px', "padding-bottom": '32px' }}>
          <Show when={loading() && sessions().length === 0}>
            <div style={{ display: 'flex', "justify-content": 'center', padding: '64px 0' }}>
              <Spinner size="lg" />
            </div>
          </Show>

          <Show when={error()}>
            <div
              style={{
                padding: '14px 16px',
                background: 'var(--color-accent-muted)',
                border: '1px solid var(--color-accent)',
                "border-radius": '12px',
                color: 'var(--color-accent)',
                "font-size": '14px',
                "margin-bottom": '16px',
              }}
            >
              {error()}
            </div>
          </Show>

          <Show when={!loading() && sessions().length === 0 && !error()}>
            <div style={{ "text-align": 'center', padding: '64px 0' }}>
              <div
                style={{
                  width: '64px',
                  height: '64px',
                  margin: '0 auto 16px',
                  "border-radius": '50%',
                  background: 'var(--color-bg-surface)',
                  border: '1px solid var(--color-bg-overlay)',
                  display: 'flex',
                  "align-items": 'center',
                  "justify-content": 'center',
                }}
              >
                <span class="text-mono" style={{ color: 'var(--color-accent)', "font-size": '24px' }}>&gt;_</span>
              </div>
              <p style={{ color: 'var(--color-text-secondary)', "margin-bottom": '16px' }}>
                No sessions yet
              </p>
              <Button onClick={() => setShowNewSession(true)}>
                Start your first session
              </Button>
            </div>
          </Show>

          {/* Session Cards */}
          <div style={{ display: 'flex', "flex-direction": 'column', gap: '12px' }}>
            <For each={sessions()}>
              {(session) => (
                <SessionCard
                  session={session}
                  onMenuOpen={openMenu}
                />
              )}
            </For>
          </div>
        </div>
      </main>

      {/* Dropdown Menu */}
      <Show when={menuState()}>
        {(state) => (
          <>
            {/* Backdrop */}
            <div
              style={{
                position: 'fixed',
                top: '0',
                left: '0',
                right: '0',
                bottom: '0',
                "z-index": '40',
              }}
              onClick={closeMenu}
            />

            {/* Menu */}
            <div
              ref={menuRef}
              class="animate-scale-in"
              style={{
                position: 'fixed',
                top: `${state().position.top}px`,
                right: `${state().position.right}px`,
                "z-index": '50',
                width: '200px',
                background: 'var(--color-bg-surface)',
                border: '1px solid var(--color-bg-overlay)',
                "border-radius": '12px',
                "box-shadow": '0 8px 32px rgba(0, 0, 0, 0.4)',
                overflow: 'hidden',
                "transform-origin": 'top right',
              }}
            >
              {/* Main Menu */}
              <Show when={state().view === 'menu'}>
                <div style={{ padding: '6px' }}>
                  <button
                    onClick={showRename}
                    style={{
                      width: '100%',
                      display: 'flex',
                      "align-items": 'center',
                      gap: '12px',
                      padding: '10px 12px',
                      background: 'none',
                      border: 'none',
                      "border-radius": '8px',
                      cursor: 'pointer',
                      "font-size": '14px',
                      color: 'var(--color-text-primary)',
                      transition: 'background 0.15s ease',
                    }}
                    onMouseEnter={(e) => e.currentTarget.style.background = 'var(--color-bg-elevated)'}
                    onMouseLeave={(e) => e.currentTarget.style.background = 'none'}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ color: 'var(--color-text-tertiary)' }}>
                      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    </svg>
                    <span>Rename</span>
                  </button>

                  <button
                    onClick={showDelete}
                    style={{
                      width: '100%',
                      display: 'flex',
                      "align-items": 'center',
                      gap: '12px',
                      padding: '10px 12px',
                      background: 'none',
                      border: 'none',
                      "border-radius": '8px',
                      cursor: 'pointer',
                      "font-size": '14px',
                      color: 'var(--color-accent)',
                      transition: 'background 0.15s ease',
                    }}
                    onMouseEnter={(e) => e.currentTarget.style.background = 'var(--color-accent-muted)'}
                    onMouseLeave={(e) => e.currentTarget.style.background = 'none'}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                    <span>Delete</span>
                  </button>
                </div>
              </Show>

              {/* Rename View */}
              <Show when={state().view === 'rename'}>
                <div style={{ padding: '12px' }}>
                  <div style={{ display: 'flex', "align-items": 'center', gap: '8px', "margin-bottom": '12px' }}>
                    <button
                      onClick={backToMenu}
                      style={{
                        width: '28px',
                        height: '28px',
                        display: 'flex',
                        "align-items": 'center',
                        "justify-content": 'center',
                        background: 'none',
                        border: 'none',
                        "border-radius": '6px',
                        cursor: 'pointer',
                        color: 'var(--color-text-muted)',
                      }}
                    >
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M15 18l-6-6 6-6" />
                      </svg>
                    </button>
                    <span style={{ "font-size": '13px', "font-weight": '600', color: 'var(--color-text-primary)' }}>
                      Rename
                    </span>
                  </div>
                  <input
                    type="text"
                    value={renameValue()}
                    onInput={(e) => setRenameValue(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleRename();
                      if (e.key === 'Escape') closeMenu();
                    }}
                    autofocus
                    style={{
                      width: '100%',
                      "box-sizing": 'border-box',
                      padding: '10px 12px',
                      "font-size": '14px',
                      "border-radius": '8px',
                      border: '1px solid var(--color-bg-overlay)',
                      background: 'var(--color-bg-base)',
                      color: 'var(--color-text-primary)',
                      outline: 'none',
                      "margin-bottom": '12px',
                    }}
                  />
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <button
                      onClick={closeMenu}
                      style={{
                        flex: '1',
                        padding: '8px',
                        "font-size": '13px',
                        "font-weight": '500',
                        "border-radius": '8px',
                        border: '1px solid var(--color-bg-overlay)',
                        background: 'var(--color-bg-elevated)',
                        color: 'var(--color-text-secondary)',
                        cursor: 'pointer',
                      }}
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleRename}
                      style={{
                        flex: '1',
                        padding: '8px',
                        "font-size": '13px',
                        "font-weight": '500',
                        "border-radius": '8px',
                        border: 'none',
                        background: 'var(--color-accent)',
                        color: '#ffffff',
                        cursor: 'pointer',
                      }}
                    >
                      Save
                    </button>
                  </div>
                </div>
              </Show>

              {/* Delete Confirmation */}
              <Show when={state().view === 'delete'}>
                <div style={{ padding: '12px' }}>
                  <div style={{ display: 'flex', "align-items": 'center', gap: '8px', "margin-bottom": '12px' }}>
                    <button
                      onClick={backToMenu}
                      style={{
                        width: '28px',
                        height: '28px',
                        display: 'flex',
                        "align-items": 'center',
                        "justify-content": 'center',
                        background: 'none',
                        border: 'none',
                        "border-radius": '6px',
                        cursor: 'pointer',
                        color: 'var(--color-text-muted)',
                      }}
                    >
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M15 18l-6-6 6-6" />
                      </svg>
                    </button>
                    <span style={{ "font-size": '13px', "font-weight": '600', color: 'var(--color-accent)' }}>
                      Delete Session
                    </span>
                  </div>
                  <p style={{
                    "font-size": '13px',
                    color: 'var(--color-text-secondary)',
                    "margin-bottom": '12px',
                    "line-height": '1.4',
                  }}>
                    This will permanently delete the session. This action cannot be undone.
                  </p>
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <button
                      onClick={closeMenu}
                      style={{
                        flex: '1',
                        padding: '8px',
                        "font-size": '13px',
                        "font-weight": '500',
                        "border-radius": '8px',
                        border: '1px solid var(--color-bg-overlay)',
                        background: 'var(--color-bg-elevated)',
                        color: 'var(--color-text-secondary)',
                        cursor: 'pointer',
                      }}
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleDelete}
                      disabled={deleting()}
                      style={{
                        flex: '1',
                        padding: '8px',
                        "font-size": '13px',
                        "font-weight": '500',
                        "border-radius": '8px',
                        border: 'none',
                        background: 'var(--color-accent)',
                        color: '#ffffff',
                        cursor: deleting() ? 'not-allowed' : 'pointer',
                        opacity: deleting() ? '0.7' : '1',
                      }}
                    >
                      {deleting() ? 'Deleting...' : 'Delete'}
                    </button>
                  </div>
                </div>
              </Show>
            </div>
          </>
        )}
      </Show>

      {/* New Session Modal */}
      <NewSessionModal
        isOpen={showNewSession()}
        onClose={() => setShowNewSession(false)}
      />
    </div>
  );
}
