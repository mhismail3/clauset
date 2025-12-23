import { Show, For, createSignal, createEffect, onMount } from 'solid-js';
import { InteractionCard } from './InteractionCard';
import { Spinner } from '../ui/Spinner';
import {
  fetchInteractions,
  fetchInteractionDetail,
  getInteractions,
  getTotalCount,
  getInteractionDetail,
  loading,
  error,
  formatCost,
  formatTokens,
} from '../../stores/interactions';
import type { InteractionSummary } from '../../lib/api';

interface TimelineViewProps {
  sessionId: string;
  onViewDiff?: (interactionId: string, file: string) => void;
}

export function TimelineView(props: TimelineViewProps) {
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [loadingDetail, setLoadingDetail] = createSignal<string | null>(null);

  onMount(() => {
    fetchInteractions(props.sessionId);
  });

  // Refetch when session changes
  createEffect(() => {
    fetchInteractions(props.sessionId);
    setExpandedId(null);
  });

  const handleToggle = async (interaction: InteractionSummary) => {
    const currentExpanded = expandedId();
    if (currentExpanded === interaction.id) {
      setExpandedId(null);
      return;
    }

    setExpandedId(interaction.id);

    // Fetch full details if not already loaded
    if (!getInteractionDetail(interaction.id)) {
      setLoadingDetail(interaction.id);
      await fetchInteractionDetail(interaction.id);
      setLoadingDetail(null);
    }
  };

  const handleViewDiff = (interactionId: string) => (file: string) => {
    props.onViewDiff?.(interactionId, file);
  };

  const interactions = () => getInteractions(props.sessionId);
  const totalCount = () => getTotalCount(props.sessionId);

  // Compute session totals
  const sessionTotals = () => {
    const list = interactions();
    return {
      cost: list.reduce((sum, i) => sum + i.cost_delta_usd, 0),
      inputTokens: list.reduce((sum, i) => sum + i.input_tokens_delta, 0),
      outputTokens: list.reduce((sum, i) => sum + i.output_tokens_delta, 0),
      toolCalls: list.reduce((sum, i) => sum + i.tool_count, 0),
      filesChanged: new Set(list.flatMap(i => i.files_changed)).size,
    };
  };

  return (
    <div style={{ display: 'flex', 'flex-direction': 'column', height: '100%' }}>
      {/* Header with session stats */}
      <div
        style={{
          padding: '12px 16px',
          'border-bottom': '1px solid var(--color-bg-overlay)',
          background: 'var(--color-bg-elevated)',
        }}
      >
        <div style={{ display: 'flex', 'align-items': 'center', 'justify-content': 'space-between' }}>
          <h3
            class="text-mono"
            style={{
              'font-size': '13px',
              'font-weight': '600',
              color: 'var(--color-text-primary)',
              margin: '0',
            }}
          >
            Interaction Timeline
          </h3>
          <span
            class="text-mono"
            style={{ 'font-size': '11px', color: 'var(--color-text-muted)' }}
          >
            {totalCount()} interactions
          </span>
        </div>

        {/* Session totals */}
        <Show when={interactions().length > 0}>
          <div
            class="text-mono"
            style={{
              display: 'flex',
              'align-items': 'center',
              gap: '12px',
              'font-size': '11px',
              color: 'var(--color-text-muted)',
              'margin-top': '8px',
              'flex-wrap': 'wrap',
            }}
          >
            <span>Total: {formatCost(sessionTotals().cost)}</span>
            <span>|</span>
            <span>
              {formatTokens(sessionTotals().inputTokens)}/{formatTokens(sessionTotals().outputTokens)} tokens
            </span>
            <span>|</span>
            <span>{sessionTotals().toolCalls} tool calls</span>
            <Show when={sessionTotals().filesChanged > 0}>
              <span>|</span>
              <span style={{ color: '#2c8f7a' }}>{sessionTotals().filesChanged} files changed</span>
            </Show>
          </div>
        </Show>
      </div>

      {/* Timeline content */}
      <div
        style={{
          flex: '1',
          overflow: 'auto',
          padding: '16px',
        }}
      >
        <Show when={loading()}>
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              padding: '32px',
            }}
          >
            <Spinner />
          </div>
        </Show>

        <Show when={error()}>
          <div
            style={{
              padding: '16px',
              background: 'rgba(196, 91, 55, 0.1)',
              'border-radius': '8px',
              'border-left': '3px solid #c45b37',
            }}
          >
            <p
              class="text-mono"
              style={{ 'font-size': '13px', color: '#c45b37', margin: '0' }}
            >
              {error()}
            </p>
          </div>
        </Show>

        <Show when={!loading() && !error() && interactions().length === 0}>
          <div
            style={{
              display: 'flex',
              'flex-direction': 'column',
              'align-items': 'center',
              'justify-content': 'center',
              padding: '48px 16px',
              'text-align': 'center',
            }}
          >
            <svg
              width="48"
              height="48"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              style={{ color: 'var(--color-text-muted)', 'margin-bottom': '16px' }}
            >
              <circle cx="12" cy="12" r="10" />
              <polyline points="12 6 12 12 16 14" />
            </svg>
            <p
              class="text-mono"
              style={{
                'font-size': '14px',
                color: 'var(--color-text-secondary)',
                margin: '0 0 8px 0',
              }}
            >
              No interactions yet
            </p>
            <p
              class="text-mono"
              style={{
                'font-size': '12px',
                color: 'var(--color-text-muted)',
                margin: '0',
              }}
            >
              Interactions will appear here as you work with Claude
            </p>
          </div>
        </Show>

        <Show when={!loading() && !error() && interactions().length > 0}>
          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '12px' }}>
            <For each={interactions()}>
              {(interaction) => {
                const detail = () => getInteractionDetail(interaction.id);
                const isExpanded = () => expandedId() === interaction.id;
                const isLoading = () => loadingDetail() === interaction.id;

                return (
                  <div style={{ position: 'relative' }}>
                    <InteractionCard
                      interaction={interaction}
                      isExpanded={isExpanded()}
                      onToggle={() => handleToggle(interaction)}
                      onViewDiff={handleViewDiff(interaction.id)}
                      toolInvocations={detail()?.tool_invocations}
                      fileChanges={detail()?.file_changes}
                    />
                    <Show when={isLoading()}>
                      <div
                        style={{
                          position: 'absolute',
                          inset: '0',
                          background: 'rgba(0, 0, 0, 0.3)',
                          display: 'flex',
                          'align-items': 'center',
                          'justify-content': 'center',
                          'border-radius': '8px',
                        }}
                      >
                        <Spinner />
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}
