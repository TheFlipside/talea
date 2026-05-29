import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Allow mobile dev: when set, Vite binds to the LAN host the device reaches.
// SECURITY: setting TAURI_DEV_HOST exposes the dev server + HMR socket to the
// local network (your source tree is reachable by LAN peers). Only set it on a
// trusted network, and unset it afterwards.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/ — tuned for use behind the Tauri dev server.
export default defineConfig({
  plugins: [react()],
  // Tauri prints its own errors; don't let Vite clear them.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host ?? false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    // The Rust side is watched by Cargo, not Vite.
    watch: { ignored: ['**/src-tauri/**'] },
  },
});
