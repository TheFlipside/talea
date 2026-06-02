/**
 * Deterministic per-account colors for the summary-account combined view, where
 * rows from several member accounts are shown together and tagged by source.
 *
 * The color is derived from the account id so it is stable across renders and
 * sessions without storing anything. These are display-only accents (a small
 * dot next to a row), never tied to money values.
 */

import type { AccountId } from '../api/types';

// A small, visually distinct palette that reads on both light and dark themes.
const PALETTE = [
  '#40c9a2', // teal (accent)
  '#e6b450', // amber
  '#d65f8a', // pink
  '#6699e8', // blue
  '#9b7ede', // violet
  '#e0795a', // terracotta
  '#5fb878', // green
  '#c0703a', // brown
] as const;

/** A stable accent color for `accountId`, picked from a fixed palette. */
export function accountColor(accountId: AccountId): string {
  // `accountId` is a positive integer; modulo spreads ids across the palette.
  const index = Math.abs(accountId) % PALETTE.length;
  return PALETTE[index];
}
