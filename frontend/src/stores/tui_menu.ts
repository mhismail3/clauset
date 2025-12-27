/**
 * TUI Menu store for native UI rendering.
 *
 * Handles TUI selection menus from Claude Code's built-in commands
 * (like /model, /config) that are detected from terminal output and
 * rendered as native UI overlays instead of raw ANSI terminal text.
 */
import { createSignal } from 'solid-js';

// Types matching the backend TuiMenu types
export interface TuiMenuOption {
  index: number;
  label: string;
  description?: string;
  is_selected: boolean;
}

export type TuiMenuType = 'model_select' | 'config' | 'permissions' | 'mode' | 'generic';

export interface TuiMenu {
  id: string;
  title: string;
  description?: string;
  options: TuiMenuOption[];
  menu_type: TuiMenuType;
  highlighted_index: number;
}

export type TuiMenuEvent =
  | { type: 'menu_presented'; session_id: string; menu: TuiMenu }
  | { type: 'menu_dismissed'; session_id: string; menu_id: string };

export type TuiMenuState =
  | { type: 'idle' }
  | { type: 'active'; menu: TuiMenu };

// Store: Map of session ID to TUI menu state
const [tuiMenuStates, setTuiMenuStates] = createSignal<Map<string, TuiMenuState>>(
  new Map()
);

/**
 * Get the current TUI menu state for a session.
 */
export function getTuiMenuState(sessionId: string): TuiMenuState {
  return tuiMenuStates().get(sessionId) ?? { type: 'idle' };
}

/**
 * Handle a TUI menu event from the WebSocket.
 */
export function handleTuiMenuEvent(event: TuiMenuEvent) {
  console.log('[tui_menu]', event.type, event);

  switch (event.type) {
    case 'menu_presented': {
      // Normalize snake_case fields from backend
      const menu: TuiMenu = {
        id: event.menu.id,
        title: event.menu.title,
        description: event.menu.description,
        options: event.menu.options.map((opt: any) => ({
          index: opt.index,
          label: opt.label,
          description: opt.description,
          is_selected: opt.is_selected,
        })),
        menu_type: event.menu.menu_type,
        highlighted_index: event.menu.highlighted_index,
      };

      setTuiMenuStates((prev) => {
        const next = new Map(prev);
        next.set(event.session_id, { type: 'active', menu });
        return next;
      });
      break;
    }

    case 'menu_dismissed': {
      setTuiMenuStates((prev) => {
        const next = new Map(prev);
        next.set(event.session_id, { type: 'idle' });
        return next;
      });
      break;
    }
  }
}

/**
 * Clear the TUI menu state for a session.
 * Called after user makes a selection or cancels.
 */
export function clearTuiMenuState(sessionId: string) {
  setTuiMenuStates((prev) => {
    const next = new Map(prev);
    next.set(sessionId, { type: 'idle' });
    return next;
  });
}

/**
 * Check if a session has an active TUI menu.
 */
export function hasActiveTuiMenu(sessionId: string): boolean {
  const state = getTuiMenuState(sessionId);
  return state.type === 'active';
}

/**
 * Get the active menu for a session if one exists.
 */
export function getActiveTuiMenu(sessionId: string): TuiMenu | null {
  const state = getTuiMenuState(sessionId);
  if (state.type === 'active') {
    return state.menu;
  }
  return null;
}

export { tuiMenuStates };
