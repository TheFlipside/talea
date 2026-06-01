/** Thin wrapper around Tauri `invoke` that normalizes errors. */

import { invoke } from '@tauri-apps/api/core';

import type { CommandError, CommandErrorCode } from './types';

const ERROR_CODES: readonly CommandErrorCode[] = [
  'validation',
  'not_found',
  'database',
  'corrupt',
  'backup',
];

/** Structural guard for the `{ code, message }` error a command rejects with. */
export function isCommandError(value: unknown): value is CommandError {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.message === 'string' &&
    ERROR_CODES.includes(candidate.code as CommandErrorCode)
  );
}

/** Normalizes any thrown value into a [`CommandError`] the UI can branch on. */
export function toCommandError(cause: unknown): CommandError {
  if (isCommandError(cause)) {
    return cause;
  }
  // An unexpected (non-command) failure — log the detail for debugging but show
  // a generic message so internal details (paths, stack traces) never reach the
  // UI. Backend command errors already arrive pre-scrubbed via the branch above.
  console.error('Unexpected command failure:', cause);
  return { code: 'database', message: 'An unexpected error occurred.' };
}

/**
 * Calls a Tauri command, rejecting with a normalized [`CommandError`].
 *
 * `args` keys follow Tauri's convention: scalar argument names are camelCase
 * (mapped to the snake_case Rust params), while nested payload objects keep
 * their serde (snake_case) field names.
 */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (cause) {
    throw toCommandError(cause);
  }
}
