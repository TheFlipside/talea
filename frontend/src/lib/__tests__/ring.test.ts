import { describe, expect, it } from 'vitest';

import type { MonthSummary } from '../../api/types';
import { ringView } from '../ring';

/** `reserved` defaults to 0, so `free` mirrors `available` — the no-budget case. */
function summary(
  carry_in: string,
  income: string,
  expenses: string,
  available: string,
  reserved = '0',
  free = available,
): MonthSummary {
  return { month: { year: 2026, month: 1 }, carry_in, income, expenses, available, reserved, free };
}

describe('ringView', () => {
  it('spent mode: fraction is expenses over available funds', () => {
    const v = ringView(summary('0', '100', '70', '30'), 'spent');
    expect(v.fraction).toBeCloseTo(0.7);
    expect(v.percent).toBe(70);
    expect(v.overspent).toBe(false);
    expect(v.labelKey).toBe('summary.ringSpent');
  });

  it('remaining mode: fraction is available over funds', () => {
    const v = ringView(summary('0', '100', '70', '30'), 'remaining');
    expect(v.fraction).toBeCloseTo(0.3);
    expect(v.percent).toBe(30);
    expect(v.labelKey).toBe('summary.ringRemaining');
  });

  it('flags overspend and clamps the fraction to [0,1]', () => {
    const v = ringView(summary('0', '100', '150', '-50'), 'spent');
    expect(v.overspent).toBe(true);
    expect(v.fraction).toBe(1); // clamped
  });

  it('handles zero available funds without dividing by zero', () => {
    const v = ringView(summary('0', '0', '0', '0'), 'spent');
    expect(v.fraction).toBe(0);
    expect(v.percent).toBe(0);
  });

  it('spent mode counts a budget reservation as committed', () => {
    // 100 funds, 20 spent, 30 still held back by a budget -> 50 committed.
    const s = summary('0', '100', '20', '80', '30', '50');
    expect(ringView(s, 'spent').fraction).toBeCloseTo(0.5);
    expect(ringView(s, 'remaining').fraction).toBeCloseTo(0.5);
  });

  it('keeps the two modes complementary', () => {
    const s = summary('40', '60', '25', '75', '15', '60');
    const spent = ringView(s, 'spent').fraction;
    const remaining = ringView(s, 'remaining').fraction;
    expect(spent + remaining).toBeCloseTo(1);
  });

  it('flags overspend when budgets outrun a month still in the black', () => {
    // `available` is positive, but the budgets reserve more than it.
    const s = summary('0', '100', '90', '10', '40', '-30');
    const v = ringView(s, 'remaining');
    expect(v.overspent).toBe(true);
    expect(v.fraction).toBe(0); // clamped
    expect(ringView(s, 'spent').fraction).toBe(1);
  });
});
