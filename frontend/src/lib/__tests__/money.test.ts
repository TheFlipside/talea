import { describe, expect, it } from 'vitest';

import { formatMoney, parseMoneyForDisplay } from '../money';

describe('formatMoney', () => {
  it('formats a known currency', () => {
    // Non-breaking spaces vary by ICU; assert the digits/symbol are present.
    const out = formatMoney('12.34', 'USD');
    expect(out).toContain('12.34');
    expect(out).toContain('$');
  });

  it('formats zero and negative amounts', () => {
    expect(formatMoney('0.10', 'USD')).toContain('0.10');
    expect(formatMoney('-50.00', 'USD')).toContain('50.00');
  });

  it('falls back to a number + code for an unknown currency', () => {
    const out = formatMoney('5.00', 'ZZZ');
    expect(out).toContain('5.00');
    expect(out).toContain('ZZZ');
  });

  it('falls back to the raw string for an unparseable amount', () => {
    expect(formatMoney('not-money', 'USD')).toBe('not-money');
  });
});

describe('parseMoneyForDisplay', () => {
  it('parses valid amounts and zeroes invalid ones', () => {
    expect(parseMoneyForDisplay('12.34')).toBeCloseTo(12.34);
    expect(parseMoneyForDisplay('bad')).toBe(0);
  });
});
