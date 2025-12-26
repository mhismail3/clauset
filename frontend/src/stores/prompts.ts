import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { api, PromptSummary } from '../lib/api';

const PAGE_SIZE = 50;

// Store state
const [promptsStore, setPromptsStore] = createStore<{
  prompts: PromptSummary[];
  totalCount: number;
  hasMore: boolean;
  expandedId: string | null;
  expandedContent: string | null;
}>({
  prompts: [],
  totalCount: 0,
  hasMore: false,
  expandedId: null,
  expandedContent: null,
});

// Loading states
const [loading, setLoading] = createSignal(false);
const [loadingMore, setLoadingMore] = createSignal(false);
const [loadingExpanded, setLoadingExpanded] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

// Fetch prompts with optional reset (for initial load or refresh)
export async function fetchPrompts(reset = false) {
  if (reset) {
    setLoading(true);
    setError(null);
  } else {
    setLoadingMore(true);
  }

  try {
    const offset = reset ? 0 : promptsStore.prompts.length;
    const response = await api.prompts.list(PAGE_SIZE, offset);

    if (reset) {
      setPromptsStore('prompts', response.prompts);
    } else {
      setPromptsStore('prompts', [...promptsStore.prompts, ...response.prompts]);
    }
    setPromptsStore('totalCount', response.total_count);
    setPromptsStore('hasMore', response.has_more);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to load prompts');
  } finally {
    setLoading(false);
    setLoadingMore(false);
  }
}

// Load more prompts (pagination)
export async function loadMore() {
  if (loadingMore() || !promptsStore.hasMore) return;
  await fetchPrompts(false);
}

// Toggle expanded state for a prompt
export async function toggleExpanded(id: string) {
  // If already expanded, collapse
  if (promptsStore.expandedId === id) {
    setPromptsStore('expandedId', null);
    setPromptsStore('expandedContent', null);
    return;
  }

  // Expand and fetch full content
  setPromptsStore('expandedId', id);
  setPromptsStore('expandedContent', null);
  setLoadingExpanded(true);

  try {
    const prompt = await api.prompts.get(id);
    // Only set content if this prompt is still the expanded one
    if (promptsStore.expandedId === id) {
      setPromptsStore('expandedContent', prompt.content);
    }
  } catch (e) {
    console.error('Failed to load prompt content:', e);
    // Show the preview as fallback
    const summary = promptsStore.prompts.find(p => p.id === id);
    if (summary && promptsStore.expandedId === id) {
      setPromptsStore('expandedContent', summary.preview + '...');
    }
  } finally {
    setLoadingExpanded(false);
  }
}

// Copy prompt content to clipboard
export async function copyPrompt(content: string) {
  // Try modern clipboard API first
  if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
      await navigator.clipboard.writeText(content);
      return true;
    } catch (e) {
      console.error('Clipboard API failed:', e);
    }
  }

  // Fallback for non-HTTPS contexts
  try {
    const textArea = document.createElement('textarea');
    textArea.value = content;
    textArea.style.position = 'fixed';
    textArea.style.left = '-9999px';
    textArea.style.top = '-9999px';
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();

    const successful = document.execCommand('copy');
    document.body.removeChild(textArea);

    if (successful) {
      return true;
    } else {
      console.error('execCommand copy failed');
      return false;
    }
  } catch (e) {
    console.error('Fallback copy failed:', e);
    return false;
  }
}

// Add a new prompt from real-time update
export function addNewPrompt(prompt: PromptSummary) {
  setPromptsStore(
    produce((store) => {
      // Add at the beginning (newest first)
      store.prompts.unshift(prompt);
      store.totalCount++;
    })
  );
}

// Reset store state
export function resetPrompts() {
  setPromptsStore('prompts', []);
  setPromptsStore('totalCount', 0);
  setPromptsStore('hasMore', false);
  setPromptsStore('expandedId', null);
  setPromptsStore('expandedContent', null);
  setError(null);
}

// Exports
export {
  promptsStore,
  loading,
  loadingMore,
  loadingExpanded,
  error,
};
