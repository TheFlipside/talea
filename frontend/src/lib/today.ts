/** Today's local date as React state, kept current across midnight and resume. */

import { useEffect, useState } from 'react';

import type { IsoDate } from '../api/types';
import { todayISO } from './date';

/**
 * Milliseconds from `now` until the next local midnight. Built from local
 * calendar parts rather than by adding 24 hours, so a DST change can't shift it.
 */
export function msUntilNextLocalMidnight(now: Date): number {
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  return midnight.getTime() - now.getTime();
}

/**
 * Today's local date (`YYYY-MM-DD`), re-read at local midnight and whenever the
 * app returns to the foreground. The resume check matters on mobile: a
 * backgrounded webview suspends its timers, so the midnight timer alone would
 * miss a day that passed while the app was away.
 *
 * Re-reading costs no extra renders: React skips an update to an identical
 * string, so state only changes when the day actually does. A timer that fires
 * a moment early just finds the same day and re-arms for the remainder.
 */
export function useToday(): IsoDate {
  const [today, setToday] = useState(todayISO);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = () => setToday(todayISO());
    const armMidnight = () => {
      timer = setTimeout(() => {
        refresh();
        armMidnight();
      }, msUntilNextLocalMidnight(new Date()));
    };

    armMidnight();
    document.addEventListener('visibilitychange', refresh);
    return () => {
      clearTimeout(timer);
      document.removeEventListener('visibilitychange', refresh);
    };
  }, []);

  return today;
}
