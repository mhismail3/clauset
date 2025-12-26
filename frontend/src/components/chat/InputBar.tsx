import { createSignal, createEffect } from 'solid-js';
import { useKeyboard } from '../../lib/keyboard';
import { CommandPicker } from '../commands/CommandPicker';
import { Command } from '../../lib/api';
import {
  selectNext,
  selectPrevious,
  getSelectedCommand,
} from '../../stores/commands';

interface InputBarProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

const MAX_ROWS = 10;
const LINE_HEIGHT = 23; // 16px font (iOS override in CSS) * 1.4 line-height
const VERTICAL_PADDING = 20; // 10px top + 10px bottom
const SINGLE_ROW_HEIGHT = LINE_HEIGHT + VERTICAL_PADDING; // 43px

export function InputBar(props: InputBarProps) {
  const [message, setMessage] = createSignal('');
  const [focused, setFocused] = createSignal(false);
  const [rows, setRows] = createSignal(1);
  const [showCommandPicker, setShowCommandPicker] = createSignal(false);
  const [commandQuery, setCommandQuery] = createSignal('');
  let textareaRef: HTMLTextAreaElement | undefined;

  // iOS keyboard handling - adjust bottom padding when keyboard visible
  const { isVisible: keyboardVisible } = useKeyboard();

  // Detect "/" trigger for command picker
  createEffect(() => {
    const text = message();
    // Show picker if starts with "/" and no space yet (still typing command)
    if (text.startsWith('/') && !text.includes(' ')) {
      setShowCommandPicker(true);
      setCommandQuery(text.slice(1)); // Remove leading "/"
    } else {
      setShowCommandPicker(false);
      setCommandQuery('');
    }
  });

  // Calculate rows based on content
  createEffect(() => {
    const text = message();

    // If empty, always reset to 1 row
    if (!text || !textareaRef) {
      setRows(1);
      return;
    }

    // Count explicit newlines
    const newlineCount = (text.match(/\n/g) || []).length + 1;

    // Temporarily set rows to 1 and height to auto to get true minimum scrollHeight
    const originalRows = textareaRef.rows;
    textareaRef.rows = 1;
    textareaRef.style.height = 'auto';
    const scrollHeight = textareaRef.scrollHeight;
    textareaRef.rows = originalRows;

    // Calculate rows from scrollHeight (accounts for wrapped lines)
    // Subtract padding (10px top + 10px bottom = 20px)
    const contentHeight = scrollHeight - 20;
    const scrollRows = Math.ceil(contentHeight / LINE_HEIGHT);

    // Use the larger of newline count or scroll-based rows
    const calculatedRows = Math.max(newlineCount, scrollRows, 1);
    setRows(Math.min(calculatedRows, MAX_ROWS));
  });

  function handleSubmit(e: Event) {
    e.preventDefault();
    const content = message().trim();
    if (content && !props.disabled) {
      props.onSend(content);
      setMessage('');
      setRows(1);
    }
  }

  function handleCommandSelect(cmd: Command) {
    setShowCommandPicker(false);
    if (cmd.argument_hint) {
      // Has arguments - insert command and let user add args
      setMessage(`${cmd.display_name} `);
      textareaRef?.focus();
    } else {
      // No arguments - send immediately
      props.onSend(cmd.display_name);
      setMessage('');
      setRows(1);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Handle command picker navigation
    if (showCommandPicker()) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        selectNext();
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        selectPrevious();
        return;
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        const cmd = getSelectedCommand();
        if (cmd) {
          handleCommandSelect(cmd);
        }
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        setShowCommandPicker(false);
        setMessage('');
        return;
      }
      if (e.key === 'Tab') {
        // Tab completes the selected command
        e.preventDefault();
        const cmd = getSelectedCommand();
        if (cmd) {
          setMessage(`${cmd.display_name} `);
        }
        return;
      }
    }

    // Normal Enter handling for send
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }

  const canSend = () => !props.disabled && message().trim();
  const shouldScroll = () => rows() >= MAX_ROWS;

  // Calculate anchor bottom for command picker (above input bar)
  const inputBarHeight = () => {
    const textareaHeight = rows() * LINE_HEIGHT + VERTICAL_PADDING;
    const padding = 24; // 12px top + 12px bottom
    const safeArea = keyboardVisible() ? 0 : 20; // approximate safe area
    return textareaHeight + padding + safeArea;
  };

  return (
    <>
      {/* Command Picker */}
      <CommandPicker
        isOpen={showCommandPicker()}
        query={commandQuery()}
        onSelect={handleCommandSelect}
        onClose={() => setShowCommandPicker(false)}
        anchorBottom={inputBarHeight()}
      />

      <form
      onSubmit={handleSubmit}
      class="flex-none glass"
      style={{
        padding: '12px 16px',
        // Reduce bottom safe area when keyboard visible (no home indicator needed)
        "padding-bottom": keyboardVisible()
          ? '12px'
          : 'calc(env(safe-area-inset-bottom, 0px) + 12px)',
      }}
    >
      <div style={{ display: 'flex', gap: '10px', "align-items": 'flex-end' }}>
        {/* Textarea with retro styling */}
        <textarea
          ref={textareaRef}
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          placeholder={props.placeholder || "Chat with Claude Code..."}
          rows={rows()}
          disabled={props.disabled}
          class="text-mono"
          style={{
            flex: '1',
            "min-width": '0',
            "box-sizing": 'border-box',
            // Explicit height for reliable cross-browser sizing
            height: `${rows() * LINE_HEIGHT + VERTICAL_PADDING}px`,
            "min-height": `${SINGLE_ROW_HEIGHT}px`,
            padding: '10px 14px',
            "font-size": '13px',
            "line-height": '1.4',
            resize: 'none',
            background: 'var(--color-bg-base)',
            color: 'var(--color-text-primary)',
            border: focused()
              ? '1.5px solid var(--color-accent)'
              : '1.5px solid var(--color-bg-overlay)',
            "border-radius": '10px',
            outline: 'none',
            "box-shadow": focused()
              ? '0 0 0 3px var(--color-accent-muted), 1px 1px 0px rgba(0, 0, 0, 0.2)'
              : '1px 1px 0px rgba(0, 0, 0, 0.2)',
            transition: 'border-color 0.15s ease, box-shadow 0.15s ease, height 0.1s ease',
            opacity: props.disabled ? '0.5' : '1',
            "overflow-y": shouldScroll() ? 'auto' : 'hidden',
          }}
        />

        {/* Icon-only send button */}
        <button
          type="submit"
          disabled={!canSend()}
          style={{
            width: `${SINGLE_ROW_HEIGHT}px`,
            height: `${SINGLE_ROW_HEIGHT}px`,
            "flex-shrink": '0',
            display: 'flex',
            "align-items": 'center',
            "justify-content": 'center',
            background: 'transparent',
            color: canSend() ? 'var(--color-accent)' : 'var(--color-text-muted)',
            border: 'none',
            cursor: canSend() ? 'pointer' : 'default',
            transition: 'color 0.15s ease, transform 0.1s ease',
            opacity: canSend() ? 1 : 0.4,
          }}
          onMouseDown={(e) => {
            if (canSend()) e.currentTarget.style.transform = 'scale(0.85)';
          }}
          onMouseUp={(e) => {
            e.currentTarget.style.transform = 'scale(1)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'scale(1)';
          }}
          onTouchStart={(e) => {
            if (canSend()) e.currentTarget.style.transform = 'scale(0.85)';
          }}
          onTouchEnd={(e) => {
            e.currentTarget.style.transform = 'scale(1)';
          }}
        >
          {/* Up arrow icon */}
          <svg
            width="26"
            height="26"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="12" y1="19" x2="12" y2="5" />
            <polyline points="5 12 12 5 19 12" />
          </svg>
        </button>
      </div>
    </form>
    </>
  );
}
