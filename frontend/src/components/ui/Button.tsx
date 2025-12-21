import { JSX, splitProps } from 'solid-js';

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'class', 'children']);

  const variant = local.variant ?? 'primary';
  const size = local.size ?? 'md';

  const baseClasses = 'inline-flex items-center justify-center rounded-lg font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-bg-base disabled:opacity-50 disabled:pointer-events-none';

  const variantClasses = {
    primary: 'bg-accent text-white hover:bg-accent-hover',
    secondary: 'bg-bg-elevated text-text-primary hover:bg-bg-overlay',
    ghost: 'text-text-secondary hover:text-text-primary hover:bg-bg-elevated',
  };

  const sizeClasses = {
    sm: 'h-8 px-3 text-sm',
    md: 'h-10 px-4 text-base',
    lg: 'h-12 px-6 text-lg',
  };

  return (
    <button
      class={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${local.class ?? ''}`}
      {...rest}
    >
      {local.children}
    </button>
  );
}
