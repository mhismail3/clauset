import { Show, createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button } from '../ui/Button';
import { api } from '../../lib/api';

interface NewSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function NewSessionModal(props: NewSessionModalProps) {
  const navigate = useNavigate();
  const [projectPath, setProjectPath] = createSignal('');
  const [prompt, setPrompt] = createSignal('');
  const [terminalMode, setTerminalMode] = createSignal(true);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function handleSubmit(e: Event) {
    e.preventDefault();
    setLoading(true);
    setError(null);

    try {
      const response = await api.sessions.create({
        project_path: projectPath(),
        prompt: prompt(),
        terminal_mode: terminalMode(),
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

              {/* Project Path */}
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
                  Project Path
                </label>
                <input
                  type="text"
                  value={projectPath()}
                  onInput={(e) => setProjectPath(e.currentTarget.value)}
                  placeholder="/path/to/your/project"
                  required
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
                  }}
                />
              </div>

              {/* Initial Prompt */}
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
                  Initial Prompt
                </label>
                <textarea
                  value={prompt()}
                  onInput={(e) => setPrompt(e.currentTarget.value)}
                  placeholder="What would you like Claude to help with?"
                  rows={4}
                  required
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

              {/* Terminal Mode Toggle */}
              <label
                class="bg-bg-base"
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "12px",
                  padding: "16px",
                  "border-radius": "12px",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={terminalMode()}
                  onChange={(e) => setTerminalMode(e.currentTarget.checked)}
                  style={{
                    width: "20px",
                    height: "20px",
                    "accent-color": "var(--color-accent)",
                    cursor: "pointer",
                  }}
                />
                <div style={{ flex: "1", "min-width": "0" }}>
                  <span
                    class="text-text-primary"
                    style={{ display: "block", "font-size": "14px", "font-weight": "500" }}
                  >
                    Terminal Mode
                  </span>
                  <span
                    class="text-text-muted"
                    style={{ display: "block", "font-size": "12px", "margin-top": "2px" }}
                  >
                    Full PTY access - uses Claude Max subscription
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
                  disabled={loading()}
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
