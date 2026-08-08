/** Pure helpers over a budget's month-keyed amount history. */

import type { BudgetSegment, CategoryId, Money, Month, RecurringRule } from '../api/types';
import { isoMonthOf } from './date';
import { nextMonth } from './month';

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
 * The history with `amount` applying to `at` **and to that month alone**: the
 * month after it gets a breakpoint restoring whatever was running there before,
 * so nothing downstream shifts.
 *
 * This is the "just this one month" edit, which the segment model expresses
 * exactly — a one-month change is simply two breakpoints. Without it the only
 * ways to lower a single month were to lower every later month too, or to end
 * the budget after it, which throws the later months away entirely.
 *
 * `end` bounds the restore: if `at` is the budget's last month there is nothing
 * after it to restore, and a segment past the end could never apply.
 */
export function withAmountForMonth(
  amounts: readonly BudgetSegment[],
  at: Month,
  amount: Money,
  end?: Month | null,
): BudgetSegment[] {
  const after = nextMonth(at);
  // Read before the change: this is the figure `after` runs at today.
  const restore = amountInMonth(amounts, after);
  const changed = withAmountFrom(amounts, at, amount);
  if (end && isBeforeMonth(end, after)) {
    return changed;
  }
  return withAmountFrom(changed, after, restore);
}

/**
 * The history moved to a new start month, keeping every month's figure intact
 * where it still exists.
 *
 * The domain requires the first segment to sit exactly on the budget's start, so
 * changing the start has to rebuild the sequence:
 * - moving it **earlier** extends the base amount back over months that had no
 *   budget at all — it can't falsify anything, because there was nothing there;
 * - moving it **later** drops the segments that fall outside the new span, and
 *   re-anchors on whatever amount was in effect at the new start.
 *
 * A segment that merely restates the amount already running is dropped, so
 * re-anchoring doesn't leave redundant entries in the history list.
 */
export function reanchorAmounts(
  amounts: readonly BudgetSegment[],
  start: Month,
): BudgetSegment[] {
  const rebuilt: BudgetSegment[] = [
    { effective_from: start, amount: amountInMonth(amounts, start) },
    ...amounts.filter((s) => isBeforeMonth(start, s.effective_from)),
  ];
  return rebuilt.filter((s, i) => i === 0 || s.amount !== rebuilt[i - 1].amount);
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

/** The months a budget covers; `end` absent or null means open-ended. */
export interface BudgetSpan {
  start: Month;
  end?: Month | null;
}

/**
 * A recurring **expense** rule that books `categoryId` during `span`.
 *
 * Such a rule charges its full amount every month it runs, so it would consume
 * the budget on day one while still double-counting against `available` — the
 * form offers to retire it and the screen flags it. The date check matters
 * because that offer is destructive: a rule that already ended (or hasn't begun)
 * shares the category but never overlaps, so deleting it would be pure loss.
 */
export function clashingExpenseRule(
  rules: readonly RecurringRule[],
  categoryId: CategoryId,
  span: BudgetSpan,
): RecurringRule | undefined {
  return rules.find((rule) => {
    if (rule.kind !== 'expense' || rule.category_id !== categoryId) {
      return false;
    }
    const ruleStart = isoMonthOf(rule.start_date);
    const ruleEnd = rule.end.kind === 'until' ? isoMonthOf(rule.end.date) : null;
    if (ruleEnd && isBeforeMonth(ruleEnd, span.start)) {
      return false; // the rule finished before the budget begins
    }
    if (span.end && isBeforeMonth(span.end, ruleStart)) {
      return false; // the budget finishes before the rule begins
    }
    return true;
  });
}
