/** The app's query client. */

import { QueryClient } from '@tanstack/react-query';

/**
 * Builds the app's query client.
 *
 * Every query and save is a call into the app's own shell, never a request
 * over the network, so none waits for a connection (`networkMode: 'always'`).
 * TanStack's default, `'online'`, pauses them all while the webview reports
 * itself offline, as in airplane mode with the app open: nothing could be
 * saved, and no screen would refresh, until the connection came back. The
 * Nextcloud backup, restore and connection test, which do need the network,
 * now report their own error instead of waiting for it (at once in airplane
 * mode, after the shell's 30 s timeout on a connection that has died).
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Local IPC: retrying a validation/not-found error just delays display.
        retry: false,
        refetchOnWindowFocus: false,
        staleTime: 30_000,
        networkMode: 'always',
      },
      mutations: {
        networkMode: 'always',
      },
    },
  });
}
