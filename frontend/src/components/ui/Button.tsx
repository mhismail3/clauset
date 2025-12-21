import { JSX, splitProps } from 'solid-js';

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost' | 'destructive';
  size?: 'sm' | 'md' | 'lg';
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'class', 'children', 'style']);

  const variant = local.variant ?? 'primary';
  const size = local.size ?? 'md';

  const baseStyles: JSX.CSSProperties = {
    display: 'inline-flex',
    "align-items": 'center',
    "justify-content": 'center',
    "font-family": 'var(--font-mono)',
    "font-weight": '600',
    "border-radius": '10px',
    border: '1px solid var(--color-bg-overlay)',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    "white-space": 'nowrap',
  };

  const variantStyles: Record<string, JSX.CSSProperties> = {
    primary: {
      background: 'var(--color-accent)',
      color: '#ffffff',
      "box-shadow": 'var(--shadow-retro-sm)',
      border: '1px solid var(--color-accent)',
    },
    secondary: {
      background: 'var(--color-bg-elevated)',
      color: 'var(--color-text-primary)',
      "box-shadow": 'var(--shadow-retro-sm)',
    },
    ghost: {
      background: 'transparent',
      color: 'var(--color-accent)',
      border: 'none',
      "box-shadow": 'none',
    },
    destructive: {
      background: 'var(--color-accent-muted)',
      color: 'var(--color-accent)',
      border: '1px solid var(--color-accent)',
      "box-shadow": 'none',
    },
  };

  const sizeStyles: Record<string, JSX.CSSProperties> = {
    sm: {
      height: '36px',
      padding: '0 14px',
      "font-size": '12px',
      "min-width": '44px',
    },
    md: {
      height: '42px',
      padding: '0 18px',
      "font-size": '13px',
      "min-width": '44px',
    },
    lg: {
      height: '48px',
      padding: '0 24px',
      "font-size": '14px',
      "min-width": '44px',
    },
  };

  const mergedStyle = {
    ...baseStyles,
    ...variantStyles[variant],
    ...sizeStyles[size],
    ...(typeof local.style === 'object' ? local.style : {}),
  };

  return (
    <button
      class={`pressable ${local.class ?? ''}`}
      style={mergedStyle}
      {...rest}
    >
      {local.children}
    </button>
  );
}
