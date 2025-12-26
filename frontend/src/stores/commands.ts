import { createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import { api, Command, CommandCategory } from '../lib/api';

// Category labels for display
export const CATEGORY_LABELS: Record<CommandCategory, string> = {
  built_in: 'Built-in',
  user: 'User Commands',
  skill: 'Skills',
  plugin: 'Plugins',
};

// Category colors
export const CATEGORY_COLORS: Record<CommandCategory, string> = {
  built_in: 'var(--color-accent)',
  user: '#6b9e6b',
  skill: '#d4a644',
  plugin: '#8b7ec8',
};

// Store state
const [commandsStore, setCommandsStore] = createStore<{
  commands: Command[];
  filteredCommands: Command[];
  filterQuery: string;
  selectedIndex: number;
}>({
  commands: [],
  filteredCommands: [],
  filterQuery: '',
  selectedIndex: 0,
});

// Loading states
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);
const [isCached, setIsCached] = createSignal(false);

// Fetch commands from backend (cached)
export async function fetchCommands(forceRefresh = false) {
  if (isCached() && !forceRefresh) return;

  setLoading(true);
  setError(null);

  try {
    const response = await api.commands.list();
    setCommandsStore('commands', response.commands);
    setCommandsStore('filteredCommands', response.commands);
    setIsCached(true);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Failed to load commands');
  } finally {
    setLoading(false);
  }
}

// Filter commands based on query
export function filterCommands(query: string) {
  setCommandsStore('filterQuery', query);
  setCommandsStore('selectedIndex', 0);

  const q = query.toLowerCase().replace(/^\//, '');

  if (!q) {
    setCommandsStore('filteredCommands', commandsStore.commands);
    return;
  }

  const filtered = commandsStore.commands.filter(
    (cmd) =>
      cmd.name.toLowerCase().includes(q) || cmd.description.toLowerCase().includes(q)
  );

  setCommandsStore('filteredCommands', filtered);
}

// Keyboard navigation
export function selectNext() {
  const max = commandsStore.filteredCommands.length - 1;
  if (max < 0) return;
  setCommandsStore('selectedIndex', Math.min(commandsStore.selectedIndex + 1, max));
}

export function selectPrevious() {
  if (commandsStore.filteredCommands.length === 0) return;
  setCommandsStore('selectedIndex', Math.max(commandsStore.selectedIndex - 1, 0));
}

export function getSelectedCommand(): Command | null {
  return commandsStore.filteredCommands[commandsStore.selectedIndex] ?? null;
}

// Group commands by category
export function getGroupedCommands(): Map<CommandCategory, Command[]> {
  const grouped = new Map<CommandCategory, Command[]>();
  const order: CommandCategory[] = ['built_in', 'user', 'skill', 'plugin'];

  for (const category of order) {
    const cmds = commandsStore.filteredCommands.filter((c) => c.category === category);
    if (cmds.length > 0) {
      grouped.set(category, cmds);
    }
  }

  return grouped;
}

// Reset filter state
export function resetFilter() {
  setCommandsStore('filterQuery', '');
  setCommandsStore('selectedIndex', 0);
  setCommandsStore('filteredCommands', commandsStore.commands);
}

// Exports
export { commandsStore, loading, error };
