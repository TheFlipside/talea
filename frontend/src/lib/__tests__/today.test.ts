import { describe, expect, it, vi } from 'vitest';

import { msUntilNextLocalMidnight } from '../today';

const HOUR = 60 * 60 * 1000;

describe('msUntilNextLocalMidnight', () => {
  it('counts the rest of the day', () => {
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 13, 23, 0, 0))).toBe(HOUR);
  });

  it('is a whole day from midnight itself, never zero', () => {
    // Zero would re-arm the timer in a tight loop on the very tick it fires.
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 13, 0, 0, 0))).toBe(24 * HOUR);
  });

  it('rolls over the end of a month and of a year', () => {
    expect(msUntilNextLocalMidnight(new Date(2026, 9, 31, 12, 0, 0))).toBe(12 * HOUR);
    expect(msUntilNextLocalMidnight(new Date(2026, 11, 31, 18, 30, 0))).toBe(5.5 * HOUR);
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
