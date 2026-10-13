import { describe, expect, it } from 'vitest';

import type { AccountId } from '../../api/types';
import { accountsAffectedBy } from '../accounts';
import { account } from '../../test/fixtures';

/** Ascending, duplicates kept: the contract is the set (and each id once), not the order. */
function sorted(ids: AccountId[]): AccountId[] {
  return [...ids].sort((a, b) => a - b);
}

// Account 2 sits in both summaries, in the middle of summary 10's three
// members; account 4 is in none. Account 5 is a normal account listing a
// member: the backend can't produce one (core rejects members on a normal
// account), but the kind check must hold regardless.
const ACCOUNTS = [
  account(1),
  account(2),
  account(3),
  account(4),
  account(5, 'normal', [1]),
  account(10, 'summary', [1, 2, 3]),
  account(11, 'summary', [2, 3]),
];

// Every case runs against the list in both orders, so no position is skipped.
describe.each([
  ['in list order', ACCOUNTS],
  ['reversed', [...ACCOUNTS].reverse()],
])('accountsAffectedBy (%s)', (_order, accounts) => {
  it('is just the account when no summary contains it', () => {
    expect(accountsAffectedBy(accounts, [4])).toEqual([4]);
  });

  it('adds every summary that has the account as a member', () => {
    expect(sorted(accountsAffectedBy(accounts, [1]))).toEqual([1, 10]);
    expect(sorted(accountsAffectedBy(accounts, [2]))).toEqual([2, 10, 11]);
    expect(sorted(accountsAffectedBy(accounts, [3]))).toEqual([3, 10, 11]);
  });

  it('lists each account once, however often it is reached', () => {
    // A transfer between two members of one summary.
    expect(sorted(accountsAffectedBy(accounts, [1, 3]))).toEqual([1, 3, 10, 11]);
    // The same account passed twice, and a summary passed with its member.
    expect(sorted(accountsAffectedBy(accounts, [2, 2]))).toEqual([2, 10, 11]);
    expect(sorted(accountsAffectedBy(accounts, [1, 10]))).toEqual([1, 10]);
  });

  it('weighs every changed id, whatever their order', () => {
    // A transfer passes [viewed, counterpart], and either may be the member.
    expect(sorted(accountsAffectedBy(accounts, [1, 4]))).toEqual([1, 4, 10]);
    expect(sorted(accountsAffectedBy(accounts, [4, 1]))).toEqual([1, 4, 10]);
  });

  it('adds nothing for a summary itself, whose members are its inputs', () => {
    expect(accountsAffectedBy(accounts, [10])).toEqual([10]);
  });
});

describe('accountsAffectedBy at the edges', () => {
  it('still covers the changed accounts when there are no accounts', () => {
    expect(accountsAffectedBy([], [7])).toEqual([7]);
  });

  it('is empty when nothing changed', () => {
    expect(accountsAffectedBy(ACCOUNTS, [])).toEqual([]);
  });
});
