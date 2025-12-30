import { Show, For, createSignal, createEffect, createMemo, onCleanup } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button } from '../ui/Button';
import { Spinner } from '../ui/Spinner';
import { api, Project, ClaudeSession, ClaudeTranscriptMessage } from '../../lib/api';
import { useKeyboard } from '../../lib/keyboard';

interface NewSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function NewSessionModal(props: NewSessionModalProps) {
  const navigate = useNavigate();
  const { isVisible: keyboardVisible, viewportHeight } = useKeyboard();
  const [mode, setMode] = createSignal<'new' | 'import'>('new');
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [projectsLoading, setProjectsLoading] = createSignal(false);
  const [projectInput, setProjectInput] = createSignal('');
  const [showProjectDropdown, setShowProjectDropdown] = createSignal(false);
  const [selectedModel, setSelectedModel] = createSignal('haiku');
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [prompt, setPrompt] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Import mode state
  const [claudeSessions, setClaudeSessions] = createSignal<ClaudeSession[]>([]);
  const [importLoading, setImportLoading] = createSignal(false);
  const [importingId, setImportingId] = createSignal<string | null>(null);
  const [previewSession, setPreviewSession] = createSignal<ClaudeSession | null>(null);
  const [previewMessages, setPreviewMessages] = createSignal<ClaudeTranscriptMessage[]>([]);
  const [previewLoading, setPreviewLoading] = createSignal(false);
  const [previewError, setPreviewError] = createSignal<string | null>(null);

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
      setMode('new');
      setProjectInput('');
      setPrompt('');
      setError(null);
      setClaudeSessions([]);
      setPreviewSession(null);
      setPreviewMessages([]);
      setPreviewError(null);
      setPreviewLoading(false);
      fetchProjects();
    }
  });

  // Fetch claude sessions when project changes (in import mode)
  async function fetchClaudeSessions(projectPath: string) {
    if (!projectPath) {
      setClaudeSessions([]);
      return;
    }
    setImportLoading(true);
    try {
      const response = await api.sessions.listClaudeSessions(projectPath);
      // Filter to show only sessions not already in Clauset
      setClaudeSessions(response.sessions.filter(s => !s.in_clauset));
    } catch (e) {
      console.error('Failed to fetch Claude sessions:', e);
      setClaudeSessions([]);
    } finally {
      setImportLoading(false);
    }
  }

  // Handle import session
  async function handleImport(session: ClaudeSession) {
    setImportingId(session.session_id);
    setError(null);
    try {
      const response = await api.sessions.import({
        claude_session_id: session.session_id,
        project_path: session.project_path,
      });
      props.onClose();
      navigate(`/session/${response.session_id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to import session');
    } finally {
      setImportingId(null);
    }
  }

  async function openPreview(session: ClaudeSession) {
    setPreviewSession(session);
    setPreviewMessages([]);
    setPreviewError(null);
    setPreviewLoading(true);
    try {
      const response = await api.sessions.getClaudeTranscript(session.session_id, session.project_path);
      setPreviewMessages(response.messages);
    } catch (e) {
      setPreviewError(e instanceof Error ? e.message : 'Failed to load transcript');
    } finally {
      setPreviewLoading(false);
    }
  }

  function closePreview() {
    setPreviewSession(null);
    setPreviewMessages([]);
    setPreviewError(null);
    setPreviewLoading(false);
  }

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

  // Shared input styles - use monospace font to match session card titles
  const inputStyles = {
    width: "100%",
    "box-sizing": "border-box",
    padding: "8px 10px",
    "font-size": "13px",
    "font-family": "var(--font-mono)",
    "border-radius": "8px",
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
    "margin-top": "4px",
    "border-radius": "8px",
    border: "1px solid var(--color-bg-overlay)",
    background: "var(--color-bg-surface)",
    "max-height": "160px",
    "overflow-y": "auto",
    "box-shadow": "0 6px 20px rgba(0, 0, 0, 0.4)",
    "z-index": "100",
  } as const;

  const dropdownItemStyles = {
    padding: "7px 10px",
    cursor: "pointer",
    "font-size": "13px",
    "font-family": "var(--font-mono)",
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
            width: "min(380px, calc(100vw - 32px))",
            "max-height": keyboardVisible()
              ? `${viewportHeight() - 32}px`
              : "calc(100dvh - 32px)",
            "border-radius": "12px",
            border: "1.5px solid var(--color-bg-overlay)",
            background: "var(--color-bg-surface)",
            "box-shadow": "3px 3px 0px rgba(0, 0, 0, 0.4)",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
          }}
        >
          {/* Header with Tabs */}
          <div
            style={{
              display: "flex",
              "flex-direction": "column",
              "border-bottom": "1px solid var(--color-bg-overlay)",
              "flex-shrink": "0",
            }}
          >
            {/* Title row */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                padding: "12px 16px 8px",
              }}
            >
              <h2
                class="text-text-primary text-mono"
                style={{
                  "font-size": "14px",
                  "font-weight": "600",
                  margin: "0",
                }}
              >
                {mode() === 'new' ? 'New Session' : 'Import Session'}
              </h2>
              <button
                onClick={props.onClose}
                class="text-text-muted hover:text-text-primary pressable"
                style={{
                  width: "28px",
                  height: "28px",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  "border-radius": "6px",
                  border: "none",
                  background: "transparent",
                  cursor: "pointer",
                  transition: "color 0.15s ease",
                }}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Tabs */}
            <div style={{ display: "flex", padding: "0 16px", gap: "4px" }}>
              <button
                onClick={() => {
                  setMode('new');
                  closePreview();
                }}
                class="text-mono"
                style={{
                  padding: "8px 12px",
                  "font-size": "12px",
                  "font-weight": mode() === 'new' ? "600" : "400",
                  border: "none",
                  background: mode() === 'new' ? "var(--color-bg-elevated)" : "transparent",
                  color: mode() === 'new' ? "var(--color-text-primary)" : "var(--color-text-muted)",
                  "border-radius": "6px 6px 0 0",
                  cursor: "pointer",
                  transition: "all 0.15s ease",
                }}
              >
                New
              </button>
              <button
                onClick={() => {
                  setMode('import');
                  closePreview();
                  // Fetch claude sessions for current project
                  const matched = matchedProject();
                  if (matched) {
                    fetchClaudeSessions(matched.path);
                  }
                }}
                class="text-mono"
                style={{
                  padding: "8px 12px",
                  "font-size": "12px",
                  "font-weight": mode() === 'import' ? "600" : "400",
                  border: "none",
                  background: mode() === 'import' ? "var(--color-bg-elevated)" : "transparent",
                  color: mode() === 'import' ? "var(--color-text-primary)" : "var(--color-text-muted)",
                  "border-radius": "6px 6px 0 0",
                  cursor: "pointer",
                  transition: "all 0.15s ease",
                }}
              >
                Import from Terminal
              </button>
            </div>
          </div>

          {/* New Session Form */}
          <Show when={mode() === 'new'}>
            <form
              onSubmit={handleSubmit}
              class="scrollable"
              style={{
                padding: "14px 16px",
                "overflow-y": "auto",
                flex: "1",
                "-webkit-overflow-scrolling": "touch",
              }}
            >
              <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
                {/* Error message */}
                <Show when={error()}>
                  <div
                    class="text-mono"
                    style={{
                      padding: "8px 10px",
                      "border-radius": "6px",
                      border: "1px solid var(--color-accent)",
                      background: "var(--color-accent-muted)",
                      color: "var(--color-accent)",
                      "font-size": "12px",
                      "font-weight": "500",
                    }}
                  >
                    {error()}
                  </div>
                </Show>

                {/* Project Selection */}
              <div style={{ position: "relative" }}>
                <label
                  class="text-label text-mono"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "11px",
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
                      onFocus={() => { setShowModelDropdown(false); setShowProjectDropdown(true); }}
                      onBlur={() => setTimeout(() => setShowProjectDropdown(false), 150)}
                      placeholder="Select or type to create..."
                      class="modal-input placeholder:text-text-muted"
                      style={inputStyles}
                    />
                    {/* Chevron indicator */}
                    <div
                      style={{
                        position: "absolute",
                        right: "10px",
                        top: "50%",
                        transform: "translateY(-50%)",
                        "pointer-events": "none",
                        color: "var(--color-text-muted)",
                      }}
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
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
                      class="text-accent text-mono"
                      style={{
                        "font-size": "11px",
                        "font-weight": "500",
                        "margin-top": "6px",
                        display: "flex",
                        "align-items": "center",
                        gap: "4px",
                      }}
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <path d="M12 5v14M5 12h14" />
                      </svg>
                      <span>New: <strong>{projectInput().trim()}</strong></span>
                    </div>
                  </Show>
                </Show>
              </div>

              {/* Model Selection - Custom Dropdown */}
              <div style={{ position: "relative" }}>
                <label
                  class="text-label text-mono"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "11px",
                  }}
                >
                  Model
                </label>
                <div style={{ position: "relative" }}>
                  <button
                    type="button"
                    onClick={() => { setShowProjectDropdown(false); setShowModelDropdown(!showModelDropdown()); }}
                    onBlur={() => setTimeout(() => setShowModelDropdown(false), 150)}
                    class="modal-input text-text-primary"
                    style={{
                      ...inputStyles,
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      cursor: "pointer",
                      "text-align": "left",
                      "padding-right": "32px",
                    }}
                  >
                    <span>
                      {selectedModelData().label}
                      <span class="text-text-tertiary" style={{ "margin-left": "6px" }}>
                        — {selectedModelData().description}
                      </span>
                    </span>
                  </button>
                  {/* Chevron indicator */}
                  <div
                    style={{
                      position: "absolute",
                      right: "10px",
                      top: "50%",
                      transform: `translateY(-50%) rotate(${showModelDropdown() ? '180deg' : '0deg'})`,
                      "pointer-events": "none",
                      color: "var(--color-text-muted)",
                      transition: "transform 0.15s ease",
                    }}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
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
                              <span class="text-text-tertiary" style={{ "margin-left": "6px" }}>
                                — {model.description}
                              </span>
                            </span>
                            <Show when={model.value === selectedModel()}>
                              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-accent)" stroke-width="2.5">
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
                  class="text-label text-mono"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "11px",
                  }}
                >
                  Initial Prompt
                  <span class="text-text-muted" style={{ "margin-left": "4px", "text-transform": "none" }}>(optional)</span>
                </label>
                <textarea
                  value={prompt()}
                  onInput={(e) => setPrompt(e.currentTarget.value)}
                  onFocus={() => { setShowProjectDropdown(false); setShowModelDropdown(false); }}
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
              </div>
            </form>

            {/* Footer Actions for New Mode */}
            <div
              style={{
                display: "flex",
                gap: "10px",
                padding: "12px 16px",
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
          </Show>

          {/* Import Mode Content */}
          <Show when={mode() === 'import'}>
            <div
              class="scrollable"
              style={{
                padding: "14px 16px",
                "overflow-y": "auto",
                flex: "1",
                "-webkit-overflow-scrolling": "touch",
              }}
            >
              <Show
                when={previewSession()}
                fallback={
                  <>
                    {/* Error message */}
                    <Show when={error()}>
                      <div
                        class="text-mono"
                        style={{
                          padding: "8px 10px",
                          "border-radius": "6px",
                          border: "1px solid var(--color-accent)",
                          background: "var(--color-accent-muted)",
                          color: "var(--color-accent)",
                          "font-size": "12px",
                          "font-weight": "500",
                          "margin-bottom": "12px",
                        }}
                      >
                        {error()}
                      </div>
                    </Show>

                    {/* Project Selection for Import */}
                    <div style={{ "margin-bottom": "16px" }}>
                      <label
                        class="text-label text-mono"
                        style={{
                          display: "block",
                          "margin-bottom": "6px",
                          "font-size": "11px",
                        }}
                      >
                        Select Project
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
                            onInput={(e) => {
                              setProjectInput(e.currentTarget.value);
                              // Fetch sessions when project changes
                              const matched = projects().find(
                                p => p.name.toLowerCase() === e.currentTarget.value.toLowerCase()
                              );
                              if (matched) {
                                fetchClaudeSessions(matched.path);
                              }
                            }}
                            onFocus={() => setShowProjectDropdown(true)}
                            onBlur={() => setTimeout(() => setShowProjectDropdown(false), 150)}
                            placeholder="Select a project..."
                            class="modal-input placeholder:text-text-muted"
                            style={inputStyles}
                          />
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
                                      fetchClaudeSessions(project.path);
                                    }}
                                  >
                                    {project.name}
                                  </div>
                                )}
                              </For>
                            </div>
                          </Show>
                        </div>
                      </Show>
                    </div>

                    {/* Session List */}
                    <Show when={importLoading()}>
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "center",
                          padding: "32px",
                        }}
                      >
                        <Spinner size="md" />
                      </div>
                    </Show>

                    <Show when={!importLoading() && claudeSessions().length === 0 && matchedProject()}>
                      <div
                        style={{
                          "text-align": "center",
                          padding: "32px 16px",
                          color: "var(--color-text-muted)",
                        }}
                      >
                        <p class="text-mono" style={{ "font-size": "13px", margin: "0" }}>
                          No terminal sessions found
                        </p>
                        <p style={{ "font-size": "12px", margin: "8px 0 0", "font-family": "var(--font-serif)" }}>
                          All Claude sessions for this project are already in Clauset
                        </p>
                      </div>
                    </Show>

                    <Show when={!importLoading() && !matchedProject()}>
                      <div
                        style={{
                          "text-align": "center",
                          padding: "32px 16px",
                          color: "var(--color-text-muted)",
                        }}
                      >
                        <p class="text-mono" style={{ "font-size": "13px", margin: "0" }}>
                          Select a project above
                        </p>
                        <p style={{ "font-size": "12px", margin: "8px 0 0", "font-family": "var(--font-serif)" }}>
                          Sessions from the terminal will appear here
                        </p>
                      </div>
                    </Show>

                    <Show when={!importLoading() && claudeSessions().length > 0}>
                      <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
                        <For each={claudeSessions()}>
                          {(session) => (
                            <div
                              onClick={() => openPreview(session)}
                              style={{
                                padding: "12px",
                                "border-radius": "8px",
                                border: "1px solid var(--color-bg-overlay)",
                                background: "var(--color-bg-elevated)",
                                cursor: "pointer",
                              }}
                            >
                              <div style={{ display: "flex", "justify-content": "space-between", "align-items": "flex-start" }}>
                                <div style={{ flex: "1", "min-width": "0" }}>
                                  <p
                                    class="text-mono"
                                    style={{
                                      "font-size": "13px",
                                      "font-weight": "500",
                                      margin: "0 0 4px",
                                      color: "var(--color-text-primary)",
                                      overflow: "hidden",
                                      "text-overflow": "ellipsis",
                                      "white-space": "nowrap",
                                    }}
                                  >
                                    {session.preview || 'Untitled session'}
                                  </p>
                                  <p
                                    style={{
                                      "font-size": "11px",
                                      color: "var(--color-text-muted)",
                                      margin: "0",
                                    }}
                                  >
                                    {new Date(session.timestamp).toLocaleString()}
                                  </p>
                                </div>
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleImport(session);
                                  }}
                                  disabled={importingId() === session.session_id}
                                  style={{
                                    padding: "6px 12px",
                                    "font-size": "12px",
                                    "font-weight": "500",
                                    "border-radius": "6px",
                                    border: "none",
                                    background: "var(--color-accent)",
                                    color: "#ffffff",
                                    cursor: importingId() === session.session_id ? "not-allowed" : "pointer",
                                    opacity: importingId() === session.session_id ? "0.7" : "1",
                                    "white-space": "nowrap",
                                  }}
                                >
                                  {importingId() === session.session_id ? 'Importing...' : 'Import'}
                                </button>
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </>
                }
              >
                {(session) => (
                  <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
                    <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
                      <button
                        type="button"
                        onClick={closePreview}
                        class="text-mono"
                        style={{
                          padding: "6px 10px",
                          "font-size": "12px",
                          "border-radius": "6px",
                          border: "1px solid var(--color-bg-overlay)",
                          background: "transparent",
                          color: "var(--color-text-secondary)",
                          cursor: "pointer",
                        }}
                      >
                        Back
                      </button>
                      <button
                        type="button"
                        onClick={() => handleImport(session())}
                        disabled={importingId() === session().session_id}
                        style={{
                          padding: "6px 12px",
                          "font-size": "12px",
                          "font-weight": "500",
                          "border-radius": "6px",
                          border: "none",
                          background: "var(--color-accent)",
                          color: "#ffffff",
                          cursor: importingId() === session().session_id ? "not-allowed" : "pointer",
                          opacity: importingId() === session().session_id ? "0.7" : "1",
                        }}
                      >
                        {importingId() === session().session_id ? 'Importing...' : 'Import'}
                      </button>
                    </div>

                    <div>
                      <p
                        class="text-mono"
                        style={{
                          "font-size": "13px",
                          "font-weight": "600",
                          margin: "0 0 4px",
                          color: "var(--color-text-primary)",
                        }}
                      >
                        {session().preview || 'Untitled session'}
                      </p>
                      <p
                        style={{
                          "font-size": "11px",
                          color: "var(--color-text-muted)",
                          margin: "0",
                        }}
                      >
                        {new Date(session().timestamp).toLocaleString()}
                      </p>
                    </div>

                    <Show when={previewLoading()}>
                      <div style={{ display: "flex", "justify-content": "center", padding: "24px 0" }}>
                        <Spinner size="md" />
                      </div>
                    </Show>

                    <Show when={previewError()}>
                      <div
                        class="text-mono"
                        style={{
                          padding: "8px 10px",
                          "border-radius": "6px",
                          border: "1px solid var(--color-accent)",
                          background: "var(--color-accent-muted)",
                          color: "var(--color-accent)",
                          "font-size": "12px",
                          "font-weight": "500",
                        }}
                      >
                        {previewError()}
                      </div>
                    </Show>

                    <Show when={!previewLoading() && !previewError() && previewMessages().length === 0}>
                      <div
                        style={{
                          "text-align": "center",
                          padding: "24px 0",
                          color: "var(--color-text-muted)",
                        }}
                      >
                        <p class="text-mono" style={{ "font-size": "12px", margin: "0" }}>
                          No transcript messages found
                        </p>
                      </div>
                    </Show>

                    <Show when={!previewLoading() && previewMessages().length > 0}>
                      <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
                        <For each={previewMessages()}>
                          {(message) => (
                            <div
                              style={{
                                padding: "10px",
                                "border-radius": "8px",
                                border: "1px solid var(--color-bg-overlay)",
                                background: "var(--color-bg-base)",
                              }}
                            >
                              <div style={{ display: "flex", "justify-content": "space-between", "align-items": "center", "margin-bottom": "6px" }}>
                                <span
                                  class="text-mono"
                                  style={{
                                    "font-size": "11px",
                                    "font-weight": "600",
                                    color: message.role === 'user' ? 'var(--color-accent)' : 'var(--color-text-secondary)',
                                    "text-transform": "capitalize",
                                  }}
                                >
                                  {message.role === 'user' ? 'You' : 'Claude'}
                                </span>
                                <span style={{ "font-size": "10px", color: "var(--color-text-muted)" }}>
                                  {new Date(message.timestamp).toLocaleString()}
                                </span>
                              </div>
                              <div
                                style={{
                                  "white-space": "pre-wrap",
                                  "font-size": "13px",
                                  "font-family": "var(--font-serif)",
                                  color: "var(--color-text-primary)",
                                }}
                              >
                                {message.content}
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                )}
              </Show>
            </div>

            {/* Footer for Import Mode */}
            <div
              style={{
                display: "flex",
                gap: "10px",
                padding: "12px 16px",
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
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
}
