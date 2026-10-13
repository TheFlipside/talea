/** Shared builders for test data. Nothing in the app imports this, so it is never bundled. */

import type { Account, AccountId, AccountKind } from '../api/types';

/** A minimal account: normal by default, or a summary over `members`. */
export function account(
  id: AccountId,
  kind: AccountKind = 'normal',
  members: AccountId[] = [],
): Account {
  return {
    id,
    name: `Account ${id}`,
    icon: '💰',
    currency: 'EUR',
    opening_balance: '0',
    anchor: { year: 2026, month: 1 },
    kind,
    members,
  };
}
