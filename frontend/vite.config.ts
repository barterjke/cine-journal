/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The Rust API listens on 3001 (not 3000 — that collides with a Next.js dev
// server on many machines). Proxying /api and /img keeps the browser
// same-origin, so CORS never enters the picture in dev and the poster `src`
// paths stay identical to the ones in the static export.
//
// In production Caddy does the same two prefixes, as `handle` blocks in front of the
// nginx container that serves this build — so both environments are single-origin.
// `api.ts` uses root-relative paths, so the API's location is never in the bundle.
// See Caddyfile and frontend/Dockerfile.
//
// Both prefixes, not just `/api`: TMDB posters are absolute CDN URLs, but the
// social layer's avatars come from `/img`, so missing that one breaks every avatar
// while leaving the posters fine.
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
  // Vitest shares this config, so tests run through the same plugins as the dev
  // server. The proxy above never applies to them: nothing under test reaches the
  // network, because `src/__tests__/api-stub.ts` replaces `api.ts`.
  test: {
    environment: 'jsdom',
    // Globals on, so the jest-dom matchers can be registered once in the setup file.
    // `tsconfig.json` lists `vitest/globals` so `tsc --noEmit` still sees them.
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
    // Explicit, so the helpers beside the tests aren't collected as tests.
    include: ['src/**/*.test.tsx'],
  },
})
