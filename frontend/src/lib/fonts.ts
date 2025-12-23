// Font loading strategy with timeout and fallback
// Phase 3.1: Ensures reliable character dimension measurement

export interface FontLoadResult {
  loaded: boolean;
  fontFamily: string;
  charWidth: number;
  charHeight: number;
  fallbackUsed: boolean;
  loadTimeMs: number;
}

// Primary font and fallbacks
const PRIMARY_FONT = 'JetBrains Mono';
const FALLBACK_FONTS = ['ui-monospace', 'SF Mono', 'Menlo', 'Monaco', 'Consolas', 'monospace'];

// Default timeout for font loading (ms)
const FONT_LOAD_TIMEOUT_MS = 2000;

// Reference characters for dimension measurement
const MEASUREMENT_CHARS = 'MWXO0123456789';

/**
 * Load terminal font with timeout and fallback.
 * Returns the loaded font family and measured character dimensions.
 */
export async function loadTerminalFont(options: {
  timeout?: number;
  fontSize?: number;
}): Promise<FontLoadResult> {
  const timeout = options.timeout ?? FONT_LOAD_TIMEOUT_MS;
  const fontSize = options.fontSize ?? 13;
  const startTime = Date.now();

  // Try to load the primary font with timeout
  let fontFamily = PRIMARY_FONT;
  let fallbackUsed = false;

  try {
    const loaded = await Promise.race([
      loadFontWithCheck(PRIMARY_FONT, fontSize),
      timeoutPromise(timeout),
    ]);

    if (!loaded) {
      // Primary font failed to load, use fallback
      fontFamily = await findAvailableFallback(fontSize);
      fallbackUsed = true;
    }
  } catch (e) {
    // Error loading font, use fallback
    console.warn('Font loading failed, using fallback:', e);
    fontFamily = await findAvailableFallback(fontSize);
    fallbackUsed = true;
  }

  // Build the full font family string
  const fullFontFamily = buildFontFamilyString(fontFamily, fallbackUsed);

  // Measure character dimensions with the loaded font
  const { charWidth, charHeight } = measureCharacterDimensions(fullFontFamily, fontSize);

  const loadTimeMs = Date.now() - startTime;

  return {
    loaded: !fallbackUsed,
    fontFamily: fullFontFamily,
    charWidth,
    charHeight,
    fallbackUsed,
    loadTimeMs,
  };
}

/**
 * Load a specific font and verify it's actually available.
 */
async function loadFontWithCheck(fontName: string, fontSize: number): Promise<boolean> {
  // Use the Font Loading API if available
  if ('fonts' in document && document.fonts) {
    try {
      // First, wait for all fonts to be ready
      await document.fonts.ready;

      // Check if the specific font is loaded
      const fontSpec = `${fontSize}px "${fontName}"`;
      const loaded = document.fonts.check(fontSpec);

      if (!loaded) {
        // Try to explicitly load the font
        try {
          await document.fonts.load(fontSpec);
          return document.fonts.check(fontSpec);
        } catch {
          return false;
        }
      }

      return true;
    } catch {
      return false;
    }
  }

  // Fallback: Use canvas-based font detection
  return detectFontWithCanvas(fontName, fontSize);
}

/**
 * Detect if a font is available using canvas measurement.
 * Compares the rendered width of text in the target font vs a fallback.
 */
function detectFontWithCanvas(fontName: string, fontSize: number): boolean {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return false;

  const testString = 'mmmmmmmmmmlli';

  // Measure with fallback font
  ctx.font = `${fontSize}px monospace`;
  const fallbackWidth = ctx.measureText(testString).width;

  // Measure with target font (with fallback)
  ctx.font = `${fontSize}px "${fontName}", monospace`;
  const targetWidth = ctx.measureText(testString).width;

  // If widths differ significantly, the target font is likely available
  return Math.abs(targetWidth - fallbackWidth) > 1;
}

/**
 * Find the first available fallback font.
 */
async function findAvailableFallback(fontSize: number): Promise<string> {
  for (const font of FALLBACK_FONTS) {
    if (font === 'monospace') {
      // Always available as last resort
      return font;
    }

    const available = detectFontWithCanvas(font, fontSize);
    if (available) {
      return font;
    }
  }

  return 'monospace';
}

/**
 * Build the full font-family CSS string.
 */
function buildFontFamilyString(primaryFont: string, isFallback: boolean): string {
  if (isFallback) {
    // Start with the detected fallback, then include system fallbacks
    const otherFallbacks = FALLBACK_FONTS.filter(f => f !== primaryFont);
    return [primaryFont, ...otherFallbacks].map(f =>
      f.includes(' ') ? `"${f}"` : f
    ).join(', ');
  }

  // Primary font loaded, use full chain
  return [`"${PRIMARY_FONT}"`, ...FALLBACK_FONTS.map(f =>
    f.includes(' ') ? `"${f}"` : f
  )].join(', ');
}

/**
 * Measure character dimensions for a given font.
 * Uses a hidden canvas for accurate measurement.
 */
function measureCharacterDimensions(fontFamily: string, fontSize: number): { charWidth: number; charHeight: number } {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');

  if (!ctx) {
    // Fallback dimensions based on typical monospace ratios
    return {
      charWidth: fontSize * 0.6,
      charHeight: fontSize * 1.2,
    };
  }

  ctx.font = `${fontSize}px ${fontFamily}`;

  // Measure width using multiple characters for accuracy
  let totalWidth = 0;
  for (const char of MEASUREMENT_CHARS) {
    totalWidth += ctx.measureText(char).width;
  }
  const charWidth = totalWidth / MEASUREMENT_CHARS.length;

  // For height, use the font metrics or estimate
  const metrics = ctx.measureText('M');
  let charHeight: number;

  if (metrics.fontBoundingBoxAscent !== undefined && metrics.fontBoundingBoxDescent !== undefined) {
    // Modern browsers provide full font metrics
    charHeight = metrics.fontBoundingBoxAscent + metrics.fontBoundingBoxDescent;
  } else if (metrics.actualBoundingBoxAscent !== undefined && metrics.actualBoundingBoxDescent !== undefined) {
    // Fallback to actual bounding box
    charHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent;
  } else {
    // Estimate based on font size
    charHeight = fontSize * 1.2;
  }

  return { charWidth, charHeight };
}

/**
 * Create a promise that rejects after the specified timeout.
 */
function timeoutPromise(ms: number): Promise<false> {
  return new Promise((_, reject) => {
    setTimeout(() => reject(new Error('Font load timeout')), ms);
  });
}

/**
 * Get device-specific font size recommendations.
 * Smaller screens may benefit from smaller fonts.
 */
export function getRecommendedFontSize(): number {
  const screenWidth = window.screen.width;
  const pixelRatio = window.devicePixelRatio || 1;
  const logicalWidth = screenWidth / pixelRatio;

  // iPhone SE, iPhone mini: smaller font
  if (logicalWidth <= 375) {
    return 11;
  }

  // Standard iPhones, most phones
  if (logicalWidth <= 430) {
    return 12;
  }

  // Tablets, desktops
  return 13;
}

/**
 * Check if we're on an iOS device.
 */
export function isIOS(): boolean {
  return /iPad|iPhone|iPod/.test(navigator.userAgent) ||
    (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
}

/**
 * Check if we're in a PWA (standalone mode).
 */
export function isPWA(): boolean {
  return window.matchMedia('(display-mode: standalone)').matches ||
    (window.navigator as Navigator & { standalone?: boolean }).standalone === true;
}
