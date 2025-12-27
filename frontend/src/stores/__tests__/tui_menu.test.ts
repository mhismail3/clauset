import { describe, it, expect, beforeEach } from 'vitest';
import {
  getTuiMenuState,
  handleTuiMenuEvent,
  clearTuiMenuState,
  hasActiveTuiMenu,
  getActiveTuiMenu,
  type TuiMenu,
  type TuiMenuEvent,
} from '../tui_menu';

describe('TUI Menu Store', () => {
  const sessionId = 'test-session-1';

  beforeEach(() => {
    // Reset state for each test
    clearTuiMenuState(sessionId);
  });

  // ==================== State Queries ====================

  describe('getTuiMenuState', () => {
    it('returns idle for unknown session', () => {
      const state = getTuiMenuState('unknown-session');
      expect(state).toEqual({ type: 'idle' });
    });

    it('returns idle after clearing state', () => {
      // First set up an active menu
      const menu: TuiMenu = {
        id: 'menu-1',
        title: 'Select model',
        options: [{ index: 0, label: 'Opus', is_selected: false }],
        menu_type: 'model_select',
        highlighted_index: 0,
      };
      handleTuiMenuEvent({
        type: 'menu_presented',
        session_id: sessionId,
        menu,
      });

      // Then clear it
      clearTuiMenuState(sessionId);
      const state = getTuiMenuState(sessionId);
      expect(state).toEqual({ type: 'idle' });
    });
  });

  describe('hasActiveTuiMenu', () => {
    it('returns false for unknown session', () => {
      expect(hasActiveTuiMenu('unknown-session')).toBe(false);
    });

    it('returns false when idle', () => {
      expect(hasActiveTuiMenu(sessionId)).toBe(false);
    });

    it('returns true when menu is active', () => {
      const menu: TuiMenu = {
        id: 'menu-1',
        title: 'Select option',
        options: [{ index: 0, label: 'A', is_selected: false }],
        menu_type: 'generic',
        highlighted_index: 0,
      };
      handleTuiMenuEvent({
        type: 'menu_presented',
        session_id: sessionId,
        menu,
      });
      expect(hasActiveTuiMenu(sessionId)).toBe(true);
    });
  });

  describe('getActiveTuiMenu', () => {
    it('returns null for unknown session', () => {
      expect(getActiveTuiMenu('unknown-session')).toBeNull();
    });

    it('returns null when idle', () => {
      expect(getActiveTuiMenu(sessionId)).toBeNull();
    });

    it('returns menu when active', () => {
      const menu: TuiMenu = {
        id: 'menu-1',
        title: 'Select model',
        description: 'Choose a Claude model',
        options: [
          { index: 0, label: 'Opus', description: 'Most capable', is_selected: true },
          { index: 1, label: 'Sonnet', description: 'Balanced', is_selected: false },
        ],
        menu_type: 'model_select',
        highlighted_index: 1,
      };
      handleTuiMenuEvent({
        type: 'menu_presented',
        session_id: sessionId,
        menu,
      });

      const activeMenu = getActiveTuiMenu(sessionId);
      expect(activeMenu).not.toBeNull();
      expect(activeMenu?.title).toBe('Select model');
      expect(activeMenu?.options.length).toBe(2);
      expect(activeMenu?.highlighted_index).toBe(1);
    });
  });

  // ==================== Event Handling ====================

  describe('handleTuiMenuEvent', () => {
    it('handles menu_presented event', () => {
      const menu: TuiMenu = {
        id: 'menu-123',
        title: 'Test Menu',
        options: [
          { index: 0, label: 'Option A', is_selected: false },
          { index: 1, label: 'Option B', is_selected: true },
        ],
        menu_type: 'generic',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({
        type: 'menu_presented',
        session_id: sessionId,
        menu,
      });

      const state = getTuiMenuState(sessionId);
      expect(state.type).toBe('active');
      if (state.type === 'active') {
        expect(state.menu.id).toBe('menu-123');
        expect(state.menu.title).toBe('Test Menu');
        expect(state.menu.options.length).toBe(2);
        expect(state.menu.options[1].is_selected).toBe(true);
      }
    });

    it('handles menu_dismissed event', () => {
      // First present a menu
      const menu: TuiMenu = {
        id: 'menu-to-dismiss',
        title: 'Dismissable Menu',
        options: [{ index: 0, label: 'X', is_selected: false }],
        menu_type: 'generic',
        highlighted_index: 0,
      };
      handleTuiMenuEvent({
        type: 'menu_presented',
        session_id: sessionId,
        menu,
      });
      expect(hasActiveTuiMenu(sessionId)).toBe(true);

      // Then dismiss it
      handleTuiMenuEvent({
        type: 'menu_dismissed',
        session_id: sessionId,
        menu_id: 'menu-to-dismiss',
      });
      expect(hasActiveTuiMenu(sessionId)).toBe(false);
      expect(getTuiMenuState(sessionId)).toEqual({ type: 'idle' });
    });

    it('handles multiple sessions independently', () => {
      const session1 = 'session-1';
      const session2 = 'session-2';

      const menu1: TuiMenu = {
        id: 'menu-s1',
        title: 'Menu for Session 1',
        options: [{ index: 0, label: 'A', is_selected: false }],
        menu_type: 'model_select',
        highlighted_index: 0,
      };

      const menu2: TuiMenu = {
        id: 'menu-s2',
        title: 'Menu for Session 2',
        options: [{ index: 0, label: 'B', is_selected: false }],
        menu_type: 'config',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: session1, menu: menu1 });
      handleTuiMenuEvent({ type: 'menu_presented', session_id: session2, menu: menu2 });

      expect(hasActiveTuiMenu(session1)).toBe(true);
      expect(hasActiveTuiMenu(session2)).toBe(true);

      const active1 = getActiveTuiMenu(session1);
      const active2 = getActiveTuiMenu(session2);

      expect(active1?.title).toBe('Menu for Session 1');
      expect(active2?.title).toBe('Menu for Session 2');

      // Dismiss one session
      clearTuiMenuState(session1);
      expect(hasActiveTuiMenu(session1)).toBe(false);
      expect(hasActiveTuiMenu(session2)).toBe(true);
    });
  });

  // ==================== Menu Type Detection ====================

  describe('Menu types', () => {
    it('handles model_select menu type', () => {
      const menu: TuiMenu = {
        id: 'model-menu',
        title: 'Select model',
        options: [
          { index: 0, label: 'Opus', is_selected: false },
          { index: 1, label: 'Sonnet', is_selected: true },
          { index: 2, label: 'Haiku', is_selected: false },
        ],
        menu_type: 'model_select',
        highlighted_index: 2,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.menu_type).toBe('model_select');
    });

    it('handles config menu type', () => {
      const menu: TuiMenu = {
        id: 'config-menu',
        title: 'Configuration',
        options: [
          { index: 0, label: 'Option 1', is_selected: false },
          { index: 1, label: 'Option 2', is_selected: false },
        ],
        menu_type: 'config',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.menu_type).toBe('config');
    });

    it('handles mode menu type', () => {
      const menu: TuiMenu = {
        id: 'mode-menu',
        title: 'Select mode',
        options: [
          { index: 0, label: 'Normal', is_selected: true },
          { index: 1, label: 'Plan', is_selected: false },
        ],
        menu_type: 'mode',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.menu_type).toBe('mode');
    });
  });

  // ==================== Option Properties ====================

  describe('Option handling', () => {
    it('preserves option descriptions', () => {
      const menu: TuiMenu = {
        id: 'desc-menu',
        title: 'Options with descriptions',
        options: [
          { index: 0, label: 'Fast', description: 'Quick responses', is_selected: false },
          { index: 1, label: 'Smart', description: 'Most capable', is_selected: false },
        ],
        menu_type: 'generic',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.options[0].description).toBe('Quick responses');
      expect(active?.options[1].description).toBe('Most capable');
    });

    it('handles options without descriptions', () => {
      const menu: TuiMenu = {
        id: 'no-desc-menu',
        title: 'Simple options',
        options: [
          { index: 0, label: 'A', is_selected: false },
          { index: 1, label: 'B', is_selected: false },
        ],
        menu_type: 'generic',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.options[0].description).toBeUndefined();
    });

    it('tracks highlighted index', () => {
      const menu: TuiMenu = {
        id: 'highlight-menu',
        title: 'Test',
        options: [
          { index: 0, label: 'A', is_selected: false },
          { index: 1, label: 'B', is_selected: false },
          { index: 2, label: 'C', is_selected: false },
        ],
        menu_type: 'generic',
        highlighted_index: 2,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.highlighted_index).toBe(2);
    });

    it('tracks selected option', () => {
      const menu: TuiMenu = {
        id: 'selected-menu',
        title: 'Test',
        options: [
          { index: 0, label: 'A', is_selected: false },
          { index: 1, label: 'B', is_selected: true },
          { index: 2, label: 'C', is_selected: false },
        ],
        menu_type: 'generic',
        highlighted_index: 0,
      };

      handleTuiMenuEvent({ type: 'menu_presented', session_id: sessionId, menu });

      const active = getActiveTuiMenu(sessionId);
      expect(active?.options.find(o => o.is_selected)?.label).toBe('B');
    });
  });
});
