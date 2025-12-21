import { onMount, onCleanup, createSignal, For } from 'solid-js';
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

// Special keys for mobile keyboard toolbar
const SPECIAL_KEYS = [
  { label: 'Esc', code: '\x1b' },
  { label: 'Tab', code: '\t' },
  { label: 'Ctrl', code: null, isModifier: true },
  { label: '↑', code: '\x1b[A' },
  { label: '↓', code: '\x1b[B' },
  { label: '←', code: '\x1b[D' },
  { label: '→', code: '\x1b[C' },
  { label: 'Home', code: '\x1b[H' },
  { label: 'End', code: '\x1b[F' },
] as const;

export function TerminalView(props: TerminalViewProps) {
  let containerRef: HTMLDivElement | undefined;
  let terminal: Terminal | undefined;
  let fitAddon: FitAddon | undefined;

  const [fontSize, setFontSize] = createSignal(14);
  const [ctrlActive, setCtrlActive] = createSignal(false);

  function sendSpecialKey(key: typeof SPECIAL_KEYS[number]) {
    if (key.isModifier) {
      setCtrlActive(!ctrlActive());
      return;
    }

    const encoder = new TextEncoder();
    let code = key.code!;

    // If Ctrl is active and it's a single character, convert to control character
    if (ctrlActive() && code.length === 1) {
      const charCode = code.toUpperCase().charCodeAt(0);
      if (charCode >= 65 && charCode <= 90) { // A-Z
        code = String.fromCharCode(charCode - 64);
      }
    }

    props.onInput(encoder.encode(code));
    setCtrlActive(false); // Reset Ctrl after sending
    terminal?.focus();
  }

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

      {/* Special Keys Toolbar for Mobile */}
      <div class="flex items-center gap-1 px-2 py-2 border-t border-bg-elevated overflow-x-auto safe-bottom">
        <For each={SPECIAL_KEYS}>
          {(key) => (
            <button
              onClick={() => sendSpecialKey(key)}
              class={`px-3 py-1.5 text-sm font-mono rounded transition-colors flex-shrink-0 ${
                key.isModifier && ctrlActive()
                  ? 'bg-accent text-white'
                  : 'bg-bg-surface hover:bg-bg-elevated text-text-primary'
              }`}
            >
              {key.label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
