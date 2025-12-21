import { onMount, onCleanup, createSignal, For } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface TerminalViewProps {
  onInput: (data: Uint8Array) => void;
  onResize: (cols: number, rows: number) => void;
  onClose: () => void;
  onReady?: (write: (data: Uint8Array) => void) => void;
}

// Special keys for mobile keyboard toolbar
const SPECIAL_KEYS = [
  { label: 'Esc', code: '\x1b', icon: null },
  { label: 'Tab', code: '\t', icon: null },
  { label: 'Ctrl', code: null, isModifier: true, icon: null },
  { label: '↑', code: '\x1b[A', icon: null },
  { label: '↓', code: '\x1b[B', icon: null },
  { label: '←', code: '\x1b[D', icon: null },
  { label: '→', code: '\x1b[C', icon: null },
] as const;

const CTRL_SHORTCUTS = [
  { label: 'C', desc: 'Cancel' },
  { label: 'D', desc: 'EOF' },
  { label: 'Z', desc: 'Suspend' },
  { label: 'L', desc: 'Clear' },
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

    if (ctrlActive() && code.length === 1) {
      const charCode = code.toUpperCase().charCodeAt(0);
      if (charCode >= 65 && charCode <= 90) {
        code = String.fromCharCode(charCode - 64);
      }
    }

    props.onInput(encoder.encode(code));
    setCtrlActive(false);
    terminal?.focus();
  }

  function sendCtrlKey(char: string) {
    const encoder = new TextEncoder();
    const charCode = char.toUpperCase().charCodeAt(0);
    const code = String.fromCharCode(charCode - 64);
    props.onInput(encoder.encode(code));
    setCtrlActive(false);
    terminal?.focus();
  }

  onMount(() => {
    terminal = new Terminal({
      theme: {
        background: '#000000',
        foreground: '#ffffff',
        cursor: '#da7756',
        cursorAccent: '#000000',
        selectionBackground: 'rgba(218, 119, 86, 0.3)',
        black: '#1c1c1e',
        red: '#ff453a',
        green: '#30d158',
        yellow: '#ffd60a',
        blue: '#0a84ff',
        magenta: '#bf5af2',
        cyan: '#64d2ff',
        white: '#ffffff',
        brightBlack: '#636366',
        brightRed: '#ff6961',
        brightGreen: '#4cd964',
        brightYellow: '#ffcc00',
        brightBlue: '#5ac8fa',
        brightMagenta: '#ff2d55',
        brightCyan: '#5ac8fa',
        brightWhite: '#ffffff',
      },
      fontFamily: '"SF Mono", ui-monospace, Menlo, Monaco, monospace',
      fontSize: fontSize(),
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 5000,
      allowProposedApi: true,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    terminal.open(containerRef!);
    fitAddon.fit();

    terminal.onData((data) => {
      const encoder = new TextEncoder();
      props.onInput(encoder.encode(data));
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon?.fit();
      if (terminal) {
        props.onResize(terminal.cols, terminal.rows);
      }
    });
    resizeObserver.observe(containerRef!);

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

  return (
    <div class="flex-1 flex flex-col bg-black">
      {/* Terminal Container */}
      <div
        ref={containerRef}
        class="flex-1 px-2 pt-2 overflow-hidden"
        style={{ "min-height": "0" }}
      />

      {/* Special Keys Toolbar */}
      <div class="flex-none bg-bg-surface border-t border-bg-overlay">
        {/* Ctrl shortcuts (shown when Ctrl is active) */}
        <div
          class={`flex items-center gap-1 px-2 py-2 border-b border-bg-overlay overflow-x-auto scrollable-x transition-all ${
            ctrlActive() ? 'max-h-12 opacity-100' : 'max-h-0 opacity-0 py-0 border-0'
          }`}
        >
          <span class="text-caption text-text-muted px-2">Ctrl+</span>
          <For each={CTRL_SHORTCUTS}>
            {(shortcut) => (
              <button
                onClick={() => sendCtrlKey(shortcut.label)}
                class="flex items-center gap-2 px-3 py-1.5 text-sm font-mono bg-bg-elevated rounded-lg text-text-primary active:bg-bg-overlay transition-colors"
              >
                <span class="font-semibold">{shortcut.label}</span>
                <span class="text-text-muted text-xs">{shortcut.desc}</span>
              </button>
            )}
          </For>
        </div>

        {/* Main toolbar */}
        <div class="flex items-center gap-1.5 px-3 py-2.5 safe-bottom">
          <For each={SPECIAL_KEYS}>
            {(key) => (
              <button
                onClick={() => sendSpecialKey(key)}
                class={`
                  min-w-[44px] h-10 px-3
                  text-sm font-semibold
                  rounded-lg
                  transition-all duration-100
                  active:scale-95
                  ${key.isModifier && ctrlActive()
                    ? 'bg-accent text-white shadow-sm'
                    : 'bg-bg-elevated text-text-primary'
                  }
                `}
              >
                {key.label}
              </button>
            )}
          </For>

          <div class="flex-1" />

          {/* Font size controls */}
          <button
            onClick={() => adjustFontSize(-2)}
            class="w-10 h-10 flex items-center justify-center text-text-muted bg-bg-elevated rounded-lg active:bg-bg-overlay transition-colors"
          >
            <span class="text-xs font-bold">A-</span>
          </button>
          <button
            onClick={() => adjustFontSize(2)}
            class="w-10 h-10 flex items-center justify-center text-text-muted bg-bg-elevated rounded-lg active:bg-bg-overlay transition-colors"
          >
            <span class="text-sm font-bold">A+</span>
          </button>
        </div>
      </div>
    </div>
  );
}
