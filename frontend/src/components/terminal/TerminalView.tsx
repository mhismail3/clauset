import { onMount, onCleanup, createSignal, createEffect, For } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { loadTerminalFont, getRecommendedFontSize, isIOS } from '../../lib/fonts';
import { calculateDimensions, getDeviceHint, type ConfidenceLevel } from '../../lib/terminalSizing';
import { useKeyboard } from '../../lib/keyboard';

interface TerminalViewProps {
  onInput: (data: Uint8Array) => void;
  onResize: (cols: number, rows: number) => void;
  onClose: () => void;
  onReady?: (write: (data: Uint8Array) => void) => void;
  onNegotiateDimensions?: (params: {
    cols: number;
    rows: number;
    confidence: ConfidenceLevel;
    source: 'fitaddon' | 'container' | 'estimation' | 'defaults';
    cellWidth?: number;
    fontLoaded: boolean;
    deviceHint: 'iphone' | 'ipad' | 'desktop';
  }) => void;
  isConnected?: boolean;
  isVisible?: boolean;
}

// Touch scroll physics configuration
const SCROLL_DECELERATION = 0.94; // Friction coefficient (lower = faster stop)
const VELOCITY_SCALE = 2.2; // Amplify velocity for more responsive feel
const MIN_VELOCITY = 0.3; // Stop animation when velocity drops below this
const RUBBER_BAND_FACTOR = 0.3; // Resistance when scrolling past bounds
const SNAP_BACK_DURATION = 250; // ms to snap back from overscroll
const TAP_THRESHOLD_MS = 120; // Max duration to consider a touch a tap
const SCROLL_LOCK_THRESHOLD = 4; // Pixels moved before we decide scroll vs tap
const DIRECTION_LOCK_THRESHOLD = 6; // Pixels before we lock scroll direction
const VELOCITY_SAMPLE_COUNT = 5; // Number of velocity samples to track

interface VelocitySample {
  velocity: number;
  time: number;
}

interface TouchState {
  startY: number;
  startX: number;
  startTime: number;
  lastY: number;
  lastTime: number;
  velocitySamples: VelocitySample[];
  isScrolling: boolean;
  directionLocked: boolean;
  startScrollTop: number;
  scrolledDistance: number; // Track how much we've actually scrolled
}

function createTouchScroller(getViewport: () => HTMLElement | null) {
  let touchState: TouchState | null = null;
  let animationFrame: number | null = null;
  let snapBackAnimation: number | null = null;

  function getScrollBounds(viewport: HTMLElement) {
    const maxScroll = viewport.scrollHeight - viewport.clientHeight;
    return { min: 0, max: Math.max(0, maxScroll) };
  }

  // Get the best velocity estimate from recent samples
  function getBestVelocity(samples: VelocitySample[], fallbackVelocity: number): number {
    if (samples.length === 0) return fallbackVelocity;

    // Use samples from the last 100ms for better accuracy
    const now = Date.now();
    const recentSamples = samples.filter(s => now - s.time < 100);

    if (recentSamples.length === 0) {
      // Fall back to all samples if none are recent enough
      return samples.reduce((max, s) => Math.abs(s.velocity) > Math.abs(max) ? s.velocity : max, 0);
    }

    // Weight recent samples higher and find the one with maximum magnitude
    // This captures the "flick" velocity better
    let bestVelocity = 0;
    let bestWeight = 0;

    for (const sample of recentSamples) {
      const age = now - sample.time;
      const weight = 1 - (age / 100); // More recent = higher weight
      const weightedMagnitude = Math.abs(sample.velocity) * weight;

      if (weightedMagnitude > bestWeight) {
        bestWeight = weightedMagnitude;
        bestVelocity = sample.velocity;
      }
    }

    return bestVelocity || fallbackVelocity;
  }

  function handleTouchStart(e: TouchEvent) {
    const viewport = getViewport();
    if (!viewport) return;

    // Cancel any ongoing animations immediately
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
      velocitySamples: [],
      isScrolling: false,
      directionLocked: false,
      startScrollTop: viewport.scrollTop,
      scrolledDistance: 0,
    };
  }

  function handleTouchMove(e: TouchEvent) {
    if (!touchState) return;
    const viewport = getViewport();
    if (!viewport) return;

    const touch = e.touches[0];
    const now = Date.now();
    const timeDelta = Math.max(1, now - touchState.lastTime);

    const deltaY = touchState.lastY - touch.clientY;
    const totalDeltaY = touch.clientY - touchState.startY;
    const totalDeltaX = touch.clientX - touchState.startX;
    const absTotalY = Math.abs(totalDeltaY);
    const absTotalX = Math.abs(totalDeltaX);

    // Direction locking: once we've moved enough, lock to vertical or horizontal
    if (!touchState.directionLocked && (absTotalY > DIRECTION_LOCK_THRESHOLD || absTotalX > DIRECTION_LOCK_THRESHOLD)) {
      touchState.directionLocked = true;

      // If horizontal movement dominates, abandon this touch for scrolling
      if (absTotalX > absTotalY) {
        touchState = null;
        return;
      }
    }

    // Start scrolling once we've moved past the small threshold
    // This is a very small threshold to feel immediate
    if (!touchState.isScrolling && absTotalY > SCROLL_LOCK_THRESHOLD) {
      touchState.isScrolling = true;

      // Compensate: apply the initial movement that got us here
      const initialDelta = -totalDeltaY; // Negative because scroll direction
      const bounds = getScrollBounds(viewport);
      const compensatedScroll = viewport.scrollTop + initialDelta;

      if (compensatedScroll >= bounds.min && compensatedScroll <= bounds.max) {
        viewport.scrollTop = compensatedScroll;
        touchState.scrolledDistance += Math.abs(initialDelta);
      }
    }

    if (touchState.isScrolling) {
      // Prevent default to stop the page from scrolling
      e.preventDefault();

      // Calculate and store velocity sample
      if (timeDelta > 0) {
        const instantVelocity = deltaY / timeDelta;
        touchState.velocitySamples.push({ velocity: instantVelocity, time: now });

        // Keep only recent samples
        if (touchState.velocitySamples.length > VELOCITY_SAMPLE_COUNT) {
          touchState.velocitySamples.shift();
        }
      }

      // Apply scroll
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

      touchState.scrolledDistance += Math.abs(deltaY);
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

    const duration = Date.now() - state.startTime;
    const totalMoved = Math.abs(state.lastY - state.startY);

    // Tap detection: very short duration AND minimal movement AND didn't scroll
    if (duration < TAP_THRESHOLD_MS && totalMoved < SCROLL_LOCK_THRESHOLD && !state.isScrolling) {
      // This was a tap, let it pass through
      return;
    }

    // If we scrolled, handle momentum
    if (state.isScrolling && state.scrolledDistance > 0) {
      const bounds = getScrollBounds(viewport);
      const currentScroll = viewport.scrollTop;

      // If we're outside bounds, snap back
      if (currentScroll < bounds.min || currentScroll > bounds.max) {
        snapBack(viewport, bounds);
        return;
      }

      // Calculate velocity for momentum
      // Use displacement/time as fallback for very short gestures
      const fallbackVelocity = totalMoved > 0 && duration > 0
        ? (-Math.sign(state.lastY - state.startY) * totalMoved / duration)
        : 0;

      const bestVelocity = getBestVelocity(state.velocitySamples, fallbackVelocity);
      let velocity = bestVelocity * VELOCITY_SCALE * 16; // Convert to pixels per frame

      // For short quick flicks, boost the velocity
      if (duration < 150 && totalMoved > 10) {
        velocity *= 1.3;
      }

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
// Order: / (commands), esc, tab, arrows, enter, ctrl (modifier last)
const SPECIAL_KEYS = [
  { label: '/', code: '/' },
  { label: 'esc', code: '\x1b' },
  { label: 'tab', code: '\t' },
  { label: '↑', code: '\x1b[A' },
  { label: '↓', code: '\x1b[B' },
  { label: '←', code: '\x1b[D' },
  { label: '→', code: '\x1b[C' },
  { label: 'enter', code: '\r' },
  { label: 'ctrl', code: null, isModifier: true },
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

  // Font loading state
  let fontLoaded = false;
  let charWidth: number | null = null;
  let charHeight: number | null = null;

  // Fixed font size based on device (no user adjustment)
  const fontSize = getRecommendedFontSize();
  const [ctrlActive, setCtrlActive] = createSignal(false);
  const [dimensions, setDimensions] = createSignal({ cols: 80, rows: 24 });

  // Track scroll position for keyboard transitions
  // We save the scroll offset in pixels from the bottom of the scrollable area
  let savedScrollState: {
    scrollTop: number;
    maxScroll: number;
    wasAtBottom: boolean;
  } | null = null;

  // Track if we're in a keyboard transition to prevent scroll jumping
  const [isKeyboardTransitioning, setIsKeyboardTransitioning] = createSignal(false);

  // Helper to get xterm viewport element
  function getViewport(): HTMLElement | null {
    return containerRef?.querySelector('.xterm-viewport') as HTMLElement | null;
  }

  // Check if we're currently at the bottom of the scroll
  function isAtBottom(): boolean {
    const viewport = getViewport();
    if (!viewport) return true;
    const maxScroll = viewport.scrollHeight - viewport.clientHeight;
    return maxScroll <= 0 || viewport.scrollTop >= maxScroll - 10;
  }

  // Scroll to bottom of terminal
  function scrollToBottom() {
    const viewport = getViewport();
    if (!viewport) return;
    viewport.scrollTop = viewport.scrollHeight - viewport.clientHeight;
  }

  // Save scroll position BEFORE any height changes
  function saveScrollPosition() {
    if (!terminal) return;
    const viewport = getViewport();
    if (!viewport) return;

    const maxScroll = viewport.scrollHeight - viewport.clientHeight;
    const currentScroll = viewport.scrollTop;
    // Consider "at bottom" if within 10px of the bottom
    const wasAtBottom = maxScroll <= 0 || currentScroll >= maxScroll - 10;

    savedScrollState = {
      scrollTop: currentScroll,
      maxScroll,
      wasAtBottom,
    };
  }

  // Restore scroll position AFTER terminal resize
  function restoreScrollPosition() {
    if (!terminal || !savedScrollState) return;
    const viewport = getViewport();
    if (!viewport) return;

    const newMaxScroll = viewport.scrollHeight - viewport.clientHeight;

    if (savedScrollState.wasAtBottom) {
      // If we were at the bottom, stay at the bottom
      viewport.scrollTop = newMaxScroll;
    } else {
      // Otherwise, maintain the same absolute scroll position
      // (same content at the top of the viewport)
      viewport.scrollTop = Math.min(savedScrollState.scrollTop, newMaxScroll);
    }

    savedScrollState = null;
  }

  // Write data to terminal
  // Scroll handling is done by keyboard transition callbacks (saveScrollPosition/restoreScrollPosition)
  // We don't manipulate scroll here to avoid fighting with TUI applications like Claude Code's autocomplete
  function writeToTerminal(data: Uint8Array) {
    if (!terminal) return;
    terminal.write(data);
  }

  // iOS keyboard handling - follows visualViewport in real-time (no animation delay)
  const { isVisible: keyboardVisible, viewportHeight } = useKeyboard({
    // Save scroll position BEFORE keyboard state changes
    onBeforeShow: () => {
      setIsKeyboardTransitioning(true);
      saveScrollPosition();
    },
    onBeforeHide: () => {
      setIsKeyboardTransitioning(true);
      saveScrollPosition();
    },
    // Resize terminal and restore scroll AFTER keyboard animation settles
    onShow: () => {
      doFitAndResize();
      requestAnimationFrame(() => {
        restoreScrollPosition();
        // Clear transition flag after a delay to allow scroll to settle
        setTimeout(() => { setIsKeyboardTransitioning(false); }, 100);
      });
    },
    onHide: () => {
      doFitAndResize();
      requestAnimationFrame(() => {
        restoreScrollPosition();
        setTimeout(() => { setIsKeyboardTransitioning(false); }, 100);
      });
    },
  });

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
    // Don't focus terminal - this would bring up the keyboard on mobile
    // User can tap the terminal directly if they want to type
  }

  function sendCtrlKey(char: string) {
    const encoder = new TextEncoder();
    const charCode = char.toUpperCase().charCodeAt(0);
    const code = String.fromCharCode(charCode - 64);
    props.onInput(encoder.encode(code));
    setCtrlActive(false);
    // Don't focus terminal - this would bring up the keyboard on mobile
  }

  function doFitAndResize() {
    if (!terminal || !containerRef || !fitAddon) return;

    try {
      const containerWidth = containerRef.clientWidth;
      const containerHeight = containerRef.clientHeight;

      if (containerWidth === 0 || containerHeight === 0) {
        setTimeout(doFitAndResize, 50);
        return;
      }

      // Use FitAddon to calculate dimensions
      const proposed = fitAddon.proposeDimensions();
      if (!proposed || !proposed.cols || !proposed.rows) return;

      const cols = Math.max(20, proposed.cols);
      const rows = Math.max(5, proposed.rows);

      if (cols !== terminal.cols || rows !== terminal.rows) {
        terminal.resize(cols, rows);
      }

      const newDims = { cols: terminal.cols, rows: terminal.rows };
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
      fontSize: fontSize,
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

    terminal.onData((data) => {
      const encoder = new TextEncoder();
      props.onInput(encoder.encode(data));
    });

    // Watch for container resize
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(containerRef!);

    // Also listen to window resize for orientation changes
    window.addEventListener('resize', handleResize);

    // CRITICAL: Wait for fonts to load and calculate dimensions BEFORE signaling ready
    // This ensures the terminal is properly sized before any data is written
    const initializeTerminal = async () => {
      // Stage 1: Load fonts with robust timeout handling
      const fontResult = await loadTerminalFont({
        timeout: 2000,
        fontSize: fontSize,
      });

      fontLoaded = fontResult.loaded;
      charWidth = fontResult.charWidth;
      charHeight = fontResult.charHeight;

      // Update terminal font family if we got a different one
      if (terminal && fontResult.fontFamily !== terminal.options.fontFamily) {
        terminal.options.fontFamily = fontResult.fontFamily;
      }

      // Log font loading result for debugging
      console.log(`Font loaded: ${fontResult.loaded ? 'primary' : 'fallback'}, ` +
        `family=${fontResult.fontFamily}, ` +
        `char=${fontResult.charWidth.toFixed(1)}x${fontResult.charHeight.toFixed(1)}, ` +
        `time=${fontResult.loadTimeMs}ms`);

      // Wait for next animation frames to ensure layout is stable
      await new Promise<void>(resolve => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            resolve();
          });
        });
      });

      // Stage 2: Calculate dimensions using multi-stage strategy
      const dims = calculateDimensions({
        fitAddon: fitAddon ?? null,
        container: containerRef ?? null,
        charWidth,
        charHeight,
        fontLoaded,
        fontSize: fontSize,
      });

      console.log(`Dimensions calculated: ${dims.cols}x${dims.rows}, ` +
        `confidence=${dims.confidence}, source=${dims.source}`);

      // Apply dimensions to terminal (local only)
      if (terminal && (dims.cols !== terminal.cols || dims.rows !== terminal.rows)) {
        terminal.resize(dims.cols, dims.rows);
      }

      setDimensions({ cols: dims.cols, rows: dims.rows });

      // Stage 3: Negotiate dimensions with server (if callback provided)
      // IMPORTANT: Only send dimensions to server if container is visible
      // If hidden, the isVisible createEffect will send correct dimensions later
      const containerWidth = containerRef?.clientWidth ?? 0;
      const containerHeight = containerRef?.clientHeight ?? 0;
      const isContainerVisible = containerWidth > 0 && containerHeight > 0;

      if (isContainerVisible) {
        if (props.onNegotiateDimensions) {
          props.onNegotiateDimensions({
            cols: dims.cols,
            rows: dims.rows,
            confidence: dims.confidence,
            source: dims.source,
            cellWidth: dims.cellWidth,
            fontLoaded,
            deviceHint: getDeviceHint(),
          });
        } else {
          // Fall back to simple resize callback
          props.onResize(dims.cols, dims.rows);
        }
      } else {
        console.log('Container hidden during init, deferring dimension sync until visible');
      }

      // NOW signal that we're ready to receive data
      // Use writeToTerminal to preserve scroll position during keyboard transitions
      if (props.onReady) {
        props.onReady((data: Uint8Array) => {
          writeToTerminal(data);
        });
      }
    };

    // Start initialization
    initializeTerminal();

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

  // Re-fit when terminal becomes visible (e.g., switching from chat tab)
  // This fixes the issue where terminal mounts with display:none and gets wrong dimensions
  createEffect(() => {
    if (props.isVisible) {
      // Use double RAF to ensure layout is fully computed after display:none → flex
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          doFitAndResize();
        });
      });
    }
  });

  return (
    <div
      style={{
        display: 'flex',
        "flex-direction": 'column',
        // On iOS, follow visualViewport height in real-time to sync with keyboard
        height: isIOS() ? `${viewportHeight()}px` : '100%',
        width: '100%',
        background: '#0d0d0d',
        overflow: 'hidden',
        // Hint to browser about upcoming changes to reduce flicker
        "will-change": isKeyboardTransitioning() ? 'height' : 'auto',
        // Contain layout recalculations to this element
        contain: 'layout size',
      }}
    >
      {/* Terminal area with padding for visual spacing */}
      <div
        style={{
          flex: '1 1 0',
          "min-height": '0',
          padding: '8px 12px',
          overflow: 'hidden',
        }}
      >
        {/* Inner container - xterm attaches here, FitAddon measures this */}
        <div
          ref={containerRef}
          style={{
            height: '100%',
            width: '100%',
          }}
        />
      </div>

      {/* Special Keys Toolbar */}
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
                onMouseDown={(e) => e.preventDefault()}
                onTouchStart={(e) => e.preventDefault()}
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
            // Reduce bottom padding when keyboard visible (no home indicator needed)
            "padding-bottom": keyboardVisible()
              ? '10px'
              : 'calc(max(env(safe-area-inset-bottom, 0px), 12px) + 16px)',
            // Explicit overflow for scrollability
            "overflow-x": 'auto',
            "overflow-y": 'hidden',
            "-webkit-overflow-scrolling": 'touch',
          }}
        >
          <For each={SPECIAL_KEYS}>
            {(key) => (
              <button
                onMouseDown={(e) => e.preventDefault()}
                onTouchStart={(e) => e.preventDefault()}
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
        </div>
      </div>
    </div>
  );
}
