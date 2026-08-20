/**
 * Who is signed in, shared by every screen.
 *
 * One request for the whole app, not one per component. The cookie is HttpOnly, so
 * `GET /api/auth/me` is the only way to know.
 *
 * The state lives in this module, the way `Chrome`'s status request does, rather than
 * in a React context. A screen mounted on its own still gets an answer. That is how
 * every test mounts one.
 */
import { useEffect, useState } from 'react'

import type { User } from './api'
import { api } from './api'

export interface Auth {
  /** The signed-in account, or `null` for an anonymous visitor. */
  user: User | null
  /** True until the first answer arrives. The chrome draws nothing yet. */
  loading: boolean
}

const UNKNOWN: Auth = { user: null, loading: true }

let current: Auth = UNKNOWN
const listeners = new Set<(auth: Auth) => void>()
/** The one in-flight or finished request. Every later `useAuth` joins it. */
let request: Promise<void> | null = null

function publish(next: Auth) {
  current = next
  for (const listener of listeners) listener(next)
}

async function read(): Promise<void> {
  let user: User | null = null
  try {
    user = await api.me()
  } catch {
    // A 401 means nobody is signed in. A dead API means the same thing here. Screens
    // report an unreachable API themselves; the chrome only picks which buttons to
    // draw.
  }
  publish({ user, loading: false })
}

function load(): Promise<void> {
  request ??= read()
  return request
}

/**
 * Forget the answer, so it is asked again.
 *
 * Called after signing out. Tests call it between cases, which each start from
 * "nobody has asked yet".
 */
export function resetAuth(): void {
  request = null
  publish(UNKNOWN)
}

/** Sign out, then re-read, so every bar on the page updates without a reload. */
export async function signOut(): Promise<void> {
  await api.logout()
  resetAuth()
  await load()
}

export function useAuth(): Auth {
  const [auth, setAuth] = useState(current)

  useEffect(() => {
    listeners.add(setAuth)
    // In case the answer landed between this render and this subscription.
    setAuth(current)
    void load()
    return () => {
      listeners.delete(setAuth)
    }
  }, [])

  return auth
}
