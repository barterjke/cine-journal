import { useCallback, useEffect, useRef, useState } from 'react'

interface Action<A extends unknown[]> {
  run: (...args: A) => Promise<void>
  /** True while the request is in flight, for disabling the control. */
  busy: boolean
  /** Message from the last failure, or null. Cleared when the next call starts. */
  error: string | null
  clearError: () => void
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

  const latest = useRef(perform)
  useEffect(() => {
    latest.current = perform
  })

  const run = useCallback(async (...args: A) => {
    setBusy(true)
    setError(null)
    try {
      await latest.current(...args)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }, [])

  const clearError = useCallback(() => setError(null), [])

  return { run, busy, error, clearError }
}
