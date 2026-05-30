import { describe, expect, it } from 'vitest';

import { resolveTheme } from '../theme';

describe('resolveTheme', () => {
  it('returns the explicit preference unchanged', () => {
    expect(resolveTheme('light', false)).toBe('light');
    expect(resolveTheme('dark', true)).toBe('dark');
  });

  it('follows the OS preference for "system"', () => {
    expect(resolveTheme('system', true)).toBe('light');
    expect(resolveTheme('system', false)).toBe('dark');
  });
});
