// iOS Virtual Keyboard Handler
// Uses visualViewport API to detect keyboard and adjust layout

import { createSignal, onMount, onCleanup, Accessor } from 'solid-js';
import { isIOS } from './fonts';

export interface UseKeyboardOptions {
  onShow?: (height: number) => void;
  onHide?: () => void;
  debounceMs?: number;
}

export interface UseKeyboardResult {
  isVisible: Accessor<boolean>;
  keyboardHeight: Accessor<number>;
  viewportHeight: Accessor<number>;
}

/**
 * Reactive hook for iOS keyboard visibility and height.
 * Uses visualViewport API to detect when the virtual keyboard appears.
 * Only active on iOS - returns static values on other platforms.
 */
export function useKeyboard(options: UseKeyboardOptions = {}): UseKeyboardResult {
  const { onShow, onHide, debounceMs = 16 } = options;

  const [isVisible, setIsVisible] = createSignal(false);
  const [keyboardHeight, setKeyboardHeight] = createSignal(0);
  const [viewportHeight, setViewportHeight] = createSignal(
    typeof window !== 'undefined' ? window.innerHeight : 800
  );

  // Track stable height (viewport without keyboard)
  let stableViewportHeight = typeof window !== 'undefined' ? window.innerHeight : 800;
  let debounceTimer: number | null = null;

  onMount(() => {
    // Only apply on iOS - other platforms handle keyboard natively
    if (!isIOS()) {
      return;
    }

    const visualViewport = window.visualViewport;
    if (!visualViewport) {
      console.warn('[keyboard] visualViewport not supported');
      return;
    }

    // Initialize with current viewport height
    stableViewportHeight = visualViewport.height;
    setViewportHeight(visualViewport.height);

    function handleViewportChange() {
      if (!visualViewport) return;

      // Debounce rapid resize events during keyboard animation
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
      }

      debounceTimer = window.setTimeout(() => {
        const currentHeight = visualViewport.height;
        const heightDiff = stableViewportHeight - currentHeight;

        // Threshold: Keyboard is visible if viewport shrunk by > 150px
        // This filters out address bar changes (~50px) while catching keyboard (~300-400px)
        const KEYBOARD_THRESHOLD = 150;
        const keyboardNowVisible = heightDiff > KEYBOARD_THRESHOLD;

        setViewportHeight(currentHeight);

        if (keyboardNowVisible && !isVisible()) {
          // Keyboard just appeared
          setKeyboardHeight(heightDiff);
          setIsVisible(true);
          onShow?.(heightDiff);
        } else if (!keyboardNowVisible && isVisible()) {
          // Keyboard just dismissed - update stable height
          stableViewportHeight = currentHeight;
          setKeyboardHeight(0);
          setIsVisible(false);
          onHide?.();
        } else if (keyboardNowVisible) {
          // Keyboard still visible but height changed (orientation?)
          setKeyboardHeight(heightDiff);
        }
      }, debounceMs);
    }

    // Handle orientation changes - reset stable height when keyboard is hidden
    function handleOrientationChange() {
      setTimeout(() => {
        if (visualViewport && !isVisible()) {
          stableViewportHeight = visualViewport.height;
          setViewportHeight(visualViewport.height);
        }
      }, 300);
    }

    visualViewport.addEventListener('resize', handleViewportChange);
    visualViewport.addEventListener('scroll', handleViewportChange);
    window.addEventListener('orientationchange', handleOrientationChange);

    onCleanup(() => {
      visualViewport.removeEventListener('resize', handleViewportChange);
      visualViewport.removeEventListener('scroll', handleViewportChange);
      window.removeEventListener('orientationchange', handleOrientationChange);
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
      }
    });
  });

  return { isVisible, keyboardHeight, viewportHeight };
}
