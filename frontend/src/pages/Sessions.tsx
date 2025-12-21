import { For, Show, onMount, createSignal } from 'solid-js';
import { A } from '@solidjs/router';
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

export default function Sessions() {
  const [showNewSession, setShowNewSession] = createSignal(false);

  onMount(() => {
    fetchSessions();
    // Refresh every 30 seconds
    const interval = setInterval(fetchSessions, 30000);
    return () => clearInterval(interval);
  });

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
              <A
                href={`/session/${session.id}`}
                class="block bg-bg-surface rounded-xl p-4 hover:bg-bg-elevated transition-colors"
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
                  <span class="text-xs text-text-muted whitespace-nowrap">
                    {formatRelativeTime(session.last_activity_at)}
                  </span>
                </div>
                <div class="flex items-center gap-4 mt-2 text-xs text-text-muted">
                  <span>{session.model}</span>
                  <Show when={session.total_cost_usd > 0}>
                    <span>${session.total_cost_usd.toFixed(4)}</span>
                  </Show>
                </div>
              </A>
            )}
          </For>
        </div>
      </main>

      {/* New Session Modal */}
      <NewSessionModal
        isOpen={showNewSession()}
        onClose={() => setShowNewSession(false)}
      />
    </div>
  );
}
