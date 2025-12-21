// WebSocket connection manager

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface WsMessage {
  type: string;
  [key: string]: unknown;
}

export interface WebSocketManagerOptions {
  url: string;
  onMessage?: (data: WsMessage) => void;
  onStateChange?: (state: ConnectionState) => void;
  reconnectAttempts?: number;
  reconnectDelay?: number;
}

export function createWebSocketManager(options: WebSocketManagerOptions) {
  let ws: WebSocket | null = null;
  let reconnectCount = 0;
  let reconnectTimer: number | null = null;
  let state: ConnectionState = 'disconnected';

  const maxReconnectAttempts = options.reconnectAttempts ?? 10;
  const baseReconnectDelay = options.reconnectDelay ?? 1000;

  function setState(newState: ConnectionState) {
    state = newState;
    options.onStateChange?.(newState);
  }

  function connect() {
    if (ws?.readyState === WebSocket.OPEN) return;

    setState('connecting');

    try {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = options.url.startsWith('ws') ? options.url : `${protocol}//${window.location.host}${options.url}`;
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setState('connected');
        reconnectCount = 0;
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          options.onMessage?.(data);
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      ws.onclose = (event) => {
        ws = null;
        if (!event.wasClean) {
          scheduleReconnect();
        } else {
          setState('disconnected');
        }
      };

      ws.onerror = () => {
        console.error('WebSocket error');
      };
    } catch (e) {
      console.error('WebSocket connection failed:', e);
      scheduleReconnect();
    }
  }

  function scheduleReconnect() {
    if (reconnectCount >= maxReconnectAttempts) {
      setState('disconnected');
      return;
    }

    setState('reconnecting');
    reconnectCount++;

    // Exponential backoff with jitter
    const delay = Math.min(
      baseReconnectDelay * Math.pow(2, reconnectCount - 1) + Math.random() * 1000,
      30000
    );

    reconnectTimer = window.setTimeout(connect, delay);
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    ws?.close(1000, 'Client disconnect');
    ws = null;
    setState('disconnected');
  }

  function send(data: unknown): boolean {
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(data));
      return true;
    }
    return false;
  }

  function getState(): ConnectionState {
    return state;
  }

  return {
    connect,
    disconnect,
    send,
    getState,
  };
}
