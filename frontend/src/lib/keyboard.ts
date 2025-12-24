// iOS Virtual Keyboard Handler
// Uses visualViewport API to detect keyboard and adjust layout

import { createSignal, onMount, onCleanup, Accessor } from 'solid-js';
import { isIOS } from './fonts';

export interface UseKeyboardOptions {
  /** Called BEFORE state changes when keyboard is about to show */
  onBeforeShow?: () => void;
  /** Called AFTER state changes when keyboard has shown */
  onShow?: (targetHeight: number) => void;
  /** Called BEFORE state changes when keyboard is about to hide */
  onBeforeHide?: () => void;
  /** Called AFTER state changes when keyboard has hidden */
  onHide?: (targetHeight: number) => void;
  debounceMs?: number;
}

export interface UseKeyboardResult {
  isVisible: Accessor<boolean>;
  keyboardHeight: Accessor<number>;
  /** The target viewport height (what we should animate to) */
  targetHeight: Accessor<number>;
  /** The current animated height (smoothly transitions to targetHeight) */
  animatedHeight: Accessor<number>;
}

/**
 * Reactive hook for iOS keyboard visibility and height.
 * Uses visualViewport API to detect when the virtual keyboard appears.
 * Provides both target height (for immediate use) and animated height (for smooth transitions).
 * Only active on iOS - returns static values on other platforms.
 */
export function useKeyboard(options: UseKeyboardOptions = {}): UseKeyboardResult {
  const { onBeforeShow, onShow, onBeforeHide, onHide, debounceMs = 16 } = options;

  const initialHeight = typeof window !== 'undefined' ? window.innerHeight : 800;

  const [isVisible, setIsVisible] = createSignal(false);
  const [keyboardHeight, setKeyboardHeight] = createSignal(0);
  const [targetHeight, setTargetHeight] = createSignal(initialHeight);
  const [animatedHeight, setAnimatedHeight] = createSignal(initialHeight);

  // Track stable height (viewport without keyboard)
  let stableViewportHeight = initialHeight;
  let debounceTimer: number | null = null;
  let animationFrame: number | null = null;

  // Animation configuration
  const ANIMATION_DURATION = 250; // ms

  /**
   * Smoothly animate height from current to target while calling onFrame for scroll adjustment.
   */
  function animateToHeight(
    fromHeight: number,
    toHeight: number,
    onFrame?: (currentHeight: number, progress: number) => void,
    onComplete?: () => void
  ) {
    // Cancel any existing animation
    if (animationFrame !== null) {
      cancelAnimationFrame(animationFrame);
    }

    const startTime = performance.now();

    function animate(now: number) {
      const elapsed = now - startTime;
      const progress = Math.min(1, elapsed / ANIMATION_DURATION);
      // Ease-out cubic for smooth deceleration
      const eased = 1 - Math.pow(1 - progress, 3);

      const currentHeight = fromHeight + (toHeight - fromHeight) * eased;
      setAnimatedHeight(currentHeight);

      // Callback for scroll adjustment at each frame
      onFrame?.(currentHeight, progress);

      if (progress < 1) {
        animationFrame = requestAnimationFrame(animate);
      } else {
        animationFrame = null;
        onComplete?.();
      }
    }

    animationFrame = requestAnimationFrame(animate);
  }

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
    setTargetHeight(visualViewport.height);
    setAnimatedHeight(visualViewport.height);

    function handleViewportChange() {
      if (!visualViewport) return;

      // Debounce rapid resize events during keyboard animation
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
      }

      debounceTimer = window.setTimeout(() => {
        const newTargetHeight = visualViewport.height;
        const heightDiff = stableViewportHeight - newTargetHeight;

        // Threshold: Keyboard is visible if viewport shrunk by > 150px
        const KEYBOARD_THRESHOLD = 150;
        const keyboardNowVisible = heightDiff > KEYBOARD_THRESHOLD;

        const currentAnimatedHeight = animatedHeight();

        if (keyboardNowVisible && !isVisible()) {
          // Keyboard just appeared
          onBeforeShow?.(); // Call BEFORE state changes

          setKeyboardHeight(heightDiff);
          setIsVisible(true);
          setTargetHeight(newTargetHeight);

          // Start animation and call onShow with target height
          animateToHeight(currentAnimatedHeight, newTargetHeight, undefined, () => {
            onShow?.(newTargetHeight);
          });
        } else if (!keyboardNowVisible && isVisible()) {
          // Keyboard just dismissed
          onBeforeHide?.(); // Call BEFORE state changes

          stableViewportHeight = newTargetHeight;
          setKeyboardHeight(0);
          setIsVisible(false);
          setTargetHeight(newTargetHeight);

          // Start animation and call onHide with target height
          animateToHeight(currentAnimatedHeight, newTargetHeight, undefined, () => {
            onHide?.(newTargetHeight);
          });
        } else if (keyboardNowVisible) {
          // Keyboard still visible but height changed (orientation?)
          setKeyboardHeight(heightDiff);
          setTargetHeight(newTargetHeight);
          animateToHeight(currentAnimatedHeight, newTargetHeight);
        } else if (Math.abs(newTargetHeight - currentAnimatedHeight) > 5) {
          // Height changed without keyboard (orientation change?)
          setTargetHeight(newTargetHeight);
          animateToHeight(currentAnimatedHeight, newTargetHeight);
        }
      }, debounceMs);
    }

    // Handle orientation changes - reset stable height when keyboard is hidden
    function handleOrientationChange() {
      setTimeout(() => {
        if (visualViewport && !isVisible()) {
          stableViewportHeight = visualViewport.height;
          setTargetHeight(visualViewport.height);
          setAnimatedHeight(visualViewport.height);
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
      if (animationFrame !== null) {
        cancelAnimationFrame(animationFrame);
      }
    });
  });

  return { isVisible, keyboardHeight, targetHeight, animatedHeight };
}
