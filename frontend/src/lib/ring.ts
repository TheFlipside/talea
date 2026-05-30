/** Budget-ring view model: what the ring/percentage shows. */

import type { MonthSummary } from '../api/types';
import { parseMoneyForDisplay } from './money';

export type RingMode = 'spent' | 'remaining';

export interface RingView {
  /** Fraction to fill, 0..1. */
  fraction: number;
  /** Rounded percentage, 0..100. */
  percent: number;
  /** Whether the month is overspent (available < 0). */
  overspent: boolean;
  /** Translation key for the accessible/label text. */
  labelKey: 'summary.ringSpent' | 'summary.ringRemaining';
}

function clamp01(n: number): number {
  return Math.min(Math.max(n, 0), 1);
}

/**
 * Computes the ring view for a month summary. `spent` shows expenses over funds
 * available before spending; `remaining` shows what's left. Parsing here is
 * display-only — the figures of record stay strings from the backend.
 */
export function ringView(summary: MonthSummary, mode: RingMode): RingView {
  const funds = parseMoneyForDisplay(summary.carry_in) + parseMoneyForDisplay(summary.income);
  const spent = parseMoneyForDisplay(summary.expenses);
  const available = parseMoneyForDisplay(summary.available);

  const fraction =
    funds > 0 ? clamp01((mode === 'spent' ? spent : available) / funds) : 0;

  return {
    fraction,
    percent: Math.round(fraction * 100),
    overspent: available < 0,
    labelKey: mode === 'spent' ? 'summary.ringSpent' : 'summary.ringRemaining',
  };
}
