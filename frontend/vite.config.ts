import react from '@vitejs/plugin-react';
// `vitest/config` re-exports Vite's `defineConfig` with the `test` field typed.
import { defineConfig } from 'vitest/config';

// Allow mobile dev: when set, Vite binds to the LAN host the device reaches.
// SECURITY: setting TAURI_DEV_HOST exposes the dev server + HMR socket to the
// local network: LAN peers can read your source tree, and since it is served
// over plain http/ws, an on-path one can alter the code the device's app
// webview runs. Only set it on a trusted network, and unset it afterwards.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/ — tuned for use behind the Tauri dev server.
export default defineConfig({
  plugins: [react()],
  // Tauri prints its own errors; don't let Vite clear them.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // `||`, not `??`: an empty `TAURI_DEV_HOST` (e.g. `export TAURI_DEV_HOST=`
    // to "unset" it) must mean loopback too — Vite hands `''` to `listen()`,
    // which binds every interface.
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    // The Rust side is watched by Cargo, not Vite.
    watch: { ignored: ['**/src-tauri/**'] },
  },
  // Pure helpers only — no DOM needed, so the lighter `node` environment.
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    // Forked processes, pinned rather than left to the default: the DST tests
    // set `TZ` at runtime, which Node honours for a whole process but not
    // inside a worker thread. (Each test restores `TZ` itself.)
    pool: 'forks',
  },
});
