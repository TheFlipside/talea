import { describe, expect, it } from 'vitest';

import { AVAILABLE_LANGUAGES } from '../../lib/languages';
import de from '../locales/de.json';
import en from '../locales/en.json';
import es from '../locales/es.json';
import fr from '../locales/fr.json';
import itLocale from '../locales/it.json';
import ja from '../locales/ja.json';
import nl from '../locales/nl.json';
import pl from '../locales/pl.json';
import pt from '../locales/pt.json';
import ru from '../locales/ru.json';
import tr from '../locales/tr.json';
import zh from '../locales/zh.json';

// Non-English locales. A locale may have *extra* keys (e.g. Russian/Polish carry
// the `_few`/`_many` plural forms their grammar needs), so we only assert that
// every English key is present — a missing one silently falls back to English.
const locales: Record<string, unknown> = {
  de,
  es,
  fr,
  it: itLocale,
  pt,
  nl,
  ja,
  zh,
  ru,
  pl,
  tr,
};

function keysOf(value: unknown, prefix = ''): string[] {
  if (value === null || typeof value !== 'object') {
    return [prefix];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keysOf(child, prefix ? `${prefix}.${key}` : key),
  );
}

const englishKeys = keysOf(en);

describe('locale catalogs', () => {
  for (const [code, data] of Object.entries(locales)) {
    it(`${code} translates every English key`, () => {
      const present = new Set(keysOf(data));
      const missing = englishKeys.filter((key) => !present.has(key));
      expect(missing).toEqual([]);
    });
  }

  it('every bundled locale is offered in Settings (and vice versa)', () => {
    const offered = new Set(AVAILABLE_LANGUAGES.map((language) => language.code));
    const bundled = new Set(['en', ...Object.keys(locales)]);
    expect(offered).toEqual(bundled);
  });
});
