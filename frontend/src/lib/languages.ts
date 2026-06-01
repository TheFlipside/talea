/** Languages offered in Settings. Add a locale JSON + register it in
 * `i18n/index.ts`, then add an entry here. */

export interface LanguageOption {
  code: string;
  label: string;
}

// Labels are each language's own endonym, so the picker is readable whatever
// the current UI language. Order: English first, then alphabetical by code.
export const AVAILABLE_LANGUAGES: LanguageOption[] = [
  { code: 'en', label: 'English' },
  { code: 'de', label: 'Deutsch' },
  { code: 'es', label: 'Español' },
  { code: 'fr', label: 'Français' },
  { code: 'it', label: 'Italiano' },
  { code: 'ja', label: '日本語' },
  { code: 'nl', label: 'Nederlands' },
  { code: 'pl', label: 'Polski' },
  { code: 'pt', label: 'Português' },
  { code: 'ru', label: 'Русский' },
  { code: 'tr', label: 'Türkçe' },
  { code: 'zh', label: '中文（简体）' },
];
