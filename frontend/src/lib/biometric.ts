/**
 * Thin wrapper over the Tauri biometric plugin with graceful degradation.
 *
 * The plugin is mobile-only (Android/iOS). On desktop — or any device without
 * enrolled biometrics — the calls reject or report unavailable; we map that to
 * "not available" so the app never locks the user out where it can't
 * authenticate (desktop is a development target).
 */

import { authenticate, checkStatus } from '@tauri-apps/plugin-biometric';

/** Whether the device can perform a biometric (or device-credential) check. */
export async function biometricAvailable(): Promise<boolean> {
  try {
    return (await checkStatus()).isAvailable;
  } catch {
    // Plugin absent (desktop build) or status call failed.
    return false;
  }
}

/**
 * Prompts for biometric (or device-credential) authentication. Resolves `true`
 * on success, `false` on failure/cancel. `reason` is shown to the user.
 */
export async function biometricAuthenticate(reason: string): Promise<boolean> {
  try {
    await authenticate(reason, {
      // Fall back to the device PIN/passcode if biometrics fail, so a transient
      // sensor failure doesn't strand the user.
      allowDeviceCredential: true,
    });
    return true;
  } catch {
    return false;
  }
}
