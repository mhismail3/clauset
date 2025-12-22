import { onMount, onCleanup, createSignal, createEffect, For } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface TerminalViewProps {
  onInput: (data: Uint8Array) => void;
  onResize: (cols: number, rows: number) => void;
  onClose: () => void;
  onReady?: (write: (data: Uint8Array) => void) => void;
  isConnected?: boolean;
}

// Special keys for mobile keyboard toolbar
const SPECIAL_KEYS = [
  { label: 'esc', code: '\x1b' },
  { label: 'tab', code: '\t' },
  { label: 'ctrl', code: null, isModifier: true },
  { label: '↑', code: '\x1b[A' },
  { label: '↓', code: '\x1b[B' },
  { label: '←', code: '\x1b[D' },
  { label: '→', code: '\x1b[C' },
] as const;

const CTRL_SHORTCUTS = [
  { label: 'C', desc: 'cancel' },
  { label: 'D', desc: 'eof' },
  { label: 'Z', desc: 'suspend' },
  { label: 'L', desc: 'clear' },
] as const;

export function TerminalView(props: TerminalViewProps) {
  let containerRef: HTMLDivElement | undefined;
  let terminal: Terminal | undefined;
  let fitAddon: FitAddon | undefined;
  let resizeTimeout: number | undefined;

  const [fontSize, setFontSize] = createSignal(13);
  const [ctrlActive, setCtrlActive] = createSignal(false);
  const [dimensions, setDimensions] = createSignal({ cols: 80, rows: 24 });

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

  function doFitAndResize() {
    if (!terminal || !fitAddon || !containerRef) {
      return;
    }

    try {
      const rect = containerRef.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) {
        setTimeout(doFitAndResize, 50);
        return;
      }

      // First let FitAddon do its calculation
      fitAddon.fit();

      // Then get xterm's actual cell dimensions from the renderer
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const core = (terminal as any)._core;
      const actualCellWidth = core?._renderService?.dimensions?.css?.cell?.width;
      const actualCellHeight = core?._renderService?.dimensions?.css?.cell?.height;

      if (actualCellWidth && actualCellHeight) {
        // Use actual measured cell dimensions with safety buffer
        const availableWidth = rect.width - 1;
        const cols = Math.floor(availableWidth / actualCellWidth);
        const rows = Math.floor(rect.height / actualCellHeight);

        if (cols > 0 && rows > 0 && cols !== terminal.cols) {
          terminal.resize(cols, rows);
        }
      }

      const newDims = { cols: terminal.cols, rows: terminal.rows };

      // Only send resize if dimensions actually changed
      const current = dimensions();
      if (newDims.cols !== current.cols || newDims.rows !== current.rows) {
        setDimensions(newDims);
        props.onResize(newDims.cols, newDims.rows);
      }
    } catch (e) {
      console.warn('Terminal fit failed:', e);
    }
  }

  // Debounced resize handler
  function handleResize() {
    if (resizeTimeout) {
      clearTimeout(resizeTimeout);
    }
    resizeTimeout = window.setTimeout(doFitAndResize, 50);
  }

  onMount(() => {
    terminal = new Terminal({
      theme: {
        background: '#0d0d0d',
        foreground: '#f0ebe3',
        cursor: '#c45b37',
        cursorAccent: '#0d0d0d',
        selectionBackground: 'rgba(196, 91, 55, 0.3)',
        selectionForeground: '#f0ebe3',
        black: '#171615',
        red: '#c45b37',
        green: '#2c8f7a',
        yellow: '#d4a644',
        blue: '#5b8a9a',
        magenta: '#8a6b94',
        cyan: '#5b9a8a',
        white: '#f0ebe3',
        brightBlack: '#5c5855',
        brightRed: '#d4704c',
        brightGreen: '#3aa58d',
        brightYellow: '#e0b856',
        brightBlue: '#6d9cac',
        brightMagenta: '#9c7da6',
        brightCyan: '#6dac9c',
        brightWhite: '#ffffff',
      },
      fontFamily: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
      fontSize: fontSize(),
      fontWeight: '400',
      fontWeightBold: '600',
      lineHeight: 1.25,
      letterSpacing: 0,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 10000,
      allowProposedApi: true,
      convertEol: true,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    terminal.open(containerRef!);

    // Wait for fonts to load before fitting to ensure accurate column calculation
    const fitAfterFonts = () => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          doFitAndResize();
        });
      });
    };

    // Check if fonts are ready, otherwise wait
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(fitAfterFonts);
    } else {
      // Fallback for browsers without font loading API
      setTimeout(fitAfterFonts, 100);
    }

    terminal.onData((data) => {
      const encoder = new TextEncoder();
      props.onInput(encoder.encode(data));
    });

    // Watch for container resize
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(containerRef!);

    // Also listen to window resize for orientation changes
    window.addEventListener('resize', handleResize);

    if (props.onReady) {
      props.onReady((data: Uint8Array) => {
        terminal?.write(data);
      });
    }

    onCleanup(() => {
      if (resizeTimeout) {
        clearTimeout(resizeTimeout);
      }
      window.removeEventListener('resize', handleResize);
      resizeObserver.disconnect();
      terminal?.dispose();
    });
  });

  // Re-fit when connection state changes to send initial size
  createEffect(() => {
    if (props.isConnected) {
      // Send resize with a delay to ensure connection is fully ready
      // A single 300ms delay is sufficient - the server will receive and apply it
      setTimeout(doFitAndResize, 300);
    }
  });

  function adjustFontSize(delta: number) {
    const newSize = Math.max(9, Math.min(20, fontSize() + delta));
    setFontSize(newSize);
    if (terminal) {
      terminal.options.fontSize = newSize;
      // Delay fit to allow font change to take effect
      requestAnimationFrame(() => {
        doFitAndResize();
      });
    }
  }

  return (
    <div
      style={{
        display: 'flex',
        "flex-direction": 'column',
        flex: '1 1 0%',
        "min-height": '0',
        width: '100%',
        background: '#0d0d0d',
        overflow: 'hidden',
      }}
    >
      {/* Terminal area - fills space above toolbar */}
      <div
        style={{
          flex: '1 1 0%',
          "min-height": '0',
          padding: "8px 12px 0 12px",
          overflow: "hidden",
          display: 'flex',
          "flex-direction": 'column',
        }}
      >
        <div
          ref={containerRef}
          style={{
            flex: '1 1 0%',
            "min-height": '0',
            width: "100%",
            overflow: "hidden",
          }}
        />
      </div>

      {/* Special Keys Toolbar - fixed at bottom */}
      <div
        style={{
          "flex-shrink": '0',
          background: 'var(--color-bg-surface)',
          "border-top": '1px solid var(--color-bg-overlay)',
        }}
      >
        {/* Ctrl shortcuts (shown when Ctrl is active) */}
        <div
          class="scrollable-x"
          style={{
            display: 'flex',
            "align-items": 'center',
            gap: '6px',
            padding: ctrlActive() ? '10px 12px' : '0 12px',
            "max-height": ctrlActive() ? '48px' : '0',
            opacity: ctrlActive() ? '1' : '0',
            overflow: 'hidden',
            transition: 'all 0.15s ease',
            "border-bottom": ctrlActive() ? '1px solid var(--color-bg-overlay)' : 'none',
          }}
        >
          <span
            class="text-mono"
            style={{
              "font-size": '11px',
              color: 'var(--color-text-muted)',
              "padding-right": '4px',
              "white-space": 'nowrap',
            }}
          >
            ctrl+
          </span>
          <For each={CTRL_SHORTCUTS}>
            {(shortcut) => (
              <button
                onClick={() => sendCtrlKey(shortcut.label)}
                class="key-button"
                style={{
                  display: 'flex',
                  "align-items": 'center',
                  gap: '6px',
                  padding: '6px 12px',
                  "white-space": 'nowrap',
                }}
              >
                <span style={{ color: 'var(--color-accent)' }}>{shortcut.label}</span>
                <span style={{ color: 'var(--color-text-muted)', "font-size": '11px' }}>
                  {shortcut.desc}
                </span>
              </button>
            )}
          </For>
        </div>

        {/* Main toolbar - scrollable for narrow screens */}
        <div
          class="scrollable-x"
          style={{
            display: 'flex',
            "align-items": 'center',
            gap: '8px',
            padding: '10px 16px',
            "padding-bottom": 'calc(max(env(safe-area-inset-bottom, 0px), 12px) + 16px)',
          }}
        >
          <For each={SPECIAL_KEYS}>
            {(key) => (
              <button
                onClick={() => sendSpecialKey(key)}
                class="key-button"
                style={{
                  "min-width": '44px',
                  "flex-shrink": '0',
                  height: '38px',
                  padding: '0 12px',
                  display: 'flex',
                  "align-items": 'center',
                  "justify-content": 'center',
                  background: key.isModifier && ctrlActive()
                    ? 'var(--color-accent)'
                    : 'var(--color-bg-elevated)',
                  color: key.isModifier && ctrlActive()
                    ? '#ffffff'
                    : 'var(--color-text-primary)',
                  "box-shadow": key.isModifier && ctrlActive()
                    ? 'none'
                    : 'var(--shadow-retro-sm)',
                  transform: key.isModifier && ctrlActive()
                    ? 'translate(2px, 2px)'
                    : 'none',
                }}
              >
                {key.label}
              </button>
            )}
          </For>

          {/* Spacer - but with min-width so it shrinks on narrow screens */}
          <div style={{ "flex-grow": '1', "min-width": '8px' }} />

          {/* Font size controls */}
          <button
            onClick={() => adjustFontSize(-1)}
            class="key-button"
            style={{
              width: '38px',
              "flex-shrink": '0',
              height: '38px',
              display: 'flex',
              "align-items": 'center',
              "justify-content": 'center',
              color: 'var(--color-text-muted)',
            }}
          >
            <span style={{ "font-size": '11px' }}>A−</span>
          </button>
          <button
            onClick={() => adjustFontSize(1)}
            class="key-button"
            style={{
              width: '38px',
              "flex-shrink": '0',
              height: '38px',
              display: 'flex',
              "align-items": 'center',
              "justify-content": 'center',
              color: 'var(--color-text-muted)',
            }}
          >
            <span style={{ "font-size": '14px' }}>A+</span>
          </button>
        </div>
      </div>
    </div>
  );
}
