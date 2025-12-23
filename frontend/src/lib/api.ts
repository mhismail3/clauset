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

export interface CreateProjectRequest {
  name: string;
}

export interface CreateSessionResponse {
  session_id: string;
  claude_session_id: string;
  ws_url: string;
}

// Interaction types
export interface Interaction {
  id: string;
  session_id: string;
  sequence_number: number;
  user_prompt: string;
  assistant_summary?: string;
  started_at: string;
  ended_at?: string;
  cost_usd_delta: number;
  input_tokens_delta: number;
  output_tokens_delta: number;
  status: 'active' | 'completed' | 'error';
  error_message?: string;
}

export interface InteractionSummary {
  id: string;
  sequence_number: number;
  user_prompt: string;
  user_prompt_preview: string;
  started_at: string;
  ended_at?: string;
  cost_delta_usd: number;
  input_tokens_delta: number;
  output_tokens_delta: number;
  tool_count: number;
  files_changed: string[];
}

export interface InteractionListResponse {
  interactions: InteractionSummary[];
  total_count: number;
}

export interface ToolInvocation {
  id: string;
  interaction_id: string;
  tool_name: string;
  tool_input?: string;
  tool_output_preview?: string;
  is_error: boolean;
  file_path?: string;
  duration_ms?: number;
  created_at: string;
}

export interface DiffLine {
  change_type: 'add' | 'remove' | 'context';
  old_line_num?: number;
  new_line_num?: number;
  content: string;
}

export interface DiffHunk {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: DiffLine[];
}

export interface FileDiff {
  lines_added: number;
  lines_removed: number;
  hunks: DiffHunk[];
  is_identical: boolean;
  is_binary: boolean;
}

export interface FileChangeWithDiff {
  file_path: string;
  change_type: 'created' | 'modified' | 'deleted';
  diff: FileDiff;
}

export interface InteractionDetailResponse {
  interaction: Interaction;
  tool_invocations: ToolInvocation[];
  file_changes: FileChangeWithDiff[];
}

export interface DiffResponse {
  file_path: string;
  from_interaction: string;
  to_interaction: string;
  diff: FileDiff;
  unified_diff: string;
}

export interface FileChangeSummary {
  file_path: string;
  change_count: number;
  interactions: string[];
}

export interface FilesChangedResponse {
  files: FileChangeSummary[];
}

// Search types
export interface SearchResult {
  interaction: Interaction;
  relevance_score: number;
  matched_field: string;
}

export interface FilePathMatch {
  file_path: string;
  interaction_id: string;
  tool_invocation_id: string;
  change_type: string;
}

export interface GlobalSearchResults {
  interactions: SearchResult[];
  tool_invocations: ToolInvocation[];
  file_matches: FilePathMatch[];
}

// Analytics types
export interface SessionAnalytics {
  session_id: string;
  interaction_count: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  first_interaction_at?: string;
  last_interaction_at?: string;
}

export interface DailyCostEntry {
  date: string;
  interaction_count: number;
  total_cost_usd: number;
  input_tokens: number;
  output_tokens: number;
}

export interface ToolCostEntry {
  tool_name: string;
  invocation_count: number;
  avg_duration_ms?: number;
}

export interface AnalyticsSummary {
  session_count: number;
  interaction_count: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  avg_cost_per_interaction: number;
  total_tool_invocations: number;
  total_file_changes: number;
}

export interface AnalyticsResponse {
  summary: AnalyticsSummary;
  daily_costs: DailyCostEntry[];
  tool_costs: ToolCostEntry[];
  session_analytics: SessionAnalytics[];
}

export interface StorageStats {
  interaction_count: number;
  tool_count: number;
  snapshot_count: number;
  content_count: number;
  total_content_size: number;
  total_compressed_size: number;
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
    create: (req: CreateProjectRequest) =>
      fetchJSON<Project>('/projects', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
  },

  interactions: {
    list: (sessionId: string, limit?: number, offset?: number) => {
      const params = new URLSearchParams();
      if (limit) params.set('limit', limit.toString());
      if (offset) params.set('offset', offset.toString());
      const query = params.toString();
      return fetchJSON<InteractionListResponse>(
        `/sessions/${sessionId}/interactions${query ? `?${query}` : ''}`
      );
    },

    get: (id: string) => fetchJSON<InteractionDetailResponse>(`/interactions/${id}`),

    filesChanged: (sessionId: string) =>
      fetchJSON<FilesChangedResponse>(`/sessions/${sessionId}/files-changed`),
  },

  diff: {
    compute: (fromInteraction: string, toInteraction: string, file: string, context?: number) => {
      const params = new URLSearchParams({
        from: fromInteraction,
        to: toInteraction,
        file,
      });
      if (context) params.set('context', context.toString());
      return fetchJSON<DiffResponse>(`/diff?${params.toString()}`);
    },
  },

  search: {
    query: (q: string, options?: { scope?: string; sessionId?: string; limit?: number; offset?: number }) => {
      const params = new URLSearchParams({ q });
      if (options?.scope) params.set('scope', options.scope);
      if (options?.sessionId) params.set('session_id', options.sessionId);
      if (options?.limit) params.set('limit', options.limit.toString());
      if (options?.offset) params.set('offset', options.offset.toString());
      return fetchJSON<GlobalSearchResults>(`/search?${params.toString()}`);
    },
  },

  analytics: {
    get: (days?: number) => {
      const params = days ? `?days=${days}` : '';
      return fetchJSON<AnalyticsResponse>(`/analytics${params}`);
    },

    expensive: (limit?: number) => {
      const params = limit ? `?limit=${limit}` : '';
      return fetchJSON<Interaction[]>(`/analytics/expensive${params}`);
    },

    storage: () => fetchJSON<StorageStats>('/analytics/storage'),
  },
};
