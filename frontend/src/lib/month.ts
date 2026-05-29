/** Pure calendar-month helpers for the selected-month UI state. */

import type { Month } from '../api/types';

/** The current calendar month, from the local clock. */
export function currentMonth(): Month {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() + 1 };
}

/** The following month, rolling into the next year after December. */
export function nextMonth(m: Month): Month {
  return m.month === 12
    ? { year: m.year + 1, month: 1 }
    : { year: m.year, month: m.month + 1 };
}

/** The preceding month, rolling into the previous year before January. */
export function prevMonth(m: Month): Month {
  return m.month === 1
    ? { year: m.year - 1, month: 12 }
    : { year: m.year, month: m.month - 1 };
}

export function monthEquals(a: Month, b: Month): boolean {
  return a.year === b.year && a.month === b.month;
}

/** A human label like "May 2026". */
export function monthLabel(m: Month): string {
  const date = new Date(m.year, m.month - 1, 1);
  return new Intl.DateTimeFormat(undefined, {
    month: 'long',
    year: 'numeric',
  }).format(date);
}
