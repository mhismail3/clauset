import { Show, For, createSignal, createEffect, createMemo } from 'solid-js';
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
  const [projectInput, setProjectInput] = createSignal('');
  const [showDropdown, setShowDropdown] = createSignal(false);
  const [selectedModel, setSelectedModel] = createSignal('haiku');
  const [prompt, setPrompt] = createSignal('');
  const [chatMode, setChatMode] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Find matching project by path or name
  const matchedProject = createMemo(() => {
    const input = projectInput().toLowerCase();
    return projects().find(
      p => p.path === projectInput() || p.name.toLowerCase() === input
    );
  });

  // Filter projects for dropdown
  const filteredProjects = createMemo(() => {
    const input = projectInput().toLowerCase();
    if (!input) return projects();
    return projects().filter(p => p.name.toLowerCase().includes(input));
  });

  // Check if creating new project
  const isCreatingNew = createMemo(() => {
    const input = projectInput().trim();
    if (!input) return false;
    return !matchedProject();
  });

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
      if (response.projects.length > 0 && !projectInput()) {
        setProjectInput(response.projects[0].name);
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
    const input = projectInput().trim();
    if (!input) {
      setError('Please enter a project name');
      return;
    }

    setLoading(true);
    setError(null);

    try {
      let projectPath: string;

      if (isCreatingNew()) {
        // Create new project first
        const newProject = await api.projects.create({ name: input });
        projectPath = newProject.path;
        // Add to list so it's available next time
        setProjects([...projects(), newProject]);
      } else {
        // Use existing project path
        projectPath = matchedProject()!.path;
      }

      const response = await api.sessions.create({
        project_path: projectPath,
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
          class="bg-bg-surface animate-slide-up overflow-hidden"
          style={{
            width: "min(400px, calc(100vw - 32px))",
            "max-height": "calc(100vh - 32px)",
            "max-height": "calc(100dvh - 32px)",
            "border-radius": "16px",
            "box-shadow": "0 8px 32px rgba(0, 0, 0, 0.5)",
          }}
        >
          {/* Header */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "20px 20px 16px",
            }}
          >
            <h2 class="text-text-primary" style={{ "font-size": "18px", "font-weight": "600", margin: "0" }}>
              New Session
            </h2>
            <button
              onClick={props.onClose}
              class="text-text-muted hover:text-text-primary transition-colors pressable"
              style={{
                width: "28px",
                height: "28px",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "border-radius": "8px",
                border: "none",
                background: "transparent",
                cursor: "pointer",
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Form */}
          <form
            onSubmit={handleSubmit}
            class="scrollable"
            style={{
              padding: "0 20px 20px",
              "max-height": "calc(100vh - 100px)",
              "max-height": "calc(100dvh - 100px)",
            }}
          >
            <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
              <Show when={error()}>
                <div
                  class="text-status-error"
                  style={{
                    padding: "10px 12px",
                    "border-radius": "8px",
                    background: "rgba(196, 91, 55, 0.1)",
                    "font-size": "13px",
                  }}
                >
                  {error()}
                </div>
              </Show>

              {/* Project Selection */}
              <div style={{ position: "relative" }}>
                <label
                  class="text-label"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                  }}
                >
                  Project
                </label>
                <Show
                  when={!projectsLoading()}
                  fallback={
                    <div
                      class="text-text-muted"
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "8px",
                        padding: "10px 12px",
                        "border-radius": "8px",
                        background: "var(--color-bg-base)",
                        "font-size": "14px",
                      }}
                    >
                      <Spinner size="sm" />
                      <span>Loading projects...</span>
                    </div>
                  }
                >
                  <div style={{ position: "relative" }}>
                    <input
                      type="text"
                      value={projectInput()}
                      onInput={(e) => setProjectInput(e.currentTarget.value)}
                      onFocus={() => setShowDropdown(true)}
                      onBlur={() => setTimeout(() => setShowDropdown(false), 150)}
                      placeholder="Select or create a project..."
                      class="text-text-primary placeholder:text-text-muted"
                      style={{
                        width: "100%",
                        "box-sizing": "border-box",
                        padding: "10px 12px",
                        "font-size": "15px",
                        "border-radius": "8px",
                        border: "none",
                        background: "var(--color-bg-base)",
                        outline: "none",
                      }}
                    />
                    {/* Dropdown */}
                    <Show when={showDropdown() && filteredProjects().length > 0}>
                      <div
                        class="bg-bg-surface"
                        style={{
                          position: "absolute",
                          top: "100%",
                          left: "0",
                          right: "0",
                          "margin-top": "4px",
                          "border-radius": "8px",
                          "max-height": "160px",
                          "overflow-y": "auto",
                          "box-shadow": "0 4px 16px rgba(0, 0, 0, 0.3)",
                          "z-index": "100",
                        }}
                      >
                        <For each={filteredProjects()}>
                          {(project) => (
                            <div
                              class="text-text-primary hover:bg-bg-base"
                              style={{
                                padding: "8px 12px",
                                cursor: "pointer",
                                "font-size": "14px",
                              }}
                              onMouseDown={(e) => {
                                e.preventDefault();
                                setProjectInput(project.name);
                                setShowDropdown(false);
                              }}
                            >
                              {project.name}
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                  {/* Create new indicator */}
                  <Show when={isCreatingNew() && projectInput().trim()}>
                    <div
                      class="text-accent"
                      style={{
                        "font-size": "12px",
                        "margin-top": "6px",
                        display: "flex",
                        "align-items": "center",
                        gap: "4px",
                      }}
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M12 5v14M5 12h14" />
                      </svg>
                      <span>Will create new project: <strong>{projectInput().trim()}</strong></span>
                    </div>
                  </Show>
                </Show>
              </div>

              {/* Model Selection */}
              <div>
                <label
                  class="text-label"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
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
                    padding: "10px 12px",
                    "font-size": "15px",
                    "border-radius": "8px",
                    border: "none",
                    background: "var(--color-bg-base)",
                    outline: "none",
                    cursor: "pointer",
                    appearance: "none",
                    "-webkit-appearance": "none",
                    "background-image": `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='%235c5855' stroke-width='2'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E")`,
                    "background-repeat": "no-repeat",
                    "background-position": "right 10px center",
                    "padding-right": "32px",
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
                  class="text-label"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                  }}
                >
                  Initial Prompt{' '}
                  <span style={{ opacity: "0.6" }}>(optional)</span>
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
                    padding: "10px 12px",
                    "font-size": "15px",
                    "border-radius": "8px",
                    border: "none",
                    background: "var(--color-bg-base)",
                    outline: "none",
                    resize: "none",
                    "font-family": "inherit",
                  }}
                />
              </div>

              {/* Chat Mode Toggle */}
              <label
                style={{
                  display: "flex",
                  "align-items": "flex-start",
                  gap: "10px",
                  padding: "12px",
                  "border-radius": "8px",
                  background: "var(--color-bg-base)",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={chatMode()}
                  onChange={(e) => setChatMode(e.currentTarget.checked)}
                  style={{
                    width: "18px",
                    height: "18px",
                    "margin-top": "1px",
                    "accent-color": "var(--color-accent)",
                    cursor: "pointer",
                    "flex-shrink": "0",
                  }}
                />
                <div style={{ flex: "1", "min-width": "0" }}>
                  <span
                    class="text-text-primary"
                    style={{ display: "block", "font-size": "13px", "font-weight": "500" }}
                  >
                    Chat Mode
                  </span>
                  <span
                    class="text-text-tertiary"
                    style={{ display: "block", "font-size": "11px", "margin-top": "2px", "line-height": "1.4" }}
                  >
                    Uses Claude API (per token). Uncheck for Terminal Mode (Max subscription).
                  </span>
                </div>
              </label>

              {/* Actions */}
              <div
                class="safe-bottom"
                style={{ display: "flex", gap: "10px", "padding-top": "4px" }}
              >
                <Button
                  type="button"
                  variant="ghost"
                  style={{ flex: "1" }}
                  onClick={props.onClose}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  style={{ flex: "1" }}
                  disabled={loading() || !projectInput().trim()}
                >
                  {loading() ? 'Creating...' : 'Create'}
                </Button>
              </div>
            </div>
          </form>
        </div>
      </div>
    </Show>
  );
}
