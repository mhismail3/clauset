/**
 * Formatting utilities for display values.
 */

/**
 * Format token count for display.
 * - Values < 1000: show as-is (e.g., "99")
 * - Values >= 1000 and < 10000: show with one decimal (e.g., "1.8K")
 * - Values >= 10000: show rounded (e.g., "29K")
 *
 * This matches Claude Code's status line format.
 */
export function formatTokens(tokens: number | undefined): string {
  if (tokens === undefined || tokens === 0) return '0';
  if (tokens >= 1000) {
    const k = tokens / 1000;
    return k >= 10 ? `${Math.round(k)}K` : `${k.toFixed(1)}K`;
  }
  return tokens.toString();
}

/**
 * Format cost in USD for display.
 * Always shows 2 decimal places with $ prefix.
 */
export function formatCost(cost: number | undefined): string {
  if (cost === undefined || cost === 0) return '$0.00';
  return `$${cost.toFixed(2)}`;
}

/**
 * Shorten model name for display.
 * Converts full model IDs to friendly names.
 */
export function shortenModel(model: string | undefined): string {
  if (!model) return 'Unknown';
  const modelLower = model.toLowerCase();
  if (modelLower.includes('opus')) return 'Opus 4.5';
  if (modelLower.includes('sonnet')) return 'Sonnet 4';
  if (modelLower.includes('haiku')) return 'Haiku 4.5';
  // Strip version suffixes
  return model.replace(/-\d{8}$/, '').replace('claude-', '');
}
