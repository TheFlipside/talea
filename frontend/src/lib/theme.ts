/** Theme preference resolution. */

export type ThemePref = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

/** Resolves a preference to a concrete theme. For `system`, the caller supplies
 * the OS preference (so this stays pure and testable). */
export function resolveTheme(pref: ThemePref, prefersLight: boolean): ResolvedTheme {
  if (pref === 'system') {
    return prefersLight ? 'light' : 'dark';
  }
  return pref;
}

/** Whether the OS currently prefers a light color scheme. */
export function systemPrefersLight(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: light)').matches
  );
}
