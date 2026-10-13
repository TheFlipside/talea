import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  loadAppLock,
  loadBool,
  loadString,
  persist,
  saveAppLock,
  storeThenApply,
} from '../storage';

/** How the stub storage treats writes, or reads. */
type Behaviour = 'stores' | 'throws' | 'drops' | 'readThrows';

/** A minimal `localStorage`: a map that can refuse writes (loudly or silently)
 *  or fail to read. */
function stubStorage(
  behaviour: Behaviour,
  initial: Record<string, string> = {},
  error: unknown = new DOMException('quota exceeded', 'QuotaExceededError'),
): Storage {
  const items = new Map(Object.entries(initial));
  const storage = {
    get length() {
      return items.size;
    },
    key: (index: number) => [...items.keys()][index] ?? null,
    getItem: (key: string) => {
      if (behaviour === 'readThrows') {
        throw new DOMException('denied', 'SecurityError');
      }
      return items.get(key) ?? null;
    },
    setItem: (key: string, value: string) => {
      if (behaviour === 'throws') {
        throw error;
      }
      if (behaviour !== 'drops') {
        items.set(key, value);
      }
    },
    removeItem: (key: string) => {
      items.delete(key);
    },
    clear: () => items.clear(),
  };
  vi.stubGlobal('localStorage', storage);
  return storage;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('persist', () => {
  it('reports a write storage accepted', () => {
    const storage = stubStorage('stores');
    expect(persist('talea.appLock', 'true')).toBe(true);
    expect(storage.getItem('talea.appLock')).toBe('true');
  });

  it('reports rewriting the value already there as accepted', () => {
    stubStorage('stores', { 'talea.appLock': 'true' });
    expect(persist('talea.appLock', 'true')).toBe(true);
  });

  // The stale opposite value is load-bearing: with nothing stored, a read-back
  // that only checked for "something" would pass.
  it.each([
    ['true', 'false'],
    ['false', 'true'],
  ])('reports a dropped write of %s over a stale %s', (value, stale) => {
    stubStorage('drops', { 'talea.appLock': stale });
    expect(persist('talea.appLock', value)).toBe(false);
  });

  it.each([
    new DOMException('full', 'QuotaExceededError'),
    new DOMException('denied', 'SecurityError'),
    new TypeError('no storage'),
  ])('reports a write refused with %s', (error) => {
    stubStorage('throws', {}, error);
    expect(persist('talea.appLock', 'true')).toBe(false);
  });

  it('reports a read-back that fails', () => {
    stubStorage('readThrows');
    expect(persist('talea.appLock', 'true')).toBe(false);
  });
});

describe('storeThenApply', () => {
  // Both directions, each over the stale opposite, so writing a constant (or
  // the wrong key, or nothing) can't pass for having stored the value.
  it.each([
    ['true', 'false'],
    ['false', 'true'],
  ])('stores %s over a stale %s under its key, then applies', (value, stale) => {
    const storage = stubStorage('stores', { 'talea.appLock': stale });
    const apply = vi.fn();
    expect(storeThenApply('talea.appLock', value, apply)).toBe(true);
    expect(storage.getItem('talea.appLock')).toBe(value);
    expect(apply).toHaveBeenCalledOnce();
  });

  it.each<Behaviour>(['throws', 'drops', 'readThrows'])(
    'applies nothing when storage %s',
    (behaviour) => {
      stubStorage(behaviour, { 'talea.appLock': 'false' });
      const apply = vi.fn();
      expect(storeThenApply('talea.appLock', 'true', apply)).toBe(false);
      expect(apply).not.toHaveBeenCalled();
    },
  );
});

describe('loadBool', () => {
  it('reads what was stored, and falls back either way when nothing was', () => {
    stubStorage('stores', { on: 'true', off: 'false', garbled: 'yes' });
    expect(loadBool('on', false)).toBe(true);
    expect(loadBool('off', true)).toBe(false);
    // Anything but exactly "true" reads as off.
    expect(loadBool('garbled', true)).toBe(false);
    expect(loadBool('unset', true)).toBe(true);
    expect(loadBool('unset', false)).toBe(false);
  });

  it('falls back either way when storage cannot be read', () => {
    stubStorage('readThrows');
    expect(loadBool('talea.appLock', false)).toBe(false);
    expect(loadBool('talea.appLock', true)).toBe(true);
  });
});

describe('the app-lock setting', () => {
  it.each([true, false])('stores %s as the launch gate reads it, then applies it', (next) => {
    const storage = stubStorage('stores', { 'talea.appLock': String(!next) });
    const apply = vi.fn();
    expect(saveAppLock(next, apply)).toBe(true);
    expect(storage.getItem('talea.appLock')).toBe(String(next));
    expect(apply).toHaveBeenCalledWith(next);
    expect(loadAppLock()).toBe(next);
  });

  it.each<Behaviour>(['throws', 'drops'])(
    'leaves the switch alone when storage %s the write',
    (behaviour) => {
      stubStorage(behaviour, { 'talea.appLock': 'false' });
      const apply = vi.fn();
      expect(saveAppLock(true, apply)).toBe(false);
      expect(apply).not.toHaveBeenCalled();
      expect(loadAppLock()).toBe(false);
    },
  );

  it('is off when nothing is stored or storage cannot be read', () => {
    stubStorage('stores');
    expect(loadAppLock()).toBe(false);
    stubStorage('readThrows');
    expect(loadAppLock()).toBe(false);
  });
});

describe('loadString', () => {
  const modes = ['spent', 'remaining'] as const;

  // Falling back to the *second* allowed value, so "return the first" can't pass.
  it('reads an allowed value, and falls back for anything else', () => {
    stubStorage('stores', { good: 'spent', bad: 'sideways' });
    expect(loadString('good', modes, 'remaining')).toBe('spent');
    expect(loadString('bad', modes, 'remaining')).toBe('remaining');
    expect(loadString('unset', modes, 'remaining')).toBe('remaining');
  });

  it('falls back when storage cannot be read', () => {
    stubStorage('readThrows');
    expect(loadString('talea.ringMode', modes, 'remaining')).toBe('remaining');
  });
});
