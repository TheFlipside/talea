/** Failure-tolerant wrappers over `localStorage` for the app's settings. */

/** A stored boolean, or `fallback` when it is unset or storage can't be read. */
export function loadBool(key: string, fallback: boolean): boolean {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : raw === 'true';
  } catch {
    return fallback;
  }
}

/** A stored value from `allowed`, or `fallback` when it is unset, unknown, or
 *  storage can't be read. */
export function loadString<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw && (allowed as readonly string[]).includes(raw)) {
      return raw as T;
    }
  } catch {
    // Unreadable storage: use the fallback.
  }
  return fallback;
}

/**
 * Stores `value` under `key` and reports whether storage accepted it. Storage
 * can refuse a write (full, or disabled), so the value is read back rather
 * than trusted. That shows acceptance, not that the value reached disk, which
 * no web page can see. Most settings can shrug a refusal off and still apply
 * for the session; see {@link storeThenApply} for one that can't.
 */
export function persist(key: string, value: string): boolean {
  try {
    localStorage.setItem(key, value);
    return localStorage.getItem(key) === value;
  } catch {
    return false;
  }
}

/**
 * For a setting whose stored value is its whole effect: stores `value`, and
 * calls `apply` only once storage has accepted it, so what the app shows can
 * never run ahead of what is stored. Returns whether it applied. It never
 * throws on a storage failure; an exception from `apply` itself propagates.
 */
export function storeThenApply(key: string, value: string, apply: () => void): boolean {
  if (!persist(key, value)) {
    return false;
  }
  apply();
  return true;
}

/** Where the app-lock setting lives. */
const APP_LOCK_KEY = 'talea.appLock';

/** The stored app-lock setting: off unless storage holds exactly `true`. */
export function loadAppLock(): boolean {
  return loadBool(APP_LOCK_KEY, false);
}

/**
 * Stores the app-lock setting and, once storage has accepted it, hands it to
 * `apply`. The launch gate enforces the lock from storage, so the switch must
 * never run ahead of it ({@link storeThenApply}). Returns whether it applied.
 */
export function saveAppLock(next: boolean, apply: (next: boolean) => void): boolean {
  return storeThenApply(APP_LOCK_KEY, String(next), () => apply(next));
}
