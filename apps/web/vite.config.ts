import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { tanstackRouter } from '@tanstack/router-plugin/vite';

import tailwindcss from '@tailwindcss/vite';


export default defineConfig({
  plugins: [
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    react(),
    tailwindcss(),
  ],
  // Tauri expects a fixed dev port (see apps/desktop/src-tauri/tauri.conf.json devUrl).
  server: {
    port: 5173,
    strictPort: true,
  },
  // Do not obscure Rust-side errors in the webview console.
  clearScreen: false,
});
