// Multi-stage terminal dimension calculation
// Phase 3.2: Ensures correct dimensions even with slow font loading

import type { FitAddon } from '@xterm/addon-fit';
import { isIOS, isPWA } from './fonts';

export type ConfidenceLevel = 'high' | 'medium' | 'low';

export interface TerminalDimensions {
  cols: number;
  rows: number;
  confidence: ConfidenceLevel;
  source: 'fitaddon' | 'container' | 'estimation' | 'defaults';
  cellWidth?: number;
  cellHeight?: number;
}

export interface DimensionContext {
  fitAddon: FitAddon | null;
  container: HTMLElement | null;
  charWidth: number | null;
  charHeight: number | null;
  fontLoaded: boolean;
  fontSize: number;
}

// Minimum and maximum acceptable dimensions
const MIN_COLS = 20;
const MAX_COLS = 300;
const MIN_ROWS = 5;
const MAX_ROWS = 100;

// Device-specific safe defaults
const IPHONE_SAFE_DEFAULTS = { cols: 40, rows: 20 };
const IPAD_SAFE_DEFAULTS = { cols: 80, rows: 30 };
const DESKTOP_SAFE_DEFAULTS = { cols: 80, rows: 24 };

/**
 * Calculate terminal dimensions using multi-stage strategy.
 * Returns dimensions with confidence level indicating reliability.
 */
export function calculateDimensions(context: DimensionContext): TerminalDimensions {
  // Stage 1: FitAddon (highest confidence if font loaded correctly)
  // IMPORTANT: Only trust fitAddon if container is actually visible (has dimensions)
  // fitAddon.proposeDimensions() returns minimum values (20x5) for hidden containers
  if (context.fontLoaded && context.fitAddon && context.container) {
    const containerWidth = context.container.clientWidth;
    const containerHeight = context.container.clientHeight;

    // Don't trust fitAddon if container is hidden (display:none makes clientWidth=0)
    if (containerWidth > 0 && containerHeight > 0) {
      try {
        const proposed = context.fitAddon.proposeDimensions();
        if (proposed && proposed.cols && proposed.rows) {
          const cols = clamp(proposed.cols, MIN_COLS, MAX_COLS);
          const rows = clamp(proposed.rows, MIN_ROWS, MAX_ROWS);

          return {
            cols,
            rows,
            confidence: 'high',
            source: 'fitaddon',
            cellWidth: context.charWidth ?? undefined,
            cellHeight: context.charHeight ?? undefined,
          };
        }
      } catch (e) {
        console.warn('FitAddon calculation failed:', e);
      }
    }
  }

  // Stage 2: Container measurement with known character dimensions
  if (context.container && context.charWidth && context.charHeight) {
    const containerWidth = context.container.clientWidth;
    const containerHeight = context.container.clientHeight;

    if (containerWidth > 0 && containerHeight > 0) {
      // Account for padding in container
      const effectiveWidth = containerWidth - 24; // 12px padding on each side
      const effectiveHeight = containerHeight - 16; // 8px padding top/bottom

      const cols = Math.floor(effectiveWidth / context.charWidth);
      const rows = Math.floor(effectiveHeight / context.charHeight);

      if (cols >= MIN_COLS && rows >= MIN_ROWS) {
        return {
          cols: clamp(cols, MIN_COLS, MAX_COLS),
          rows: clamp(rows, MIN_ROWS, MAX_ROWS),
          confidence: context.fontLoaded ? 'high' : 'medium',
          source: 'container',
          cellWidth: context.charWidth,
          cellHeight: context.charHeight,
        };
      }
    }
  }

  // Stage 3: Container estimation with default character dimensions
  if (context.container) {
    const containerWidth = context.container.clientWidth;
    const containerHeight = context.container.clientHeight;

    if (containerWidth > 0 && containerHeight > 0) {
      // Estimate character dimensions based on font size
      const estimatedCharWidth = context.fontSize * 0.6;
      const estimatedCharHeight = context.fontSize * 1.25;

      const effectiveWidth = containerWidth - 24;
      const effectiveHeight = containerHeight - 16;

      const cols = Math.floor(effectiveWidth / estimatedCharWidth);
      const rows = Math.floor(effectiveHeight / estimatedCharHeight);

      if (cols >= MIN_COLS && rows >= MIN_ROWS) {
        return {
          cols: clamp(cols, MIN_COLS, MAX_COLS),
          rows: clamp(rows, MIN_ROWS, MAX_ROWS),
          confidence: 'medium',
          source: 'estimation',
          cellWidth: estimatedCharWidth,
          cellHeight: estimatedCharHeight,
        };
      }
    }
  }

  // Stage 4: Device-specific safe defaults (lowest confidence)
  const defaults = getDeviceDefaults();

  return {
    cols: defaults.cols,
    rows: defaults.rows,
    confidence: 'low',
    source: 'defaults',
  };
}

/**
 * Get safe default dimensions based on device type.
 */
function getDeviceDefaults(): { cols: number; rows: number } {
  const screenWidth = window.screen.width;
  const pixelRatio = window.devicePixelRatio || 1;
  const logicalWidth = screenWidth / pixelRatio;

  if (isIOS()) {
    // Detect iPad vs iPhone based on logical screen width
    if (logicalWidth >= 768) {
      return IPAD_SAFE_DEFAULTS;
    }
    return IPHONE_SAFE_DEFAULTS;
  }

  // Desktop or Android
  return DESKTOP_SAFE_DEFAULTS;
}

/**
 * Clamp a value between min and max.
 */
function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * Validate dimensions against reasonable bounds.
 * Returns validation result with suggested adjustments.
 */
export function validateDimensions(
  cols: number,
  rows: number,
  deviceHint?: 'iphone' | 'ipad' | 'desktop'
): {
  valid: boolean;
  adjustedCols: number;
  adjustedRows: number;
  warnings: string[];
} {
  const warnings: string[] = [];
  let adjustedCols = cols;
  let adjustedRows = rows;

  // Check column bounds
  if (cols < MIN_COLS) {
    warnings.push(`Columns ${cols} too small, minimum ${MIN_COLS}`);
    adjustedCols = MIN_COLS;
  } else if (cols > MAX_COLS) {
    warnings.push(`Columns ${cols} too large, maximum ${MAX_COLS}`);
    adjustedCols = MAX_COLS;
  }

  // Check row bounds
  if (rows < MIN_ROWS) {
    warnings.push(`Rows ${rows} too small, minimum ${MIN_ROWS}`);
    adjustedRows = MIN_ROWS;
  } else if (rows > MAX_ROWS) {
    warnings.push(`Rows ${rows} too large, maximum ${MAX_ROWS}`);
    adjustedRows = MAX_ROWS;
  }

  // Check aspect ratio (cols/rows)
  const aspectRatio = cols / rows;
  if (aspectRatio < 0.3) {
    warnings.push(`Aspect ratio ${aspectRatio.toFixed(2)} too narrow`);
  } else if (aspectRatio > 8.0) {
    warnings.push(`Aspect ratio ${aspectRatio.toFixed(2)} too wide`);
  }

  // Device-specific warnings
  if (deviceHint === 'iphone' && cols > 60) {
    warnings.push(`iPhone requesting ${cols} cols may indicate sizing issue`);
  }

  return {
    valid: warnings.length === 0,
    adjustedCols,
    adjustedRows,
    warnings,
  };
}

/**
 * Get device hint string for dimension negotiation.
 */
export function getDeviceHint(): 'iphone' | 'ipad' | 'desktop' {
  const screenWidth = window.screen.width;
  const pixelRatio = window.devicePixelRatio || 1;
  const logicalWidth = screenWidth / pixelRatio;

  if (isIOS()) {
    if (logicalWidth >= 768) {
      return 'ipad';
    }
    return 'iphone';
  }

  return 'desktop';
}

/**
 * Detect if dimensions might indicate a sizing problem.
 * Called periodically to monitor for dimension drift.
 */
export function detectDimensionMismatch(
  terminal: { cols: number; rows: number },
  container: HTMLElement,
  charWidth: number,
  charHeight: number
): {
  mismatch: boolean;
  expectedCols: number;
  expectedRows: number;
  severity: 'none' | 'minor' | 'major';
} {
  const containerWidth = container.clientWidth - 24;
  const containerHeight = container.clientHeight - 16;

  const expectedCols = Math.floor(containerWidth / charWidth);
  const expectedRows = Math.floor(containerHeight / charHeight);

  const colDiff = Math.abs(terminal.cols - expectedCols);
  const rowDiff = Math.abs(terminal.rows - expectedRows);

  // Minor mismatch: 1-2 cells difference (could be rounding)
  // Major mismatch: > 5 cells difference
  let severity: 'none' | 'minor' | 'major' = 'none';

  if (colDiff > 5 || rowDiff > 3) {
    severity = 'major';
  } else if (colDiff > 2 || rowDiff > 1) {
    severity = 'minor';
  }

  return {
    mismatch: severity !== 'none',
    expectedCols,
    expectedRows,
    severity,
  };
}
