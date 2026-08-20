import { useCallback, useEffect, useRef, useState } from 'react'

import { isUnauthorized } from './api'

/** What a notice needs to know about a failed action — see `ActionError`. */
export interface ActionState {
  /** Message from the last failure, or null. Cleared when the next call starts. */
  error: string | null
  /**
   * Whether that failure was a 401, so the write needs an account.
   *
   * Every write in the API refuses an anonymous caller. That is a normal first
   * visit, not a fault, so the notice says "sign in" and offers the button.
   */
  signInRequired: boolean
  clearError: () => void
}

interface Action<A extends unknown[]> extends ActionState {
  run: (...args: A) => Promise<void>
  /** True while the request is in flight, for disabling the control. */
  busy: boolean
}

/**
 * Wraps a mutation in busy/error state.
 *
 * Optimistic updates live at the call site rather than here: each screen knows
 * how to apply and undo its own patch, and this hook stays agnostic about what
 * is being mutated. What it guarantees is that `run` never rejects — a thrown
 * error becomes `error` — so callers don't need their own try/catch around it.
 *
 * `perform` is kept in a ref and refreshed every render, so `run` keeps a stable
 * identity (safe in dependency arrays) while still calling the newest closure.
 * Holding `perform` in `run`'s deps instead would either recreate `run` on every
 * render or freeze it around the first render's state.
 */
export function useAction<A extends unknown[]>(perform: (...args: A) => Promise<void>): Action<A> {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [signInRequired, setSignInRequired] = useState(false)

  const latest = useRef(perform)
  useEffect(() => {
    latest.current = perform
  })

  const run = useCallback(async (...args: A) => {
    setBusy(true)
    setError(null)
    setSignInRequired(false)
    try {
      await latest.current(...args)
    } catch (cause) {
      // A 401 gets plain copy instead of the API's line. That line is the same for
      // every write, and with the method and path in front of it, it reads like a bug
      // report about a button the visitor simply can't use yet.
      const unauthorized = isUnauthorized(cause)
      setSignInRequired(unauthorized)
      if (unauthorized) setError('Sign in to do that.')
      else setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }, [])

  const clearError = useCallback(() => {
    setError(null)
    setSignInRequired(false)
  }, [])

  return { run, busy, error, signInRequired, clearError }
}
