/** Central TanStack Query key factory. */

import type { AccountId, Month } from './types';

export const queryKeys = {
  accounts: ['accounts'] as const,
  categories: ['categories'] as const,
  /** Prefix matching every account's entries (e.g. after a category delete). */
  entriesAll: ['entries'] as const,
  entries: (accountId: AccountId) => ['entries', accountId] as const,
  /** Month flattened to primitives so equality is stable across object identity. */
  monthSummary: (accountId: AccountId, month: Month) =>
    ['monthSummary', accountId, month.year, month.month] as const,
  /** Prefix matching every cached month summary for an account. */
  monthSummaryByAccount: (accountId: AccountId) => ['monthSummary', accountId] as const,
  /** Per-month category expense breakdown (stats screen). */
  expensesByCategory: (accountId: AccountId, month: Month) =>
    ['expensesByCategory', accountId, month.year, month.month] as const,
  /** Prefix matching every cached breakdown for an account. */
  expensesByCategoryByAccount: (accountId: AccountId) =>
    ['expensesByCategory', accountId] as const,
  /** Prefix matching every account's breakdowns (e.g. after a category delete,
   *  which re-buckets entries across accounts into "Other"). */
  expensesByCategoryAll: ['expensesByCategory'] as const,
};
