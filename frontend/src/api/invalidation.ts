/**
 * What each kind of write makes stale in the query cache, shared by the
 * mutation hooks. In its own module so every write's effect on the cache can be
 * tested against a real `QueryClient`.
 */

import type { QueryClient, QueryKey } from '@tanstack/react-query';

import { accountsAffectedBy, type Membership } from '../lib/accounts';
import { queryKeys } from './queryKeys';
import type { AccountId } from './types';

/** Which of an account's record lists a write touched. */
interface ChangedRecords {
  entries?: boolean;
  occurrences?: boolean;
}

/**
 * The views computed from an account's whole ledger: every cached month
 * summary and day balance (carry-over couples every later month, so all of
 * them, by prefix match), category breakdown, and budget progress (spending in
 * a budgeted category consumes its reservation, moving `reserved`/`free`).
 */
function ledgerFigures(id: AccountId): QueryKey[] {
  return [
    queryKeys.monthSummaryByAccount(id),
    queryKeys.balanceAsOfByAccount(id),
    queryKeys.expensesByCategoryByAccount(id),
    queryKeys.monthBudgetsByAccount(id),
  ];
}

/** The same views for every account at once. */
const EVERY_LEDGER_FIGURE: readonly QueryKey[] = [
  queryKeys.monthSummaryAll,
  queryKeys.balanceAsOfAll,
  queryKeys.expensesByCategoryAll,
  queryKeys.monthBudgetsAll,
];

/**
 * Whether a cached row is an account the membership lookup can read safely and
 * match correctly: anything looser either throws in the lookup or quietly
 * fails to match a summary, leaving it stale with no fallback.
 */
function isMembership(row: unknown): row is Membership {
  return (
    typeof row === 'object' &&
    row !== null &&
    'id' in row &&
    Number.isInteger(row.id) &&
    'kind' in row &&
    (row.kind === 'normal' || row.kind === 'summary') &&
    'members' in row &&
    Array.isArray(row.members) &&
    // Densified like the rows below, so a hole can't pass as a member id.
    Array.from(row.members).every((member) => Number.isInteger(member))
  );
}

/**
 * The cached account list, if it can be trusted to say which summaries contain
 * which accounts; `null` if not.
 *
 * `getQueryData` is an unchecked cast, and this runs inside mutation callbacks,
 * where a throw would report an already-committed write as failed and invite a
 * duplicating retry — so the list's shape is checked here, at the one place
 * untyped cache data enters. An invalidated list is distrusted too: its refetch
 * is pending, e.g. right after a membership edit, so it may be out of date. If
 * that refetch fails, the list stays distrusted until one succeeds, and writes
 * keep falling back to the sweep: wider than needed, but never stale. (This
 * rests on the list being fetched only through its own query: a `fetchQuery`
 * or `prefetchQuery` on it could clear the flag with a pre-edit read.)
 */
function trustedMemberships(client: QueryClient): Membership[] | null {
  if (client.getQueryState(queryKeys.accounts)?.isInvalidated) {
    return null;
  }
  const cached: unknown = client.getQueryData(queryKeys.accounts);
  if (!Array.isArray(cached)) {
    return null;
  }
  // A dense copy: `every` skips holes, which the lookup's loop would then read
  // as `undefined`. Copied, a hole becomes `undefined` and fails the check.
  const rows: unknown[] = Array.from(cached);
  return rows.every(isMembership) ? rows : null;
}

/** Every account's ledger views — the fallback when no account list can say
 *  which summaries a changed account belongs to. */
function invalidateEveryLedgerView(client: QueryClient, records: ChangedRecords): void {
  const keys = [...EVERY_LEDGER_FIGURE];
  if (records.entries) {
    keys.push(queryKeys.entriesAll);
  }
  if (records.occurrences) {
    keys.push(queryKeys.occurrencesAll);
  }
  for (const key of keys) {
    void client.invalidateQueries({ queryKey: key });
  }
}

/**
 * Refresh the views built from the `changed` accounts' records: the record
 * lists named in `records`, plus every ledger figure — for each changed account
 * and every summary containing one, each key once.
 *
 * Without the summaries, a member's change left a summary's figures and merged
 * list stale until its cache expired — and the home-screen widget, which keeps
 * every account's month summary mounted, could show it stale all session.
 */
function invalidateLedgerViews(
  client: QueryClient,
  changed: readonly AccountId[],
  records: ChangedRecords,
): void {
  const memberships = trustedMemberships(client);
  if (memberships === null) {
    // Refreshing every account beats leaving a summary stale.
    invalidateEveryLedgerView(client, records);
    return;
  }
  for (const id of accountsAffectedBy(memberships, changed)) {
    const keys = ledgerFigures(id);
    if (records.entries) {
      keys.push(queryKeys.entries(id));
    }
    if (records.occurrences) {
      keys.push(queryKeys.occurrencesByAccount(id));
    }
    for (const key of keys) {
      void client.invalidateQueries({ queryKey: key });
    }
  }
}

/**
 * After an entry is created, edited or deleted. A transfer writes one on each
 * of two accounts: pass both together, so a summary containing both refreshes
 * once rather than twice.
 */
export function invalidateEntryData(client: QueryClient, accountIds: readonly AccountId[]): void {
  invalidateLedgerViews(client, accountIds, { entries: true });
}

/** After a single occurrence is skipped: the month's occurrences and the
 *  ledger figures change, but not the rule list itself. */
export function invalidateOccurrenceData(client: QueryClient, accountId: AccountId): void {
  invalidateLedgerViews(client, [accountId], { occurrences: true });
}

/** After an occurrence is detached: it leaves the occurrences and arrives as a
 *  standalone entry, so both lists change. */
export function invalidateDetachData(client: QueryClient, accountId: AccountId): void {
  invalidateLedgerViews(client, [accountId], { entries: true, occurrences: true });
}

/**
 * After a recurring rule is created, edited or deleted: the rule list and,
 * since rules feed the ledger, the occurrences and every derived figure. Only
 * the account itself lists the rule: a summary keeps no rules of its own.
 */
export function invalidateRuleData(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.rules(accountId) });
  invalidateLedgerViews(client, [accountId], { occurrences: true });
}

/**
 * After a budget change: the budget list, the month's progress, and the month
 * summaries carrying `reserved`/`free` (and so the ring).
 *
 * Everything is invalidated for **every** account, not just this one: a summary
 * account aggregates its members' budgets *and* their `reserved`, so scoping to
 * the mutated account would leave the summary's list and its "Free to spend"
 * figure stale — the latter visible on the home-screen widget. Budget edits are
 * rare, so the wider sweep costs nothing in practice; entry and rule writes,
 * which are not, reach just the accounts and the summaries containing them.
 */
export function invalidateBudgetData(client: QueryClient): void {
  void client.invalidateQueries({ queryKey: queryKeys.budgetsAll });
  void client.invalidateQueries({ queryKey: queryKeys.monthBudgetsAll });
  void client.invalidateQueries({ queryKey: queryKeys.monthSummaryAll });
}

/**
 * After an account is deleted. Its records cascade away with it, so its own
 * views are dropped rather than refreshed into errors. (A view still showing
 * the account would rebuild its query and fail once, until the refreshed
 * account list moves it on; deletion happens on the accounts screen, where
 * none does.) Deletions are
 * rare, so every other account's ledger views and budget lists are swept too,
 * rather than leaning on the backend refusing to delete a summary's member.
 */
export function invalidateDeletedAccount(client: QueryClient, accountId: AccountId): void {
  void client.invalidateQueries({ queryKey: queryKeys.accounts });
  for (const key of [
    ...ledgerFigures(accountId),
    queryKeys.entries(accountId),
    queryKeys.occurrencesByAccount(accountId),
    queryKeys.rules(accountId),
    queryKeys.budgets(accountId),
  ]) {
    client.removeQueries({ queryKey: key });
  }
  invalidateEveryLedgerView(client, { entries: true, occurrences: true });
  void client.invalidateQueries({ queryKey: queryKeys.budgetsAll });
}
