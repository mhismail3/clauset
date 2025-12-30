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
  if (tokens >= 1_000_000) {
    const m = tokens / 1_000_000;
    return m >= 10 ? `${Math.round(m)}M` : `${m.toFixed(1)}M`;
  }
  if (tokens >= 1000) {
    const k = tokens / 1000;
    return k >= 10 ? `${Math.round(k)}K` : `${k.toFixed(1)}K`;
  }
  return tokens.toString();
}

export type PermissionMode = 'default' | 'accept_edits' | 'bypass_permissions' | 'plan';

export function normalizePermissionMode(mode?: string): PermissionMode | undefined {
  if (!mode) return undefined;
  const normalized = mode.trim().toLowerCase();
  switch (normalized) {
    case 'default':
    case 'normal':
      return 'default';
    case 'plan':
    case 'plan mode':
      return 'plan';
    case 'accept_edits':
    case 'acceptedits':
    case 'accept edits':
      return 'accept_edits';
    case 'bypass_permissions':
    case 'bypasspermissions':
    case 'bypass permissions':
      return 'bypass_permissions';
    default:
      return undefined;
  }
}

export function formatPermissionMode(mode?: string): string {
  const normalized = normalizePermissionMode(mode);
  if (!normalized) return mode?.trim() || 'Default';
  switch (normalized) {
    case 'default':
      return 'Default';
    case 'accept_edits':
      return 'Accept Edits';
    case 'bypass_permissions':
      return 'Bypass Permissions';
    case 'plan':
      return 'Plan';
  }
}

/**
 * Normalize token counts that appear to be reported in thousands.
 * Some sources return values like 12 (meaning 12K).
 */
export function normalizeTokenCount(
  tokens: number | undefined,
  hints?: { cost?: number; contextPercent?: number }
): number | undefined {
  if (tokens === undefined || tokens === 0) return tokens;
  if (!Number.isFinite(tokens)) return tokens;
  if (tokens >= 1000) return tokens;

  const hasFractional = Math.abs(tokens % 1) > Number.EPSILON;
  const cost = hints?.cost ?? 0;
  const contextPercent = hints?.contextPercent ?? 0;
  const shouldScale = hasFractional || cost >= 0.05 || contextPercent >= 1;

  return shouldScale ? Math.round(tokens * 1000) : tokens;
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
  const cleaned = model.replace(/^claude-/, '').replace(/-\d{8}$/, '');
  const lower = cleaned.toLowerCase();

  const formatFamily = (family: string, major?: string, minor?: string) => {
    const name = family.charAt(0).toUpperCase() + family.slice(1);
    if (!major) return name;
    const version = minor ? `${major}.${minor}` : major;
    return `${name} ${version}`;
  };

  const matchFamilyFirst = lower.match(/(opus|sonnet|haiku)[- ](\d+)(?:[-.](\d+))?/);
  if (matchFamilyFirst) {
    return formatFamily(matchFamilyFirst[1], matchFamilyFirst[2], matchFamilyFirst[3]);
  }

  const matchVersionFirst = lower.match(/(\d+)(?:[-.](\d+))?[- ](opus|sonnet|haiku)/);
  if (matchVersionFirst) {
    return formatFamily(matchVersionFirst[3], matchVersionFirst[1], matchVersionFirst[2]);
  }

  const familyOnly = lower.match(/(opus|sonnet|haiku)/);
  if (familyOnly) {
    const defaults: Record<string, string> = { opus: '4.5', sonnet: '4.5', haiku: '4.5' };
    const version = defaults[familyOnly[1]];
    if (version) {
      const [major, minor] = version.split('.');
      return formatFamily(familyOnly[1], major, minor);
    }
    return formatFamily(familyOnly[1]);
  }

  return cleaned.replace('claude-', '');
}
