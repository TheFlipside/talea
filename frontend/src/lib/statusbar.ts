/**
 * Drives the native status-bar appearance (see `tauri-plugin-statusbar`) so the
 * OS status/navigation bar icons match the app theme. A no-op on desktop and
 * harmless if the platform doesn't support it.
 */

import { invoke } from '@tauri-apps/api/core';

/** Tell the native side whether the app is currently in its dark theme. */
export async function setStatusBarDark(dark: boolean): Promise<void> {
  try {
    await invoke('plugin:statusbar|set_dark', { dark });
  } catch {
    // Plugin/command unavailable (e.g. an unsupported platform) — ignore.
  }
}
