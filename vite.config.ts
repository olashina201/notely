import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri dev server on 1420; frontend only talks to Rust via Tauri commands/events.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2021',
  },
})
