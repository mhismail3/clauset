import { Show, For, createSignal, createEffect, createMemo, onCleanup } from 'solid-js';
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
  const [showProjectDropdown, setShowProjectDropdown] = createSignal(false);
  const [selectedModel, setSelectedModel] = createSignal('haiku');
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [prompt, setPrompt] = createSignal('');
  const [chatMode, setChatMode] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Find matching project by path or name
  const matchedProject = createMemo(() => {
    const input = projectInput().toLowerCase();
    if (!input) return null;
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

  const selectedModelData = createMemo(() =>
    models.find(m => m.value === selectedModel()) || models[0]
  );

  async function fetchProjects() {
    setProjectsLoading(true);
    try {
      const response = await api.projects.list();
      setProjects(response.projects);
      // Don't auto-select - leave empty for user to choose or type
    } catch (e) {
      console.error('Failed to fetch projects:', e);
    } finally {
      setProjectsLoading(false);
    }
  }

  // Reset form when modal opens
  createEffect(() => {
    if (props.isOpen) {
      setProjectInput('');
      setPrompt('');
      setError(null);
      fetchProjects();
    }
  });

  // Handle escape key
  createEffect(() => {
    if (!props.isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        props.onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
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
        const newProject = await api.projects.create({ name: input });
        projectPath = newProject.path;
        setProjects([...projects(), newProject]);
      } else {
        projectPath = matchedProject()!.path;
      }

      const response = await api.sessions.create({
        project_path: projectPath,
        prompt: prompt() || undefined,
        model: selectedModel(),
        terminal_mode: !chatMode(),
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

  // Shared input styles
  const inputStyles = {
    width: "100%",
    "box-sizing": "border-box",
    padding: "12px 14px",
    "font-size": "14px",
    "font-family": "inherit",
    "border-radius": "10px",
    border: "1px solid var(--color-bg-overlay)",
    background: "var(--color-bg-base)",
    color: "var(--color-text-primary)",
    outline: "none",
    transition: "border-color 0.15s ease, box-shadow 0.15s ease",
  } as const;

  const inputFocusStyles = `
    .modal-input:focus {
      border-color: var(--color-accent);
      box-shadow: 0 0 0 2px var(--color-accent-muted);
    }
  `;

  // Shared dropdown styles
  const dropdownStyles = {
    position: "absolute",
    top: "100%",
    left: "0",
    right: "0",
    "margin-top": "6px",
    "border-radius": "10px",
    border: "1px solid var(--color-bg-overlay)",
    background: "var(--color-bg-surface)",
    "max-height": "180px",
    "overflow-y": "auto",
    "box-shadow": "0 8px 24px rgba(0, 0, 0, 0.4)",
    "z-index": "100",
  } as const;

  const dropdownItemStyles = {
    padding: "10px 14px",
    cursor: "pointer",
    "font-size": "14px",
    transition: "background 0.1s ease",
  } as const;

  return (
    <Show when={props.isOpen}>
      {/* Inject focus styles */}
      <style>{inputFocusStyles}</style>

      {/* Backdrop with blur */}
      <div
        class="animate-fade-in"
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
          "padding-top": "max(16px, env(safe-area-inset-top))",
          "padding-bottom": "max(16px, env(safe-area-inset-bottom))",
          background: "rgba(0, 0, 0, 0.7)",
          "-webkit-backdrop-filter": "blur(8px)",
          "backdrop-filter": "blur(8px)",
        }}
        onClick={handleBackdropClick}
      >
        {/* Modal with retro card style */}
        <div
          class="animate-slide-up"
          style={{
            width: "min(420px, calc(100vw - 32px))",
            "max-height": "calc(100vh - 32px)",
            "max-height": "calc(100dvh - 32px)",
            "border-radius": "14px",
            border: "1.5px solid var(--color-bg-overlay)",
            background: "var(--color-bg-surface)",
            "box-shadow": "4px 4px 0px rgba(0, 0, 0, 0.4)",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
          }}
        >
          {/* Header */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "18px 20px",
              "border-bottom": "1px solid var(--color-bg-overlay)",
              "flex-shrink": "0",
            }}
          >
            <h2
              class="text-text-primary"
              style={{
                "font-size": "17px",
                "font-weight": "600",
                margin: "0",
                "letter-spacing": "-0.01em",
              }}
            >
              New Session
            </h2>
            <button
              onClick={props.onClose}
              class="text-text-muted hover:text-text-primary pressable"
              style={{
                width: "32px",
                height: "32px",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "border-radius": "8px",
                border: "none",
                background: "transparent",
                cursor: "pointer",
                transition: "color 0.15s ease",
              }}
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Form - scrollable */}
          <form
            onSubmit={handleSubmit}
            class="scrollable"
            style={{
              padding: "20px",
              "overflow-y": "auto",
              flex: "1",
              "-webkit-overflow-scrolling": "touch",
            }}
          >
            <div style={{ display: "flex", "flex-direction": "column", gap: "18px" }}>
              {/* Error message */}
              <Show when={error()}>
                <div
                  style={{
                    padding: "12px 14px",
                    "border-radius": "10px",
                    border: "1px solid var(--color-accent)",
                    background: "var(--color-accent-muted)",
                    color: "var(--color-accent)",
                    "font-size": "13px",
                    "font-weight": "500",
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
                    "margin-bottom": "8px",
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
                        gap: "10px",
                        ...inputStyles,
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
                      onFocus={() => setShowProjectDropdown(true)}
                      onBlur={() => setTimeout(() => setShowProjectDropdown(false), 150)}
                      placeholder="Select or type to create..."
                      class="modal-input placeholder:text-text-muted"
                      style={inputStyles}
                    />
                    {/* Chevron indicator */}
                    <div
                      style={{
                        position: "absolute",
                        right: "12px",
                        top: "50%",
                        transform: "translateY(-50%)",
                        "pointer-events": "none",
                        color: "var(--color-text-muted)",
                      }}
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <polyline points="6 9 12 15 18 9" />
                      </svg>
                    </div>

                    {/* Dropdown */}
                    <Show when={showProjectDropdown() && filteredProjects().length > 0}>
                      <div style={dropdownStyles}>
                        <For each={filteredProjects()}>
                          {(project) => (
                            <div
                              class="text-text-primary hover:bg-bg-elevated"
                              style={dropdownItemStyles}
                              onMouseDown={(e) => {
                                e.preventDefault();
                                setProjectInput(project.name);
                                setShowProjectDropdown(false);
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
                        "font-weight": "500",
                        "margin-top": "8px",
                        display: "flex",
                        "align-items": "center",
                        gap: "6px",
                      }}
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <path d="M12 5v14M5 12h14" />
                      </svg>
                      <span>Will create: <strong>{projectInput().trim()}</strong></span>
                    </div>
                  </Show>
                </Show>
              </div>

              {/* Model Selection - Custom Dropdown */}
              <div style={{ position: "relative" }}>
                <label
                  class="text-label"
                  style={{
                    display: "block",
                    "margin-bottom": "8px",
                  }}
                >
                  Model
                </label>
                <div style={{ position: "relative" }}>
                  <button
                    type="button"
                    onClick={() => setShowModelDropdown(!showModelDropdown())}
                    onBlur={() => setTimeout(() => setShowModelDropdown(false), 150)}
                    class="modal-input text-text-primary"
                    style={{
                      ...inputStyles,
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      cursor: "pointer",
                      "text-align": "left",
                      "padding-right": "40px",
                    }}
                  >
                    <span>
                      {selectedModelData().label}
                      <span class="text-text-tertiary" style={{ "margin-left": "8px" }}>
                        — {selectedModelData().description}
                      </span>
                    </span>
                  </button>
                  {/* Chevron indicator */}
                  <div
                    style={{
                      position: "absolute",
                      right: "12px",
                      top: "50%",
                      transform: `translateY(-50%) rotate(${showModelDropdown() ? '180deg' : '0deg'})`,
                      "pointer-events": "none",
                      color: "var(--color-text-muted)",
                      transition: "transform 0.15s ease",
                    }}
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="6 9 12 15 18 9" />
                    </svg>
                  </div>

                  {/* Dropdown */}
                  <Show when={showModelDropdown()}>
                    <div style={dropdownStyles}>
                      <For each={models}>
                        {(model) => (
                          <div
                            class={`text-text-primary ${model.value === selectedModel() ? 'bg-bg-elevated' : ''} hover:bg-bg-elevated`}
                            style={{
                              ...dropdownItemStyles,
                              display: "flex",
                              "align-items": "center",
                              "justify-content": "space-between",
                            }}
                            onMouseDown={(e) => {
                              e.preventDefault();
                              setSelectedModel(model.value);
                              setShowModelDropdown(false);
                            }}
                          >
                            <span>
                              {model.label}
                              <span class="text-text-tertiary" style={{ "margin-left": "8px" }}>
                                — {model.description}
                              </span>
                            </span>
                            <Show when={model.value === selectedModel()}>
                              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-accent)" stroke-width="2.5">
                                <polyline points="20 6 9 17 4 12" />
                              </svg>
                            </Show>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              </div>

              {/* Initial Prompt (Optional) */}
              <div>
                <label
                  class="text-label"
                  style={{
                    display: "block",
                    "margin-bottom": "8px",
                  }}
                >
                  Initial Prompt
                  <span class="text-text-muted" style={{ "margin-left": "6px", "text-transform": "none" }}>(optional)</span>
                </label>
                <textarea
                  value={prompt()}
                  onInput={(e) => setPrompt(e.currentTarget.value)}
                  placeholder="What would you like Claude to help with?"
                  rows={3}
                  class="modal-input placeholder:text-text-muted"
                  style={{
                    ...inputStyles,
                    resize: "none",
                    "line-height": "1.5",
                  }}
                />
              </div>

              {/* Chat Mode Toggle */}
              <label
                style={{
                  display: "flex",
                  "align-items": "flex-start",
                  gap: "12px",
                  padding: "14px",
                  "border-radius": "10px",
                  border: "1px solid var(--color-bg-overlay)",
                  background: "var(--color-bg-base)",
                  cursor: "pointer",
                  transition: "border-color 0.15s ease",
                }}
                class="hover:border-text-muted"
              >
                <input
                  type="checkbox"
                  checked={chatMode()}
                  onChange={(e) => setChatMode(e.currentTarget.checked)}
                  style={{
                    width: "18px",
                    height: "18px",
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
                    class="text-text-tertiary"
                    style={{ display: "block", "font-size": "12px", "margin-top": "4px", "line-height": "1.5" }}
                  >
                    Uses Claude API (per token). Uncheck for Terminal Mode (Max subscription).
                  </span>
                </div>
              </label>
            </div>
          </form>

          {/* Footer Actions */}
          <div
            style={{
              display: "flex",
              gap: "12px",
              padding: "16px 20px",
              "padding-bottom": "max(16px, env(safe-area-inset-bottom))",
              "border-top": "1px solid var(--color-bg-overlay)",
              background: "var(--color-bg-surface)",
              "flex-shrink": "0",
            }}
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
              onClick={handleSubmit}
            >
              {loading() ? 'Creating...' : 'Create Session'}
            </Button>
          </div>
        </div>
      </div>
    </Show>
  );
}
