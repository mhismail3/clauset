import { For, Show, onMount, createSignal } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { Button } from '../components/ui/Button';
import { Badge } from '../components/ui/Badge';
import { Spinner } from '../components/ui/Spinner';
import {
  sessions,
  activeCount,
  loading,
  error,
  fetchSessions,
  getStatusVariant,
  getStatusLabel,
  formatRelativeTime,
} from '../stores/sessions';
import { NewSessionModal } from '../components/chat/NewSessionModal';
import { api, Session } from '../lib/api';

export default function Sessions() {
  const [showNewSession, setShowNewSession] = createSignal(false);
  const [actionMenuSession, setActionMenuSession] = createSignal<Session | null>(null);
  const [renameValue, setRenameValue] = createSignal('');
  const [showRenameInput, setShowRenameInput] = createSignal(false);
  const navigate = useNavigate();

  onMount(() => {
    fetchSessions();
    const interval = setInterval(fetchSessions, 30000);
    return () => clearInterval(interval);
  });

  async function handleDelete(session: Session) {
    if (!confirm(`Delete session "${session.preview}"? This cannot be undone.`)) {
      return;
    }
    try {
      await api.sessions.delete(session.id);
      fetchSessions();
    } catch (e) {
      console.error('Failed to delete session:', e);
    }
    setActionMenuSession(null);
  }

  async function handleRename(session: Session, newName: string) {
    if (!newName.trim()) return;
    try {
      await api.sessions.rename(session.id, newName.trim());
      fetchSessions();
    } catch (e) {
      console.error('Failed to rename session:', e);
    }
    setShowRenameInput(false);
    setActionMenuSession(null);
  }

  function openActionMenu(e: Event, session: Session) {
    e.preventDefault();
    e.stopPropagation();
    setActionMenuSession(session);
    setRenameValue(session.preview);
    setShowRenameInput(false);
  }

  function closeActionMenu() {
    setActionMenuSession(null);
    setShowRenameInput(false);
  }

  return (
    <div class="flex flex-col h-full">
      {/* Header */}
      <header class="flex-none glass safe-top">
        <div
          style={{
            display: 'flex',
            "align-items": 'center',
            "justify-content": 'space-between',
            padding: '16px 20px',
          }}
        >
          <div>
            <h1 class="text-brand" style={{ color: 'var(--color-accent)', "font-size": '22px' }}>
              Clauset
            </h1>
            <p
              class="text-mono"
              style={{
                "font-size": '12px',
                color: 'var(--color-text-muted)',
                "margin-top": '2px',
              }}
            >
              <Show when={activeCount() > 0} fallback="no active sessions">
                {activeCount()} active
              </Show>
            </p>
          </div>
          <Button onClick={() => setShowNewSession(true)} size="sm">
            + new
          </Button>
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
                <div
                  class="card-retro card-pressable animate-fade-in"
                  style={{ overflow: 'hidden' }}
                >
                  <A
                    href={`/session/${session.id}`}
                    style={{
                      display: 'block',
                      padding: '16px',
                      "text-decoration": 'none',
                      color: 'inherit',
                    }}
                  >
                    {/* Top row: Project name + Badge + Menu */}
                    <div style={{ display: 'flex', "align-items": 'center', gap: '12px', "margin-bottom": '8px' }}>
                      <div style={{ flex: '1', "min-width": '0', display: 'flex', "align-items": 'center', gap: '8px' }}>
                        <span
                          class="text-mono"
                          style={{
                            "font-size": '14px',
                            "font-weight": '600',
                            color: 'var(--color-text-primary)',
                            overflow: 'hidden',
                            "text-overflow": 'ellipsis',
                            "white-space": 'nowrap',
                          }}
                        >
                          {session.project_path.split('/').pop() || 'Unknown'}
                        </span>
                        <Badge variant={getStatusVariant(session.status)}>
                          {getStatusLabel(session.status)}
                        </Badge>
                      </div>
                      <button
                        onClick={(e) => openActionMenu(e, session)}
                        style={{
                          width: '32px',
                          height: '32px',
                          display: 'flex',
                          "align-items": 'center',
                          "justify-content": 'center',
                          color: 'var(--color-text-muted)',
                          background: 'none',
                          border: 'none',
                          "border-radius": '50%',
                          cursor: 'pointer',
                        }}
                      >
                        <svg width="18" height="18" viewBox="0 0 20 20" fill="currentColor">
                          <circle cx="4" cy="10" r="1.5" />
                          <circle cx="10" cy="10" r="1.5" />
                          <circle cx="16" cy="10" r="1.5" />
                        </svg>
                      </button>
                    </div>

                    {/* Preview text */}
                    <p
                      style={{
                        "font-size": '14px',
                        color: 'var(--color-text-secondary)',
                        "margin-bottom": '12px',
                        display: '-webkit-box',
                        "-webkit-line-clamp": '2',
                        "-webkit-box-orient": 'vertical',
                        overflow: 'hidden',
                        "line-height": '1.4',
                      }}
                    >
                      {session.preview || 'No preview available'}
                    </p>

                    {/* Bottom row: Meta info */}
                    <div
                      class="text-mono"
                      style={{
                        display: 'flex',
                        "align-items": 'center',
                        gap: '12px',
                        "font-size": '11px',
                        color: 'var(--color-text-muted)',
                      }}
                    >
                      <span style={{ display: 'flex', "align-items": 'center', gap: '4px' }}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <circle cx="12" cy="12" r="10" />
                          <polyline points="12 6 12 12 16 14" />
                        </svg>
                        {formatRelativeTime(session.last_activity_at)}
                      </span>
                      <span style={{ color: 'var(--color-bg-overlay)' }}>·</span>
                      <span>{session.model}</span>
                      <Show when={session.total_cost_usd > 0}>
                        <span style={{ color: 'var(--color-bg-overlay)' }}>·</span>
                        <span>${session.total_cost_usd.toFixed(4)}</span>
                      </Show>
                    </div>
                  </A>
                </div>
              )}
            </For>
          </div>
        </div>
      </main>

      {/* Action Menu Sheet */}
      <Show when={actionMenuSession()}>
        {(session) => (
          <div
            class="overlay-backdrop animate-fade-in"
            style={{
              position: "fixed",
              top: "0",
              left: "0",
              right: "0",
              bottom: "0",
              "z-index": "50",
              display: "flex",
              "align-items": "flex-end",
              "justify-content": "center",
              padding: "0 16px 16px 16px",
            }}
            onClick={closeActionMenu}
          >
            <div
              class="animate-slide-up safe-bottom"
              style={{
                width: "min(400px, 100%)",
                background: 'var(--color-bg-surface)',
                "border-radius": "18px",
                border: '1px solid var(--color-bg-overlay)',
                overflow: "hidden",
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <div class="sheet-handle" />

              {/* Session info */}
              <div style={{ padding: "0 20px 16px", "text-align": "center" }}>
                <p class="text-headline">{session().preview || 'Session'}</p>
                <p class="text-mono text-caption" style={{ "margin-top": "4px" }}>
                  {session().project_path.split('/').pop()}
                </p>
              </div>

              <div class="divider" />

              <Show when={showRenameInput()}>
                <div style={{ padding: "20px" }}>
                  <input
                    type="text"
                    value={renameValue()}
                    onInput={(e) => setRenameValue(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleRename(session(), renameValue());
                      if (e.key === 'Escape') setShowRenameInput(false);
                    }}
                    class="input-retro"
                    placeholder="New name..."
                    autofocus
                    style={{ width: '100%', "box-sizing": 'border-box' }}
                  />
                  <div style={{ display: "flex", gap: "12px", "margin-top": "16px" }}>
                    <Button
                      variant="secondary"
                      style={{ flex: "1" }}
                      onClick={() => setShowRenameInput(false)}
                    >
                      Cancel
                    </Button>
                    <Button
                      style={{ flex: "1" }}
                      onClick={() => handleRename(session(), renameValue())}
                    >
                      Save
                    </Button>
                  </div>
                </div>
              </Show>

              <Show when={!showRenameInput()}>
                <div style={{ padding: "8px 0" }}>
                  <button
                    onClick={() => setShowRenameInput(true)}
                    class="hover:bg-bg-elevated transition-colors"
                    style={{
                      width: "100%",
                      display: "flex",
                      "align-items": "center",
                      gap: "16px",
                      padding: "14px 20px",
                      "text-align": "left",
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      "font-size": "15px",
                      color: 'var(--color-text-primary)',
                    }}
                  >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ color: 'var(--color-text-tertiary)' }}>
                      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    </svg>
                    <span>Rename</span>
                  </button>

                  <button
                    onClick={() => handleDelete(session())}
                    class="hover:bg-accent-muted transition-colors"
                    style={{
                      width: "100%",
                      display: "flex",
                      "align-items": "center",
                      gap: "16px",
                      padding: "14px 20px",
                      "text-align": "left",
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      "font-size": "15px",
                      color: 'var(--color-accent)',
                    }}
                  >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                    <span>Delete</span>
                  </button>

                  <div class="divider" style={{ margin: "8px 0" }} />

                  <button
                    onClick={closeActionMenu}
                    class="hover:bg-bg-elevated transition-colors"
                    style={{
                      width: "100%",
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "center",
                      padding: "14px 20px",
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      "font-size": "15px",
                      color: 'var(--color-text-muted)',
                    }}
                  >
                    Cancel
                  </button>
                </div>
              </Show>
            </div>
          </div>
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
