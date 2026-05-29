import { describe, expect, it } from 'vitest';

import { monthEquals, nextMonth, prevMonth } from '../month';

describe('month arithmetic', () => {
  it('advances within a year', () => {
    expect(nextMonth({ year: 2026, month: 5 })).toEqual({ year: 2026, month: 6 });
  });

  it('rolls forward across the year boundary', () => {
    expect(nextMonth({ year: 2026, month: 12 })).toEqual({ year: 2027, month: 1 });
  });

  it('rolls backward across the year boundary', () => {
    expect(prevMonth({ year: 2026, month: 1 })).toEqual({ year: 2025, month: 12 });
  });

  it('compares months for equality', () => {
    expect(monthEquals({ year: 2026, month: 5 }, { year: 2026, month: 5 })).toBe(true);
    expect(monthEquals({ year: 2026, month: 5 }, { year: 2026, month: 6 })).toBe(false);
  });
});
