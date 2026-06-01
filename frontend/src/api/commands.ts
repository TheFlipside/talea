/**
 * Typed wrappers over the Tauri commands.
 *
 * This is the single place that encodes Tauri's argument convention: the
 * command name is the snake_case Rust fn; scalar argument keys are camelCase
 * (`accountId`, `id`); a wrapped payload's outer key matches the Rust parameter
 * (`entry`, `account`) and its inner fields stay snake_case (see `types.ts`).
 */

import { call } from './client';
import type {
  Account,
  AccountId,
  Category,
  CategoryExpense,
  CategoryId,
  Entry,
  EntryId,
  IsoDate,
  Month,
  MonthSummary,
  NewAccount,
  NewCategory,
  NewEntry,
  NewRule,
  NextcloudConfigView,
  Occurrence,
  RecurringRule,
  RecurringRuleId,
} from './types';

export const listAccounts = (): Promise<Account[]> => call('list_accounts');

export const createAccount = (account: NewAccount): Promise<Account> =>
  call('create_account', { account });

export const updateAccount = (account: Account): Promise<Account> =>
  call('update_account', { account });

export const deleteAccount = (id: AccountId): Promise<void> =>
  call('delete_account', { id });

export const listEntries = (accountId: AccountId): Promise<Entry[]> =>
  call('list_entries', { accountId });

export const createEntry = (entry: NewEntry): Promise<Entry> =>
  call('create_entry', { entry });

/** Records a transfer: `entry` on its account + its mirror on `counterAccountId`.
 *  Returns the `[primary, counterpart]` entries. */
export const createTransfer = (
  entry: NewEntry,
  counterAccountId: AccountId,
): Promise<[Entry, Entry]> => call('create_transfer', { entry, counterAccountId });

export const updateEntry = (entry: Entry): Promise<Entry> =>
  call('update_entry', { entry });

export const deleteEntry = (id: EntryId): Promise<void> =>
  call('delete_entry', { id });

export const monthSummary = (accountId: AccountId, month: Month): Promise<MonthSummary> =>
  call('month_summary', { accountId, month });

export const expensesByCategory = (
  accountId: AccountId,
  month: Month,
): Promise<CategoryExpense[]> => call('expenses_by_category', { accountId, month });

export const listCategories = (): Promise<Category[]> => call('list_categories');

export const createCategory = (category: NewCategory): Promise<Category> =>
  call('create_category', { category });

export const updateCategory = (category: Category): Promise<Category> =>
  call('update_category', { category });

export const deleteCategory = (id: CategoryId): Promise<void> =>
  call('delete_category', { id });

export const listRules = (accountId: AccountId): Promise<RecurringRule[]> =>
  call('list_rules', { accountId });

export const createRule = (rule: NewRule): Promise<RecurringRule> =>
  call('create_rule', { rule });

export const updateRule = (rule: RecurringRule): Promise<RecurringRule> =>
  call('update_rule', { rule });

export const deleteRule = (id: RecurringRuleId): Promise<void> =>
  call('delete_rule', { id });

export const monthOccurrences = (accountId: AccountId, month: Month): Promise<Occurrence[]> =>
  call('month_occurrences', { accountId, month });

export const skipOccurrence = (
  accountId: AccountId,
  ruleId: RecurringRuleId,
  date: IsoDate,
): Promise<void> =>
  call('skip_occurrence', { accountId, occurrence: { rule_id: ruleId, date } });

export const detachOccurrence = (
  accountId: AccountId,
  ruleId: RecurringRuleId,
  date: IsoDate,
  entry: NewEntry,
): Promise<Entry> =>
  call('detach_occurrence', { accountId, occurrence: { rule_id: ruleId, date }, entry });

// ---- Nextcloud backup / restore --------------------------------------------

/** The stored Nextcloud settings (never the password — see `NextcloudConfigView`). */
export const nextcloudGetConfig = (): Promise<NextcloudConfigView> =>
  call('nextcloud_get_config');

/** Saves the address/username and, when `password` is non-empty, the app
 *  password (an empty string keeps the stored one). */
export const nextcloudSetConfig = (
  baseUrl: string,
  username: string,
  password: string,
): Promise<void> => call('nextcloud_set_config', { baseUrl, username, password });

/** Verifies the stored address and credentials against the server. */
export const nextcloudTest = (): Promise<void> => call('nextcloud_test');

/** Snapshots the database and uploads it; resolves to the backup's RFC-3339 time. */
export const backupNow = (): Promise<string> => call('backup_now');

/** Downloads the latest backup and replaces all local data with it. */
export const restoreNow = (): Promise<void> => call('restore_now');
