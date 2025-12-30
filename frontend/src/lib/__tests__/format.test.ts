import { describe, it, expect } from 'vitest';
import {
  formatTokens,
  formatCost,
  shortenModel,
  normalizeTokenCount,
  formatPermissionMode,
  normalizePermissionMode,
} from '../format';

describe('formatTokens', () => {
  it('returns "0" for undefined', () => {
    expect(formatTokens(undefined)).toBe('0');
  });

  it('returns "0" for zero', () => {
    expect(formatTokens(0)).toBe('0');
  });

  it('returns raw number for values under 1000', () => {
    expect(formatTokens(1)).toBe('1');
    expect(formatTokens(99)).toBe('99');
    expect(formatTokens(500)).toBe('500');
    expect(formatTokens(999)).toBe('999');
  });

  it('returns value with K suffix and one decimal for 1000-9999', () => {
    expect(formatTokens(1000)).toBe('1.0K');
    expect(formatTokens(1500)).toBe('1.5K');
    expect(formatTokens(1800)).toBe('1.8K');
    expect(formatTokens(2400)).toBe('2.4K');
    expect(formatTokens(9999)).toBe('10.0K'); // rounds up
  });

  it('returns rounded K value for 10000+', () => {
    expect(formatTokens(10000)).toBe('10K');
    expect(formatTokens(10500)).toBe('11K');
    expect(formatTokens(29200)).toBe('29K');
    expect(formatTokens(38104)).toBe('38K');
    expect(formatTokens(100000)).toBe('100K');
  });

  it('returns M suffix for 1,000,000+', () => {
    expect(formatTokens(1_000_000)).toBe('1.0M');
    expect(formatTokens(1_250_000)).toBe('1.3M');
    expect(formatTokens(12_000_000)).toBe('12M');
  });

  it('handles real-world Claude session values', () => {
    // Small response (like "I'm Claude")
    expect(formatTokens(5)).toBe('5');
    expect(formatTokens(99)).toBe('99');

    // Typical session with cache
    expect(formatTokens(1800)).toBe('1.8K');
    expect(formatTokens(38104)).toBe('38K');
  });
});

describe('formatCost', () => {
  it('returns "$0.00" for undefined', () => {
    expect(formatCost(undefined)).toBe('$0.00');
  });

  it('returns "$0.00" for zero', () => {
    expect(formatCost(0)).toBe('$0.00');
  });

  it('formats cost with 2 decimal places', () => {
    expect(formatCost(0.02)).toBe('$0.02');
    expect(formatCost(0.68)).toBe('$0.68');
    expect(formatCost(1.234)).toBe('$1.23');
    expect(formatCost(10)).toBe('$10.00');
  });
});

describe('shortenModel', () => {
  it('returns "Unknown" for undefined', () => {
    expect(shortenModel(undefined)).toBe('Unknown');
  });

  it('shortens Opus models', () => {
    expect(shortenModel('claude-opus-4-5-20251101')).toBe('Opus 4.5');
    expect(shortenModel('opus')).toBe('Opus 4.5');
  });

  it('shortens Sonnet models', () => {
    expect(shortenModel('claude-sonnet-4-20250514')).toBe('Sonnet 4');
    expect(shortenModel('claude-sonnet-4-5-20250929')).toBe('Sonnet 4.5');
    expect(shortenModel('sonnet')).toBe('Sonnet 4.5');
  });

  it('shortens Haiku models', () => {
    expect(shortenModel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
    expect(shortenModel('haiku')).toBe('Haiku 4.5');
  });

  it('strips version suffix from unknown models', () => {
    expect(shortenModel('claude-something-20250101')).toBe('something');
  });

  it('handles legacy version-first model IDs', () => {
    expect(shortenModel('claude-3-5-sonnet-20241022')).toBe('Sonnet 3.5');
  });
});

describe('normalizeTokenCount', () => {
  it('returns undefined when tokens are undefined', () => {
    expect(normalizeTokenCount(undefined)).toBeUndefined();
  });

  it('preserves zero', () => {
    expect(normalizeTokenCount(0)).toBe(0);
  });

  it('does not scale values already in full token units', () => {
    expect(normalizeTokenCount(1200, { cost: 0.24 })).toBe(1200);
  });

  it('scales fractional values that appear to be in thousands', () => {
    expect(normalizeTokenCount(10.2)).toBe(10200);
  });

  it('scales small values when usage hints imply thousands', () => {
    expect(normalizeTokenCount(12, { cost: 0.24 })).toBe(12000);
    expect(normalizeTokenCount(8, { contextPercent: 3 })).toBe(8000);
  });

  it('leaves small values alone when there are no scaling hints', () => {
    expect(normalizeTokenCount(12, { cost: 0.0 })).toBe(12);
    expect(normalizeTokenCount(42)).toBe(42);
  });
});

describe('normalizePermissionMode', () => {
  it('normalizes default variants', () => {
    expect(normalizePermissionMode('default')).toBe('default');
    expect(normalizePermissionMode('normal')).toBe('default');
    expect(normalizePermissionMode(' Default ')).toBe('default');
  });

  it('normalizes accept edits variants', () => {
    expect(normalizePermissionMode('accept_edits')).toBe('accept_edits');
    expect(normalizePermissionMode('acceptEdits')).toBe('accept_edits');
    expect(normalizePermissionMode('accept edits')).toBe('accept_edits');
  });

  it('normalizes bypass permissions variants', () => {
    expect(normalizePermissionMode('bypass_permissions')).toBe('bypass_permissions');
    expect(normalizePermissionMode('bypassPermissions')).toBe('bypass_permissions');
    expect(normalizePermissionMode('bypass permissions')).toBe('bypass_permissions');
  });

  it('normalizes plan variants', () => {
    expect(normalizePermissionMode('plan')).toBe('plan');
    expect(normalizePermissionMode('Plan')).toBe('plan');
  });

  it('returns undefined for empty inputs', () => {
    expect(normalizePermissionMode(undefined)).toBeUndefined();
    expect(normalizePermissionMode('')).toBeUndefined();
  });
});

describe('formatPermissionMode', () => {
  it('returns friendly labels for known modes', () => {
    expect(formatPermissionMode('default')).toBe('Default');
    expect(formatPermissionMode('accept_edits')).toBe('Accept Edits');
    expect(formatPermissionMode('bypass_permissions')).toBe('Bypass Permissions');
    expect(formatPermissionMode('plan')).toBe('Plan');
  });

  it('normalizes raw variants before formatting', () => {
    expect(formatPermissionMode('acceptEdits')).toBe('Accept Edits');
    expect(formatPermissionMode('bypass permissions')).toBe('Bypass Permissions');
  });
});
