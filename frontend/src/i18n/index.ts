import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';

import de from './locales/de.json';
import en from './locales/en.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import it from './locales/it.json';
import ja from './locales/ja.json';
import nl from './locales/nl.json';
import pl from './locales/pl.json';
import pt from './locales/pt.json';
import ru from './locales/ru.json';
import tr from './locales/tr.json';
import zh from './locales/zh.json';

// Resources are bundled (synchronous), so `useSuspense: false` keeps rendering
// immediate with no loading flash. New languages: add a JSON here + an entry in
// `lib/languages.ts`.
const resources = {
  en: { translation: en },
  de: { translation: de },
  es: { translation: es },
  fr: { translation: fr },
  it: { translation: it },
  pt: { translation: pt },
  nl: { translation: nl },
  ja: { translation: ja },
  zh: { translation: zh },
  ru: { translation: ru },
  pl: { translation: pl },
  tr: { translation: tr },
} as const;

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: 'en',
    supportedLngs: Object.keys(resources),
    // Match a device locale like "de-DE" / "pt-BR" / "zh-CN" to its base code.
    load: 'languageOnly',
    // Safe because translations are only ever rendered as React text nodes
    // (never via dangerouslySetInnerHTML), so React already escapes them. Keep
    // that discipline: any HTML-bearing string must use <Trans>, not raw markup.
    interpolation: { escapeValue: false },
    detection: { caches: ['localStorage'] },
    react: { useSuspense: false },
  });

export default i18n;
