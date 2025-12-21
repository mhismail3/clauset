import { JSX, splitProps } from 'solid-js';

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost' | 'destructive';
  size?: 'sm' | 'md' | 'lg';
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'class', 'children']);

  const variant = local.variant ?? 'primary';
  const size = local.size ?? 'md';

  const baseClasses = `
    inline-flex items-center justify-center
    font-semibold
    transition-all duration-150 ease-out
    focus:outline-none
    disabled:opacity-40 disabled:pointer-events-none
    active:scale-95 active:opacity-80
    touch-manipulation
  `.replace(/\s+/g, ' ').trim();

  const variantClasses = {
    primary: 'bg-accent text-white rounded-xl shadow-sm shadow-accent/20',
    secondary: 'bg-bg-elevated text-text-primary rounded-xl',
    ghost: 'text-accent bg-transparent rounded-lg',
    destructive: 'bg-destructive/10 text-destructive rounded-xl',
  };

  const sizeClasses = {
    sm: 'h-9 px-4 text-sm min-w-[44px]',
    md: 'h-11 px-5 text-base min-w-[44px]',
    lg: 'h-13 px-6 text-lg min-w-[44px]',
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
