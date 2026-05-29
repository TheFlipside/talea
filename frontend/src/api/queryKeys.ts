/** Central TanStack Query key factory. */

import type { AccountId, Month } from './types';

export const queryKeys = {
  accounts: ['accounts'] as const,
  entries: (accountId: AccountId) => ['entries', accountId] as const,
  /** Month flattened to primitives so equality is stable across object identity. */
  monthSummary: (accountId: AccountId, month: Month) =>
    ['monthSummary', accountId, month.year, month.month] as const,
  /** Prefix matching every cached month summary for an account. */
  monthSummaryByAccount: (accountId: AccountId) => ['monthSummary', accountId] as const,
};
