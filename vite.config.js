/** @format */

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import MillionLint from '@million/lint';

export default defineConfig(async () => ({
  plugins: [react(), MillionLint.vite()],

  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
}));
