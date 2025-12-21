import { JSX, Show } from 'solid-js';

interface BadgeProps {
  variant: 'active' | 'idle' | 'completed' | 'error' | 'starting';
  children: JSX.Element;
  showDot?: boolean;
}

export function Badge(props: BadgeProps) {
  const variantClasses = {
    active: 'bg-status-active/15 text-status-active',
    starting: 'bg-status-idle/15 text-status-idle',
    idle: 'bg-status-idle/15 text-status-idle',
    completed: 'bg-text-muted/15 text-text-tertiary',
    error: 'bg-status-error/15 text-status-error',
  };

  const dotClasses = {
    active: 'bg-status-active',
    starting: 'bg-status-idle animate-pulse',
    idle: 'bg-status-idle',
    completed: 'bg-text-muted',
    error: 'bg-status-error',
  };

  const showDot = props.showDot ?? (props.variant === 'active' || props.variant === 'starting');

  return (
    <span
      class={`
        inline-flex items-center gap-1.5
        px-2.5 py-1
        rounded-full
        text-xs font-medium
        ${variantClasses[props.variant]}
      `}
    >
      <Show when={showDot}>
        <span class={`w-1.5 h-1.5 rounded-full ${dotClasses[props.variant]}`} />
      </Show>
      {props.children}
    </span>
  );
}
