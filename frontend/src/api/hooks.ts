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
  CommandError,
  Entry,
  EntryId,
  Month,
  MonthSummary,
  NewAccount,
  NewEntry,
} from './types';

/**
 * Refresh everything that an entry change can affect. Carry-over couples every
 * later month, so we invalidate the account's entries AND *all* of its cached
 * month summaries (prefix match), not just the edited month.
 */
function invalidateAccountData(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.entries(accountId) });
  void client.invalidateQueries({ queryKey: queryKeys.monthSummaryByAccount(accountId) });
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

export function useCreateEntry(accountId: AccountId) {
  const client = useQueryClient();
  return useMutation<Entry, CommandError, NewEntry>({
    mutationFn: api.createEntry,
    onSuccess: () => invalidateAccountData(client, accountId),
  });
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
