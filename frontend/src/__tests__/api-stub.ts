/**
 * Stub for `api.ts`, the one seam the tests mock.
 *
 * Mocking here rather than at `fetch` is enough: every screen reaches the network
 * through `useApi`/`useAction` into one of these methods.
 *
 * `ApiError` and `isNotFound` stay real — callers spread the actual module around this
 * stub — because `Collection` asks `isNotFound` whether an error was a 404.
 *
 * Methods come from the real object's keys, so a method added to `api.ts` is stubbed
 * too. Each rejects with its own name; an unstubbed call otherwise looks like a screen
 * stuck in its loading state.
 *
 * No runtime imports from `src/`: the `vi.mock` factory calls this while `api.ts` is
 * still loading.
 */
import { vi } from 'vitest'

import type { api } from '../api'

type Api = typeof api

/** Every method as a `vi.fn()` that rejects until a test says otherwise. */
export function stubApi(real: Api): Api {
  const stub: Record<string, unknown> = {}

  for (const name of Object.keys(real)) {
    stub[name] = vi.fn(() =>
      Promise.reject(new Error(`api.${name} was called, but no test stubbed it`)),
    )
  }

  return stub as Api
}
