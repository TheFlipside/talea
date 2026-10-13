/**
 * Keeps the native home-screen widget's abstract snapshot up to date while the
 * app is open. For every account it computes the current month's ring fraction
 * (reusing `ringView`, so money stays string-typed at the boundary) and pushes
 * the abstract per-account health to shared storage whenever it changes.
 * Renders nothing; a no-op on platforms without the widget plugin.
 */

import { useQueries } from '@tanstack/react-query';
import { useEffect } from 'react';

import * as api from '../api/commands';
import { useAccounts } from '../api/hooks';
import { queryKeys } from '../api/queryKeys';
import type { MonthSummary } from '../api/types';
import { publishBudgetHealth, type AccountHealth } from '../lib/budgetWidget';
import { isoMonthOf } from '../lib/date';
import { ringView } from '../lib/ring';
import { useSettings, useToday } from '../state/contexts';

export function BudgetWidgetPublisher() {
  const { ringMode } = useSettings();
  const { data: accounts } = useAccounts();

  // The widget always reflects the wall-clock current month, read from the
  // app's one clock (`TodayProvider`) that the month bar's "Balance today"
  // follows too: it moves on at local midnight and on resume, so a session
  // left open past a month boundary neither keeps publishing the previous
  // month nor disagrees with the bar about which month it is.
  const month = isoMonthOf(useToday());

  const summaries = useQueries({
    queries: (accounts ?? []).map((account) => ({
      queryKey: queryKeys.monthSummary(account.id, month),
      queryFn: () => api.monthSummary(account.id, month),
    })),
  });

  // Only publish once every account's current-month summary has loaded.
  const accountList = accounts ?? [];
  const ready =
    accountList.length > 0 &&
    summaries.length === accountList.length &&
    summaries.every((query) => query.data);

  const payload: AccountHealth[] = ready
    ? accountList.map((account, index) => {
        const view = ringView(summaries[index].data as MonthSummary, ringMode);
        return {
          id: String(account.id),
          name: account.name,
          fraction: view.fraction,
          overspent: view.overspent,
        };
      })
    : [];

  // A stable string key so the effect republishes only when the snapshot changes.
  const signature = ready ? JSON.stringify(payload) : '';

  useEffect(() => {
    if (!signature) {
      return;
    }
    void publishBudgetHealth(JSON.parse(signature) as AccountHealth[]);
  }, [signature]);

  return null;
}
