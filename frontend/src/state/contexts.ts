/** Context objects + hooks for app-wide UI state (no components — keeping these
 * out of the provider files keeps React Fast Refresh happy). */

import { createContext, useContext } from 'react';

import type { AccountId, Month } from '../api/types';
import type { RingMode } from '../lib/ring';
import type { ThemePref } from '../lib/theme';

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

export interface SettingsValue {
  theme: ThemePref;
  setTheme: (theme: ThemePref) => void;
  ringMode: RingMode;
  setRingMode: (mode: RingMode) => void;
  /** Whether to require a biometric unlock on launch (mobile; see `LockGate`). */
  appLock: boolean;
  setAppLock: (on: boolean) => void;
}

export const SettingsContext = createContext<SettingsValue | null>(null);

export function useSettings(): SettingsValue {
  const value = useContext(SettingsContext);
  if (!value) {
    throw new Error('useSettings must be used within SettingsProvider');
  }
  return value;
}

/** Top-level screens reachable from the navigation bar / settings cog. */
export type Screen = 'month' | 'accounts' | 'categories' | 'recurring' | 'stats' | 'settings';

export interface NavigationValue {
  screen: Screen;
  navigate: (screen: Screen) => void;
}

export const NavigationContext = createContext<NavigationValue | null>(null);

export function useNavigation(): NavigationValue {
  const value = useContext(NavigationContext);
  if (!value) {
    throw new Error('useNavigation must be used within NavigationProvider');
  }
  return value;
}
