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
  const [terminalMode, setTerminalMode] = createSignal(false);
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

      // Start the session
      await api.sessions.start(response.session_id, prompt());

      props.onClose();
      navigate(`/session/${response.session_id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create session');
    } finally {
      setLoading(false);
    }
  }

  return (
    <Show when={props.isOpen}>
      <div class="fixed inset-0 z-50 flex items-end justify-center sm:items-center">
        {/* Backdrop */}
        <div
          class="absolute inset-0 bg-black/50"
          onClick={props.onClose}
        />

        {/* Modal */}
        <div class="relative bg-bg-surface rounded-t-2xl sm:rounded-2xl w-full max-w-lg safe-bottom">
          <div class="p-4 border-b border-bg-elevated">
            <h2 class="text-lg font-semibold">New Session</h2>
          </div>

          <form onSubmit={handleSubmit} class="p-4 space-y-4">
            <Show when={error()}>
              <div class="bg-red-500/10 border border-red-500/20 rounded-lg p-3 text-red-400 text-sm">
                {error()}
              </div>
            </Show>

            <div>
              <label class="block text-sm font-medium mb-1">Project Path</label>
              <input
                type="text"
                value={projectPath()}
                onInput={(e) => setProjectPath(e.currentTarget.value)}
                placeholder="/Users/moose/projects/my-project"
                class="w-full bg-bg-base border border-bg-overlay rounded-lg px-3 py-2 text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-accent"
                required
              />
            </div>

            <div>
              <label class="block text-sm font-medium mb-1">Initial Prompt</label>
              <textarea
                value={prompt()}
                onInput={(e) => setPrompt(e.currentTarget.value)}
                placeholder="What would you like Claude to help with?"
                rows={3}
                class="w-full bg-bg-base border border-bg-overlay rounded-lg px-3 py-2 text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-accent resize-none"
                required
              />
            </div>

            <div class="flex items-center gap-2">
              <input
                type="checkbox"
                id="terminal-mode"
                checked={terminalMode()}
                onChange={(e) => setTerminalMode(e.currentTarget.checked)}
                class="w-4 h-4 rounded border-bg-overlay bg-bg-base text-accent focus:ring-accent"
              />
              <label for="terminal-mode" class="text-sm">
                Terminal mode (full PTY access)
              </label>
            </div>

            <div class="flex gap-3 pt-2">
              <Button
                type="button"
                variant="secondary"
                class="flex-1"
                onClick={props.onClose}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                class="flex-1"
                disabled={loading()}
              >
                {loading() ? 'Creating...' : 'Create'}
              </Button>
            </div>
          </form>
        </div>
      </div>
    </Show>
  );
}
