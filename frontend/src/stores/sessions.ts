import { createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import { api, Session, SessionListResponse, RecentAction } from '../lib/api';
import type { ActivityUpdate } from '../lib/globalWs';
import { cleanupOldSessions } from './terminal';

// Use a store for more granular reactivity (avoids re-rendering entire cards)
const [sessionsStore, setSessionsStore] = createStore<{ list: Session[] }>({ list: [] });
const [activeCount, setActiveCount] = createSignal(0);
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

// Accessor to maintain API compatibility
const sessions = () => sessionsStore.list;

export async function fetchSessions() {
  setLoading(true);
  setError(null);

  try {
    const response: SessionListResponse = await api.sessions.list();
    const existingSessions = sessionsStore.list;

    // Merge API data with existing local state, preserving recent_actions
    // This is important because recent_actions come from WebSocket updates
    // and may not be persisted in the backend or may be stale in the API response
    const sessionsWithDefaults = response.sessions.map(newSession => {
      const existing = existingSessions.find(s => s.id === newSession.id);

      // Preserve local recent_actions if:
      // 1. API response has no recent_actions or empty array
      // 2. Local state has recent_actions that are more recent or more complete
      let mergedActions = newSession.recent_actions || [];
      if (existing?.recent_actions && existing.recent_actions.length > 0) {
        if (mergedActions.length === 0) {
          // API has no actions, keep local
          mergedActions = existing.recent_actions;
        } else {
          // Merge: keep actions from both, deduplicate by type+summary, limit to 5
          const actionKey = (a: RecentAction) => `${a.action_type}:${a.summary}`;
          const seen = new Set<string>();
          const combined: RecentAction[] = [];

          // Add new actions first (they're more recent)
          for (const action of mergedActions) {
            const key = actionKey(action);
            if (!seen.has(key)) {
              seen.add(key);
              combined.push(action);
            }
          }

          // Add existing actions that aren't duplicates
          for (const action of existing.recent_actions) {
            const key = actionKey(action);
            if (!seen.has(key) && combined.length < 5) {
              seen.add(key);
              combined.push(action);
            }
          }

          mergedActions = combined.slice(0, 5);
        }
      }

      // Also preserve current_step if API doesn't have it but we do locally
      const currentStep = newSession.current_step || existing?.current_step;

      return {
        ...newSession,
        recent_actions: mergedActions,
        current_step: currentStep,
      };
    });

    setSessionsStore('list', sessionsWithDefaults);
    setActiveCount(response.active_count);

    // Cleanup terminal history for sessions that no longer exist
    const activeSessionIds = sessionsWithDefaults.map(s => s.id);
    cleanupOldSessions(activeSessionIds);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to fetch sessions');
  } finally {
    setLoading(false);
  }
}

/**
 * Update a session from an activity update received via global WebSocket.
 * Uses fine-grained path-based updates for better performance.
 * This prevents the entire card from flickering on updates.
 */
export function updateSessionFromActivity(update: ActivityUpdate) {
  // Find the index of the session to update
  const idx = sessionsStore.list.findIndex(s => s.id === update.session_id);
  if (idx === -1) return;

  const session = sessionsStore.list[idx];

  // Preserve existing actions if update has empty array
  // This prevents actions from disappearing when the prompt appears
  // (the tool invocations might have scrolled out of the parse window)
  const newActions = (update.recent_actions && update.recent_actions.length > 0)
    ? update.recent_actions
    : session.recent_actions || [];

  // Use path-based update for fine-grained reactivity
  // This only invalidates the specific session, not the entire list
  setSessionsStore('list', idx, {
    model: update.model || session.model,
    total_cost_usd: update.cost,
    input_tokens: update.input_tokens,
    output_tokens: update.output_tokens,
    context_percent: update.context_percent,
    preview: update.current_activity || session.preview,
    current_step: update.current_step,
    recent_actions: newActions,
  });
}

/**
 * Update a session's status when it changes (e.g., when session stops/completes).
 * Called from global WebSocket when status_change event is received.
 */
export function updateSessionStatus(sessionId: string, newStatus: Session['status']) {
  // Find the index of the session to update
  const idx = sessionsStore.list.findIndex(s => s.id === sessionId);
  if (idx === -1) return;

  const session = sessionsStore.list[idx];

  // Use path-based update for fine-grained reactivity
  setSessionsStore('list', idx, {
    status: newStatus,
    // Clear current_step when session is done
    current_step: newStatus === 'stopped' ? undefined : session.current_step,
  });
}

/**
 * Get a specific session by ID with reactive access
 */
export function getSession(id: string): Session | undefined {
  return sessionsStore.list.find(s => s.id === id);
}

export function getStatusVariant(status: Session['status']): 'active' | 'starting' | 'idle' | 'completed' | 'error' {
  switch (status) {
    case 'active':
      return 'active';
    case 'starting':
      return 'starting';
    case 'waiting_input':
      return 'idle';
    case 'stopped':
    case 'created':
      return 'completed';
    case 'error':
      return 'error';
    default:
      return 'completed';
  }
}

export function getStatusLabel(status: Session['status']): string {
  switch (status) {
    case 'active':
      return 'Active';
    case 'starting':
      return 'Starting';
    case 'waiting_input':
      return 'Waiting';
    case 'stopped':
      return 'Stopped';
    case 'created':
      return 'Created';
    case 'error':
      return 'Error';
    default:
      return status;
  }
}

export function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;
  return date.toLocaleDateString();
}

export { sessions, activeCount, loading, error };
