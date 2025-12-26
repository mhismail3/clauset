import { onMount, onCleanup } from 'solid-js';

/**
 * Prevents iOS PWA viewport rubber-banding/overscroll when there's no
 * scrollable content. Intercepts touchmove events and only allows them
 * when the touch is inside a legitimately scrollable container.
 */
export function usePreventOverscroll() {
  onMount(() => {
    let startX = 0;
    let startY = 0;

    function handleTouchStart(e: TouchEvent) {
      startX = e.touches[0].clientX;
      startY = e.touches[0].clientY;
    }

    function handleTouchMove(e: TouchEvent) {
      const currentX = e.touches[0].clientX;
      const currentY = e.touches[0].clientY;
      const deltaX = currentX - startX;
      const deltaY = currentY - startY;

      // Determine primary scroll direction
      const isHorizontalScroll = Math.abs(deltaX) > Math.abs(deltaY);

      let target = e.target as HTMLElement | null;

      // Walk up the DOM tree to find a scrollable container
      while (target && target !== document.body && target !== document.documentElement) {
        const style = getComputedStyle(target);

        if (isHorizontalScroll) {
          // Check for horizontal scrollability
          const overflowX = style.overflowX;
          const isScrollableX = overflowX === 'auto' || overflowX === 'scroll';
          const hasScrollableContentX = target.scrollWidth > target.clientWidth;

          if (isScrollableX && hasScrollableContentX) {
            const { scrollLeft, scrollWidth, clientWidth } = target;
            const atLeft = scrollLeft <= 0;
            const atRight = scrollLeft + clientWidth >= scrollWidth;

            // Allow horizontal scroll if not at bounds
            if (deltaX > 0 && !atLeft) {
              return; // Scrolling right and not at left edge
            }
            if (deltaX < 0 && !atRight) {
              return; // Scrolling left and not at right edge
            }
            if (!atLeft && !atRight) {
              return; // In the middle - allow any scroll
            }
            // At bounds - fall through to prevent
            break;
          }
        } else {
          // Check for vertical scrollability
          const overflowY = style.overflowY;
          const isScrollableY = overflowY === 'auto' || overflowY === 'scroll';
          const hasScrollableContentY = target.scrollHeight > target.clientHeight;

          if (isScrollableY && hasScrollableContentY) {
            const { scrollTop, scrollHeight, clientHeight } = target;
            const atTop = scrollTop <= 0;
            const atBottom = scrollTop + clientHeight >= scrollHeight;

            // Allow scroll if we're not at the bounds, or if we're scrolling
            // in a direction that would move content (not escape the container)
            if (deltaY > 0 && !atTop) {
              return; // Scrolling down and not at top
            }
            if (deltaY < 0 && !atBottom) {
              return; // Scrolling up and not at bottom
            }
            if (!atTop && !atBottom) {
              return; // In the middle - allow any scroll
            }
            // At bounds and trying to scroll past - fall through to prevent
            break;
          }
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
