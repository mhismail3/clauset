import { ParentProps, createSignal, onMount, onCleanup } from 'solid-js';
import { ConnectionStatus } from './components/ui/ConnectionStatus';
import { connectGlobalWs, disconnectGlobalWs } from './lib/globalWs';
import { usePreventOverscroll } from './lib/preventOverscroll';

export default function App(props: ParentProps) {
  const [isOnline, setIsOnline] = createSignal(navigator.onLine);

  // Prevent iOS PWA viewport rubber-banding when no scrollable content
  usePreventOverscroll();

  onMount(() => {
    const handleOnline = () => {
      setIsOnline(true);
      // Reconnect global WebSocket when coming back online
      connectGlobalWs();
    };
    const handleOffline = () => setIsOnline(false);

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    // Connect global WebSocket for dashboard real-time updates
    connectGlobalWs();

    onCleanup(() => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
      disconnectGlobalWs();
    });
  });

  return (
    <div class="h-full flex flex-col bg-bg-base text-text-primary" style={{ overflow: 'hidden' }}>
      <ConnectionStatus isOnline={isOnline()} />
      {props.children}
    </div>
  );
}
