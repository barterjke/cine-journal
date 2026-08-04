import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The Rust API listens on 3001 (not 3000 — that collides with a Next.js dev
// server on many machines). Proxying /api and /img keeps the browser
// same-origin, so CORS never enters the picture in dev and the poster `src`
// paths stay identical to the ones in the static export.
const API_TARGET = process.env.API_URL ?? 'http://127.0.0.1:3001'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: API_TARGET, changeOrigin: true },
      '/img': { target: API_TARGET, changeOrigin: true },
    },
  },
})
