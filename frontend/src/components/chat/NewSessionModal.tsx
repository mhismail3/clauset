import { Show, For, createSignal, onMount, createEffect } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button } from '../ui/Button';
import { Spinner } from '../ui/Spinner';
import { api, Project } from '../../lib/api';

interface NewSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function NewSessionModal(props: NewSessionModalProps) {
  const navigate = useNavigate();
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [projectsLoading, setProjectsLoading] = createSignal(false);
  const [selectedProject, setSelectedProject] = createSignal('');
  const [selectedModel, setSelectedModel] = createSignal('haiku');
  const [prompt, setPrompt] = createSignal('');
  const [chatMode, setChatMode] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const models = [
    { value: 'haiku', label: 'Haiku', description: 'Fast and efficient' },
    { value: 'sonnet', label: 'Sonnet', description: 'Balanced performance' },
    { value: 'opus', label: 'Opus', description: 'Most capable' },
  ];

  async function fetchProjects() {
    setProjectsLoading(true);
    try {
      const response = await api.projects.list();
      setProjects(response.projects);
      // Auto-select first project if none selected
      if (response.projects.length > 0 && !selectedProject()) {
        setSelectedProject(response.projects[0].path);
      }
    } catch (e) {
      console.error('Failed to fetch projects:', e);
    } finally {
      setProjectsLoading(false);
    }
  }

  // Fetch projects when modal opens
  createEffect(() => {
    if (props.isOpen) {
      fetchProjects();
    }
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!selectedProject()) {
      setError('Please select a project');
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const response = await api.sessions.create({
        project_path: selectedProject(),
        prompt: prompt() || undefined,
        model: selectedModel(),
        terminal_mode: !chatMode(), // Invert: chatMode=false means terminal_mode=true
      });

      await api.sessions.start(response.session_id, prompt());

      props.onClose();
      navigate(`/session/${response.session_id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create session');
    } finally {
      setLoading(false);
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      props.onClose();
    }
  }

  return (
    <Show when={props.isOpen}>
      {/* Backdrop - uses fixed positioning with explicit dimensions */}
      <div
        class="overlay-backdrop animate-fade-in"
        style={{
          position: "fixed",
          top: "0",
          left: "0",
          right: "0",
          bottom: "0",
          "z-index": "50",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          padding: "16px",
        }}
        onClick={handleBackdropClick}
      >
        {/* Modal - explicit width that doesn't depend on flex */}
        <div
          class="bg-bg-surface rounded-2xl animate-slide-up overflow-hidden"
          style={{
            width: "min(448px, calc(100vw - 32px))",
            "max-height": "calc(100vh - 32px)",
            "max-height": "calc(100dvh - 32px)",
          }}
        >
          {/* Header */}
          <div
            class="border-b border-bg-overlay"
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "16px 20px",
            }}
          >
            <h2 style={{ "font-size": "20px", "font-weight": "600", margin: "0" }}>
              New Session
            </h2>
            <button
              onClick={props.onClose}
              class="bg-bg-elevated text-text-muted hover:text-text-primary transition-colors"
              style={{
                width: "32px",
                height: "32px",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "border-radius": "50%",
                border: "none",
                cursor: "pointer",
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Form */}
          <form
            onSubmit={handleSubmit}
            class="scrollable"
            style={{
              padding: "20px",
              "max-height": "calc(100vh - 100px)",
              "max-height": "calc(100dvh - 100px)",
            }}
          >
            <div style={{ display: "flex", "flex-direction": "column", gap: "20px" }}>
              <Show when={error()}>
                <div
                  class="bg-status-error/10 text-status-error"
                  style={{
                    padding: "12px 16px",
                    "border-radius": "12px",
                    border: "1px solid rgba(255, 69, 58, 0.2)",
                    "font-size": "14px",
                  }}
                >
                  {error()}
                </div>
              </Show>

              {/* Project Selection */}
              <div>
                <label
                  style={{
                    display: "block",
                    "font-size": "14px",
                    "font-weight": "500",
                    "margin-bottom": "8px",
                    color: "var(--color-text-secondary)",
                  }}
                >
                  Project
                </label>
                <Show
                  when={!projectsLoading()}
                  fallback={
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "8px",
                        padding: "12px 16px",
                        "border-radius": "12px",
                        border: "1px solid var(--color-bg-overlay)",
                        background: "var(--color-bg-base)",
                        color: "var(--color-text-muted)",
                      }}
                    >
                      <Spinner size="sm" />
                      <span>Loading projects...</span>
                    </div>
                  }
                >
                  <Show
                    when={projects().length > 0}
                    fallback={
                      <div
                        style={{
                          padding: "12px 16px",
                          "border-radius": "12px",
                          border: "1px solid var(--color-bg-overlay)",
                          background: "var(--color-bg-base)",
                          color: "var(--color-text-muted)",
                          "font-size": "14px",
                        }}
                      >
                        No projects found in ~/Downloads/projects
                      </div>
                    }
                  >
                    <select
                      value={selectedProject()}
                      onChange={(e) => setSelectedProject(e.currentTarget.value)}
                      required
                      class="text-text-primary"
                      style={{
                        width: "100%",
                        "box-sizing": "border-box",
                        padding: "12px 16px",
                        "font-size": "16px",
                        "border-radius": "12px",
                        border: "1px solid var(--color-bg-overlay)",
                        background: "var(--color-bg-base)",
                        outline: "none",
                        cursor: "pointer",
                        appearance: "none",
                        "-webkit-appearance": "none",
                        "background-image": `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='%239a9590' stroke-width='2'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E")`,
                        "background-repeat": "no-repeat",
                        "background-position": "right 12px center",
                        "padding-right": "40px",
                      }}
                    >
                      <For each={projects()}>
                        {(project) => (
                          <option value={project.path}>{project.name}</option>
                        )}
                      </For>
                    </select>
                  </Show>
                </Show>
              </div>

              {/* Model Selection */}
              <div>
                <label
                  style={{
                    display: "block",
                    "font-size": "14px",
                    "font-weight": "500",
                    "margin-bottom": "8px",
                    color: "var(--color-text-secondary)",
                  }}
                >
                  Model
                </label>
                <select
                  value={selectedModel()}
                  onChange={(e) => setSelectedModel(e.currentTarget.value)}
                  class="text-text-primary"
                  style={{
                    width: "100%",
                    "box-sizing": "border-box",
                    padding: "12px 16px",
                    "font-size": "16px",
                    "border-radius": "12px",
                    border: "1px solid var(--color-bg-overlay)",
                    background: "var(--color-bg-base)",
                    outline: "none",
                    cursor: "pointer",
                    appearance: "none",
                    "-webkit-appearance": "none",
                    "background-image": `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='%239a9590' stroke-width='2'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E")`,
                    "background-repeat": "no-repeat",
                    "background-position": "right 12px center",
                    "padding-right": "40px",
                  }}
                >
                  <For each={models}>
                    {(model) => (
                      <option value={model.value}>
                        {model.label} — {model.description}
                      </option>
                    )}
                  </For>
                </select>
              </div>

              {/* Initial Prompt (Optional) */}
              <div>
                <label
                  style={{
                    display: "block",
                    "font-size": "14px",
                    "font-weight": "500",
                    "margin-bottom": "8px",
                    color: "var(--color-text-secondary)",
                  }}
                >
                  Initial Prompt{' '}
                  <span style={{ color: "var(--color-text-muted)", "font-weight": "400" }}>
                    (optional)
                  </span>
                </label>
                <textarea
                  value={prompt()}
                  onInput={(e) => setPrompt(e.currentTarget.value)}
                  placeholder="What would you like Claude to help with?"
                  rows={3}
                  class="text-text-primary placeholder:text-text-muted"
                  style={{
                    width: "100%",
                    "box-sizing": "border-box",
                    padding: "12px 16px",
                    "font-size": "16px",
                    "border-radius": "12px",
                    border: "1px solid var(--color-bg-overlay)",
                    background: "var(--color-bg-base)",
                    outline: "none",
                    resize: "none",
                    "font-family": "inherit",
                  }}
                />
              </div>

              {/* Chat Mode Toggle */}
              <label
                class="bg-bg-base"
                style={{
                  display: "flex",
                  "align-items": "flex-start",
                  gap: "12px",
                  padding: "16px",
                  "border-radius": "12px",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={chatMode()}
                  onChange={(e) => setChatMode(e.currentTarget.checked)}
                  style={{
                    width: "20px",
                    height: "20px",
                    "margin-top": "2px",
                    "accent-color": "var(--color-accent)",
                    cursor: "pointer",
                    "flex-shrink": "0",
                  }}
                />
                <div style={{ flex: "1", "min-width": "0" }}>
                  <span
                    class="text-text-primary"
                    style={{ display: "block", "font-size": "14px", "font-weight": "500" }}
                  >
                    Chat Mode
                  </span>
                  <span
                    class="text-text-muted"
                    style={{ display: "block", "font-size": "12px", "margin-top": "4px", "line-height": "1.4" }}
                  >
                    Uses Claude API (billed per token). Uncheck for Terminal Mode which uses your Claude Max subscription.
                  </span>
                </div>
              </label>

              {/* Actions */}
              <div
                class="safe-bottom"
                style={{ display: "flex", gap: "12px", "padding-top": "8px" }}
              >
                <Button
                  type="button"
                  variant="secondary"
                  style={{ flex: "1" }}
                  onClick={props.onClose}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  style={{ flex: "1" }}
                  disabled={loading() || !selectedProject()}
                >
                  {loading() ? 'Creating...' : 'Create Session'}
                </Button>
              </div>
            </div>
          </form>
        </div>
      </div>
    </Show>
  );
}
