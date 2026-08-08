import { describe, expect, it } from 'vitest';

import type { BudgetSegment } from '../../api/types';
import {
  amountInMonth,
  clashingExpenseRule,
  isBeforeMonth,
  isSameMonth,
  reanchorAmounts,
  withAmountFrom,
} from '../budget';

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

  it('reports a named month at its own amount, not the newest one', () => {
    // Regression: the "not active this month" row paired the start month's label
    // with `amounts[amounts.length - 1]`, so 100 from August, raised to 120 in
    // October and 130 in November, read as "from August: 130".
    const raised = [seg(2026, 8, '100.00'), seg(2026, 10, '120.00'), seg(2026, 11, '130.00')];
    expect(amountInMonth(raised, m(2026, 8))).toBe('100.00');
    expect(amountInMonth(raised, m(2026, 9))).toBe('100.00');
    expect(amountInMonth(raised, m(2026, 10))).toBe('120.00');
  });
});

describe('isSameMonth', () => {
  it('compares both components', () => {
    expect(isSameMonth(m(2026, 8), m(2026, 8))).toBe(true);
    expect(isSameMonth(m(2026, 8), m(2026, 9))).toBe(false);
    expect(isSameMonth(m(2026, 8), m(2027, 8))).toBe(false);
  });
});

describe('withAmountFrom', () => {
  const history = [seg(2026, 8, '100.00'), seg(2026, 10, '120.00'), seg(2026, 11, '130.00')];

  it('leaves later breakpoints alone', () => {
    // The whole point: correcting August from the Budgets screen must not wipe
    // the October and November plans as a side effect.
    expect(withAmountFrom(history, m(2026, 8), '110.00')).toEqual([
      seg(2026, 8, '110.00'),
      seg(2026, 10, '120.00'),
      seg(2026, 11, '130.00'),
    ]);
  });

  it('replaces an existing segment for the same month', () => {
    const out = withAmountFrom(history, m(2026, 10), '125.00');
    expect(out).toHaveLength(3);
    expect(out[1]).toEqual(seg(2026, 10, '125.00'));
  });

  it('inserts a new breakpoint in chronological order', () => {
    expect(withAmountFrom(history, m(2026, 9), '105.00')).toEqual([
      seg(2026, 8, '100.00'),
      seg(2026, 9, '105.00'),
      seg(2026, 10, '120.00'),
      seg(2026, 11, '130.00'),
    ]);
  });

  it('keeps the base anchored when the edited month is the first one', () => {
    const out = withAmountFrom([seg(2026, 8, '100.00')], m(2026, 8), '110.00');
    expect(out).toEqual([seg(2026, 8, '110.00')]);
  });
});

describe('reanchorAmounts', () => {
  const history = [seg(2026, 8, '100.00'), seg(2026, 10, '120.00'), seg(2026, 11, '130.00')];

  it('extends the base back when the start moves earlier', () => {
    // Paging back to July and moving the start there must cover July onward at
    // the base amount, without disturbing the October and November raises.
    expect(reanchorAmounts(history, m(2026, 7))).toEqual([
      seg(2026, 7, '100.00'),
      seg(2026, 10, '120.00'),
      seg(2026, 11, '130.00'),
    ]);
  });

  it('drops segments outside the span and re-anchors when the start moves later', () => {
    expect(reanchorAmounts(history, m(2026, 10))).toEqual([
      seg(2026, 10, '120.00'),
      seg(2026, 11, '130.00'),
    ]);
  });

  it('re-anchors mid-segment on the amount then in effect', () => {
    expect(reanchorAmounts(history, m(2026, 9))).toEqual([
      seg(2026, 9, '100.00'),
      seg(2026, 10, '120.00'),
      seg(2026, 11, '130.00'),
    ]);
  });

  it('drops a breakpoint that would merely restate the running amount', () => {
    // Moving the start to July would otherwise leave July=100 followed by a
    // redundant August=100 in the history list.
    expect(reanchorAmounts([seg(2026, 8, '100.00')], m(2026, 7))).toEqual([
      seg(2026, 7, '100.00'),
    ]);
  });

  it('is a no-op when the start is unchanged', () => {
    expect(reanchorAmounts(history, m(2026, 8))).toEqual(history);
  });
});

describe('clashingExpenseRule', () => {
  const rule = (
    id: number,
    start_date: string,
    end: { kind: 'never' } | { kind: 'until'; date: string },
    kind: 'expense' | 'income' = 'expense',
    category_id: number | null = 1,
  ) =>
    ({
      id,
      account_id: 1,
      amounts: [{ effective_from: start_date, amount: '400.00' }],
      kind,
      category_id,
      start_date,
      end,
      frequency: { unit: 'monthly' as const, interval: 1 },
    }) as never;

  const span = { start: m(2026, 8), end: null };

  it('finds an open-ended expense rule in the same category', () => {
    const rules = [rule(1, '2026-01-01', { kind: 'never' })];
    expect(clashingExpenseRule(rules, 1, span)?.id).toBe(1);
  });

  it('ignores income rules and other categories', () => {
    expect(
      clashingExpenseRule([rule(1, '2026-01-01', { kind: 'never' }, 'income')], 1, span),
    ).toBeUndefined();
    expect(
      clashingExpenseRule([rule(1, '2026-01-01', { kind: 'never' }, 'expense', 2)], 1, span),
    ).toBeUndefined();
  });

  it('ignores a rule that ended before the budget begins', () => {
    // The destructive "delete this rule" offer must not fire on a rule that
    // never overlaps: deleting it would be pure loss.
    const rules = [rule(1, '2025-01-01', { kind: 'until', date: '2025-12-31' })];
    expect(clashingExpenseRule(rules, 1, span)).toBeUndefined();
  });

  it('ignores a rule starting after the budget ends', () => {
    const rules = [rule(1, '2027-01-01', { kind: 'never' })];
    expect(clashingExpenseRule(rules, 1, { start: m(2026, 8), end: m(2026, 12) })).toBeUndefined();
  });

  it('counts a rule whose last month is the budget’s first', () => {
    const rules = [rule(1, '2025-01-01', { kind: 'until', date: '2026-08-31' })];
    expect(clashingExpenseRule(rules, 1, span)?.id).toBe(1);
  });
});
