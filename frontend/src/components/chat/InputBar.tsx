import { createSignal, createEffect } from 'solid-js';
import { useKeyboard } from '../../lib/keyboard';

interface InputBarProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

const MAX_ROWS = 10;
const LINE_HEIGHT = 23; // 16px font (iOS override in CSS) * 1.4 line-height

export function InputBar(props: InputBarProps) {
  const [message, setMessage] = createSignal('');
  const [focused, setFocused] = createSignal(false);
  const [rows, setRows] = createSignal(1);
  let textareaRef: HTMLTextAreaElement | undefined;

  // iOS keyboard handling - adjust bottom padding when keyboard visible
  const { isVisible: keyboardVisible } = useKeyboard();

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

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }

  const canSend = () => !props.disabled && message().trim();
  const shouldScroll = () => rows() >= MAX_ROWS;

  return (
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
          placeholder={props.placeholder || "Message Claude..."}
          rows={rows()}
          disabled={props.disabled}
          class="text-mono"
          style={{
            flex: '1',
            "min-width": '0',
            "box-sizing": 'border-box',
            padding: '10px 14px',
            "font-size": '14px',
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
            transition: 'border-color 0.15s ease, box-shadow 0.15s ease',
            opacity: props.disabled ? '0.5' : '1',
            "overflow-y": shouldScroll() ? 'auto' : 'hidden',
          }}
        />

        {/* Icon-only send button */}
        <button
          type="submit"
          disabled={!canSend()}
          class="pressable"
          style={{
            width: '40px',
            height: '40px',
            "flex-shrink": '0',
            display: 'flex',
            "align-items": 'center',
            "justify-content": 'center',
            background: canSend() ? 'var(--color-accent)' : 'var(--color-bg-elevated)',
            color: canSend() ? '#ffffff' : 'var(--color-text-muted)',
            border: canSend()
              ? '1px solid var(--color-accent)'
              : '1px solid var(--color-bg-overlay)',
            "border-radius": '10px',
            cursor: canSend() ? 'pointer' : 'not-allowed',
            "box-shadow": canSend()
              ? '2px 2px 0px rgba(0, 0, 0, 0.3)'
              : '1px 1px 0px rgba(0, 0, 0, 0.2)',
            transition: 'all 0.15s ease',
          }}
        >
          {/* Arrow icon */}
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="5" y1="12" x2="19" y2="12" />
            <polyline points="12 5 19 12 12 19" />
          </svg>
        </button>
      </div>
    </form>
  );
}
