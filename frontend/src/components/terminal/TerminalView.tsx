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

// Touch scroll physics configuration
const SCROLL_DECELERATION = 0.95; // Friction coefficient (0.95 = smooth, 0.9 = faster stop)
const VELOCITY_SCALE = 1.8; // Amplify velocity for more responsive feel
const MIN_VELOCITY = 0.5; // Stop animation when velocity drops below this
const RUBBER_BAND_FACTOR = 0.3; // Resistance when scrolling past bounds
const SNAP_BACK_DURATION = 300; // ms to snap back from overscroll
const TAP_THRESHOLD_MS = 150; // Max duration to consider a touch a tap
const TAP_MOVE_THRESHOLD = 10; // Max movement to consider a touch a tap

interface TouchState {
  startY: number;
  startX: number;
  startTime: number;
  lastY: number;
  lastTime: number;
  velocityY: number;
  isScrolling: boolean;
  startScrollTop: number;
}

function createTouchScroller(getViewport: () => HTMLElement | null) {
  let touchState: TouchState | null = null;
  let animationFrame: number | null = null;
  let snapBackAnimation: number | null = null;

  function getScrollBounds(viewport: HTMLElement) {
    const maxScroll = viewport.scrollHeight - viewport.clientHeight;
    return { min: 0, max: Math.max(0, maxScroll) };
  }

  function handleTouchStart(e: TouchEvent) {
    const viewport = getViewport();
    if (!viewport) return;

    // Cancel any ongoing animations
    if (animationFrame) {
      cancelAnimationFrame(animationFrame);
      animationFrame = null;
    }
    if (snapBackAnimation) {
      cancelAnimationFrame(snapBackAnimation);
      snapBackAnimation = null;
    }

    const touch = e.touches[0];
    touchState = {
      startY: touch.clientY,
      startX: touch.clientX,
      startTime: Date.now(),
      lastY: touch.clientY,
      lastTime: Date.now(),
      velocityY: 0,
      isScrolling: false,
      startScrollTop: viewport.scrollTop,
    };
  }

  function handleTouchMove(e: TouchEvent) {
    if (!touchState) return;
    const viewport = getViewport();
    if (!viewport) return;

    const touch = e.touches[0];
    const deltaY = touchState.lastY - touch.clientY;
    const deltaX = touch.clientX - touchState.startX;
    const now = Date.now();
    const timeDelta = Math.max(1, now - touchState.lastTime);

    // Determine if this is primarily a vertical scroll gesture
    const totalDeltaY = Math.abs(touch.clientY - touchState.startY);
    const totalDeltaX = Math.abs(deltaX);

    // If horizontal movement dominates, let the system handle it
    if (!touchState.isScrolling && totalDeltaX > totalDeltaY && totalDeltaX > TAP_MOVE_THRESHOLD) {
      touchState = null;
      return;
    }

    // Start scrolling if we've moved enough vertically
    if (!touchState.isScrolling && totalDeltaY > TAP_MOVE_THRESHOLD) {
      touchState.isScrolling = true;
    }

    if (touchState.isScrolling) {
      // Prevent default to stop the page from scrolling
      e.preventDefault();

      // Calculate instantaneous velocity (pixels per ms)
      const instantVelocity = deltaY / timeDelta;
      // Smooth velocity using exponential moving average
      touchState.velocityY = 0.7 * instantVelocity + 0.3 * touchState.velocityY;

      // Get scroll bounds
      const bounds = getScrollBounds(viewport);
      const newScrollTop = viewport.scrollTop + deltaY;

      // Apply rubber band effect when scrolling past bounds
      if (newScrollTop < bounds.min) {
        const overscroll = bounds.min - newScrollTop;
        viewport.scrollTop = bounds.min - overscroll * RUBBER_BAND_FACTOR;
      } else if (newScrollTop > bounds.max) {
        const overscroll = newScrollTop - bounds.max;
        viewport.scrollTop = bounds.max + overscroll * RUBBER_BAND_FACTOR;
      } else {
        viewport.scrollTop = newScrollTop;
      }
    }

    touchState.lastY = touch.clientY;
    touchState.lastTime = now;
  }

  function handleTouchEnd(e: TouchEvent) {
    if (!touchState) return;
    const viewport = getViewport();
    if (!viewport) return;

    const state = touchState;
    touchState = null;

    // Check if this was a tap (short duration, minimal movement)
    const duration = Date.now() - state.startTime;
    const moved = Math.abs(state.lastY - state.startY);
    if (duration < TAP_THRESHOLD_MS && moved < TAP_MOVE_THRESHOLD) {
      // This was a tap, don't interfere
      return;
    }

    if (!state.isScrolling) return;

    const bounds = getScrollBounds(viewport);
    const currentScroll = viewport.scrollTop;

    // If we're outside bounds, snap back
    if (currentScroll < bounds.min || currentScroll > bounds.max) {
      snapBack(viewport, bounds);
      return;
    }

    // Apply momentum scrolling
    let velocity = state.velocityY * VELOCITY_SCALE * 16; // Convert to pixels per frame (~16ms)

    if (Math.abs(velocity) < MIN_VELOCITY) return;

    function momentumStep() {
      const bounds = getScrollBounds(viewport);
      const currentScroll = viewport.scrollTop;

      // Check if we've hit bounds - just stop, don't snap back
      // (snap back is only for recovering from manual overscroll, not momentum)
      if (currentScroll <= bounds.min && velocity < 0) {
        viewport.scrollTop = bounds.min;
        animationFrame = null;
        return;
      }
      if (currentScroll >= bounds.max && velocity > 0) {
        viewport.scrollTop = bounds.max;
        animationFrame = null;
        return;
      }

      // Apply velocity
      viewport.scrollTop = currentScroll + velocity;

      // Apply deceleration
      velocity *= SCROLL_DECELERATION;

      // Continue or stop
      if (Math.abs(velocity) > MIN_VELOCITY) {
        animationFrame = requestAnimationFrame(momentumStep);
      } else {
        animationFrame = null;
      }
    }

    animationFrame = requestAnimationFrame(momentumStep);
  }

  function snapBack(viewport: HTMLElement, bounds: { min: number; max: number }) {
    const startScroll = viewport.scrollTop;
    // Determine which bound we overshot
    const targetScroll = startScroll < bounds.min ? bounds.min
                       : startScroll > bounds.max ? bounds.max
                       : startScroll; // Already in bounds, no-op
    const startTime = Date.now();

    function snapStep() {
      const elapsed = Date.now() - startTime;
      const progress = Math.min(1, elapsed / SNAP_BACK_DURATION);
      // Ease out cubic
      const eased = 1 - Math.pow(1 - progress, 3);

      viewport.scrollTop = startScroll + (targetScroll - startScroll) * eased;

      if (progress < 1) {
        snapBackAnimation = requestAnimationFrame(snapStep);
      } else {
        snapBackAnimation = null;
      }
    }

    snapBackAnimation = requestAnimationFrame(snapStep);
  }

  function cleanup() {
    if (animationFrame) cancelAnimationFrame(animationFrame);
    if (snapBackAnimation) cancelAnimationFrame(snapBackAnimation);
  }

  return {
    handleTouchStart,
    handleTouchMove,
    handleTouchEnd,
    cleanup,
  };
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
  let touchScrollerCleanup: (() => void) | undefined;

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

    // Set up custom touch scrolling for smooth iOS experience
    const getViewport = () => containerRef?.querySelector('.xterm-viewport') as HTMLElement | null;
    const touchScroller = createTouchScroller(getViewport);

    // Attach touch handlers to the container (captures events before xterm)
    containerRef!.addEventListener('touchstart', touchScroller.handleTouchStart, { passive: true });
    containerRef!.addEventListener('touchmove', touchScroller.handleTouchMove, { passive: false });
    containerRef!.addEventListener('touchend', touchScroller.handleTouchEnd, { passive: true });
    touchScrollerCleanup = () => {
      containerRef?.removeEventListener('touchstart', touchScroller.handleTouchStart);
      containerRef?.removeEventListener('touchmove', touchScroller.handleTouchMove);
      containerRef?.removeEventListener('touchend', touchScroller.handleTouchEnd);
      touchScroller.cleanup();
    };

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
      touchScrollerCleanup?.();
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
