/**
 * Reads the installed app version from the Tauri shell. `tauri.conf.json` is the
 * single source of truth for the version, so this always matches the shipped
 * build. Uses `core:app:allow-version`, already granted via `core:default`.
 */

import { getVersion } from '@tauri-apps/api/app';

/**
 * The app version (e.g. `"1.4.1"`), or `null` when it can't be read — outside
 * the Tauri shell (a plain browser preview) there is no version to report.
 */
export async function getAppVersion(): Promise<string | null> {
  try {
    return await getVersion();
  } catch {
    // Not running inside the shell, or the command is unavailable — the caller
    // omits the version line rather than showing a placeholder.
    return null;
  }
}
