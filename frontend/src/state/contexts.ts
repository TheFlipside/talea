/** Context objects + hooks for app-wide UI state (no components — keeping these
 * out of the provider files keeps React Fast Refresh happy). */

import { createContext, useContext } from 'react';

import type { AccountId, Month } from '../api/types';

export interface ActiveAccountValue {
  activeAccountId: AccountId | null;
  setActiveAccountId: (id: AccountId | null) => void;
}

export const ActiveAccountContext = createContext<ActiveAccountValue | null>(null);

export function useActiveAccount(): ActiveAccountValue {
  const value = useContext(ActiveAccountContext);
  if (!value) {
    throw new Error('useActiveAccount must be used within ActiveAccountProvider');
  }
  return value;
}

export interface SelectedMonthValue {
  month: Month;
  setMonth: (month: Month) => void;
  next: () => void;
  prev: () => void;
}

export const SelectedMonthContext = createContext<SelectedMonthValue | null>(null);

export function useSelectedMonth(): SelectedMonthValue {
  const value = useContext(SelectedMonthContext);
  if (!value) {
    throw new Error('useSelectedMonth must be used within SelectedMonthProvider');
  }
  return value;
}
