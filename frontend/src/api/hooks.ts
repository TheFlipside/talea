/** TanStack Query hooks over the typed commands. */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import * as api from './commands';
import {
  invalidateBudgetData,
  invalidateDeletedAccount,
  invalidateDetachData,
  invalidateEntryData,
  invalidateOccurrenceData,
  invalidateRuleData,
} from './invalidation';
import { queryKeys } from './queryKeys';
import type {
  Account,
  AccountId,
  Budget,
  BudgetId,
  BudgetStatus,
  Category,
  CategoryExpense,
  CategoryId,
  CommandError,
  Entry,
  EntryId,
  IsoDate,
  Money,
  Month,
  MonthSummary,
  NewAccount,
  NewBudget,
  NewCategory,
  NewEntry,
  NewRule,
  NextcloudConfigView,
  Occurrence,
  RecurringRule,
  RecurringRuleId,
} from './types';

export function useAccounts() {
  return useQuery<Account[], CommandError>({
    queryKey: queryKeys.accounts,
    queryFn: api.listAccounts,
  });
}

export function useEntries(accountId: AccountId) {
  return useQuery<Entry[], CommandError>({
    queryKey: queryKeys.entries(accountId),
    queryFn: () => api.listEntries(accountId),
  });
}

export function useMonthSummary(accountId: AccountId, month: Month) {
  return useQuery<MonthSummary, CommandError>({
    queryKey: queryKeys.monthSummary(accountId, month),
    queryFn: () => api.monthSummary(accountId, month),
  });
}

/** The account's running balance at the end of `date` (in practice, today). */
export function useBalanceAsOf(accountId: AccountId, date: IsoDate) {
  return useQuery<Money, CommandError>({
    queryKey: queryKeys.balanceAsOf(accountId, date),
    queryFn: () => api.balanceAsOf(accountId, date),
  });
}

export function useExpensesByCategory(accountId: AccountId, month: Month) {
  return useQuery<CategoryExpense[], CommandError>({
    queryKey: queryKeys.expensesByCategory(accountId, month),
    queryFn: () => api.expensesByCategory(accountId, month),
  });
}

export function useCreateAccount() {
  const client = useQueryClient();
  return useMutation<Account, CommandError, NewAccount>({
    mutationFn: api.createAccount,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.accounts });
    },
  });
}

export function useUpdateAccount() {
  const client = useQueryClient();
  return useMutation<Account, CommandError, Account>({
    mutationFn: api.updateAccount,
    onSuccess: () => {
      // An account edit can move anything derived from it, for every account:
      // the opening balance and anchor seed each running balance (a summary's
      // totals included), and a summary's membership decides its whole view —
      // merged list, stats, budgets. Edits are rare, and only on-screen queries
      // refetch (the rest are just marked stale), so refresh everything.
      void client.invalidateQueries();
    },
  });
}

export function useDeleteAccount() {
  const client = useQueryClient();
  return useMutation<void, CommandError, AccountId>({
    mutationFn: api.deleteAccount,
    onSuccess: (_data, accountId) => invalidateDeletedAccount(client, accountId),
  });
}

export function useCategories() {
  return useQuery<Category[], CommandError>({
    queryKey: queryKeys.categories,
    queryFn: api.listCategories,
  });
}

export function useCreateCategory() {
  const client = useQueryClient();
  return useMutation<Category, CommandError, NewCategory>({
    mutationFn: api.createCategory,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.categories });
    },
  });
}

export function useUpdateCategory() {
  const client = useQueryClient();
  return useMutation<Category, CommandError, Category>({
    mutationFn: api.updateCategory,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.categories });
    },
  });
}

export function useDeleteCategory() {
  const client = useQueryClient();
  return useMutation<void, CommandError, CategoryId>({
    mutationFn: api.deleteCategory,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.categories });
      // Deleting a category nulls it on referencing entries AND rules (across all
      // accounts); refresh entry lists, rules, occurrences, and the stats
      // breakdowns, where those move into the "Other" bucket / lose the category.
      void client.invalidateQueries({ queryKey: queryKeys.entriesAll });
      void client.invalidateQueries({ queryKey: queryKeys.expensesByCategoryAll });
      void client.invalidateQueries({ queryKey: queryKeys.rulesAll });
      void client.invalidateQueries({ queryKey: queryKeys.occurrencesAll });
      // Budgets, unlike entries, are *deleted* with their category (a budget has
      // nothing left to measure without one). That drops `reserved` and moves
      // `free`, so — unlike the rows above — the month summaries really do change
      // and every account's must be refreshed, or the ring goes stale.
      void client.invalidateQueries({ queryKey: queryKeys.budgetsAll });
      void client.invalidateQueries({ queryKey: queryKeys.monthBudgetsAll });
      void client.invalidateQueries({ queryKey: queryKeys.monthSummaryAll });
    },
  });
}

export function useCreateEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<Entry, CommandError, NewEntry>({
    mutationFn: api.createEntry,
    onSuccess: () => invalidateEntryData(client, [accountId]),
  });
}

export function useCreateTransfer(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<[Entry, Entry], CommandError, { entry: NewEntry; counterAccountId: AccountId }>(
    {
      mutationFn: ({ entry, counterAccountId }) => api.createTransfer(entry, counterAccountId),
      onSuccess: (_data, { counterAccountId }) => {
        // A transfer writes an entry on each account. Refresh both in one call,
        // so a summary containing both is refreshed once, not twice.
        invalidateEntryData(client, [accountId, counterAccountId]);
      },
    },
  );
}

export function useUpdateEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<Entry, CommandError, Entry>({
    mutationFn: api.updateEntry,
    onSuccess: () => invalidateEntryData(client, [accountId]),
  });
}

export function useDeleteEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<void, CommandError, EntryId>({
    mutationFn: api.deleteEntry,
    onSuccess: () => invalidateEntryData(client, [accountId]),
  });
}

export function useRules(accountId: AccountId) {
  return useQuery<RecurringRule[], CommandError>({
    queryKey: queryKeys.rules(accountId),
    queryFn: () => api.listRules(accountId),
  });
}

export function useMonthOccurrences(accountId: AccountId, month: Month) {
  return useQuery<Occurrence[], CommandError>({
    queryKey: queryKeys.occurrences(accountId, month),
    queryFn: () => api.monthOccurrences(accountId, month),
  });
}

export function useCreateRule(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<RecurringRule, CommandError, NewRule>({
    mutationFn: api.createRule,
    onSuccess: () => invalidateRuleData(client, accountId),
  });
}

export function useUpdateRule(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<RecurringRule, CommandError, RecurringRule>({
    mutationFn: api.updateRule,
    onSuccess: () => invalidateRuleData(client, accountId),
  });
}

export function useDeleteRule(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<void, CommandError, RecurringRuleId>({
    mutationFn: api.deleteRule,
    onSuccess: () => invalidateRuleData(client, accountId),
  });
}

export function useSkipOccurrence(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<void, CommandError, { ruleId: RecurringRuleId; date: IsoDate }>({
    mutationFn: ({ ruleId, date }) => api.skipOccurrence(accountId, ruleId, date),
    onSuccess: () => invalidateOccurrenceData(client, accountId),
  });
}

export function useDetachOccurrence(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<
    Entry,
    CommandError,
    { ruleId: RecurringRuleId; date: IsoDate; entry: NewEntry }
  >({
    mutationFn: ({ ruleId, date, entry }) => api.detachOccurrence(accountId, ruleId, date, entry),
    onSuccess: () => {
      // A detach removes an occurrence AND adds a standalone entry.
      invalidateDetachData(client, accountId);
    },
  });
}

// ---- Category budgets -------------------------------------------------------

export function useBudgets(accountId: AccountId) {
  return useQuery<Budget[], CommandError>({
    queryKey: queryKeys.budgets(accountId),
    queryFn: () => api.listBudgets(accountId),
  });
}

export function useMonthBudgets(accountId: AccountId, month: Month) {
  return useQuery<BudgetStatus[], CommandError>({
    queryKey: queryKeys.monthBudgets(accountId, month),
    queryFn: () => api.monthBudgets(accountId, month),
  });
}

// The budget mutations take no account id: their invalidation is deliberately
// account-wide (see `invalidateBudgetData`).

export function useCreateBudget() {
  const client = useQueryClient();
  return useMutation<Budget, CommandError, NewBudget>({
    mutationFn: api.createBudget,
    onSuccess: () => invalidateBudgetData(client),
  });
}

export function useUpdateBudget() {
  const client = useQueryClient();
  return useMutation<Budget, CommandError, Budget>({
    mutationFn: api.updateBudget,
    onSuccess: () => invalidateBudgetData(client),
  });
}

export function useDeleteBudget() {
  const client = useQueryClient();
  return useMutation<void, CommandError, BudgetId>({
    mutationFn: api.deleteBudget,
    onSuccess: () => invalidateBudgetData(client),
  });
}

// ---- Nextcloud backup / restore --------------------------------------------

export function useNextcloudConfig() {
  return useQuery<NextcloudConfigView, CommandError>({
    queryKey: queryKeys.nextcloudConfig,
    queryFn: api.nextcloudGetConfig,
  });
}

export function useNextcloudSetConfig() {
  const client = useQueryClient();
  return useMutation<void, CommandError, { baseUrl: string; username: string; password: string }>({
    mutationFn: ({ baseUrl, username, password }) =>
      api.nextcloudSetConfig(baseUrl, username, password),
    // The app password rides in this mutation's variables: once the caller
    // resets it, drop it from the cache at once, not after the default 5 min.
    // (Pass no per-call options to `mutate`: the observer keeps those past a
    // reset, along with anything their callbacks close over.)
    gcTime: 0,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.nextcloudConfig });
    },
  });
}

/** Tests the connection. No cache effect — the caller surfaces success/error. */
export function useNextcloudTest() {
  return useMutation<void, CommandError, void>({ mutationFn: api.nextcloudTest });
}

/** Uploads a backup; resolves to its RFC-3339 timestamp and refreshes the
 *  "last backup" line via the config query. */
export function useBackupNow() {
  const client = useQueryClient();
  return useMutation<string, CommandError, void>({
    mutationFn: api.backupNow,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.nextcloudConfig });
    },
  });
}

/** Restores from Nextcloud, replacing all local data. Every cached query is now
 *  stale (the whole database changed), so invalidate everything. */
export function useRestoreNow() {
  const client = useQueryClient();
  return useMutation<void, CommandError, void>({
    mutationFn: api.restoreNow,
    onSuccess: () => {
      void client.invalidateQueries();
    },
  });
}
