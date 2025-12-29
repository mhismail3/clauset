import { describe, it, expect } from 'vitest';
import { formatTokens, formatCost, shortenModel } from '../format';

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
    expect(shortenModel('sonnet')).toBe('Sonnet 4');
  });

  it('shortens Haiku models', () => {
    expect(shortenModel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
    expect(shortenModel('haiku')).toBe('Haiku 4.5');
  });

  it('strips version suffix from unknown models', () => {
    expect(shortenModel('claude-something-20250101')).toBe('something');
  });
});
