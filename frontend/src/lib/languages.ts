/** Languages offered in Settings. Add a locale JSON + register it in
 * `i18n/index.ts`, then add an entry here. */

export interface LanguageOption {
  code: string;
  label: string;
}

export const AVAILABLE_LANGUAGES: LanguageOption[] = [{ code: 'en', label: 'English' }];
