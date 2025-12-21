import { Show } from 'solid-js';

interface ConnectionStatusProps {
  isOnline: boolean;
}

export function ConnectionStatus(props: ConnectionStatusProps) {
  return (
    <Show when={!props.isOnline}>
      <div class="fixed top-0 left-0 right-0 z-50 bg-amber-600 text-white text-center py-1 text-sm safe-top">
        Offline - viewing cached data
      </div>
    </Show>
  );
}
