interface SpinnerProps {
  size?: 'sm' | 'md' | 'lg';
}

export function Spinner(props: SpinnerProps) {
  const size = props.size ?? 'md';

  const sizeClasses = {
    sm: 'w-4 h-4',
    md: 'w-6 h-6',
    lg: 'w-8 h-8',
  };

  return (
    <div class={`${sizeClasses[size]} animate-spin`}>
      <svg viewBox="0 0 24 24" fill="none" class="w-full h-full">
        <circle
          cx="12"
          cy="12"
          r="10"
          stroke="currentColor"
          stroke-width="3"
          stroke-linecap="round"
          class="opacity-25"
        />
        <path
          d="M12 2a10 10 0 0 1 10 10"
          stroke="currentColor"
          stroke-width="3"
          stroke-linecap="round"
          class="opacity-75"
        />
      </svg>
    </div>
  );
}
