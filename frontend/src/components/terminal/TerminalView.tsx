import { onMount, onCleanup, createSignal } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Button } from '../ui/Button';

interface TerminalViewProps {
  onInput: (data: Uint8Array) => void;
  onResize: (cols: number, rows: number) => void;
  onClose: () => void;
  onReady?: (write: (data: Uint8Array) => void) => void;
}

export function TerminalView(props: TerminalViewProps) {
  let containerRef: HTMLDivElement | undefined;
  let terminal: Terminal | undefined;
  let fitAddon: FitAddon | undefined;

  const [fontSize, setFontSize] = createSignal(14);

  onMount(() => {
    terminal = new Terminal({
      theme: {
        background: '#0f0f0f',
        foreground: '#f5f5f5',
        cursor: '#da7756',
        cursorAccent: '#0f0f0f',
        selectionBackground: 'rgba(218, 119, 86, 0.3)',
        black: '#1a1a1a',
        red: '#ff6b6b',
        green: '#4ade80',
        yellow: '#fbbf24',
        blue: '#60a5fa',
        magenta: '#c084fc',
        cyan: '#22d3ee',
        white: '#f5f5f5',
        brightBlack: '#666666',
        brightRed: '#ff8a8a',
        brightGreen: '#6ee7a0',
        brightYellow: '#fcd34d',
        brightBlue: '#93c5fd',
        brightMagenta: '#d8b4fe',
        brightCyan: '#67e8f9',
        brightWhite: '#ffffff',
      },
      fontFamily: 'ui-monospace, "SF Mono", Menlo, Monaco, monospace',
      fontSize: fontSize(),
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 5000,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    terminal.open(containerRef!);
    fitAddon.fit();

    // Handle input
    terminal.onData((data) => {
      const encoder = new TextEncoder();
      props.onInput(encoder.encode(data));
    });

    // Handle resize
    const resizeObserver = new ResizeObserver(() => {
      fitAddon?.fit();
      if (terminal) {
        props.onResize(terminal.cols, terminal.rows);
      }
    });
    resizeObserver.observe(containerRef!);

    // Notify parent that terminal is ready with write function
    if (props.onReady) {
      props.onReady((data: Uint8Array) => {
        terminal?.write(data);
      });
    }

    onCleanup(() => {
      resizeObserver.disconnect();
      terminal?.dispose();
    });
  });

  function adjustFontSize(delta: number) {
    const newSize = Math.max(10, Math.min(24, fontSize() + delta));
    setFontSize(newSize);
    if (terminal) {
      terminal.options.fontSize = newSize;
      fitAddon?.fit();
    }
  }

  // Expose write method for incoming data
  function write(data: string | Uint8Array) {
    terminal?.write(data);
  }

  return (
    <div class="flex-1 flex flex-col bg-bg-base">
      {/* Terminal Controls */}
      <div class="flex items-center gap-2 px-4 py-2 border-b border-bg-elevated">
        <span class="text-sm font-medium">Terminal</span>
        <div class="flex-1" />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => adjustFontSize(-2)}
        >
          A-
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => adjustFontSize(2)}
        >
          A+
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={props.onClose}
        >
          Chat
        </Button>
      </div>

      {/* Terminal Container */}
      <div
        ref={containerRef}
        class="flex-1 p-2 overflow-hidden"
      />
    </div>
  );
}
