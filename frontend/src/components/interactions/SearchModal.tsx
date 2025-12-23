import { Show, For, createSignal, onMount, onCleanup } from 'solid-js';
import { A } from '@solidjs/router';
import { Spinner } from '../ui/Spinner';
import { Badge } from '../ui/Badge';
import {
  search,
  clearSearch,
  searchQuery,
  searchResults,
  searchLoading,
  formatCost,
} from '../../stores/interactions';
import { formatRelativeTime } from '../../stores/sessions';

interface SearchModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialQuery?: string;
  sessionId?: string; // Optional: limit search to specific session
}

type SearchScope = 'all' | 'prompts' | 'files' | 'tools';

export function SearchModal(props: SearchModalProps) {
  const [query, setQuery] = createSignal('');
  const [scope, setScope] = createSignal<SearchScope>('all');
  let inputRef: HTMLInputElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    if (props.initialQuery) {
      setQuery(props.initialQuery);
    }
    // Focus input when modal opens
    setTimeout(() => inputRef?.focus(), 100);

    // Handle escape key
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        props.onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  const handleSearch = (q: string) => {
    setQuery(q);

    // Debounce search
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      if (q.trim().length >= 2) {
        search(q, {
          scope: scope() === 'all' ? undefined : scope(),
          sessionId: props.sessionId,
          limit: 50,
        });
      } else {
        clearSearch();
      }
    }, 300);
  };

  const handleScopeChange = (newScope: SearchScope) => {
    setScope(newScope);
    if (query().trim().length >= 2) {
      search(query(), {
        scope: newScope === 'all' ? undefined : newScope,
        sessionId: props.sessionId,
        limit: 50,
      });
    }
  };

  const results = () => searchResults();
  const hasResults = () => {
    const r = results();
    if (!r) return false;
    return r.interactions.length > 0 || r.tool_invocations.length > 0 || r.file_matches.length > 0;
  };

  if (!props.isOpen) return null;

  return (
    <div
      style={{
        position: 'fixed',
        inset: '0',
        'z-index': '1000',
        display: 'flex',
        'align-items': 'flex-start',
        'justify-content': 'center',
        'padding-top': 'max(10vh, 60px)',
        background: 'rgba(0, 0, 0, 0.6)',
        'backdrop-filter': 'blur(4px)',
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div
        style={{
          width: 'min(600px, 90vw)',
          'max-height': '70vh',
          background: 'var(--color-bg-elevated)',
          'border-radius': '12px',
          'box-shadow': '0 8px 32px rgba(0, 0, 0, 0.3)',
          overflow: 'hidden',
          display: 'flex',
          'flex-direction': 'column',
        }}
      >
        {/* Search input */}
        <div style={{ padding: '16px', 'border-bottom': '1px solid var(--color-bg-overlay)' }}>
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              gap: '12px',
              background: 'var(--color-bg-base)',
              'border-radius': '8px',
              padding: '12px 16px',
            }}
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              style={{ color: 'var(--color-text-muted)', 'flex-shrink': '0' }}
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              ref={inputRef}
              type="text"
              placeholder="Search interactions, tools, files..."
              value={query()}
              onInput={(e) => handleSearch(e.currentTarget.value)}
              class="text-mono"
              style={{
                flex: '1',
                background: 'transparent',
                border: 'none',
                outline: 'none',
                'font-size': '14px',
                color: 'var(--color-text-primary)',
              }}
            />
            <Show when={searchLoading()}>
              <Spinner />
            </Show>
            <Show when={query() && !searchLoading()}>
              <button
                onClick={() => {
                  setQuery('');
                  clearSearch();
                  inputRef?.focus();
                }}
                style={{
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  color: 'var(--color-text-muted)',
                  padding: '4px',
                }}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </Show>
          </div>

          {/* Scope tabs */}
          <div
            style={{
              display: 'flex',
              gap: '8px',
              'margin-top': '12px',
            }}
          >
            {(['all', 'prompts', 'files', 'tools'] as SearchScope[]).map((s) => (
              <button
                onClick={() => handleScopeChange(s)}
                class="text-mono"
                style={{
                  padding: '6px 12px',
                  'border-radius': '6px',
                  border: 'none',
                  cursor: 'pointer',
                  'font-size': '12px',
                  background: scope() === s ? 'var(--color-accent)' : 'var(--color-bg-base)',
                  color: scope() === s ? 'white' : 'var(--color-text-secondary)',
                }}
              >
                {s.charAt(0).toUpperCase() + s.slice(1)}
              </button>
            ))}
          </div>
        </div>

        {/* Results */}
        <div style={{ flex: '1', overflow: 'auto', padding: '16px' }}>
          <Show when={query().length < 2 && !hasResults()}>
            <div
              style={{
                display: 'flex',
                'flex-direction': 'column',
                'align-items': 'center',
                'justify-content': 'center',
                padding: '32px',
                'text-align': 'center',
              }}
            >
              <p
                class="text-mono"
                style={{
                  'font-size': '13px',
                  color: 'var(--color-text-muted)',
                  margin: '0',
                }}
              >
                Type at least 2 characters to search
              </p>
            </div>
          </Show>

          <Show when={query().length >= 2 && !searchLoading() && !hasResults()}>
            <div
              style={{
                display: 'flex',
                'flex-direction': 'column',
                'align-items': 'center',
                'justify-content': 'center',
                padding: '32px',
                'text-align': 'center',
              }}
            >
              <p
                class="text-mono"
                style={{
                  'font-size': '13px',
                  color: 'var(--color-text-muted)',
                  margin: '0',
                }}
              >
                No results found for "{query()}"
              </p>
            </div>
          </Show>

          <Show when={hasResults()}>
            <div style={{ display: 'flex', 'flex-direction': 'column', gap: '16px' }}>
              {/* Interaction results */}
              <Show when={results()!.interactions.length > 0}>
                <div>
                  <h4
                    class="text-mono"
                    style={{
                      'font-size': '11px',
                      'text-transform': 'uppercase',
                      'letter-spacing': '0.05em',
                      color: 'var(--color-text-muted)',
                      margin: '0 0 8px 0',
                    }}
                  >
                    Interactions ({results()!.interactions.length})
                  </h4>
                  <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
                    <For each={results()!.interactions}>
                      {(result) => (
                        <A
                          href={`/session/${result.interaction.session_id}?interaction=${result.interaction.id}`}
                          onClick={props.onClose}
                          style={{
                            display: 'block',
                            padding: '10px 12px',
                            background: 'var(--color-bg-base)',
                            'border-radius': '8px',
                            'text-decoration': 'none',
                            'border-left': '3px solid var(--color-accent)',
                          }}
                        >
                          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', 'margin-bottom': '4px' }}>
                            <span
                              class="text-mono"
                              style={{ 'font-size': '11px', color: 'var(--color-text-muted)' }}
                            >
                              #{result.interaction.sequence_number}
                            </span>
                            <Badge variant="completed">{result.matched_field}</Badge>
                            <span
                              class="text-mono"
                              style={{ 'font-size': '10px', color: 'var(--color-text-muted)', 'margin-left': 'auto' }}
                            >
                              {formatRelativeTime(result.interaction.started_at)}
                            </span>
                          </div>
                          <p
                            class="text-mono"
                            style={{
                              'font-size': '12px',
                              color: 'var(--color-text-secondary)',
                              margin: '0',
                              overflow: 'hidden',
                              display: '-webkit-box',
                              '-webkit-line-clamp': '2',
                              '-webkit-box-orient': 'vertical',
                            }}
                          >
                            {result.interaction.user_prompt}
                          </p>
                        </A>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              {/* Tool invocation results */}
              <Show when={results()!.tool_invocations.length > 0}>
                <div>
                  <h4
                    class="text-mono"
                    style={{
                      'font-size': '11px',
                      'text-transform': 'uppercase',
                      'letter-spacing': '0.05em',
                      color: 'var(--color-text-muted)',
                      margin: '0 0 8px 0',
                    }}
                  >
                    Tool Invocations ({results()!.tool_invocations.length})
                  </h4>
                  <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
                    <For each={results()!.tool_invocations}>
                      {(tool) => (
                        <div
                          style={{
                            padding: '8px 12px',
                            background: 'var(--color-bg-base)',
                            'border-radius': '6px',
                            'border-left': `2px solid ${tool.is_error ? '#c45b37' : 'var(--color-text-muted)'}`,
                          }}
                        >
                          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
                            <span
                              class="text-mono"
                              style={{
                                'font-size': '12px',
                                'font-weight': '500',
                                color: 'var(--color-text-primary)',
                              }}
                            >
                              {tool.tool_name}
                            </span>
                            <Show when={tool.is_error}>
                              <Badge variant="error">Error</Badge>
                            </Show>
                            <span
                              class="text-mono"
                              style={{ 'font-size': '10px', color: 'var(--color-text-muted)', 'margin-left': 'auto' }}
                            >
                              {formatRelativeTime(tool.created_at)}
                            </span>
                          </div>
                          <Show when={tool.file_path}>
                            <p
                              class="text-mono"
                              style={{
                                'font-size': '11px',
                                color: 'var(--color-text-secondary)',
                                margin: '4px 0 0 0',
                                'word-break': 'break-all',
                              }}
                            >
                              {tool.file_path}
                            </p>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              {/* File match results */}
              <Show when={results()!.file_matches.length > 0}>
                <div>
                  <h4
                    class="text-mono"
                    style={{
                      'font-size': '11px',
                      'text-transform': 'uppercase',
                      'letter-spacing': '0.05em',
                      color: 'var(--color-text-muted)',
                      margin: '0 0 8px 0',
                    }}
                  >
                    Files ({results()!.file_matches.length})
                  </h4>
                  <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
                    <For each={results()!.file_matches}>
                      {(match) => (
                        <div
                          style={{
                            display: 'flex',
                            'align-items': 'center',
                            gap: '8px',
                            padding: '8px 12px',
                            background: 'var(--color-bg-base)',
                            'border-radius': '6px',
                          }}
                        >
                          <span
                            class="text-mono"
                            style={{
                              'font-size': '10px',
                              'font-weight': '600',
                              padding: '2px 4px',
                              'border-radius': '3px',
                              background:
                                match.change_type === 'created'
                                  ? 'rgba(44, 143, 122, 0.2)'
                                  : match.change_type === 'deleted'
                                  ? 'rgba(196, 91, 55, 0.2)'
                                  : 'rgba(212, 166, 68, 0.2)',
                              color:
                                match.change_type === 'created'
                                  ? '#2c8f7a'
                                  : match.change_type === 'deleted'
                                  ? '#c45b37'
                                  : '#d4a644',
                            }}
                          >
                            {match.change_type === 'created' ? 'A' : match.change_type === 'deleted' ? 'D' : 'M'}
                          </span>
                          <span
                            class="text-mono"
                            style={{
                              'font-size': '12px',
                              color: 'var(--color-text-secondary)',
                              flex: '1',
                              overflow: 'hidden',
                              'text-overflow': 'ellipsis',
                              'white-space': 'nowrap',
                            }}
                          >
                            {match.file_path}
                          </span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </Show>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
