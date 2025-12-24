import { onMount, onCleanup } from 'solid-js';

/**
 * Prevents iOS PWA viewport rubber-banding/overscroll when there's no
 * scrollable content. Intercepts touchmove events and only allows them
 * when the touch is inside a legitimately scrollable container.
 */
export function usePreventOverscroll() {
  onMount(() => {
    let startY = 0;

    function handleTouchStart(e: TouchEvent) {
      startY = e.touches[0].clientY;
    }

    function handleTouchMove(e: TouchEvent) {
      const currentY = e.touches[0].clientY;
      const deltaY = currentY - startY;

      let target = e.target as HTMLElement | null;

      // Walk up the DOM tree to find a scrollable container
      while (target && target !== document.body && target !== document.documentElement) {
        const style = getComputedStyle(target);
        const overflowY = style.overflowY;
        const isScrollable = overflowY === 'auto' || overflowY === 'scroll';
        const hasScrollableContent = target.scrollHeight > target.clientHeight;

        if (isScrollable && hasScrollableContent) {
          const { scrollTop, scrollHeight, clientHeight } = target;
          const atTop = scrollTop <= 0;
          const atBottom = scrollTop + clientHeight >= scrollHeight;

          // Allow scroll if we're not at the bounds, or if we're scrolling
          // in a direction that would move content (not escape the container)
          if (deltaY > 0 && !atTop) {
            // Scrolling down and not at top - allow
            return;
          }
          if (deltaY < 0 && !atBottom) {
            // Scrolling up and not at bottom - allow
            return;
          }
          if (!atTop && !atBottom) {
            // In the middle - allow any scroll
            return;
          }
          // At bounds and trying to scroll past - fall through to prevent
          break;
        }

        target = target.parentElement;
      }

      // No valid scrollable container found, or at scroll bounds - prevent body scroll
      e.preventDefault();
    }

    document.addEventListener('touchstart', handleTouchStart, { passive: true });
    document.addEventListener('touchmove', handleTouchMove, { passive: false });

    onCleanup(() => {
      document.removeEventListener('touchstart', handleTouchStart);
      document.removeEventListener('touchmove', handleTouchMove);
    });
  });
}
