# Terminal Right-Column Clipping Issue - RESOLVED

## Problem Description
On mobile screen sizes, the rightmost column of terminal text was being clipped/not displayed.
The terminal uses xterm.js with FitAddon for auto-sizing.

## Root Cause (FOUND)
**xterm.js sets `overflow: hidden` as an inline style on each row div inside `.xterm-rows`.**

When xterm's internal cell width calculation is slightly off from the actual rendered character width, the row div clips the rightmost characters. This is because:
1. xterm calculates row width = cols × actualCellWidth
2. But actualCellWidth measurement can be slightly smaller than true rendered width
3. Each row div gets `overflow: hidden`, clipping any overflow

## The Fix

### CSS Override (index.css)
```css
/* Fix xterm row clipping - xterm.js sets overflow:hidden on each row div */
.xterm .xterm-rows > div {
  overflow: visible !important;
}
```

This allows the row content to extend slightly beyond the calculated width without clipping.

### Proper Container Structure (TerminalView.tsx)
Use nested containers for proper padding:
```tsx
{/* Outer container with padding for visual spacing */}
<div style={{ padding: '8px 12px' }}>
  {/* Inner container - FitAddon measures this */}
  <div ref={containerRef} style={{ height: '100%', width: '100%' }}>
    {/* xterm attaches here */}
  </div>
</div>
```

This ensures:
- Padding provides visual spacing around the terminal
- FitAddon measures the inner container (actual terminal area)
- No mismatch between measured and rendered dimensions

## What Was Tried (Before Finding Root Cause)

These approaches did NOT fix the issue because they addressed the wrong problem:

1. **Column Safety Margins** (-1, -2 columns) - Reduced terminal width but didn't address row clipping
2. **Container Padding Changes** - Changed measurements but xterm rows still clipped
3. **Server-Side Buffer Protocol** - Useful for buffer replay, but not related to clipping
4. **CSS width constraints on .xterm-screen** - Didn't help because clipping was on row divs

## Key Learnings

1. **Inspect the actual clipping element** - The issue was at the row level, not the container level
2. **xterm.js internal styles matter** - Inline styles can override CSS and cause unexpected behavior
3. **Browser DevTools are essential** - Highlighting elements revealed the exact clipping boundary
