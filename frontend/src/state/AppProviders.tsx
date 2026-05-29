/** Provider components for app-wide UI state. */

import { useCallback, useMemo, useState, type ReactNode } from 'react';

import type { AccountId } from '../api/types';
import { currentMonth, nextMonth, prevMonth } from '../lib/month';
import {
  ActiveAccountContext,
  SelectedMonthContext,
  type ActiveAccountValue,
  type SelectedMonthValue,
} from './contexts';

const STORAGE_KEY = 'talea.activeAccountId';

function loadStoredAccountId(): AccountId | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      return null;
    }
    const parsed = Number(raw);
    // Autoincrement ids start at 1; reject 0/negative/non-integer.
    return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
  } catch {
    return null;
  }
}

function ActiveAccountProvider({ children }: { children: ReactNode }) {
  const [activeAccountId, setActiveAccountIdState] = useState<AccountId | null>(loadStoredAccountId);

  const setActiveAccountId = useCallback((id: AccountId | null) => {
    setActiveAccountIdState(id);
    try {
      if (id === null) {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, String(id));
      }
    } catch {
      // Ignore persistence failures; selection still works for the session.
    }
  }, []);

  const value = useMemo<ActiveAccountValue>(
    () => ({ activeAccountId, setActiveAccountId }),
    [activeAccountId, setActiveAccountId],
  );

  return <ActiveAccountContext.Provider value={value}>{children}</ActiveAccountContext.Provider>;
}

function SelectedMonthProvider({ children }: { children: ReactNode }) {
  const [month, setMonth] = useState(currentMonth);

  const value = useMemo<SelectedMonthValue>(
    () => ({
      month,
      setMonth,
      next: () => {
        setMonth((m) => nextMonth(m));
      },
      prev: () => {
        setMonth((m) => prevMonth(m));
      },
    }),
    [month],
  );

  return <SelectedMonthContext.Provider value={value}>{children}</SelectedMonthContext.Provider>;
}

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <ActiveAccountProvider>
      <SelectedMonthProvider>{children}</SelectedMonthProvider>
    </ActiveAccountProvider>
  );
}
