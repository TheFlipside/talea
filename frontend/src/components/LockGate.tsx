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
 * It also **re-engages when the app returns from the background**, so switching
 * away and back requires authenticating again. If the device has no biometrics
 * (e.g. the desktop dev build), we unlock rather than lock the user out, since
 * there's no way to authenticate.
 */
export function LockGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { appLock } = useSettings();
  // Captured once: whether to lock at mount. Re-lock on resume is handled below.
  const [locked, setLocked] = useState(() => appLock);
  const [authenticating, setAuthenticating] = useState(false);
  // Refs the (non-React) visibility listener can read synchronously.
  const authenticatingRef = useRef(false);
  const promptedForThisLock = useRef(false);
  const wasBackgrounded = useRef(false);

  const setAuth = useCallback((value: boolean) => {
    authenticatingRef.current = value;
    setAuthenticating(value);
  }, []);

  const unlock = useCallback(async () => {
    setAuth(true);
    const ok = await biometricAuthenticate(t('lock.reason'));
    setAuth(false);
    if (ok) {
      promptedForThisLock.current = false; // re-arm for the next lock
      setLocked(false);
    }
  }, [t, setAuth]);

  // Auto-prompt whenever the app becomes locked (launch or resume). Runs once
  // per lock; the success path above re-arms it. Where biometrics are
  // unavailable we unlock instead of stranding the user.
  useEffect(() => {
    if (!locked || promptedForThisLock.current) {
      return;
    }
    promptedForThisLock.current = true;
    void (async () => {
      if (!(await biometricAvailable())) {
        promptedForThisLock.current = false;
        setLocked(false);
        return;
      }
      await unlock();
    })();
  }, [locked, unlock]);

  // Re-lock when the app returns to the foreground (only while the lock is on).
  // The `authenticating` guard ignores the background/resume the native prompt
  // itself can cause (e.g. Android's BiometricPrompt), so it can't loop.
  useEffect(() => {
    if (!appLock) {
      return undefined;
    }
    wasBackgrounded.current = false; // start clean; only count backgrounding from here
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') {
        if (!authenticatingRef.current) {
          wasBackgrounded.current = true;
        }
      } else if (document.visibilityState === 'visible') {
        if (wasBackgrounded.current && !authenticatingRef.current) {
          wasBackgrounded.current = false;
          setLocked(true);
        }
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, [appLock]);

  if (!locked) {
    return <>{children}</>;
  }
  return <LockScreen busy={authenticating} onUnlock={() => void unlock()} />;
}
