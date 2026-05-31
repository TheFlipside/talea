/**
 * Publishes an **abstract** budget-health snapshot to the native home-screen
 * widget (see `tauri-plugin-budgetwidget`). Only a ring fraction (0..1), a
 * derived percent, an overspent flag and the account name cross the boundary —
 * never any monetary figure. A no-op on desktop / unsupported platforms.
 */

import { invoke } from '@tauri-apps/api/core';

export interface AccountHealth {
  /** Account id as a string (an opaque key the widget matches against). */
  id: string;
  name: string;
  /** Ring fill fraction, 0..1. */
  fraction: number;
  overspent: boolean;
}

/** Push the current per-account health to the widget's shared storage. */
export async function publishBudgetHealth(accounts: AccountHealth[]): Promise<void> {
  try {
    await invoke('plugin:budgetwidget|publish_health', { payload: { accounts } });
  } catch {
    // Plugin/command unavailable (e.g. desktop) — ignore.
  }
}
