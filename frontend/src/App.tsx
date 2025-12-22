import { ParentProps, createSignal, onMount, onCleanup } from 'solid-js';
import { ConnectionStatus } from './components/ui/ConnectionStatus';
import { connectGlobalWs, disconnectGlobalWs } from './lib/globalWs';

export default function App(props: ParentProps) {
  const [isOnline, setIsOnline] = createSignal(navigator.onLine);

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
    <div class="min-h-screen bg-bg-base text-text-primary">
      <ConnectionStatus isOnline={isOnline()} />
      {props.children}
    </div>
  );
}
