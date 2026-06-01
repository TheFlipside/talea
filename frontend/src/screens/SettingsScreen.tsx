import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ConfirmDialog } from '../components/ConfirmDialog';
import { Select } from '../components/Select';
import {
  useBackupNow,
  useNextcloudConfig,
  useNextcloudSetConfig,
  useNextcloudTest,
  useRestoreNow,
} from '../api/hooks';
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

      <BackupSection />
    </section>
  );
}

/** A one-shot status line shown under the backup actions. */
type Status = { kind: 'ok' | 'err'; text: string } | null;

/**
 * Manual backup/restore to the user's own Nextcloud over WebDAV. Address and
 * username are persisted; the app password is write-only (left blank keeps the
 * stored one and is never read back). Restore is destructive, so it's guarded by
 * a confirmation dialog.
 */
function BackupSection() {
  const { t, i18n } = useTranslation();
  const config = useNextcloudConfig();
  const saveConfig = useNextcloudSetConfig();
  const test = useNextcloudTest();
  const backup = useBackupNow();
  const restore = useRestoreNow();

  const [baseUrl, setBaseUrl] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [status, setStatus] = useState<Status>(null);
  const [confirmRestore, setConfirmRestore] = useState(false);
  // Seed the address/username inputs from the stored config the first time it
  // loads (the password is never returned, so it stays blank). Adjusting state
  // during render — not in an effect — is React's recommended way to derive
  // initial local state from freshly loaded props.
  const [seeded, setSeeded] = useState(false);
  if (!seeded && config.data) {
    setSeeded(true);
    setBaseUrl(config.data.baseUrl);
    setUsername(config.data.username);
  }

  const configured = config.data?.configured ?? false;
  const busy =
    saveConfig.isPending || test.isPending || backup.isPending || restore.isPending;

  const lastBackup = config.data?.lastBackup;
  const lastBackupLabel = lastBackup
    ? t('settings.lastBackup', { when: new Date(lastBackup).toLocaleString(i18n.language) })
    : t('settings.lastBackupNever');

  const run = (
    action: () => Promise<unknown>,
    okText: string,
  ): void => {
    setStatus(null);
    action()
      .then(() => setStatus({ kind: 'ok', text: okText }))
      .catch((err: { message?: string }) =>
        setStatus({ kind: 'err', text: err.message ?? t('errors.backup') }),
      );
  };

  return (
    <div className="settings-backup">
      <h3>{t('settings.backupTitle')}</h3>
      <p className="settings-hint muted">{t('settings.backupIntro')}</p>

      <label className="field">
        <span>{t('settings.nextcloudUrl')}</span>
        <input
          type="url"
          inputMode="url"
          autoComplete="off"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.currentTarget.value)}
          placeholder="https://cloud.example.com"
        />
      </label>

      <label className="field">
        <span>{t('settings.nextcloudUser')}</span>
        <input
          type="text"
          autoComplete="off"
          value={username}
          onChange={(e) => setUsername(e.currentTarget.value)}
        />
      </label>

      <label className="field">
        <span>{t('settings.nextcloudPassword')}</span>
        <input
          type="password"
          autoComplete="off"
          value={password}
          onChange={(e) => setPassword(e.currentTarget.value)}
          placeholder={configured ? t('settings.nextcloudPasswordKeep') : undefined}
        />
      </label>

      <div className="settings-backup__actions">
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await saveConfig.mutateAsync({ baseUrl, username, password });
              setPassword('');
            }, t('settings.backupSaved'))
          }
        >
          {t('settings.backupSave')}
        </button>
        <button
          type="button"
          className="btn btn--ghost"
          disabled={busy || !configured}
          onClick={() => run(() => test.mutateAsync(), t('settings.backupTestOk'))}
        >
          {t('settings.backupTest')}
        </button>
        <button
          type="button"
          className="btn btn--ghost"
          disabled={busy || !configured}
          onClick={() => run(() => backup.mutateAsync(), t('settings.backupRunOk'))}
        >
          {t('settings.backupRun')}
        </button>
        <button
          type="button"
          className="btn btn--danger"
          disabled={busy || !configured}
          onClick={() => setConfirmRestore(true)}
        >
          {t('settings.backupRestore')}
        </button>
      </div>

      <p className="settings-backup__last muted">{lastBackupLabel}</p>
      {status && (
        <p className={status.kind === 'ok' ? 'settings-backup__ok' : 'settings-backup__err'}>
          {status.text}
        </p>
      )}
      <p className="settings-hint muted">{t('settings.backupHint')}</p>

      {confirmRestore && (
        <ConfirmDialog
          title={t('settings.restoreConfirmTitle')}
          message={t('settings.restoreConfirmBody')}
          confirmLabel={t('settings.restoreConfirmAction')}
          busy={restore.isPending}
          onCancel={() => setConfirmRestore(false)}
          onConfirm={() => {
            setConfirmRestore(false);
            run(() => restore.mutateAsync(), t('settings.backupRestoreOk'));
          }}
        />
      )}
    </div>
  );
}
