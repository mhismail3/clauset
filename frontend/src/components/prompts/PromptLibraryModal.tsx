import { Show, For, createEffect, onCleanup, createSignal } from 'solid-js';
import { Spinner } from '../ui/Spinner';
import {
  promptsStore,
  loading,
  loadingMore,
  loadingExpanded,
  error,
  fetchPrompts,
  loadMore,
  toggleExpanded,
  copyPrompt,
  resetPrompts,
} from '../../stores/prompts';

interface PromptLibraryModalProps {
  isOpen: boolean;
  onClose: () => void;
}

function formatRelativeTime(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp;
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) return `${days}d ago`;
  if (hours > 0) return `${hours}h ago`;
  if (minutes > 0) return `${minutes}m ago`;
  return 'just now';
}

export function PromptLibraryModal(props: PromptLibraryModalProps) {
  const [copiedId, setCopiedId] = createSignal<string | null>(null);
  let listRef: HTMLDivElement | undefined;

  // Load prompts when modal opens
  createEffect(() => {
    if (props.isOpen) {
      fetchPrompts(true);
    } else {
      resetPrompts();
    }
  });

  // Handle escape key
  createEffect(() => {
    if (!props.isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        props.onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  // Infinite scroll
  const handleScroll = (e: Event) => {
    const target = e.target as HTMLDivElement;
    const nearBottom = target.scrollHeight - target.scrollTop <= target.clientHeight + 100;
    if (nearBottom && !loadingMore() && promptsStore.hasMore) {
      loadMore();
    }
  };

  const handleCopy = async (id: string, content: string) => {
    const success = await copyPrompt(content);
    if (success) {
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    }
  };

  return (
    <Show when={props.isOpen}>
      <div
        style={{
          position: 'fixed',
          inset: '0',
          'z-index': '1000',
          display: 'flex',
          'align-items': 'flex-start',
          'justify-content': 'center',
          'padding-top': 'max(env(safe-area-inset-top, 0px) + 5vh, 40px)',
          background: 'rgba(0, 0, 0, 0.6)',
          'backdrop-filter': 'blur(4px)',
          '-webkit-backdrop-filter': 'blur(4px)',
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
      >
        <div
          style={{
            width: 'min(700px, 92vw)',
            'max-height': '80vh',
            background: 'var(--color-bg-elevated)',
            'border-radius': '12px',
            'box-shadow': '0 8px 32px rgba(0, 0, 0, 0.3)',
            overflow: 'hidden',
            display: 'flex',
            'flex-direction': 'column',
          }}
        >
          {/* Header */}
          <div
            style={{
              padding: '16px 20px',
              'border-bottom': '1px solid var(--color-bg-overlay)',
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'space-between',
            }}
          >
            <div style={{ display: 'flex', 'align-items': 'center', gap: '12px' }}>
              <svg
                width="22"
                height="22"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                style={{ color: 'var(--color-accent)' }}
              >
                <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
                <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
              </svg>
              <span
                style={{
                  'font-family': 'var(--font-mono)',
                  'font-size': '15px',
                  'font-weight': '600',
                  color: 'var(--color-text-primary)',
                }}
              >
                Prompt Library
              </span>
              <Show when={promptsStore.totalCount > 0}>
                <span
                  style={{
                    'font-family': 'var(--font-mono)',
                    'font-size': '12px',
                    color: 'var(--color-text-muted)',
                    background: 'var(--color-bg-base)',
                    padding: '2px 8px',
                    'border-radius': '10px',
                  }}
                >
                  {promptsStore.totalCount.toLocaleString()}
                </span>
              </Show>
            </div>
            <button
              onClick={props.onClose}
              style={{
                background: 'none',
                border: 'none',
                padding: '8px',
                cursor: 'pointer',
                color: 'var(--color-text-muted)',
                display: 'flex',
                'align-items': 'center',
                'justify-content': 'center',
                'border-radius': '6px',
              }}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          {/* Content */}
          <div
            ref={listRef}
            onScroll={handleScroll}
            style={{
              flex: '1',
              'overflow-y': 'auto',
              padding: '12px',
            }}
          >
            <Show when={loading()}>
              <div
                style={{
                  display: 'flex',
                  'justify-content': 'center',
                  'align-items': 'center',
                  padding: '40px',
                }}
              >
                <Spinner size="md" />
              </div>
            </Show>

            <Show when={error()}>
              <div
                style={{
                  padding: '20px',
                  color: 'var(--color-accent)',
                  'text-align': 'center',
                  'font-family': 'var(--font-mono)',
                  'font-size': '13px',
                }}
              >
                {error()}
              </div>
            </Show>

            <Show when={!loading() && !error() && promptsStore.prompts.length === 0}>
              <div
                style={{
                  padding: '40px 20px',
                  'text-align': 'center',
                  color: 'var(--color-text-muted)',
                  'font-family': 'var(--font-mono)',
                  'font-size': '13px',
                }}
              >
                No prompts yet. Start a session to see your prompts here.
              </div>
            </Show>

            <Show when={!loading() && promptsStore.prompts.length > 0}>
              <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
                <For each={promptsStore.prompts}>
                  {(prompt) => (
                    <div
                      style={{
                        background: 'var(--color-bg-base)',
                        'border-radius': '8px',
                        border: '1px solid var(--color-bg-overlay)',
                        overflow: 'hidden',
                        cursor: 'pointer',
                      }}
                      onClick={() => toggleExpanded(prompt.id)}
                    >
                      {/* Prompt header */}
                      <div
                        style={{
                          padding: '12px 14px',
                          display: 'flex',
                          'align-items': 'flex-start',
                          gap: '12px',
                        }}
                      >
                        {/* Expand indicator */}
                        <svg
                          width="14"
                          height="14"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          style={{
                            color: 'var(--color-text-muted)',
                            'flex-shrink': '0',
                            'margin-top': '3px',
                            transform: promptsStore.expandedId === prompt.id ? 'rotate(90deg)' : 'rotate(0deg)',
                            transition: 'transform 0.15s ease',
                          }}
                        >
                          <polyline points="9 18 15 12 9 6" />
                        </svg>

                        {/* Content */}
                        <div style={{ flex: '1', 'min-width': '0' }}>
                          <div
                            style={{
                              'font-family': 'var(--font-mono)',
                              'font-size': '13px',
                              color: 'var(--color-text-primary)',
                              'line-height': '1.5',
                              'white-space': 'pre-wrap',
                              'word-break': 'break-word',
                            }}
                          >
                            {promptsStore.expandedId === prompt.id && promptsStore.expandedContent
                              ? promptsStore.expandedContent
                              : prompt.preview}
                          </div>

                          <Show when={promptsStore.expandedId === prompt.id && loadingExpanded()}>
                            <div style={{ 'margin-top': '8px' }}>
                              <Spinner size="sm" />
                            </div>
                          </Show>

                          {/* Metadata row */}
                          <div
                            style={{
                              'margin-top': '8px',
                              display: 'flex',
                              'align-items': 'center',
                              gap: '12px',
                              'font-family': 'var(--font-mono)',
                              'font-size': '11px',
                              color: 'var(--color-text-muted)',
                            }}
                          >
                            <span
                              style={{
                                background: 'var(--color-bg-overlay)',
                                padding: '2px 6px',
                                'border-radius': '4px',
                              }}
                            >
                              {prompt.project_name}
                            </span>
                            <span>{formatRelativeTime(prompt.timestamp)}</span>
                            <span>{prompt.word_count} words</span>
                          </div>
                        </div>

                        {/* Copy button */}
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            const content = promptsStore.expandedId === prompt.id && promptsStore.expandedContent
                              ? promptsStore.expandedContent
                              : prompt.preview;
                            handleCopy(prompt.id, content);
                          }}
                          style={{
                            background: copiedId() === prompt.id ? 'var(--color-secondary)' : 'var(--color-bg-overlay)',
                            border: 'none',
                            padding: '6px 10px',
                            'border-radius': '6px',
                            cursor: 'pointer',
                            color: copiedId() === prompt.id ? 'white' : 'var(--color-text-secondary)',
                            'font-family': 'var(--font-mono)',
                            'font-size': '11px',
                            display: 'flex',
                            'align-items': 'center',
                            gap: '4px',
                            'flex-shrink': '0',
                            transition: 'background 0.15s ease, color 0.15s ease',
                          }}
                        >
                          <Show
                            when={copiedId() === prompt.id}
                            fallback={
                              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                              </svg>
                            }
                          >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                              <polyline points="20 6 9 17 4 12" />
                            </svg>
                          </Show>
                          {copiedId() === prompt.id ? 'Copied' : 'Copy'}
                        </button>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            {/* Load more indicator */}
            <Show when={loadingMore()}>
              <div
                style={{
                  display: 'flex',
                  'justify-content': 'center',
                  padding: '16px',
                }}
              >
                <Spinner size="sm" />
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}
