/**
 * TuiMenuOverlay - Native UI overlay for TUI selection menus.
 *
 * Renders Claude Code's built-in TUI menus (like /model, /config) as a native
 * overlay instead of raw ANSI terminal text. Provides keyboard navigation
 * and click selection.
 */
import { Show, For, createSignal, createEffect, onCleanup } from 'solid-js';
import { TuiMenu, clearTuiMenuState } from '../../stores/tui_menu';
import './TuiMenuOverlay.css';

interface TuiMenuOverlayProps {
  sessionId: string;
  menu: TuiMenu;
  onSelect: (menuId: string, selectedIndex: number) => void;
  onCancel: (menuId: string) => void;
}

export function TuiMenuOverlay(props: TuiMenuOverlayProps) {
  // Track highlighted index locally for keyboard navigation
  const [highlightedIndex, setHighlightedIndex] = createSignal(props.menu.highlighted_index);

  // Update local highlight when menu changes
  createEffect(() => {
    setHighlightedIndex(props.menu.highlighted_index);
  });

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowUp':
        e.preventDefault();
        setHighlightedIndex((prev) => Math.max(0, prev - 1));
        break;
      case 'ArrowDown':
        e.preventDefault();
        setHighlightedIndex((prev) => Math.min(props.menu.options.length - 1, prev + 1));
        break;
      case 'Enter':
        e.preventDefault();
        handleSelect(highlightedIndex());
        break;
      case 'Escape':
        e.preventDefault();
        handleCancel();
        break;
      // Number keys for quick selection (1-9)
      case '1': case '2': case '3': case '4': case '5':
      case '6': case '7': case '8': case '9':
        {
          const index = parseInt(e.key) - 1;
          if (index < props.menu.options.length) {
            e.preventDefault();
            handleSelect(index);
          }
        }
        break;
    }
  };

  createEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    onCleanup(() => window.removeEventListener('keydown', handleKeyDown));
  });

  const handleSelect = (index: number) => {
    props.onSelect(props.menu.id, index);
    clearTuiMenuState(props.sessionId);
  };

  const handleCancel = () => {
    props.onCancel(props.menu.id);
    clearTuiMenuState(props.sessionId);
  };

  // Get icon for menu type
  const menuIcon = () => {
    switch (props.menu.menu_type) {
      case 'model_select': return '\u2699'; // gear
      case 'config': return '\u2699';
      case 'permissions': return '\u26A0'; // warning
      case 'mode': return '\u21C4'; // arrows
      default: return '\u2630'; // hamburger
    }
  };

  return (
    <div class="tui-menu-overlay" onClick={handleCancel}>
      <div class="tui-menu-container" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div class="tui-menu-header">
          <span class="tui-menu-icon">{menuIcon()}</span>
          <span class="tui-menu-title">{props.menu.title}</span>
        </div>

        {/* Description */}
        <Show when={props.menu.description}>
          <p class="tui-menu-description">{props.menu.description}</p>
        </Show>

        {/* Options */}
        <div class="tui-menu-options">
          <For each={props.menu.options}>
            {(option, i) => (
              <button
                class={`tui-menu-option ${i() === highlightedIndex() ? 'highlighted' : ''} ${option.is_selected ? 'selected' : ''}`}
                onClick={() => handleSelect(option.index)}
                onMouseEnter={() => setHighlightedIndex(i())}
              >
                <span class="option-number">{option.index + 1}.</span>
                <div class="option-content">
                  <span class="option-label">
                    {option.label}
                    <Show when={option.is_selected}>
                      <span class="option-checkmark">\u2713</span>
                    </Show>
                  </span>
                  <Show when={option.description}>
                    <span class="option-description">{option.description}</span>
                  </Show>
                </div>
                <Show when={i() === highlightedIndex()}>
                  <span class="option-arrow">\u25B8</span>
                </Show>
              </button>
            )}
          </For>
        </div>

        {/* Footer */}
        <div class="tui-menu-footer">
          <span class="footer-hint">\u2191\u2193 Navigate</span>
          <span class="footer-divider">\u00B7</span>
          <span class="footer-hint">Enter Select</span>
          <span class="footer-divider">\u00B7</span>
          <span class="footer-hint">Esc Cancel</span>
        </div>
      </div>
    </div>
  );
}
