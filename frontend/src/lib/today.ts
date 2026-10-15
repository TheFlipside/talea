/** Today's local date, kept current across midnight and app resume. */

import { useSyncExternalStore } from 'react';

import type { IsoDate } from '../api/types';
import { todayISO } from './date';

/** The shortest delay the midnight timer is armed with, so it never fires back
 *  to back. */
const MIN_TIMER_MS = 1000;

/**
 * Milliseconds from `now` until the next local midnight. A copy of `now` is
 * moved to the start of its next local day rather than 24 hours on, so an
 * ordinary DST change can't shift it — and the setter, unlike the `Date`
 * constructor, leaves years 0-99 alone instead of reading them as 19xx.
 */
export function msUntilNextLocalMidnight(now: Date): number {
  const midnight = new Date(now.getTime());
  midnight.setHours(24, 0, 0, 0);
  return midnight.getTime() - now.getTime();
}

/**
 * The delay to arm the midnight timer with: {@link msUntilNextLocalMidnight},
 * at least a second. A DST fall-back that crosses midnight replays the hour
 * before it, and during the replay "the next 00:00" resolves to the instant
 * already past: a negative delay, which would fire the timer at once, again
 * and again. Floored, it re-checks once a second until the real midnight —
 * a case only historical rules produce (none in force since 2010). An invalid
 * date gets the floor too, rather than NaN.
 */
export function midnightTimerDelay(now: Date): number {
  const delay = msUntilNextLocalMidnight(now);
  return Number.isFinite(delay) ? Math.max(delay, MIN_TIMER_MS) : MIN_TIMER_MS;
}

/**
 * Calls `onChange` with today's local date whenever it may have changed: at
 * local midnight, and each time the app becomes visible again. Returns the
 * cleanup. React-free, so its timing can be tested with fake timers.
 *
 * The resume path matters on mobile, where a backgrounded webview freezes its
 * timers: a day may have passed while the app was away, and the pending
 * midnight timer, armed before suspension, would now fire hours late. So
 * becoming visible both re-reads the date and re-arms the timer from now.
 * Hiding does neither: the suspension that follows would only freeze it again.
 */
export function watchToday(onChange: (today: IsoDate) => void): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  function arm(): void {
    clearTimeout(timer);
    timer = setTimeout(sync, midnightTimerDelay(new Date()));
  }
  // Re-arm before reporting, so the timer stands even if `onChange` throws.
  function sync(): void {
    arm();
    onChange(todayISO());
  }
  function onVisibilityChange(): void {
    if (document.visibilityState === 'visible') {
      sync();
    }
  }

  arm();
  document.addEventListener('visibilitychange', onVisibilityChange);
  return () => {
    clearTimeout(timer);
    document.removeEventListener('visibilitychange', onVisibilityChange);
  };
}

/**
 * The app's shared clock for "today": owned by `TodayProvider` and read
 * through `useToday` (`state/contexts.ts`) by every view that must follow the
 * day as it turns — the month bar, month navigation and the widget — so they
 * agree on it. (A one-off read at the moment of an action, like a form's
 * default date, can take the wall clock directly.)
 *
 * An external store, read as one: React re-reads the snapshot right after
 * subscribing, so a launch whose first render lands just before midnight and
 * whose subscription lands just after still picks up the new day. The
 * snapshot is a string, so an unchanged day re-renders nothing.
 */
export function useTodayClock(): IsoDate {
  return useSyncExternalStore(watchToday, todayISO);
}
