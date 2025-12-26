// Global WebSocket manager for dashboard real-time updates
// This WebSocket stays connected across navigation and updates the session store

import { createSignal } from 'solid-js';
import { updateSessionFromActivity, updateSessionStatus } from '../stores/sessions';
import { addNewPrompt } from '../stores/prompts';
import type { Session, PromptSummary } from './api';

export type GlobalWsState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

const [globalWsState, setGlobalWsState] = createSignal<GlobalWsState>('disconnected');

let ws: WebSocket | null = null;
let reconnectCount = 0;
let reconnectTimer: number | null = null;
let isIntentionalDisconnect = false;

const MAX_RECONNECT_ATTEMPTS = 20;
const BASE_RECONNECT_DELAY = 1000;

export interface RecentAction {
  action_type: string;
  summary: string;
  detail?: string;
  timestamp: number;
}

export interface ActivityUpdate {
  session_id: string;
  model: string;
  cost: number;
  input_tokens: number;
  output_tokens: number;
  context_percent: number;
  current_activity: string;
  current_step?: string;
  recent_actions: RecentAction[];
}

function handleMessage(event: MessageEvent) {
  try {
    const data = JSON.parse(event.data);

    switch (data.type) {
      case 'activity_update': {
        const update: ActivityUpdate = {
          session_id: data.session_id,
          model: data.model,
          cost: data.cost,
          input_tokens: data.input_tokens,
          output_tokens: data.output_tokens,
          context_percent: data.context_percent,
          current_activity: data.current_activity,
          current_step: data.current_step,
          recent_actions: data.recent_actions || [],
        };
        updateSessionFromActivity(update);
        break;
      }
      case 'status_change': {
        // Map backend status strings to frontend status type
        const statusMap: Record<string, Session['status']> = {
          'Created': 'created',
          'Starting': 'starting',
          'Active': 'active',
          'WaitingInput': 'waiting_input',
          'Stopped': 'stopped',
          'Error': 'error',
        };
        const newStatus = statusMap[data.new_status] || 'stopped';
        if (data.session_id) {
          updateSessionStatus(data.session_id, newStatus);
        }
        break;
      }
      case 'error': {
        console.error('Global WS error:', data.message);
        break;
      }
      case 'new_prompt': {
        // Real-time prompt indexing update
        const prompt: PromptSummary = {
          id: data.prompt.id,
          preview: data.prompt.preview,
          project_name: data.prompt.project_name,
          timestamp: data.prompt.timestamp,
          word_count: data.prompt.word_count,
        };
        addNewPrompt(prompt);
        break;
      }
    }
  } catch (e) {
    console.error('Failed to parse global WebSocket message:', e);
  }
}

function scheduleReconnect() {
  if (isIntentionalDisconnect) return;
  if (reconnectCount >= MAX_RECONNECT_ATTEMPTS) {
    setGlobalWsState('disconnected');
    return;
  }

  setGlobalWsState('reconnecting');
  reconnectCount++;

  // Exponential backoff with jitter
  const delay = Math.min(
    BASE_RECONNECT_DELAY * Math.pow(2, reconnectCount - 1) + Math.random() * 1000,
    30000
  );

  reconnectTimer = window.setTimeout(connectGlobalWs, delay);
}

export function connectGlobalWs() {
  if (ws?.readyState === WebSocket.OPEN) return;

  isIntentionalDisconnect = false;
  setGlobalWsState('connecting');

  try {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/ws/events`;
    ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      setGlobalWsState('connected');
      reconnectCount = 0;
      console.log('Global WebSocket connected');
    };

    ws.onmessage = handleMessage;

    ws.onclose = (event) => {
      ws = null;
      if (!event.wasClean && !isIntentionalDisconnect) {
        scheduleReconnect();
      } else {
        setGlobalWsState('disconnected');
      }
    };

    ws.onerror = () => {
      console.error('Global WebSocket error');
    };
  } catch (e) {
    console.error('Global WebSocket connection failed:', e);
    scheduleReconnect();
  }
}

export function disconnectGlobalWs() {
  isIntentionalDisconnect = true;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  ws?.close(1000, 'Client disconnect');
  ws = null;
  setGlobalWsState('disconnected');
}

export function getGlobalWsState(): GlobalWsState {
  return globalWsState();
}

export { globalWsState };
