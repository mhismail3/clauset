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
    // Refresh every 30 seconds
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
      <header class="flex-none glass border-b border-bg-overlay/50 safe-top">
        <div class="flex items-center justify-between px-5 py-4">
          <div>
            <h1 class="text-title">Sessions</h1>
            <p class="text-caption mt-0.5">
              <Show when={activeCount() > 0} fallback="No active sessions">
                {activeCount()} active
              </Show>
            </p>
          </div>
          <Button onClick={() => setShowNewSession(true)} size="sm">
            New
          </Button>
        </div>
      </header>

      {/* Content */}
      <main class="flex-1 scrollable">
        <div class="p-4 pb-8">
          <Show when={loading() && sessions().length === 0}>
            <div class="flex justify-center py-16">
              <Spinner size="lg" />
            </div>
          </Show>

          <Show when={error()}>
            <div class="bg-status-error/10 border border-status-error/20 rounded-xl p-4 text-status-error mb-4">
              {error()}
            </div>
          </Show>

          <Show when={!loading() && sessions().length === 0 && !error()}>
            <div class="text-center py-16">
              <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-bg-surface flex items-center justify-center">
                <span class="text-3xl">💬</span>
              </div>
              <p class="text-text-secondary mb-4">No sessions yet</p>
              <Button onClick={() => setShowNewSession(true)}>
                Start your first session
              </Button>
            </div>
          </Show>

          {/* Session Cards */}
          <div class="space-y-3">
            <For each={sessions()}>
              {(session) => (
                <div
                  class="bg-bg-surface rounded-2xl overflow-hidden card-pressable animate-fade-in"
                >
                  <A
                    href={`/session/${session.id}`}
                    class="block p-4"
                  >
                    {/* Top row: Project name + Badge + Menu */}
                    <div class="flex items-center gap-3 mb-2">
                      <div class="flex-1 min-w-0 flex items-center gap-2">
                        <span class="text-headline truncate">
                          {session.project_path.split('/').pop() || 'Unknown'}
                        </span>
                        <Badge variant={getStatusVariant(session.status)}>
                          {getStatusLabel(session.status)}
                        </Badge>
                      </div>
                      <button
                        onClick={(e) => openActionMenu(e, session)}
                        class="w-8 h-8 flex items-center justify-center text-text-muted rounded-full hover:bg-bg-overlay transition-colors"
                      >
                        <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
                          <circle cx="4" cy="10" r="1.5" />
                          <circle cx="10" cy="10" r="1.5" />
                          <circle cx="16" cy="10" r="1.5" />
                        </svg>
                      </button>
                    </div>

                    {/* Preview text */}
                    <p class="text-body text-text-secondary line-clamp-2 mb-3">
                      {session.preview || 'No preview available'}
                    </p>

                    {/* Bottom row: Meta info */}
                    <div class="flex items-center gap-3 text-caption text-text-muted">
                      <span class="flex items-center gap-1">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <circle cx="12" cy="12" r="10" />
                          <polyline points="12 6 12 12 16 14" />
                        </svg>
                        {formatRelativeTime(session.last_activity_at)}
                      </span>
                      <span class="text-text-muted/50">•</span>
                      <span>{session.model}</span>
                      <Show when={session.total_cost_usd > 0}>
                        <span class="text-text-muted/50">•</span>
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
            class="fixed inset-0 z-50 overlay-backdrop animate-fade-in"
            onClick={closeActionMenu}
          >
            <div class="absolute inset-x-0 bottom-0 sheet-container safe-bottom animate-slide-up">
              {/* Sheet handle */}
              <div class="sheet-handle" />

              {/* Session info */}
              <div class="px-5 pb-4 text-center">
                <p class="text-headline">{session().preview || 'Session'}</p>
                <p class="text-caption mt-1">
                  {session().project_path.split('/').pop()}
                </p>
              </div>

              <div class="divider" />

              <Show when={showRenameInput()}>
                <div class="p-5" onClick={(e) => e.stopPropagation()}>
                  <input
                    type="text"
                    value={renameValue()}
                    onInput={(e) => setRenameValue(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleRename(session(), renameValue());
                      if (e.key === 'Escape') setShowRenameInput(false);
                    }}
                    class="w-full px-4 py-3 bg-bg-base border border-bg-overlay rounded-xl text-text-primary text-body focus-ring"
                    placeholder="New name..."
                    autofocus
                  />
                  <div class="flex gap-3 mt-4">
                    <Button
                      variant="secondary"
                      class="flex-1"
                      onClick={() => setShowRenameInput(false)}
                    >
                      Cancel
                    </Button>
                    <Button
                      class="flex-1"
                      onClick={() => handleRename(session(), renameValue())}
                    >
                      Save
                    </Button>
                  </div>
                </div>
              </Show>

              <Show when={!showRenameInput()}>
                <div class="py-2" onClick={(e) => e.stopPropagation()}>
                  <button
                    onClick={() => setShowRenameInput(true)}
                    class="w-full flex items-center gap-4 px-5 py-4 text-left hover:bg-bg-elevated transition-colors touch-target"
                  >
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-text-secondary">
                      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    </svg>
                    <span class="text-body">Rename</span>
                  </button>

                  <button
                    onClick={() => handleDelete(session())}
                    class="w-full flex items-center gap-4 px-5 py-4 text-left text-status-error hover:bg-status-error/10 transition-colors touch-target"
                  >
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                    <span class="text-body">Delete</span>
                  </button>

                  <div class="divider my-2" />

                  <button
                    onClick={closeActionMenu}
                    class="w-full flex items-center justify-center py-4 text-text-muted text-body touch-target"
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
