import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { biometricAuthenticate, biometricAvailable } from '../lib/biometric';
import { useSettings } from '../state/contexts';
import { LockScreen } from './LockScreen';

/**
 * Gates the app behind a biometric unlock when the setting is on.
 *
 * The lock applies from launch — toggling the setting later takes effect on the
 * next start (so enabling it can't strand you behind a prompt you then cancel).
 * If the device has no biometrics (e.g. the desktop dev build), we unlock rather
 * than lock the user out, since there's no way to authenticate.
 */
export function LockGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { appLock } = useSettings();
  // Captured once: the lock state is fixed for this session at mount.
  const [locked, setLocked] = useState(() => appLock);
  const [authenticating, setAuthenticating] = useState(false);
  const attempted = useRef(false);

  const unlock = useCallback(async () => {
    setAuthenticating(true);
    const ok = await biometricAuthenticate(t('lock.reason'));
    setAuthenticating(false);
    if (ok) {
      setLocked(false);
    }
  }, [t]);

  useEffect(() => {
    if (!locked || attempted.current) {
      return;
    }
    attempted.current = true; // run the auto-prompt once (also guards StrictMode)
    void (async () => {
      if (!(await biometricAvailable())) {
        setLocked(false);
        return;
      }
      await unlock();
    })();
  }, [locked, unlock]);

  if (!locked) {
    return <>{children}</>;
  }
  return <LockScreen busy={authenticating} onUnlock={() => void unlock()} />;
}
