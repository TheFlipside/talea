import { useTranslation } from 'react-i18next';

import { Select } from '../components/Select';
import { AVAILABLE_LANGUAGES } from '../lib/languages';
import type { RingMode } from '../lib/ring';
import type { ThemePref } from '../lib/theme';
import { useSettings } from '../state/contexts';

export function SettingsScreen() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme, ringMode, setRingMode, appLock, setAppLock } = useSettings();

  const themeOptions = [
    { value: 'system', label: t('settings.themeSystem') },
    { value: 'light', label: t('settings.themeLight') },
    { value: 'dark', label: t('settings.themeDark') },
  ];
  const ringOptions = [
    { value: 'spent', label: t('settings.ringSpent') },
    { value: 'remaining', label: t('settings.ringRemaining') },
  ];
  const languageOptions = AVAILABLE_LANGUAGES.map((l) => ({ value: l.code, label: l.label }));

  return (
    <section className="screen settings-screen">
      <h2>{t('settings.title')}</h2>

      <div className="settings-row">
        <span>{t('settings.theme')}</span>
        <Select
          value={theme}
          options={themeOptions}
          onChange={(v) => setTheme(v as ThemePref)}
          ariaLabel={t('settings.theme')}
        />
      </div>

      <div className="settings-row">
        <span>{t('settings.language')}</span>
        <Select
          value={i18n.language.split('-')[0]}
          options={languageOptions}
          onChange={(v) => void i18n.changeLanguage(v)}
          ariaLabel={t('settings.language')}
        />
      </div>

      <div className="settings-row">
        <span>{t('settings.ring')}</span>
        <Select
          value={ringMode}
          options={ringOptions}
          onChange={(v) => setRingMode(v as RingMode)}
          ariaLabel={t('settings.ring')}
        />
      </div>

      <div className="settings-row">
        <span>{t('settings.appLock')}</span>
        <input
          className="switch"
          type="checkbox"
          checked={appLock}
          onChange={(e) => setAppLock(e.currentTarget.checked)}
          aria-label={t('settings.appLock')}
        />
      </div>
      <p className="settings-hint muted">{t('settings.appLockHint')}</p>
    </section>
  );
}
