import { Show, Switch, Match, createMemo } from 'solid-js';
import type { ConnectionState } from '../../lib/ws';
import { Spinner } from './Spinner';

export interface ConnectionStatusProps {
  state: ConnectionState;
  reconnectAttempt?: number;
  maxReconnectAttempts?: number;
  queuedMessageCount?: number;
  onRetry?: () => void;
}

export function ConnectionStatus(props: ConnectionStatusProps) {
  const showBanner = createMemo(() => {
    // Don't show banner for normal states or if state is undefined
    if (!props.state) return false;
    return props.state !== 'initial' && props.state !== 'connected';
  });

  const bannerStyle = createMemo(() => {
    const base = {
      display: 'flex',
      'align-items': 'center',
      'justify-content': 'center',
      gap: '8px',
      padding: '16px 16px',
      'font-family': 'var(--font-mono)',
      'font-size': '12px',
      'font-weight': '500',
      'text-align': 'center' as const,
      'backdrop-filter': 'blur(12px)',
      '-webkit-backdrop-filter': 'blur(12px)',
    };

    switch (props.state) {
      case 'connecting':
      case 'reconnecting':
      case 'backoff':
        return {
          ...base,
          background: 'rgba(212, 166, 68, 0.25)',
          color: '#d4a644',
          'border-bottom': '1px solid rgba(212, 166, 68, 0.4)',
        };
      case 'stale':
        return {
          ...base,
          background: 'rgba(212, 166, 68, 0.3)',
          color: '#d4a644',
          'border-bottom': '1px solid rgba(212, 166, 68, 0.5)',
        };
      case 'failed':
        return {
          ...base,
          background: 'rgba(196, 91, 55, 0.25)',
          color: '#c45b37',
          'border-bottom': '1px solid rgba(196, 91, 55, 0.4)',
        };
      case 'suspended':
        return {
          ...base,
          background: 'rgba(92, 88, 85, 0.25)',
          color: '#9a9590',
          'border-bottom': '1px solid rgba(92, 88, 85, 0.4)',
        };
      default:
        return base;
    }
  });

  return (
    <Show when={showBanner()}>
      <div
        class="fixed top-0 left-0 right-0 z-50 safe-top"
        style={bannerStyle()}
      >
        <Switch>
          <Match when={props.state === 'connecting'}>
            <Spinner size="sm" />
            <span>Connecting...</span>
          </Match>

          <Match when={props.state === 'reconnecting'}>
            <Spinner size="sm" />
            <span>
              Reconnecting
              <Show when={props.reconnectAttempt && props.maxReconnectAttempts}>
                {` (${props.reconnectAttempt}/${props.maxReconnectAttempts})`}
              </Show>
              ...
            </span>
          </Match>

          <Match when={props.state === 'backoff'}>
            <Spinner size="sm" />
            <span>
              Waiting to reconnect
              <Show when={props.reconnectAttempt && props.maxReconnectAttempts}>
                {` (${props.reconnectAttempt}/${props.maxReconnectAttempts})`}
              </Show>
            </span>
          </Match>

          <Match when={props.state === 'stale'}>
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
            <span>Connection stale - attempting recovery</span>
          </Match>

          <Match when={props.state === 'failed'}>
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="15" y1="9" x2="9" y2="15" />
              <line x1="9" y1="9" x2="15" y2="15" />
            </svg>
            <span>Connection failed</span>
            <Show when={props.onRetry}>
              <button
                onClick={props.onRetry}
                style={{
                  background: 'rgba(196, 91, 55, 0.3)',
                  border: '1px solid rgba(196, 91, 55, 0.5)',
                  color: 'inherit',
                  padding: '4px 12px',
                  'border-radius': '6px',
                  'font-size': '11px',
                  'font-weight': '600',
                  cursor: 'pointer',
                  'margin-left': '8px',
                }}
              >
                Retry
              </button>
            </Show>
          </Match>

          <Match when={props.state === 'suspended'}>
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect x="6" y="4" width="4" height="16" />
              <rect x="14" y="4" width="4" height="16" />
            </svg>
            <span>Paused (app in background)</span>
          </Match>
        </Switch>

        {/* Queue indicator badge */}
        <Show when={props.queuedMessageCount && props.queuedMessageCount > 0}>
          <span
            style={{
              background: 'rgba(255, 255, 255, 0.2)',
              padding: '2px 8px',
              'border-radius': '10px',
              'font-size': '10px',
              'margin-left': '8px',
            }}
          >
            {props.queuedMessageCount} queued
          </span>
        </Show>
      </div>
    </Show>
  );
}
