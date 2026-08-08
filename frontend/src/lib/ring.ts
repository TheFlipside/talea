/** Budget-ring view model: what the ring/percentage shows. */

import type { MonthSummary } from '../api/types';
import { parseMoneyForDisplay } from './money';

export type RingMode = 'spent' | 'remaining';

export interface RingView {
  /** Fraction to fill, 0..1. */
  fraction: number;
  /** Rounded percentage, 0..100. */
  percent: number;
  /** Whether nothing is left to spend freely (free < 0). */
  overspent: boolean;
  /** Translation key for the accessible/label text. */
  labelKey: 'summary.ringSpent' | 'summary.ringRemaining';
}

function clamp01(n: number): number {
  return Math.min(Math.max(n, 0), 1);
}

/**
 * Computes the ring view for a month summary. The ring answers "can I still
 * spend?", so it follows `free` (`available - reserved`) rather than
 * `available`: money a category budget is still holding back isn't free.
 *
 * `spent` shows what's committed — `funds - free`, i.e. expenses *plus* the
 * unspent reservation; `remaining` shows `free`. The two are exact complements
 * while `free` is within `0..funds`, and with no budgets `reserved` is 0 and
 * `free` equals `available`, so both reduce to the pre-budget behaviour.
 *
 * Parsing here is display-only — the figures of record stay strings from the
 * backend.
 */
export function ringView(summary: MonthSummary, mode: RingMode): RingView {
  const funds = parseMoneyForDisplay(summary.carry_in) + parseMoneyForDisplay(summary.income);
  const free = parseMoneyForDisplay(summary.free);

  const fraction = funds > 0 ? clamp01((mode === 'spent' ? funds - free : free) / funds) : 0;

  return {
    fraction,
    percent: Math.round(fraction * 100),
    overspent: free < 0,
    labelKey: mode === 'spent' ? 'summary.ringSpent' : 'summary.ringRemaining',
  };
}
