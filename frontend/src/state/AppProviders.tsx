/** Provider components for app-wide UI state. */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';

import type { AccountId } from '../api/types';
import { currentMonth, nextMonth, prevMonth } from '../lib/month';
import type { RingMode } from '../lib/ring';
import { setStatusBarDark } from '../lib/statusbar';
import { resolveTheme, systemPrefersLight, type ThemePref } from '../lib/theme';
import {
  ActiveAccountContext,
  NavigationContext,
  SelectedMonthContext,
  SettingsContext,
  type ActiveAccountValue,
  type NavigationValue,
  type Screen,
  type SelectedMonthValue,
  type SettingsValue,
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

const THEME_KEY = 'talea.theme';
const RING_KEY = 'talea.ringMode';
const LOCK_KEY = 'talea.appLock';

function loadBool(key: string, fallback: boolean): boolean {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : raw === 'true';
  } catch {
    return fallback;
  }
}

function loadString<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw && (allowed as readonly string[]).includes(raw)) {
      return raw as T;
    }
  } catch {
    // Ignore; use fallback.
  }
  return fallback;
}

function persist(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore persistence failures.
  }
}

const THEME_PREFS = ['system', 'light', 'dark'] as const;
const RING_MODES = ['spent', 'remaining'] as const;

function SettingsProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemePref>(() => loadString(THEME_KEY, THEME_PREFS, 'system'));
  const [ringMode, setRingModeState] = useState<RingMode>(() => loadString(RING_KEY, RING_MODES, 'spent'));
  const [appLock, setAppLockState] = useState<boolean>(() => loadBool(LOCK_KEY, false));

  // Apply the resolved theme to the document, and track OS changes while on
  // "system". Touches the DOM only (no React state) so it's effect-safe.
  useEffect(() => {
    const root = document.documentElement;
    const apply = () => {
      const resolved = resolveTheme(theme, systemPrefersLight());
      root.dataset.theme = resolved;
      // Keep the native status/navigation bar icons legible against the theme.
      void setStatusBarDark(resolved === 'dark');
    };
    apply();
    if (theme !== 'system' || typeof window.matchMedia !== 'function') {
      return;
    }
    const media = window.matchMedia('(prefers-color-scheme: light)');
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [theme]);

  const setTheme = useCallback((next: ThemePref) => {
    setThemeState(next);
    persist(THEME_KEY, next);
  }, []);

  const setRingMode = useCallback((next: RingMode) => {
    setRingModeState(next);
    persist(RING_KEY, next);
  }, []);

  const setAppLock = useCallback((next: boolean) => {
    setAppLockState(next);
    persist(LOCK_KEY, String(next));
  }, []);

  const value = useMemo<SettingsValue>(
    () => ({ theme, setTheme, ringMode, setRingMode, appLock, setAppLock }),
    [theme, setTheme, ringMode, setRingMode, appLock, setAppLock],
  );

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

function NavigationProvider({ children }: { children: ReactNode }) {
  const [screen, setScreen] = useState<Screen>('month');
  const navigate = useCallback((next: Screen) => {
    setScreen(next);
  }, []);
  const value = useMemo<NavigationValue>(() => ({ screen, navigate }), [screen, navigate]);
  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <SettingsProvider>
      <NavigationProvider>
        <ActiveAccountProvider>
          <SelectedMonthProvider>{children}</SelectedMonthProvider>
        </ActiveAccountProvider>
      </NavigationProvider>
    </SettingsProvider>
  );
}
