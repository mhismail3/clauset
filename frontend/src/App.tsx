import { ParentProps, createSignal, onMount } from 'solid-js';
import { ConnectionStatus } from './components/ui/ConnectionStatus';

export default function App(props: ParentProps) {
  const [isOnline, setIsOnline] = createSignal(navigator.onLine);

  onMount(() => {
    const handleOnline = () => setIsOnline(true);
    const handleOffline = () => setIsOnline(false);

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    };
  });

  return (
    <div class="min-h-screen bg-bg-base text-text-primary safe-all">
      <ConnectionStatus isOnline={isOnline()} />
      {props.children}
    </div>
  );
}
