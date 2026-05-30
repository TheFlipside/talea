import { describe, expect, it } from 'vitest';

import type { MonthSummary } from '../../api/types';
import { ringView } from '../ring';

function summary(carry_in: string, income: string, expenses: string, available: string): MonthSummary {
  return { month: { year: 2026, month: 1 }, carry_in, income, expenses, available };
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
});
