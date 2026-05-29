import { describe, expect, it } from 'vitest';

import type { Entry } from '../../api/types';
import { entryInMonth, filterEntriesToMonth, sortEntriesForDisplay } from '../entries';

function entry(id: number, date: string): Entry {
  return { id, account_id: 1, amount: '1.00', kind: 'expense', date };
}

describe('entryInMonth', () => {
  it('matches the first and last day of the month', () => {
    const m = { year: 2026, month: 1 };
    expect(entryInMonth(entry(1, '2026-01-01'), m)).toBe(true);
    expect(entryInMonth(entry(2, '2026-01-31'), m)).toBe(true);
  });

  it('excludes adjacent months', () => {
    const m = { year: 2026, month: 1 };
    expect(entryInMonth(entry(3, '2025-12-31'), m)).toBe(false);
    expect(entryInMonth(entry(4, '2026-02-01'), m)).toBe(false);
  });
});

describe('filterEntriesToMonth', () => {
  it('keeps only entries in the month', () => {
    const all = [entry(1, '2026-01-10'), entry(2, '2026-02-10'), entry(3, '2026-01-20')];
    const got = filterEntriesToMonth(all, { year: 2026, month: 1 }).map((e) => e.id);
    expect(got).toEqual([1, 3]);
  });
});

describe('sortEntriesForDisplay', () => {
  it('orders by date desc, then id desc, without mutating input', () => {
    const all = [entry(1, '2026-01-10'), entry(2, '2026-01-20'), entry(3, '2026-01-20')];
    const sorted = sortEntriesForDisplay(all);
    expect(sorted.map((e) => e.id)).toEqual([3, 2, 1]);
    expect(all.map((e) => e.id)).toEqual([1, 2, 3]); // original untouched
  });
});
