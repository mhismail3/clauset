import { createSignal } from 'solid-js';
import { api, Session, SessionListResponse } from '../lib/api';

const [sessions, setSessions] = createSignal<Session[]>([]);
const [activeCount, setActiveCount] = createSignal(0);
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

export async function fetchSessions() {
  setLoading(true);
  setError(null);

  try {
    const response: SessionListResponse = await api.sessions.list();
    setSessions(response.sessions);
    setActiveCount(response.active_count);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to fetch sessions');
  } finally {
    setLoading(false);
  }
}

export function getStatusVariant(status: Session['status']): 'active' | 'idle' | 'completed' | 'error' {
  switch (status) {
    case 'active':
    case 'starting':
      return 'active';
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
