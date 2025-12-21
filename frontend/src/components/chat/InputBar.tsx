import { createSignal } from 'solid-js';
import { Button } from '../ui/Button';

interface InputBarProps {
  onSend: (message: string) => void;
  disabled?: boolean;
}

export function InputBar(props: InputBarProps) {
  const [message, setMessage] = createSignal('');

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
      class="flex-none border-t border-bg-elevated bg-bg-base p-4 safe-bottom"
    >
      <div class="flex gap-2">
        <textarea
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          placeholder="Message Claude..."
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
