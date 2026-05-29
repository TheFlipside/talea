/** Pure ISO-date helpers. */

import type { IsoDate, Month } from '../api/types';

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

/**
 * Today's local date as `YYYY-MM-DD`.
 *
 * Built from local Y/M/D parts rather than `toISOString()` (which is UTC and
 * can shift the day near midnight in non-UTC zones).
 */
export function todayISO(): IsoDate {
  const now = new Date();
  return `${now.getFullYear()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())}`;
}

/** The `{ year, month }` a given ISO date falls in. */
export function isoMonthOf(iso: IsoDate): Month {
  const [year, month] = iso.split('-');
  return { year: Number(year), month: Number(month) };
}

/**
 * A sensible default entry date for the month being viewed: today if it is the
 * current month, otherwise the first day of that month (so the new entry is
 * visible in the current view).
 */
export function defaultDateForMonth(m: Month): IsoDate {
  const today = todayISO();
  const current = isoMonthOf(today);
  if (current.year === m.year && current.month === m.month) {
    return today;
  }
  return `${m.year}-${pad2(m.month)}-01`;
}

/** A short human label for an entry date, e.g. "May 9". */
export function formatEntryDate(iso: IsoDate): string {
  const { year, month } = isoMonthOf(iso);
  const day = Number(iso.split('-')[2]);
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
  }).format(new Date(year, month - 1, day));
}
