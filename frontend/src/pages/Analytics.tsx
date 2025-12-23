import { Show, For, createSignal, onMount } from 'solid-js';
import { A } from '@solidjs/router';
import { Spinner } from '../components/ui/Spinner';
import {
  fetchAnalytics,
  analytics,
  analyticsLoading,
  formatCost,
  formatTokens,
  formatBytes,
} from '../stores/interactions';
import { formatRelativeTime } from '../stores/sessions';
import { api } from '../lib/api';
import type { StorageStats } from '../lib/api';

function StatCard(props: { label: string; value: string; subValue?: string; color?: string }) {
  return (
    <div
      style={{
        padding: '16px',
        background: 'var(--color-bg-elevated)',
        'border-radius': '8px',
        border: '1px solid var(--color-bg-overlay)',
      }}
    >
      <p
        class="text-mono"
        style={{
          'font-size': '11px',
          'text-transform': 'uppercase',
          'letter-spacing': '0.05em',
          color: 'var(--color-text-muted)',
          margin: '0 0 8px 0',
        }}
      >
        {props.label}
      </p>
      <p
        class="text-mono"
        style={{
          'font-size': '24px',
          'font-weight': '600',
          color: props.color || 'var(--color-text-primary)',
          margin: '0',
        }}
      >
        {props.value}
      </p>
      <Show when={props.subValue}>
        <p
          class="text-mono"
          style={{
            'font-size': '11px',
            color: 'var(--color-text-muted)',
            margin: '4px 0 0 0',
          }}
        >
          {props.subValue}
        </p>
      </Show>
    </div>
  );
}

function CostChart(props: { data: { date: string; total_cost_usd: number }[] }) {
  const maxCost = () => Math.max(...props.data.map((d) => d.total_cost_usd), 0.01);

  return (
    <div
      style={{
        padding: '16px',
        background: 'var(--color-bg-elevated)',
        'border-radius': '8px',
        border: '1px solid var(--color-bg-overlay)',
      }}
    >
      <h3
        class="text-mono"
        style={{
          'font-size': '13px',
          'font-weight': '600',
          color: 'var(--color-text-primary)',
          margin: '0 0 16px 0',
        }}
      >
        Daily Cost (Last 30 Days)
      </h3>

      <Show when={props.data.length === 0}>
        <p
          class="text-mono"
          style={{
            'font-size': '12px',
            color: 'var(--color-text-muted)',
            'text-align': 'center',
            padding: '24px 0',
          }}
        >
          No cost data available
        </p>
      </Show>

      <Show when={props.data.length > 0}>
        <div
          style={{
            display: 'flex',
            'align-items': 'flex-end',
            gap: '2px',
            height: '120px',
            padding: '0 4px',
          }}
        >
          <For each={props.data.slice(-30)}>
            {(entry) => {
              const height = () => Math.max((entry.total_cost_usd / maxCost()) * 100, 2);
              return (
                <div
                  style={{
                    flex: '1',
                    'min-width': '0',
                    height: `${height()}%`,
                    background: '#c45b37',
                    'border-radius': '2px 2px 0 0',
                    cursor: 'pointer',
                    transition: 'opacity 0.15s',
                  }}
                  title={`${entry.date}: ${formatCost(entry.total_cost_usd)}`}
                  onMouseEnter={(e) => (e.currentTarget.style.opacity = '0.7')}
                  onMouseLeave={(e) => (e.currentTarget.style.opacity = '1')}
                />
              );
            }}
          </For>
        </div>

        {/* X-axis labels */}
        <div
          class="text-mono"
          style={{
            display: 'flex',
            'justify-content': 'space-between',
            'margin-top': '8px',
            'font-size': '10px',
            color: 'var(--color-text-muted)',
          }}
        >
          <span>{props.data[0]?.date.slice(5) || ''}</span>
          <span>{props.data[props.data.length - 1]?.date.slice(5) || ''}</span>
        </div>
      </Show>
    </div>
  );
}

function ToolBreakdown(props: { data: { tool_name: string; invocation_count: number; avg_duration_ms?: number }[] }) {
  const totalInvocations = () => props.data.reduce((sum, t) => sum + t.invocation_count, 0);

  return (
    <div
      style={{
        padding: '16px',
        background: 'var(--color-bg-elevated)',
        'border-radius': '8px',
        border: '1px solid var(--color-bg-overlay)',
      }}
    >
      <h3
        class="text-mono"
        style={{
          'font-size': '13px',
          'font-weight': '600',
          color: 'var(--color-text-primary)',
          margin: '0 0 16px 0',
        }}
      >
        Tool Usage
      </h3>

      <Show when={props.data.length === 0}>
        <p
          class="text-mono"
          style={{
            'font-size': '12px',
            color: 'var(--color-text-muted)',
            'text-align': 'center',
            padding: '24px 0',
          }}
        >
          No tool data available
        </p>
      </Show>

      <Show when={props.data.length > 0}>
        <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
          <For each={props.data.slice(0, 10)}>
            {(tool) => {
              const percentage = () => (tool.invocation_count / totalInvocations()) * 100;
              return (
                <div>
                  <div
                    style={{
                      display: 'flex',
                      'align-items': 'center',
                      'justify-content': 'space-between',
                      'margin-bottom': '4px',
                    }}
                  >
                    <span
                      class="text-mono"
                      style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}
                    >
                      {tool.tool_name}
                    </span>
                    <span
                      class="text-mono"
                      style={{ 'font-size': '11px', color: 'var(--color-text-muted)' }}
                    >
                      {tool.invocation_count.toLocaleString()}
                    </span>
                  </div>
                  <div
                    style={{
                      height: '4px',
                      background: 'var(--color-bg-overlay)',
                      'border-radius': '2px',
                      overflow: 'hidden',
                    }}
                  >
                    <div
                      style={{
                        height: '100%',
                        width: `${percentage()}%`,
                        background: '#2c8f7a',
                        'border-radius': '2px',
                      }}
                    />
                  </div>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
}

function SessionList(props: { sessions: { session_id: string; interaction_count: number; total_cost_usd: number }[] }) {
  return (
    <div
      style={{
        padding: '16px',
        background: 'var(--color-bg-elevated)',
        'border-radius': '8px',
        border: '1px solid var(--color-bg-overlay)',
      }}
    >
      <h3
        class="text-mono"
        style={{
          'font-size': '13px',
          'font-weight': '600',
          color: 'var(--color-text-primary)',
          margin: '0 0 16px 0',
        }}
      >
        Sessions by Cost
      </h3>

      <Show when={props.sessions.length === 0}>
        <p
          class="text-mono"
          style={{
            'font-size': '12px',
            color: 'var(--color-text-muted)',
            'text-align': 'center',
            padding: '24px 0',
          }}
        >
          No session data available
        </p>
      </Show>

      <Show when={props.sessions.length > 0}>
        <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
          <For each={props.sessions.slice(0, 10)}>
            {(session) => (
              <A
                href={`/session/${session.session_id}`}
                style={{
                  display: 'flex',
                  'align-items': 'center',
                  'justify-content': 'space-between',
                  padding: '8px 12px',
                  background: 'var(--color-bg-base)',
                  'border-radius': '6px',
                  'text-decoration': 'none',
                }}
              >
                <span
                  class="text-mono"
                  style={{
                    'font-size': '11px',
                    color: 'var(--color-text-secondary)',
                    overflow: 'hidden',
                    'text-overflow': 'ellipsis',
                    'white-space': 'nowrap',
                    flex: '1',
                  }}
                >
                  {session.session_id.slice(0, 8)}...
                </span>
                <span
                  class="text-mono"
                  style={{ 'font-size': '11px', color: 'var(--color-text-muted)', 'margin-left': '8px' }}
                >
                  {session.interaction_count} int
                </span>
                <span
                  class="text-mono"
                  style={{ 'font-size': '11px', color: '#c45b37', 'margin-left': '8px', 'font-weight': '500' }}
                >
                  {formatCost(session.total_cost_usd)}
                </span>
              </A>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export function Analytics() {
  const [days, setDays] = createSignal(30);
  const [storage, setStorage] = createSignal<StorageStats | null>(null);

  onMount(async () => {
    await fetchAnalytics(days());
    try {
      const storageData = await api.analytics.storage();
      setStorage(storageData);
    } catch (e) {
      console.error('Failed to fetch storage stats:', e);
    }
  });

  const handleDaysChange = async (newDays: number) => {
    setDays(newDays);
    await fetchAnalytics(newDays);
  };

  const data = () => analytics();

  return (
    <div
      style={{
        'min-height': '100vh',
        background: 'var(--color-bg-base)',
        padding: 'max(env(safe-area-inset-top), 16px) 16px max(env(safe-area-inset-bottom), 16px)',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          'align-items': 'center',
          'justify-content': 'space-between',
          'margin-bottom': '24px',
          'flex-wrap': 'wrap',
          gap: '12px',
        }}
      >
        <div style={{ display: 'flex', 'align-items': 'center', gap: '12px' }}>
          <A
            href="/"
            class="pressable"
            style={{
              width: '36px',
              height: '36px',
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              background: 'var(--color-bg-elevated)',
              'border-radius': '8px',
              color: 'var(--color-text-muted)',
            }}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M19 12H5M12 19l-7-7 7-7" />
            </svg>
          </A>
          <h1
            class="text-mono"
            style={{
              'font-size': '20px',
              'font-weight': '600',
              color: 'var(--color-text-primary)',
              margin: '0',
            }}
          >
            Analytics
          </h1>
        </div>

        {/* Days selector */}
        <div style={{ display: 'flex', gap: '8px' }}>
          {[7, 30, 90].map((d) => (
            <button
              onClick={() => handleDaysChange(d)}
              class="text-mono"
              style={{
                padding: '6px 12px',
                'border-radius': '6px',
                border: 'none',
                cursor: 'pointer',
                'font-size': '12px',
                background: days() === d ? 'var(--color-accent)' : 'var(--color-bg-elevated)',
                color: days() === d ? 'white' : 'var(--color-text-secondary)',
              }}
            >
              {d}d
            </button>
          ))}
        </div>
      </div>

      <Show when={analyticsLoading()}>
        <div
          style={{
            display: 'flex',
            'align-items': 'center',
            'justify-content': 'center',
            padding: '64px',
          }}
        >
          <Spinner />
        </div>
      </Show>

      <Show when={!analyticsLoading() && data()}>
        {(analyticsData) => (
          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '16px' }}>
            {/* Summary stats */}
            <div
              style={{
                display: 'grid',
                'grid-template-columns': 'repeat(auto-fit, minmax(140px, 1fr))',
                gap: '12px',
              }}
            >
              <StatCard
                label="Total Cost"
                value={formatCost(analyticsData().summary.total_cost_usd)}
                color="#c45b37"
              />
              <StatCard
                label="Sessions"
                value={analyticsData().summary.session_count.toLocaleString()}
              />
              <StatCard
                label="Interactions"
                value={analyticsData().summary.interaction_count.toLocaleString()}
                subValue={`~${formatCost(analyticsData().summary.avg_cost_per_interaction)} avg`}
              />
              <StatCard
                label="Tokens"
                value={formatTokens(
                  analyticsData().summary.total_input_tokens + analyticsData().summary.total_output_tokens
                )}
                subValue={`${formatTokens(analyticsData().summary.total_input_tokens)} in / ${formatTokens(
                  analyticsData().summary.total_output_tokens
                )} out`}
              />
              <StatCard
                label="Tool Calls"
                value={analyticsData().summary.total_tool_invocations.toLocaleString()}
              />
              <StatCard
                label="Files Changed"
                value={analyticsData().summary.total_file_changes.toLocaleString()}
                color="#2c8f7a"
              />
            </div>

            {/* Charts row */}
            <div
              style={{
                display: 'grid',
                'grid-template-columns': 'repeat(auto-fit, minmax(300px, 1fr))',
                gap: '16px',
              }}
            >
              <CostChart data={analyticsData().daily_costs} />
              <ToolBreakdown data={analyticsData().tool_costs} />
            </div>

            {/* Sessions and storage */}
            <div
              style={{
                display: 'grid',
                'grid-template-columns': 'repeat(auto-fit, minmax(280px, 1fr))',
                gap: '16px',
              }}
            >
              <SessionList sessions={analyticsData().session_analytics} />

              {/* Storage stats */}
              <Show when={storage()}>
                {(storageData) => (
                  <div
                    style={{
                      padding: '16px',
                      background: 'var(--color-bg-elevated)',
                      'border-radius': '8px',
                      border: '1px solid var(--color-bg-overlay)',
                    }}
                  >
                    <h3
                      class="text-mono"
                      style={{
                        'font-size': '13px',
                        'font-weight': '600',
                        color: 'var(--color-text-primary)',
                        margin: '0 0 16px 0',
                      }}
                    >
                      Storage
                    </h3>
                    <div style={{ display: 'flex', 'flex-direction': 'column', gap: '12px' }}>
                      <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          Interactions
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>
                          {storageData().interaction_count.toLocaleString()}
                        </span>
                      </div>
                      <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          Tool Invocations
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>
                          {storageData().tool_count.toLocaleString()}
                        </span>
                      </div>
                      <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          File Snapshots
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>
                          {storageData().snapshot_count.toLocaleString()}
                        </span>
                      </div>
                      <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          Content Blobs
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>
                          {storageData().content_count.toLocaleString()}
                        </span>
                      </div>
                      <div
                        style={{
                          'padding-top': '12px',
                          'border-top': '1px solid var(--color-bg-overlay)',
                          display: 'flex',
                          'justify-content': 'space-between',
                        }}
                      >
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          Total Size
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>
                          {formatBytes(storageData().total_content_size)}
                        </span>
                      </div>
                      <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                        <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                          Compressed
                        </span>
                        <span class="text-mono" style={{ 'font-size': '12px', color: '#2c8f7a' }}>
                          {formatBytes(storageData().total_compressed_size)}
                        </span>
                      </div>
                      <Show when={storageData().total_content_size > 0}>
                        <div style={{ display: 'flex', 'justify-content': 'space-between' }}>
                          <span class="text-mono" style={{ 'font-size': '12px', color: 'var(--color-text-muted)' }}>
                            Compression Ratio
                          </span>
                          <span class="text-mono" style={{ 'font-size': '12px', color: '#2c8f7a' }}>
                            {(
                              (1 - storageData().total_compressed_size / storageData().total_content_size) *
                              100
                            ).toFixed(1)}
                            %
                          </span>
                        </div>
                      </Show>
                    </div>
                  </div>
                )}
              </Show>
            </div>
          </div>
        )}
      </Show>

      <Show when={!analyticsLoading() && !data()}>
        <div
          style={{
            display: 'flex',
            'flex-direction': 'column',
            'align-items': 'center',
            'justify-content': 'center',
            padding: '64px 16px',
            'text-align': 'center',
          }}
        >
          <svg
            width="64"
            height="64"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            style={{ color: 'var(--color-text-muted)', 'margin-bottom': '16px' }}
          >
            <path d="M3 3v18h18" />
            <path d="M18.7 8l-5.1 5.2-2.8-2.7L7 14.3" />
          </svg>
          <p
            class="text-mono"
            style={{
              'font-size': '14px',
              color: 'var(--color-text-secondary)',
              margin: '0 0 8px 0',
            }}
          >
            No analytics data available
          </p>
          <p
            class="text-mono"
            style={{
              'font-size': '12px',
              color: 'var(--color-text-muted)',
              margin: '0',
            }}
          >
            Analytics will appear as you use Claude Code
          </p>
        </div>
      </Show>
    </div>
  );
}

export default Analytics;
