import { QueryClient, type QueryKey } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';

import {
  invalidateBudgetData,
  invalidateDeletedAccount,
  invalidateDetachData,
  invalidateEntryData,
  invalidateOccurrenceData,
  invalidateRuleData,
} from '../invalidation';
import { queryKeys } from '../queryKeys';
import { account } from '../../test/fixtures';
import type { Account, AccountId, IsoDate, Month } from '../types';

// Summary 10 aggregates accounts 1 and 2; account 3 stands alone. Two months
// and two days are cached per account, so prefix matching is exercised.
const IDS: AccountId[] = [1, 2, 3, 10];
const MONTHS: Month[] = [
  { year: 2026, month: 2 },
  { year: 2026, month: 3 },
];
const DAYS: IsoDate[] = ['2026-02-28', '2026-03-10'];

function accounts(): Account[] {
  return [account(1), account(2), account(3), account(10, 'summary', [1, 2])];
}

/** Every ledger-derived view of one account, across the cached months and days. */
function figures(id: AccountId): QueryKey[] {
  return [
    ...MONTHS.flatMap((m) => [
      queryKeys.monthSummary(id, m),
      queryKeys.expensesByCategory(id, m),
      queryKeys.monthBudgets(id, m),
    ]),
    ...DAYS.map((d) => queryKeys.balanceAsOf(id, d)),
  ];
}

const entries = (id: AccountId): QueryKey[] => [queryKeys.entries(id)];
const occurrences = (id: AccountId): QueryKey[] => MONTHS.map((m) => queryKeys.occurrences(id, m));
const ownLists = (id: AccountId): QueryKey[] => [queryKeys.rules(id), queryKeys.budgets(id)];

/**
 * A client caching every view of every account, plus unrelated queries no
 * records change may touch. `accountList` seeds the account list as given, so
 * tests can leave it out or corrupt it.
 */
function seeded(accountList?: unknown): QueryClient {
  // `gcTime: Infinity` schedules no garbage-collection timers in the test process.
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });
  if (accountList !== undefined) {
    client.setQueryData(queryKeys.accounts, accountList);
  }
  client.setQueryData(queryKeys.categories, []);
  client.setQueryData(queryKeys.nextcloudConfig, {});
  for (const id of IDS) {
    for (const key of [...figures(id), ...entries(id), ...occurrences(id), ...ownLists(id)]) {
      client.setQueryData(key, []);
    }
  }
  return client;
}

/** The keys of every cached query now marked stale, comparable as a sorted list. */
function staleKeys(client: QueryClient): string[] {
  return client
    .getQueryCache()
    .getAll()
    .filter((query) => query.state.isInvalidated)
    .map((query) => JSON.stringify(query.queryKey))
    .sort();
}

function keyList(keys: QueryKey[]): string[] {
  return keys.map((key) => JSON.stringify(key)).sort();
}

describe('invalidateEntryData', () => {
  it("refreshes a member's entries and figures, and its summary's — nothing else", () => {
    const client = seeded(accounts());
    invalidateEntryData(client, [1]);
    expect(staleKeys(client)).toEqual(
      keyList([1, 10].flatMap((id) => [...entries(id), ...figures(id)])),
    );
  });

  // Both orders: the hook passes the viewed account first, member or not.
  it.each([
    [[2, 3]],
    [[3, 2]],
  ])('refreshes both sides of a transfer %j and the summary over either', (sides) => {
    const client = seeded(accounts());
    invalidateEntryData(client, sides);
    expect(staleKeys(client)).toEqual(
      keyList([2, 3, 10].flatMap((id) => [...entries(id), ...figures(id)])),
    );
  });
});

describe('invalidateOccurrenceData', () => {
  it('refreshes occurrences and figures, but neither entries nor rules', () => {
    const client = seeded(accounts());
    invalidateOccurrenceData(client, 1);
    expect(staleKeys(client)).toEqual(
      keyList([1, 10].flatMap((id) => [...occurrences(id), ...figures(id)])),
    );
  });
});

describe('invalidateDetachData', () => {
  it('refreshes both record lists and the figures', () => {
    const client = seeded(accounts());
    invalidateDetachData(client, 1);
    expect(staleKeys(client)).toEqual(
      keyList([1, 10].flatMap((id) => [...entries(id), ...occurrences(id), ...figures(id)])),
    );
  });
});

describe('invalidateRuleData', () => {
  it("refreshes the account's own rule list, plus occurrences and figures", () => {
    const client = seeded(accounts());
    invalidateRuleData(client, 1);
    expect(staleKeys(client)).toEqual(
      keyList([
        queryKeys.rules(1),
        ...[1, 10].flatMap((id) => [...occurrences(id), ...figures(id)]),
      ]),
    );
  });
});

describe('invalidateBudgetData', () => {
  it("refreshes every account's budget lists, progress and month summaries", () => {
    const client = seeded(accounts());
    invalidateBudgetData(client);
    expect(staleKeys(client)).toEqual(
      keyList(
        IDS.flatMap((id) => [
          queryKeys.budgets(id),
          ...MONTHS.flatMap((m) => [queryKeys.monthBudgets(id, m), queryKeys.monthSummary(id, m)]),
        ]),
      ),
    );
  });
});

describe('invalidateDeletedAccount', () => {
  it("drops the account's own views and sweeps everyone else's", () => {
    const client = seeded(accounts());
    invalidateDeletedAccount(client, 3);

    for (const key of [...entries(3), ...occurrences(3), ...figures(3), ...ownLists(3)]) {
      expect(client.getQueryState(key), JSON.stringify(key)).toBeUndefined();
    }
    expect(staleKeys(client)).toEqual(
      keyList([
        queryKeys.accounts,
        ...[1, 2, 10].flatMap((id) => [
          ...entries(id),
          ...occurrences(id),
          ...figures(id),
          queryKeys.budgets(id),
        ]),
      ]),
    );
  });
});

describe('when the account list cannot be trusted', () => {
  /** What an entry write must refresh when no list can name the summaries. */
  const sweep = keyList(IDS.flatMap((id) => [...entries(id), ...figures(id)]));

  it('refreshes every account when the list is not cached', () => {
    const client = seeded();
    invalidateEntryData(client, [1]);
    expect(staleKeys(client)).toEqual(sweep);
  });

  it('refreshes every account while the list awaits a refetch', () => {
    // E.g. right after a membership edit: the cached membership may be stale.
    const client = seeded(accounts());
    void client.invalidateQueries({ queryKey: queryKeys.accounts });
    invalidateEntryData(client, [1]);
    expect(staleKeys(client)).toEqual(
      [...sweep, JSON.stringify(queryKeys.accounts)].sort(),
    );
  });

  const summary10 = account(10, 'summary', [1, 2]);
  // A hole, which `every` alone would skip: `[a, , b]` but lint-clean.
  const sparse: unknown[] = new Array<unknown>(3);
  sparse[0] = account(1);
  sparse[2] = summary10;

  const malformed: [string, unknown][] = [
    ['null', null],
    ['an object', {}],
    ['a string', 'abc'],
    ['a number', 7],
    ['a null row', [null]],
    ['a sparse list', sparse],
    // A bad row among good ones, either side: every row must pass, not one.
    ['a null row after good ones', [account(1), summary10, null]],
    ['a null row before good ones', [null, account(1), summary10]],
    // Each field on its own, so no check can hide behind another.
    ['a row without an id', [{ kind: 'summary', members: [1] }]],
    ['a row with a string id', [{ ...summary10, id: '10' }]],
    ['a row with a NaN id', [{ ...summary10, id: Number.NaN }]],
    ['a row without a kind', [{ id: 10, members: [1] }]],
    ['a row with a numeric kind', [{ ...summary10, kind: 5 }]],
    ['a row with an unknown kind', [{ ...summary10, kind: 'Summary' }]],
    ['a row without members', [{ id: 10, kind: 'summary' }]],
    ...[null, 'abc', {}, 7, ['1'], [null], [1.5], [Number.NaN]].map((members): [string, unknown] => [
      `a summary with members ${JSON.stringify(members)}`,
      [{ ...summary10, members }],
    ]),
  ];

  it.each(malformed)('refreshes every account, without throwing, when the list is %s', (_shape, list) => {
    const client = seeded(list);
    expect(() => invalidateEntryData(client, [1])).not.toThrow();
    expect(staleKeys(client)).toEqual(sweep);
  });

  it('falls back the same way for occurrence, detach and rule writes', () => {
    const every = (lists: (id: AccountId) => QueryKey[]): QueryKey[] =>
      IDS.flatMap((id) => [...lists(id), ...figures(id)]);

    const skipped = seeded();
    invalidateOccurrenceData(skipped, 1);
    expect(staleKeys(skipped)).toEqual(keyList(every(occurrences)));

    const detached = seeded();
    invalidateDetachData(detached, 1);
    expect(staleKeys(detached)).toEqual(
      keyList(every((id) => [...entries(id), ...occurrences(id)])),
    );

    const ruled = seeded();
    invalidateRuleData(ruled, 1);
    expect(staleKeys(ruled)).toEqual(keyList([queryKeys.rules(1), ...every(occurrences)]));
  });
});
