import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { midnightTimerDelay, msUntilNextLocalMidnight, watchToday } from '../today';

const HOUR = 60 * 60 * 1000;

describe('msUntilNextLocalMidnight', () => {
  it('counts the rest of the day', () => {
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 13, 23, 0, 0))).toBe(HOUR);
  });

  it('is a whole day from midnight itself, never zero', () => {
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 13, 0, 0, 0))).toBe(24 * HOUR);
  });

  it('rolls over the end of a month and of a year', () => {
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 31, 12, 0, 0))).toBe(12 * HOUR);
    expect(msUntilNextLocalMidnight(new Date(2026, 11, 31, 18, 30, 0))).toBe(5.5 * HOUR);
  });

  it('keeps years 0-99 as they are, unlike the Date constructor', () => {
    // `new Date(50, …)` means 1950; a clock in year 50 must still see 6 hours.
    const year50 = new Date(2026, 0, 1, 18, 0, 0);
    year50.setFullYear(50);
    expect(msUntilNextLocalMidnight(year50)).toBe(6 * HOUR);
  });

  it('follows the wall clock across a DST change, not a fixed 24 hours', () => {
    // Pinned, since no date above is a transition day in any zone: a naive
    // "+24 h" version would pass them all. Berlin springs forward on 29 March
    // 2026 and falls back on 25 October.
    vi.stubEnv('TZ', 'Europe/Berlin');
    try {
      expect(msUntilNextLocalMidnight(new Date(2026, 2, 29, 0, 0, 0))).toBe(23 * HOUR);
      expect(msUntilNextLocalMidnight(new Date(2026, 9, 25, 0, 0, 0))).toBe(25 * HOUR);
    } finally {
      vi.unstubAllEnvs();
    }
  });
});

describe('midnightTimerDelay', () => {
  it('is the time to midnight on an ordinary day', () => {
    const now = new Date(2026, 9, 13, 23, 0, 0);
    expect(midnightTimerDelay(now)).toBe(msUntilNextLocalMidnight(now));
  });

  it('never fires at once when a fall-back crosses midnight', () => {
    // Goose Bay fell back at 00:01 on 7 November 2010, replaying the hour
    // before midnight; during the replay "tomorrow 00:00" is already past.
    vi.stubEnv('TZ', 'America/Goose_Bay');
    try {
      const now = new Date('2010-11-07T03:30:00Z');
      expect(msUntilNextLocalMidnight(now)).toBeLessThan(0);
      expect(midnightTimerDelay(now)).toBe(1000);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('floors an invalid clock instead of arming with NaN', () => {
    expect(midnightTimerDelay(new Date(Number.NaN))).toBe(1000);
  });
});

describe('watchToday', () => {
  let doc: EventTarget & { visibilityState: DocumentVisibilityState };

  beforeEach(() => {
    vi.stubEnv('TZ', 'UTC');
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 13, 8, 0, 0)); // 13 October, 08:00
    doc = Object.assign(new EventTarget(), { visibilityState: 'visible' as const });
    vi.stubGlobal('document', doc);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  function becomes(state: DocumentVisibilityState): void {
    doc.visibilityState = state;
    doc.dispatchEvent(new Event('visibilitychange'));
  }

  // First, and bounded: a re-arm that spins fails here in milliseconds, rather
  // than exhausting the heap inside one of the many-hour advances below.
  it('fires nothing in its first second', () => {
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.advanceTimersByTime(1000);
    expect(onChange).not.toHaveBeenCalled();
    stop();
  });

  it('reports each new day once, at local midnight and not a moment early', () => {
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.advanceTimersByTime(16 * HOUR - 1);
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-14');
    vi.advanceTimersByTime(24 * HOUR);
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-15');
    stop();
  });

  it('reports the local date, even far from UTC', () => {
    vi.stubEnv('TZ', 'Pacific/Kiritimati'); // UTC+14: 08:00 local is 18:00Z the day before
    vi.setSystemTime(new Date(2026, 9, 13, 8, 0, 0));
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.advanceTimersByTime(16 * HOUR);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-14');
    stop();
  });

  it('premise: a clock jump leaves the pending timer exactly as far off as before', () => {
    // The resume tests below simulate a suspension by moving the system clock;
    // they only mean something if that freezes the pending timer, as a
    // backgrounded webview does, rather than making it overdue.
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.setSystemTime(new Date(2026, 9, 14, 20, 0, 0));
    vi.advanceTimersByTime(16 * HOUR - 1);
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onChange).toHaveBeenCalledTimes(1);
    stop();
  });

  it('catches up on resume, and re-arms from then rather than firing late', () => {
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    // Suspended until 14 October, 20:00: the pending timer is frozen, not fired.
    vi.setSystemTime(new Date(2026, 9, 14, 20, 0, 0));
    expect(onChange).not.toHaveBeenCalled();
    becomes('visible');
    expect(onChange).toHaveBeenLastCalledWith('2026-10-14');
    // The next midnight is 4 hours off, not the 16 the frozen timer had left.
    vi.advanceTimersByTime(4 * HOUR);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-15');
    stop();
  });

  it('re-arms on a resume within the same day, too', () => {
    // The everyday case: backgrounded for hours without crossing midnight.
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.setSystemTime(new Date(2026, 9, 13, 20, 0, 0));
    becomes('visible');
    onChange.mockClear();
    vi.advanceTimersByTime(4 * HOUR);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-14');
    stop();
  });

  it('re-arms on every resume, not just the first', () => {
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.setSystemTime(new Date(2026, 9, 14, 20, 0, 0));
    becomes('visible');
    vi.setSystemTime(new Date(2026, 9, 16, 22, 0, 0));
    becomes('visible');
    onChange.mockClear();
    vi.advanceTimersByTime(2 * HOUR);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith('2026-10-17');
    stop();
  });

  it('keeps exactly one timer through resumes, and none after cleanup', () => {
    const stop = watchToday(vi.fn());
    expect(vi.getTimerCount()).toBe(1);
    vi.setSystemTime(new Date(2026, 9, 14, 20, 0, 0));
    becomes('visible');
    expect(vi.getTimerCount()).toBe(1);
    stop();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('stays armed when onChange throws', () => {
    const stop = watchToday(() => {
      throw new Error('boom');
    });
    expect(() => vi.advanceTimersByTime(16 * HOUR)).toThrow('boom');
    expect(vi.getTimerCount()).toBe(1);
    stop();
  });

  it('re-checks at most once a second through a fall-back replay', () => {
    vi.stubEnv('TZ', 'America/Goose_Bay');
    vi.setSystemTime(new Date('2010-11-07T03:30:00Z')); // inside the replayed hour
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    vi.advanceTimersByTime(10_000);
    expect(onChange.mock.calls.length).toBeLessThanOrEqual(10);
    stop();
  });

  it('does nothing as the app hides', () => {
    const onChange = vi.fn();
    const stop = watchToday(onChange);
    becomes('hidden');
    expect(onChange).not.toHaveBeenCalled();
    stop();
  });

  it('stops entirely on cleanup', () => {
    const onChange = vi.fn();
    watchToday(onChange)();
    vi.advanceTimersByTime(48 * HOUR);
    becomes('visible');
    expect(onChange).not.toHaveBeenCalled();
  });
});
