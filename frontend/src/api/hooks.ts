/** TanStack Query hooks over the typed commands. */

import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';

import * as api from './commands';
import { queryKeys } from './queryKeys';
import type {
  Account,
  AccountId,
  Category,
  CategoryExpense,
  CategoryId,
  CommandError,
  Entry,
  EntryId,
  IsoDate,
  Month,
  MonthSummary,
  NewAccount,
  NewCategory,
  NewEntry,
  NewRule,
  Occurrence,
  RecurringRule,
  RecurringRuleId,
} from './types';

/**
 * Refresh everything that an entry change can affect. Carry-over couples every
 * later month, so we invalidate the account's entries AND *all* of its cached
 * month summaries (prefix match), not just the edited month.
 */
function invalidateAccountData(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.entries(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.monthSummaryByAccount(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.expensesByCategoryByAccount(accountId) });
}

/**
 * Refresh everything a recurring-rule change can affect: the rule list, the
 * month's occurrences, and (since rules feed the ledger) all month summaries and
 * category breakdowns for the account.
 */
function invalidateRuleData(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.rules(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.occurrencesByAccount(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.monthSummaryByAccount(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.expensesByCategoryByAccount(accountId) });
}

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
      void client.invalidateQueries({ queryKey: queryKeys.accounts });
    },
  });
}

export function useDeleteAccount() {
  const client = useQueryClient();
  return useMutation<void, CommandError, AccountId>({
    mutationFn: api.deleteAccount,
    onSuccess: (_data, accountId) => {
      void client.invalidateQueries({ queryKey: queryKeys.accounts });
      invalidateAccountData(client, accountId);
    },
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
    },
  });
}

export function useCreateEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<Entry, CommandError, NewEntry>({
    mutationFn: api.createEntry,
    onSuccess: () => invalidateAccountData(client, accountId),
  });
}

export function useCreateTransfer(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<[Entry, Entry], CommandError, { entry: NewEntry; counterAccountId: AccountId }>(
    {
      mutationFn: ({ entry, counterAccountId }) => api.createTransfer(entry, counterAccountId),
      onSuccess: (_data, { counterAccountId }) => {
        // A transfer writes an entry on each account; refresh both.
        invalidateAccountData(client, accountId);
        invalidateAccountData(client, counterAccountId);
      },
    },
  );
}

export function useUpdateEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<Entry, CommandError, Entry>({
    mutationFn: api.updateEntry,
    onSuccess: () => invalidateAccountData(client, accountId),
  });
}

export function useDeleteEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<void, CommandError, EntryId>({
    mutationFn: api.deleteEntry,
    onSuccess: () => invalidateAccountData(client, accountId),
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

/** Removing/editing a single occurrence changes the month's occurrences and the
 *  ledger-derived figures (summary, stats), but not the rule list itself. */
function invalidateOccurrenceData(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.occurrencesByAccount(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.monthSummaryByAccount(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.expensesByCategoryByAccount(accountId) });
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
      invalidateAccountData(client, accountId);
      void client.invalidateQueries({ queryKey: queryKeys.occurrencesByAccount(accountId) });
    },
  });
}
