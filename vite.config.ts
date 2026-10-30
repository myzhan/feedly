import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// Tauri expects a fixed dev port, and disables features that don't work in the
// webview (auto module loading) during production builds.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  optimizeDeps: {
    // Pre-declare every ESM dependency so the dev server pre-bundles them in
    // one pass up front. Without this, Vite discovers deps lazily, triggers
    // repeated "new dependency, reloading" re-optimisation passes, and serves a
    // deep module waterfall — the main cause of the long blank window in dev.
    include: [
      'react',
      'react-dom',
      'react-dom/client',
      '@tanstack/react-query',
      '@tauri-apps/api/core',
      '@tauri-apps/api/event',
      '@tauri-apps/plugin-opener',
    ],
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // don't watch the Rust source here
      ignored: ['**/src-tauri/**'],
    },
    // Warm the transform pipeline for the entry graph the instant the server
    // boots, so the webview's first request hits already-compiled modules.
    warmup: {
      clientFiles: ['./src/main.tsx', './src/App.tsx', './src/index.css'],
    },
  },
  build: {
    target: 'es2022',
    // The system webview supports native module preload; skip the polyfill.
    modulePreload: { polyfill: false },
  },
});
