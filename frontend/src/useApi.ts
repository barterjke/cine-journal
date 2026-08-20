import { useCallback, useEffect, useState } from 'react'

interface State<T> {
  data: T | null
  error: Error | null
  loading: boolean
}

interface Result<T> extends State<T> {
  /**
   * Patch the loaded data in place after a mutation, so a button can update
   * without a refetch flashing the whole screen through its loading state.
   * Ignored when nothing is loaded yet.
   */
  update: (patch: (current: T) => T) => void
  /** Replace the loaded data outright — for endpoints that return a fresh copy. */
  replace: (next: T) => void
  /**
   * Run the request again. What the error screen's "Try again" button calls.
   *
   * A real refetch rather than a page reload: reloading throws away the app and
   * every other screen's cached state to fix one failed call.
   */
  reload: () => void
}

/**
 * Minimal fetch-on-mount hook. There is roughly one request per screen, so
 * pulling in a query library would be overkill.
 *
 * `fetcher` is intentionally not a dependency — callers pass inline arrows, and
 * a fresh identity each render would loop. Pass anything the request varies on
 * via `deps` instead.
 */
export function useApi<T>(fetcher: () => Promise<T>, deps: unknown[] = []): Result<T> {
  const [state, setState] = useState<State<T>>({ data: null, error: null, loading: true })
  // Bumped by `reload`, and a dependency of the effect below, so a retry re-runs the
  // request without the caller having to pass anything for it.
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    let active = true
    setState({ data: null, error: null, loading: true })

    fetcher().then(
      (data) => {
        if (active) setState({ data, error: null, loading: false })
      },
      (error: unknown) => {
        if (active) {
          setState({
            data: null,
            error: error instanceof Error ? error : new Error(String(error)),
            loading: false,
          })
        }
      },
    )

    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, attempt])

  const update = useCallback((patch: (current: T) => T) => {
    setState((prev) => (prev.data === null ? prev : { ...prev, data: patch(prev.data) }))
  }, [])

  const replace = useCallback((next: T) => {
    setState({ data: next, error: null, loading: false })
  }, [])

  const reload = useCallback(() => setAttempt((n) => n + 1), [])

  return { ...state, update, replace, reload }
}
