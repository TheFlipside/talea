import { describe, expect, it } from 'vitest';

import type { Entry, Occurrence } from '../../api/types';
import { mergeMonthItems } from '../monthItems';

function entry(id: number, date: string): Entry {
  return { id, account_id: 1, amount: '1.00', kind: 'expense', date };
}

function occurrence(ruleId: number, date: string): Occurrence {
  return { rule_id: ruleId, account_id: 1, amount: '1.00', kind: 'income', date };
}

describe('mergeMonthItems', () => {
  it('orders most recent date first across entries and occurrences', () => {
    const items = mergeMonthItems(
      [entry(1, '2026-01-05'), entry(2, '2026-01-20')],
      [occurrence(9, '2026-01-10')],
    );
    expect(items.map((i) => (i.kind === 'entry' ? i.entry.date : i.occurrence.date))).toEqual([
      '2026-01-20',
      '2026-01-10',
      '2026-01-05',
    ]);
  });

  it('puts stored entries before occurrences on the same date', () => {
    const items = mergeMonthItems([entry(1, '2026-01-10')], [occurrence(9, '2026-01-10')]);
    expect(items.map((i) => i.kind)).toEqual(['entry', 'occurrence']);
  });

  it('breaks entry ties by id descending and occurrence ties by rule id', () => {
    const items = mergeMonthItems(
      [entry(1, '2026-01-10'), entry(3, '2026-01-10')],
      [occurrence(8, '2026-01-10'), occurrence(2, '2026-01-10')],
    );
    // Entries (id desc) first, then occurrences (rule id asc).
    expect(items.map((i) => (i.kind === 'entry' ? `e${i.entry.id}` : `o${i.occurrence.rule_id}`))).toEqual([
      'e3',
      'e1',
      'o2',
      'o8',
    ]);
  });

  it('does not mutate its inputs', () => {
    const entries = [entry(2, '2026-01-20'), entry(1, '2026-01-05')];
    const before = entries.map((e) => e.id);
    mergeMonthItems(entries, []);
    expect(entries.map((e) => e.id)).toEqual(before);
  });
});
