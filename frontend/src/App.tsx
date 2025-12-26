import { ParentProps, onMount, onCleanup } from 'solid-js';
import { connectGlobalWs, disconnectGlobalWs } from './lib/globalWs';
import { usePreventOverscroll } from './lib/preventOverscroll';

export default function App(props: ParentProps) {
  // Prevent iOS PWA viewport rubber-banding when no scrollable content
  usePreventOverscroll();

  onMount(() => {
    const handleOnline = () => {
      // Reconnect global WebSocket when coming back online
      connectGlobalWs();
    };

    window.addEventListener('online', handleOnline);

    // Connect global WebSocket for dashboard real-time updates
    connectGlobalWs();

    onCleanup(() => {
      window.removeEventListener('online', handleOnline);
      disconnectGlobalWs();
    });
  });

  return (
    <div class="h-full flex flex-col bg-bg-base text-text-primary" style={{ overflow: 'hidden' }}>
      {props.children}
    </div>
  );
}
