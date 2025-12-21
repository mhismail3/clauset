import { JSX, Show } from 'solid-js';

interface BadgeProps {
  variant: 'active' | 'idle' | 'completed' | 'error' | 'starting';
  children: JSX.Element;
  showDot?: boolean;
}

export function Badge(props: BadgeProps) {
  const variantStyles: Record<string, { bg: string; text: string; dot: string }> = {
    active: {
      bg: 'rgba(44, 143, 122, 0.15)',
      text: '#2c8f7a',
      dot: '#2c8f7a',
    },
    starting: {
      bg: 'rgba(212, 166, 68, 0.15)',
      text: '#d4a644',
      dot: '#d4a644',
    },
    idle: {
      bg: 'rgba(212, 166, 68, 0.15)',
      text: '#d4a644',
      dot: '#d4a644',
    },
    completed: {
      bg: 'rgba(92, 88, 85, 0.15)',
      text: '#9a9590',
      dot: '#5c5855',
    },
    error: {
      bg: 'rgba(196, 91, 55, 0.15)',
      text: '#c45b37',
      dot: '#c45b37',
    },
  };

  const style = variantStyles[props.variant];
  const showDot = props.showDot ?? (props.variant === 'active' || props.variant === 'starting');

  return (
    <span
      class="text-mono"
      style={{
        display: 'inline-flex',
        "align-items": 'center',
        gap: '5px',
        padding: '3px 8px',
        "border-radius": '6px',
        "font-size": '10px',
        "font-weight": '600',
        "text-transform": 'uppercase',
        "letter-spacing": '0.04em',
        background: style.bg,
        color: style.text,
      }}
    >
      <Show when={showDot}>
        <span
          class={props.variant === 'starting' ? 'animate-pulse' : ''}
          style={{
            width: '6px',
            height: '6px',
            "border-radius": '50%',
            background: style.dot,
            "box-shadow": props.variant === 'active' ? `0 0 6px ${style.dot}` : 'none',
          }}
        />
      </Show>
      {props.children}
    </span>
  );
}
