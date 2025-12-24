import { createSignal } from 'solid-js';
import { Button } from '../ui/Button';
import { useKeyboard } from '../../lib/keyboard';

interface InputBarProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

export function InputBar(props: InputBarProps) {
  const [message, setMessage] = createSignal('');

  // iOS keyboard handling - adjust bottom padding when keyboard visible
  const { isVisible: keyboardVisible } = useKeyboard();

  function handleSubmit(e: Event) {
    e.preventDefault();
    const content = message().trim();
    if (content && !props.disabled) {
      props.onSend(content);
      setMessage('');
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      class="flex-none border-t border-bg-elevated bg-bg-base p-4"
      style={{
        // Reduce bottom safe area when keyboard visible (no home indicator needed)
        "padding-bottom": keyboardVisible()
          ? '16px'
          : 'calc(env(safe-area-inset-bottom, 0px) + 16px)',
        // No CSS transition - animation timing is controlled by keyboard hook's JS animation
      }}
    >
      <div class="flex gap-2">
        <textarea
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          placeholder={props.placeholder || "Message Claude..."}
          rows={1}
          disabled={props.disabled}
          class="flex-1 bg-bg-surface border border-bg-overlay rounded-xl px-4 py-3 text-text-primary placeholder:text-text-muted resize-none focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
        />
        <Button
          type="submit"
          disabled={props.disabled || !message().trim()}
        >
          Send
        </Button>
      </div>
    </form>
  );
}
