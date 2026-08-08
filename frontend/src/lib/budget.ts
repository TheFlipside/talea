/** Pure helpers over a budget's month-keyed amount history. */

import type { BudgetSegment, Money, Month } from '../api/types';

/** Whether `a` is strictly earlier than `b`. */
export function isBeforeMonth(a: Month, b: Month): boolean {
  return a.year < b.year || (a.year === b.year && a.month < b.month);
}

/**
 * The planned amount in effect for `month`: the last segment effective at or
 * before it. Mirrors the core's `Budget::amount_in`, so the form always shows
 * the figure the month being viewed actually reserves — not the newest one.
 *
 * `amounts` is never empty (the domain guarantees a base segment), and months
 * before the first segment resolve to it.
 */
export function amountInMonth(amounts: readonly BudgetSegment[], month: Month): Money {
  let current = amounts[0].amount;
  for (const segment of amounts) {
    if (isBeforeMonth(month, segment.effective_from)) {
      break;
    }
    current = segment.amount;
  }
  return current;
}
