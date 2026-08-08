/** Pure helpers over a budget's month-keyed amount history. */

import type { BudgetSegment, Money, Month } from '../api/types';

/** Whether `a` is strictly earlier than `b`. */
export function isBeforeMonth(a: Month, b: Month): boolean {
  return a.year < b.year || (a.year === b.year && a.month < b.month);
}

/** Whether `a` and `b` are the same calendar month. */
export function isSameMonth(a: Month, b: Month): boolean {
  return a.year === b.year && a.month === b.month;
}

/**
 * The history with `amount` effective from `at`: replacing that month's segment
 * if it already has one, otherwise inserting it in order.
 *
 * Deliberately leaves **later** breakpoints alone, unlike the recurring-rule
 * form's equivalent. A rule's effective date is picked by hand and is almost
 * always the newest point, so dropping later segments is moot there; a budget's
 * is whichever month the Budgets screen is showing, and paging back to correct
 * an earlier month is a normal thing to do. Wiping the plan for later months as
 * a side effect of that would be a nasty surprise — flattening the history is
 * what the "All months" scope is for.
 *
 * `at` is assumed to be at or after the first segment's month, so the base stays
 * anchored at the budget's start.
 */
export function withAmountFrom(
  amounts: readonly BudgetSegment[],
  at: Month,
  amount: Money,
): BudgetSegment[] {
  return [...amounts.filter((s) => !isSameMonth(s.effective_from, at)), { effective_from: at, amount }].sort(
    (a, b) => (isBeforeMonth(a.effective_from, b.effective_from) ? -1 : 1),
  );
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
