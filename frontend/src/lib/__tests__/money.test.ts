import { describe, expect, it } from 'vitest';

import {
  formatMoney,
  isMoneyInput,
  normalizeAmountInput,
  parseMoneyForDisplay,
} from '../money';

describe('isMoneyInput', () => {
  it('accepts decimal strings, with optional sign', () => {
    expect(isMoneyInput('0')).toBe(true);
    expect(isMoneyInput('12.34')).toBe(true);
    expect(isMoneyInput('-5.00')).toBe(true);
    expect(isMoneyInput(' 7 ')).toBe(true);
  });

  it('rejects empty and non-numeric input', () => {
    expect(isMoneyInput('')).toBe(false);
    expect(isMoneyInput('abc')).toBe(false);
    expect(isMoneyInput('1,5')).toBe(false);
    expect(isMoneyInput('1.')).toBe(false);
  });
});

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

describe('normalizeAmountInput', () => {
  it('converts a decimal comma to a dot and trims', () => {
    expect(normalizeAmountInput(' 0,99 ')).toBe('0.99');
    expect(normalizeAmountInput('1234,56')).toBe('1234.56');
  });

  it('leaves a dot-decimal amount unchanged', () => {
    expect(normalizeAmountInput('12.34')).toBe('12.34');
  });

  it('produces a value isMoneyInput accepts for comma input', () => {
    expect(isMoneyInput(normalizeAmountInput('0,99'))).toBe(true);
  });
});
