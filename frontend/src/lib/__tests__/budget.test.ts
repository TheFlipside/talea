import { describe, expect, it } from 'vitest';

import type { BudgetSegment } from '../../api/types';
import { amountInMonth, isBeforeMonth } from '../budget';

const m = (year: number, month: number) => ({ year, month });
const seg = (year: number, month: number, amount: string): BudgetSegment => ({
  effective_from: m(year, month),
  amount,
});

describe('isBeforeMonth', () => {
  it('orders by year then month', () => {
    expect(isBeforeMonth(m(2025, 12), m(2026, 1))).toBe(true);
    expect(isBeforeMonth(m(2026, 1), m(2025, 12))).toBe(false);
    expect(isBeforeMonth(m(2026, 5), m(2026, 6))).toBe(true);
    expect(isBeforeMonth(m(2026, 6), m(2026, 6))).toBe(false);
  });
});

describe('amountInMonth', () => {
  const history = [seg(2026, 1, '400.00'), seg(2026, 6, '450.00'), seg(2026, 9, '500.00')];

  it('returns the base when there is only one segment', () => {
    expect(amountInMonth([seg(2026, 1, '400.00')], m(2030, 7))).toBe('400.00');
  });

  it('holds a segment until the next one starts', () => {
    expect(amountInMonth(history, m(2026, 5))).toBe('400.00');
    expect(amountInMonth(history, m(2026, 6))).toBe('450.00');
    expect(amountInMonth(history, m(2026, 8))).toBe('450.00');
    expect(amountInMonth(history, m(2026, 9))).toBe('500.00');
    expect(amountInMonth(history, m(2027, 3))).toBe('500.00');
  });

  it('falls back to the base before the first segment', () => {
    expect(amountInMonth(history, m(2025, 11))).toBe('400.00');
  });
});
