/** Pure helpers over the account list. */

import type { Account, AccountId } from '../api/types';

/** The fields of an account that membership lookups read. */
export type Membership = Pick<Account, 'id' | 'kind' | 'members'>;

/**
 * The accounts whose views derive from the `changed` accounts' own records:
 * each of them, plus every summary account that has one as a member — a
 * summary's month figures, merged entry list, statistics and budget progress
 * are all computed from its members'. Each account appears once, so a transfer
 * between two members of one summary still refreshes that summary only once.
 *
 * One level up is enough: summaries can't nest (the command layer refuses a
 * summary as a member). The input is trusted as typed, so a caller holding
 * unchecked data (the query cache) validates it first — see `isMembership` in
 * `api/invalidation.ts`.
 */
export function accountsAffectedBy(
  accounts: readonly Membership[],
  changed: readonly AccountId[],
): AccountId[] {
  const affected = new Set(changed);
  for (const account of accounts) {
    if (
      account.kind === 'summary' &&
      account.members.some((member) => changed.includes(member))
    ) {
      affected.add(account.id);
    }
  }
  return [...affected];
}
