/** Pure helper merging stored entries and rule occurrences into one ordered
 *  month view. Occurrences are read-only (derived from recurring rules). */

import type { Entry, Occurrence } from '../api/types';

/** A row in the month list: an editable stored entry, or a read-only occurrence. */
export type MonthItem =
  | { kind: 'entry'; entry: Entry }
  | { kind: 'occurrence'; occurrence: Occurrence };

function itemDate(item: MonthItem): string {
  return item.kind === 'entry' ? item.entry.date : item.occurrence.date;
}

/**
 * Merges already-month-scoped stored `entries` and rule `occurrences` into one
 * display list, most recent date first. Ties (same date) order stored entries
 * before occurrences, then entries by id descending (latest-recorded first) and
 * occurrences by their rule id — a stable, deterministic order. Does not mutate
 * its inputs.
 */
export function mergeMonthItems(entries: Entry[], occurrences: Occurrence[]): MonthItem[] {
  const items: MonthItem[] = [
    ...entries.map((entry) => ({ kind: 'entry' as const, entry })),
    ...occurrences.map((occurrence) => ({ kind: 'occurrence' as const, occurrence })),
  ];
  return items.sort((a, b) => {
    const da = itemDate(a);
    const db = itemDate(b);
    if (da !== db) {
      return da < db ? 1 : -1;
    }
    if (a.kind !== b.kind) {
      return a.kind === 'entry' ? -1 : 1;
    }
    if (a.kind === 'entry' && b.kind === 'entry') {
      return b.entry.id - a.entry.id;
    }
    if (a.kind === 'occurrence' && b.kind === 'occurrence') {
      return a.occurrence.rule_id - b.occurrence.rule_id;
    }
    return 0;
  });
}
