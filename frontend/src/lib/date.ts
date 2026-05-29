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

/** The day-of-month component of an ISO date. */
export function isoDayOf(iso: IsoDate): number {
  return Number(iso.split('-')[2]);
}

/** Builds an ISO `YYYY-MM-DD` from numeric parts (month 1..=12). */
export function isoDate(year: number, month: number, day: number): IsoDate {
  return `${year}-${pad2(month)}-${pad2(day)}`;
}

/** A full human label for a date, e.g. "9 May 2026". */
export function formatFullDate(iso: IsoDate): string {
  const { year, month } = isoMonthOf(iso);
  return new Intl.DateTimeFormat(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  }).format(new Date(year, month - 1, isoDayOf(iso)));
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
