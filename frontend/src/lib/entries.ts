/** Pure helpers for selecting and ordering entries for a month view. */

import type { Entry, Month } from '../api/types';
import { isoMonthOf } from './date';

/** Whether an entry's date falls in the given month. */
export function entryInMonth(entry: Entry, month: Month): boolean {
  const m = isoMonthOf(entry.date);
  return m.year === month.year && m.month === month.month;
}

/** The entries dated within `month`. */
export function filterEntriesToMonth(entries: Entry[], month: Month): Entry[] {
  return entries.filter((e) => entryInMonth(e, month));
}

/**
 * Orders entries for display: most recent date first, breaking ties by id
 * descending (latest-recorded first). Returns a new array; does not mutate.
 */
export function sortEntriesForDisplay(entries: Entry[]): Entry[] {
  return [...entries].sort((a, b) => {
    if (a.date !== b.date) {
      return a.date < b.date ? 1 : -1;
    }
    return b.id - a.id;
  });
}
