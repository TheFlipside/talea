import { useTranslation } from 'react-i18next';

interface LockScreenProps {
  busy: boolean;
  onUnlock: () => void;
}

/** Full-screen gate shown while the app is locked, with a button to (re)try the
 *  biometric prompt. */
export function LockScreen({ busy, onUnlock }: LockScreenProps) {
  const { t } = useTranslation();
  return (
    <div className="lock-screen">
      <div className="lock-screen__card">
        <div className="lock-screen__icon" aria-hidden="true">
          🔒
        </div>
        <h1>{t('app.title')}</h1>
        <p className="muted">{t('lock.prompt')}</p>
        <button type="button" className="btn" onClick={onUnlock} disabled={busy}>
          {busy ? t('lock.authenticating') : t('lock.unlock')}
        </button>
      </div>
    </div>
  );
}
