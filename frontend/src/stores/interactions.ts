import { createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import {
  api,
  InteractionSummary,
  InteractionDetailResponse,
  GlobalSearchResults,
  AnalyticsResponse,
  FilesChangedResponse,
  DiffResponse,
} from '../lib/api';

// Store for interaction list per session
const [interactionsStore, setInteractionsStore] = createStore<{
  bySession: Record<string, InteractionSummary[]>;
  totalCounts: Record<string, number>;
}>({
  bySession: {},
  totalCounts: {},
});

// Store for interaction details (full data including tools and file changes)
const [detailsStore, setDetailsStore] = createStore<{
  byId: Record<string, InteractionDetailResponse>;
}>({
  byId: {},
});

// Current session being viewed
const [currentSessionId, setCurrentSessionId] = createSignal<string | null>(null);
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

// Search state
const [searchQuery, setSearchQuery] = createSignal('');
const [searchResults, setSearchResults] = createSignal<GlobalSearchResults | null>(null);
const [searchLoading, setSearchLoading] = createSignal(false);

// Analytics state
const [analytics, setAnalytics] = createSignal<AnalyticsResponse | null>(null);
const [analyticsLoading, setAnalyticsLoading] = createSignal(false);

// Files changed for current session
const [filesChanged, setFilesChanged] = createSignal<FilesChangedResponse | null>(null);

// Diff state
const [currentDiff, setCurrentDiff] = createSignal<DiffResponse | null>(null);
const [diffLoading, setDiffLoading] = createSignal(false);

/**
 * Fetch interactions for a session
 */
export async function fetchInteractions(sessionId: string, limit?: number, offset?: number) {
  setLoading(true);
  setError(null);
  setCurrentSessionId(sessionId);

  try {
    const response = await api.interactions.list(sessionId, limit, offset);
    setInteractionsStore('bySession', sessionId, response.interactions);
    setInteractionsStore('totalCounts', sessionId, response.total_count);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to fetch interactions');
  } finally {
    setLoading(false);
  }
}

/**
 * Get interactions for a session (reactive accessor)
 */
export function getInteractions(sessionId: string): InteractionSummary[] {
  return interactionsStore.bySession[sessionId] ?? [];
}

/**
 * Get total interaction count for a session
 */
export function getTotalCount(sessionId: string): number {
  return interactionsStore.totalCounts[sessionId] ?? 0;
}

/**
 * Fetch full interaction detail
 */
export async function fetchInteractionDetail(interactionId: string) {
  setLoading(true);
  setError(null);

  try {
    const response = await api.interactions.get(interactionId);
    setDetailsStore('byId', interactionId, response);
    return response;
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to fetch interaction details');
    return null;
  } finally {
    setLoading(false);
  }
}

/**
 * Get interaction detail (reactive accessor)
 */
export function getInteractionDetail(interactionId: string): InteractionDetailResponse | undefined {
  return detailsStore.byId[interactionId];
}

/**
 * Fetch files changed in a session
 */
export async function fetchFilesChanged(sessionId: string) {
  try {
    const response = await api.interactions.filesChanged(sessionId);
    setFilesChanged(response);
    return response;
  } catch (e) {
    console.error('Failed to fetch files changed:', e);
    return null;
  }
}

/**
 * Compute diff between two interactions
 */
export async function computeDiff(
  fromInteraction: string,
  toInteraction: string,
  file: string,
  context?: number
) {
  setDiffLoading(true);

  try {
    const response = await api.diff.compute(fromInteraction, toInteraction, file, context);
    setCurrentDiff(response);
    return response;
  } catch (e) {
    console.error('Failed to compute diff:', e);
    return null;
  } finally {
    setDiffLoading(false);
  }
}

/**
 * Search across sessions
 */
export async function search(
  query: string,
  options?: { scope?: string; sessionId?: string; limit?: number; offset?: number }
) {
  if (!query.trim()) {
    setSearchResults(null);
    return;
  }

  setSearchLoading(true);
  setSearchQuery(query);

  try {
    const results = await api.search.query(query, options);
    setSearchResults(results);
    return results;
  } catch (e) {
    console.error('Failed to search:', e);
    return null;
  } finally {
    setSearchLoading(false);
  }
}

/**
 * Clear search results
 */
export function clearSearch() {
  setSearchQuery('');
  setSearchResults(null);
}

/**
 * Fetch analytics
 */
export async function fetchAnalytics(days?: number) {
  setAnalyticsLoading(true);

  try {
    const response = await api.analytics.get(days);
    setAnalytics(response);
    return response;
  } catch (e) {
    console.error('Failed to fetch analytics:', e);
    return null;
  } finally {
    setAnalyticsLoading(false);
  }
}

/**
 * Format duration in milliseconds to human readable
 */
export function formatDuration(ms: number | undefined): string {
  if (!ms) return '-';
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.round((ms % 60000) / 1000);
  return `${mins}m ${secs}s`;
}

/**
 * Format file size in bytes to human readable
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

/**
 * Format cost in USD
 */
export function formatCost(cost: number): string {
  if (cost === 0) return '$0.00';
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

/**
 * Format token count
 */
export function formatTokens(tokens: number): string {
  if (tokens < 1000) return tokens.toString();
  if (tokens < 1000000) return `${(tokens / 1000).toFixed(1)}K`;
  return `${(tokens / 1000000).toFixed(2)}M`;
}

// Export signals and stores
export {
  interactionsStore,
  currentSessionId,
  loading,
  error,
  searchQuery,
  searchResults,
  searchLoading,
  analytics,
  analyticsLoading,
  filesChanged,
  currentDiff,
  diffLoading,
};
