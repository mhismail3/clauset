import { JSX } from 'solid-js';

interface BadgeProps {
  variant: 'active' | 'idle' | 'completed' | 'error';
  children: JSX.Element;
}

export function Badge(props: BadgeProps) {
  const variantClasses = {
    active: 'bg-status-active/20 text-status-active',
    idle: 'bg-status-idle/20 text-status-idle',
    completed: 'bg-status-completed/20 text-status-completed',
    error: 'bg-red-500/20 text-red-400',
  };

  return (
    <span class={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${variantClasses[props.variant]}`}>
      {props.children}
    </span>
  );
}
