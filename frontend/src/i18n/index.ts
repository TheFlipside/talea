import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';

import en from './locales/en.json';

// Resources are bundled (synchronous), so `useSuspense: false` keeps rendering
// immediate with no loading flash. New languages: add a JSON here + an entry in
// `lib/languages.ts`.
void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en } },
    fallbackLng: 'en',
    // Safe because translations are only ever rendered as React text nodes
    // (never via dangerouslySetInnerHTML), so React already escapes them. Keep
    // that discipline: any HTML-bearing string must use <Trans>, not raw markup.
    interpolation: { escapeValue: false },
    detection: { caches: ['localStorage'] },
    react: { useSuspense: false },
  });

export default i18n;
