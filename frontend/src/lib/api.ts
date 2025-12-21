// API client for Clauset server

export interface RecentAction {
  action_type: string;
  summary: string;
  detail?: string;
  timestamp: number;
}

export interface Session {
  id: string;
  claude_session_id: string;
  project_path: string;
  model: string;
  status: 'created' | 'starting' | 'active' | 'waiting_input' | 'stopped' | 'error';
  mode: 'stream_json' | 'terminal';
  created_at: string;
  last_activity_at: string;
  total_cost_usd: number;
  input_tokens: number;
  output_tokens: number;
  context_percent: number;
  preview: string;
  current_step?: string;
  recent_actions: RecentAction[];
}

export interface SessionListResponse {
  sessions: Session[];
  active_count: number;
}

export interface CreateSessionRequest {
  project_path: string;
  prompt?: string;
  model?: string;
  terminal_mode?: boolean;
}

export interface Project {
  name: string;
  path: string;
}

export interface ProjectsResponse {
  projects: Project[];
  projects_root: string;
}

export interface CreateSessionResponse {
  session_id: string;
  claude_session_id: string;
  ws_url: string;
}

const BASE_URL = '/api';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${url}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.text();
    throw new Error(error || `HTTP ${response.status}`);
  }

  return response.json();
}

export const api = {
  sessions: {
    list: () => fetchJSON<SessionListResponse>('/sessions'),

    get: (id: string) => fetchJSON<Session>(`/sessions/${id}`),

    create: (req: CreateSessionRequest) =>
      fetchJSON<CreateSessionResponse>('/sessions', {
        method: 'POST',
        body: JSON.stringify(req),
      }),

    start: (id: string, prompt: string) =>
      fetch(`${BASE_URL}/sessions/${id}/start`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt }),
      }),

    resume: (id: string) =>
      fetch(`${BASE_URL}/sessions/${id}/resume`, { method: 'POST' }),

    terminate: (id: string) =>
      fetch(`${BASE_URL}/sessions/${id}`, { method: 'DELETE' }),

    delete: (id: string) =>
      fetch(`${BASE_URL}/sessions/${id}/delete`, { method: 'DELETE' }),

    rename: (id: string, name: string) =>
      fetch(`${BASE_URL}/sessions/${id}/name`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      }),

    sendInput: (id: string, content: string) =>
      fetch(`${BASE_URL}/sessions/${id}/input`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content }),
      }),
  },

  history: {
    list: (limit?: number) =>
      fetchJSON<{ entries: Array<{ display: string; timestamp: number; project: string }> }>(
        `/history${limit ? `?limit=${limit}` : ''}`
      ),
  },

  projects: {
    list: () => fetchJSON<ProjectsResponse>('/projects'),
  },
};
