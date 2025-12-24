// iOS Virtual Keyboard Handler
// Uses visualViewport API to detect keyboard and adjust layout
// Follows visualViewport in real-time (no additional animation) for sync with iOS keyboard

import { createSignal, onMount, onCleanup, Accessor } from 'solid-js';
import { isIOS } from './fonts';

export interface UseKeyboardOptions {
  /** Called when keyboard visibility changes (before height updates) */
  onBeforeShow?: () => void;
  onBeforeHide?: () => void;
  /** Called after keyboard transition completes */
  onShow?: () => void;
  onHide?: () => void;
}

export interface UseKeyboardResult {
  isVisible: Accessor<boolean>;
  keyboardHeight: Accessor<number>;
  /** Current viewport height - follows visualViewport in real-time */
  viewportHeight: Accessor<number>;
}

/**
 * Reactive hook for iOS keyboard visibility and height.
 * Follows visualViewport in real-time (no additional animation).
 * The iOS keyboard itself provides the animation - we just sync with it.
 */
export function useKeyboard(options: UseKeyboardOptions = {}): UseKeyboardResult {
  const { onBeforeShow, onBeforeHide, onShow, onHide } = options;

  const initialHeight = typeof window !== 'undefined' ? window.innerHeight : 800;

  const [isVisible, setIsVisible] = createSignal(false);
  const [keyboardHeight, setKeyboardHeight] = createSignal(0);
  const [viewportHeight, setViewportHeight] = createSignal(initialHeight);

  // Track stable height (viewport without keyboard) for threshold calculation
  let stableViewportHeight = initialHeight;
  // Track if we've fired the "complete" callback
  let transitionCompleteTimer: number | null = null;

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

      const newHeight = visualViewport.height;
      const heightDiff = stableViewportHeight - newHeight;

      // Update height immediately - follow the iOS keyboard in real-time
      setViewportHeight(newHeight);

      // Threshold: Keyboard is visible if viewport shrunk by > 150px
      const KEYBOARD_THRESHOLD = 150;
      const keyboardNowVisible = heightDiff > KEYBOARD_THRESHOLD;

      // Detect keyboard state changes
      if (keyboardNowVisible && !isVisible()) {
        // Keyboard just appeared
        onBeforeShow?.();
        setKeyboardHeight(heightDiff);
        setIsVisible(true);

        // Fire onShow after keyboard animation settles (debounced)
        if (transitionCompleteTimer) clearTimeout(transitionCompleteTimer);
        transitionCompleteTimer = window.setTimeout(() => {
          onShow?.();
        }, 100);
      } else if (!keyboardNowVisible && isVisible()) {
        // Keyboard just dismissed
        onBeforeHide?.();
        stableViewportHeight = newHeight;
        setKeyboardHeight(0);
        setIsVisible(false);

        // Fire onHide after keyboard animation settles
        if (transitionCompleteTimer) clearTimeout(transitionCompleteTimer);
        transitionCompleteTimer = window.setTimeout(() => {
          onHide?.();
        }, 100);
      } else if (keyboardNowVisible) {
        // Keyboard still visible but height changed
        setKeyboardHeight(heightDiff);
      }
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

    // Listen to visualViewport changes - this fires continuously during keyboard animation
    visualViewport.addEventListener('resize', handleViewportChange);
    visualViewport.addEventListener('scroll', handleViewportChange);
    window.addEventListener('orientationchange', handleOrientationChange);

    onCleanup(() => {
      visualViewport.removeEventListener('resize', handleViewportChange);
      visualViewport.removeEventListener('scroll', handleViewportChange);
      window.removeEventListener('orientationchange', handleOrientationChange);
      if (transitionCompleteTimer) clearTimeout(transitionCompleteTimer);
    });
  });

  return { isVisible, keyboardHeight, viewportHeight };
}
