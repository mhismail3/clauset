import { For, Show, onMount, createSignal, createEffect } from 'solid-js';
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
    <div class="min-h-screen">
      {/* Header */}
      <header class="sticky top-0 z-40 bg-bg-base/80 backdrop-blur-sm border-b border-bg-elevated safe-top">
        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <h1 class="text-xl font-semibold">Sessions</h1>
            <p class="text-sm text-text-secondary">
              {activeCount()} active
            </p>
          </div>
          <Button onClick={() => setShowNewSession(true)}>
            + New
          </Button>
        </div>
      </header>

      {/* Content */}
      <main class="p-4 pb-20">
        <Show when={loading() && sessions().length === 0}>
          <div class="flex justify-center py-12">
            <Spinner size="lg" />
          </div>
        </Show>

        <Show when={error()}>
          <div class="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-400">
            {error()}
          </div>
        </Show>

        <Show when={!loading() && sessions().length === 0 && !error()}>
          <div class="text-center py-12">
            <p class="text-text-secondary mb-4">No sessions yet</p>
            <Button onClick={() => setShowNewSession(true)}>
              Start your first session
            </Button>
          </div>
        </Show>

        <div class="space-y-3">
          <For each={sessions()}>
            {(session) => (
              <div class="relative bg-bg-surface rounded-xl hover:bg-bg-elevated transition-colors">
                <A
                  href={`/session/${session.id}`}
                  class="block p-4"
                >
                  <div class="flex items-start justify-between gap-3">
                    <div class="flex-1 min-w-0">
                      <div class="flex items-center gap-2 mb-1">
                        <span class="font-medium truncate">
                          {session.project_path.split('/').pop() || 'Unknown'}
                        </span>
                        <Badge variant={getStatusVariant(session.status)}>
                          {getStatusLabel(session.status)}
                        </Badge>
                      </div>
                      <p class="text-sm text-text-secondary truncate">
                        {session.preview || 'No preview'}
                      </p>
                    </div>
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-text-muted whitespace-nowrap">
                        {formatRelativeTime(session.last_activity_at)}
                      </span>
                      <button
                        onClick={(e) => openActionMenu(e, session)}
                        class="p-1 text-text-muted hover:text-text-primary rounded"
                      >
                        ···
                      </button>
                    </div>
                  </div>
                  <div class="flex items-center gap-4 mt-2 text-xs text-text-muted">
                    <span>{session.model}</span>
                    <Show when={session.total_cost_usd > 0}>
                      <span>${session.total_cost_usd.toFixed(4)}</span>
                    </Show>
                  </div>
                </A>
              </div>
            )}
          </For>
        </div>

        {/* Action Menu Modal */}
        <Show when={actionMenuSession()}>
          {(session) => (
            <div
              class="fixed inset-0 z-50 flex items-end justify-center bg-black/50"
              onClick={closeActionMenu}
            >
              <div
                class="w-full max-w-lg bg-bg-surface rounded-t-2xl p-4 safe-bottom"
                onClick={(e) => e.stopPropagation()}
              >
                <div class="text-center mb-4">
                  <p class="font-medium">{session().preview || 'Session'}</p>
                  <p class="text-sm text-text-secondary">
                    {session().project_path.split('/').pop()}
                  </p>
                </div>

                <Show when={showRenameInput()}>
                  <div class="mb-4">
                    <input
                      type="text"
                      value={renameValue()}
                      onInput={(e) => setRenameValue(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') handleRename(session(), renameValue());
                        if (e.key === 'Escape') setShowRenameInput(false);
                      }}
                      class="w-full px-3 py-2 bg-bg-base border border-bg-elevated rounded-lg text-text-primary"
                      placeholder="New name..."
                      autofocus
                    />
                    <div class="flex gap-2 mt-2">
                      <Button
                        variant="ghost"
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
                  <div class="space-y-2">
                    <button
                      onClick={() => setShowRenameInput(true)}
                      class="w-full px-4 py-3 text-left hover:bg-bg-elevated rounded-lg transition-colors"
                    >
                      Rename
                    </button>
                    <button
                      onClick={() => handleDelete(session())}
                      class="w-full px-4 py-3 text-left text-red-400 hover:bg-red-500/10 rounded-lg transition-colors"
                    >
                      Delete
                    </button>
                    <button
                      onClick={closeActionMenu}
                      class="w-full px-4 py-3 text-left text-text-secondary hover:bg-bg-elevated rounded-lg transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                </Show>
              </div>
            </div>
          )}
        </Show>
      </main>

      {/* New Session Modal */}
      <NewSessionModal
        isOpen={showNewSession()}
        onClose={() => setShowNewSession(false)}
      />
    </div>
  );
}
