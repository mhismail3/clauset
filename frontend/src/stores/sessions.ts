import { createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import { api, Session, SessionListResponse, RecentAction } from '../lib/api';
import type { ActivityUpdate } from '../lib/globalWs';

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
    // Ensure recent_actions is always an array
    const sessionsWithDefaults = response.sessions.map(s => ({
      ...s,
      recent_actions: s.recent_actions || [],
    }));
    setSessionsStore('list', sessionsWithDefaults);
    setActiveCount(response.active_count);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to fetch sessions');
  } finally {
    setLoading(false);
  }
}

/**
 * Update a session from an activity update received via global WebSocket.
 * Uses produce() for fine-grained reactivity - only updates changed fields.
 * This prevents the entire card from flickering on updates.
 */
export function updateSessionFromActivity(update: ActivityUpdate) {
  setSessionsStore('list', (sessions) =>
    sessions.map((session) => {
      if (session.id === update.session_id) {
        // Create updated session with new values
        return {
          ...session,
          model: update.model || session.model,
          total_cost_usd: update.cost,
          input_tokens: update.input_tokens,
          output_tokens: update.output_tokens,
          context_percent: update.context_percent,
          preview: update.current_activity || session.preview,
          current_step: update.current_step,
          recent_actions: update.recent_actions || session.recent_actions || [],
        };
      }
      return session;
    })
  );
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
