/**
 * Movie Search / Filter — Desktop. Ported from
 * `reference/stitch_lumi_cinema_social 2/movie_search_desktop/code.html`.
 *
 * Fed by `GET /api/search`, which now filters for real: the controls are the
 * source of truth and every change refetches. Two consequences worth knowing:
 *
 *  - The screen no longer opens on the state the export drew ("Showing 12
 *    results for Space Exploration", Sci-Fi + 2010s pre-selected, four cards).
 *    That state isn't self-consistent — three of its four cards aren't 2010s
 *    films and one is rated 0.0 against a 3-star minimum — so it can't survive a
 *    filter that actually runs. It opens unfiltered instead.
 *  - Each facet shows a match count, which the export didn't draw. Without it,
 *    a filter that returns nothing looks broken rather than simply empty.
 *
 * `star_rating` here is a fractional crowd average printed as a number next to a
 * single glyph — not the half-star glyph count the feed screens use, which is
 * why this page doesn't reach for `StarRating`.
 */
import { useEffect, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'

import type { SearchResult } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import {
  ActionError,
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'

const FILLED = { fontVariationSettings: "'FILL' 1" }
const OUTLINED = { fontVariationSettings: "'FILL' 0" }

/** Long enough that typing doesn't fire a request per keystroke. */
const DEBOUNCE_MS = 250

/**
 * How many numbered buttons the pager draws, at most. The rest are reached by
 * stepping — a real catalogue runs to 1250 pages of 8, and one button each put
 * 1250 of them in a row that can't wrap, which stretched the results `<section>`
 * to 50,000px and dragged the grid's four columns out to 12,599px apiece. Odd,
 * so the current page sits in the middle of the window.
 */
const PAGE_WINDOW = 7

/**
 * The page numbers to draw: always the first and last, the current page with a
 * neighbour or two either side, and `null` wherever a run was skipped (rendered
 * as an ellipsis). Short paginations are returned whole — with `page_count` at or
 * below the window there is nothing to elide, which is every demo-mode case.
 */
function pageWindow(current: number, count: number): (number | null)[] {
  if (count <= PAGE_WINDOW) return Array.from({ length: count }, (_, i) => i + 1)

  // Reserve two slots for the first and last page and two for the ellipses.
  const span = PAGE_WINDOW - 4
  const half = Math.floor(span / 2)
  // Clamped so the window keeps its width at both ends instead of shrinking.
  const start = Math.min(Math.max(current - half, 2), count - span)
  const end = start + span - 1

  return [
    1,
    ...(start > 2 ? [null] : []),
    ...Array.from({ length: end - start + 1 }, (_, i) => start + i),
    ...(end < count - 1 ? [null] : []),
    count,
  ]
}

function ResultCard({
  result,
  onToggleWatchlist,
  busy,
}: {
  result: SearchResult
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="flex flex-col group">
      <div className="relative w-full aspect-[2/3] rounded bg-surface-container mb-sm overflow-hidden poster-shadow poster-inset">
        <Link to={`/movie/${result.id}`} className="block w-full h-full" aria-label={result.title}>
          {result.poster ? (
            <img
              className={`w-full h-full object-cover transition-transform duration-500 group-hover:scale-105 ${
                result.grayscale ? 'grayscale' : ''
              }`}
              alt={result.poster.alt}
              src={result.poster.src}
            />
          ) : (
            <div className="absolute inset-0 bg-surface-container-high flex flex-col items-center justify-center text-outline text-center p-sm">
              <span
                className="material-symbols-outlined text-4xl mb-2 opacity-50"
                style={OUTLINED}
              >
                movie
              </span>
              <span className="font-label-sm text-label-sm uppercase">Poster Missing</span>
            </div>
          )}
        </Link>
        {/* Sits above the poster link so the button takes the click, not the card.
            Stays up on a logged film — otherwise the state is invisible until you
            hover, and touch devices never hover at all. */}
        <div
          className={`absolute inset-0 bg-background/40 backdrop-blur-sm transition-opacity duration-300 flex items-center justify-center pointer-events-none ${
            result.on_watchlist
              ? 'opacity-100'
              : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'
          }`}
        >
          <button
            onClick={() => onToggleWatchlist(result.id)}
            disabled={busy}
            aria-pressed={result.on_watchlist}
            className={`pointer-events-auto font-label-sm text-label-sm px-3 py-2 rounded-full flex items-center gap-1 transition-colors disabled:cursor-wait ${
              result.on_watchlist
                ? 'bg-surface text-primary border border-primary'
                : 'bg-primary text-on-primary hover:bg-primary/90'
            }`}
          >
            <span className="material-symbols-outlined text-sm" style={FILLED}>
              {result.on_watchlist ? 'check' : 'add'}
            </span>{' '}
            {result.on_watchlist ? 'Logged' : 'Log'}
          </button>
        </div>
      </div>
      <div className="flex-grow">
        <Link to={`/movie/${result.id}`}>
          <h3 className="font-headline-md text-body-lg md:text-headline-md text-on-background leading-tight mb-xs group-hover:text-primary transition-colors">
            {result.title}
          </h3>
        </Link>
        <div className="flex items-center gap-2 font-label-sm text-label-sm text-outline">
          <span>{result.year}</span>
          <span className="w-1 h-1 rounded-full bg-outline-variant"></span>
          <span className="flex items-center gap-0.5 text-on-surface-variant">
            <span className="material-symbols-outlined text-[14px]" style={FILLED}>
              star
            </span>{' '}
            {result.star_rating.toFixed(1)}
          </span>
        </div>
      </div>
    </div>
  )
}

export function Search() {
  // The URL owns the query and filters, so a search is shareable and the back
  // button steps through refinements instead of leaving the screen.
  const [params, setParams] = useSearchParams()

  const q = params.get('q') ?? ''
  const genre = params.get('genre')
  const year = params.get('year')
  const minRating = Number(params.get('min_rating') ?? 0)
  const page = Number(params.get('page') ?? 1)

  // Typing updates the input immediately but the URL only after a pause.
  const [draft, setDraft] = useState(q)
  const [gridView, setGridView] = useState(true)

  /**
   * Writes the next control state to the URL. Any change other than paging
   * resets to page 1 — staying on page 3 of a result set that just shrank to one
   * page would show an empty grid.
   */
  const commit = (next: Record<string, string | number | null>, keepPage = false) => {
    const merged = new URLSearchParams(params)
    for (const [key, value] of Object.entries(next)) {
      if (value === null || value === '' || value === 0) merged.delete(key)
      else merged.set(key, String(value))
    }
    if (!keepPage && !('page' in next)) merged.delete('page')
    setParams(merged, { replace: true })
  }

  // Debounce the text box: the filters commit instantly, typing waits.
  useEffect(() => {
    if (draft === q) return
    const timer = setTimeout(() => commit({ q: draft }), DEBOUNCE_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft, q])

  // Keep the box in step when the URL changes from elsewhere (back button, a
  // query typed into the app bar on another screen).
  useEffect(() => setDraft(q), [q])

  const { data, error, loading, update } = useApi(
    () => api.search({ q, genre, year, minRating, page }),
    [q, genre, year, minRating, page],
  )

  const watchlist = useAction(async (id: string) => {
    const target = !data?.results.find((r) => r.id === id)?.on_watchlist
    // Optimistic: flip immediately, then reconcile with what the server stored.
    update((current) => ({
      ...current,
      results: current.results.map((r) => (r.id === id ? { ...r, on_watchlist: target } : r)),
    }))
    try {
      const state = await api.setWatchlist(id, target)
      update((current) => ({
        ...current,
        results: current.results.map((r) =>
          r.id === id ? { ...r, on_watchlist: state.on_watchlist } : r,
        ),
      }))
    } catch (cause) {
      update((current) => ({
        ...current,
        results: current.results.map((r) => (r.id === id ? { ...r, on_watchlist: !target } : r)),
      }))
      throw cause
    }
  })

  const hasFilters = Boolean(q || genre || year || minRating)

  return (
    <div className="bg-background text-on-background font-body-md min-h-screen flex flex-col">
      <TopAppBar active="movies" />
      <DemoBanner />

      {error && <ErrorNote error={error} />}

      {!error && (
        <main className="flex-grow w-full max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop py-xl flex flex-col md:flex-row gap-margin-desktop">
          {/* Sidebar filters */}
          <aside className="w-full md:w-64 flex-shrink-0 mb-xl md:mb-0">
            <div className="sticky top-xxl space-y-lg">
              <div className="flex items-baseline justify-between mb-md">
                <h2 className="font-headline-md text-headline-md text-on-surface">Filters</h2>
                {hasFilters && (
                  <button
                    onClick={() => setParams(new URLSearchParams(), { replace: true })}
                    className="font-label-sm text-label-sm text-primary uppercase tracking-wider hover:underline cursor-pointer"
                  >
                    Clear
                  </button>
                )}
              </div>

              <div className="border-b border-surface-variant pb-md">
                <h3 className="font-label-sm text-label-sm text-outline mb-sm uppercase tracking-wider">
                  Search
                </h3>
                <div className="relative">
                  <span
                    className="material-symbols-outlined absolute left-2 top-1/2 -translate-y-1/2 text-outline"
                    style={{ fontSize: '18px' }}
                  >
                    search
                  </span>
                  <input
                    className="w-full bg-surface-container-low border-none rounded-full py-2 pl-9 pr-8 font-label-sm text-label-sm text-on-surface focus:ring-1 focus:ring-primary"
                    placeholder="Title or genre…"
                    type="search"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                  />
                  {draft && (
                    <button
                      onClick={() => setDraft('')}
                      aria-label="Clear search"
                      className="absolute right-2 top-1/2 -translate-y-1/2 text-outline hover:text-on-surface"
                    >
                      <span className="material-symbols-outlined" style={{ fontSize: '18px' }}>
                        close
                      </span>
                    </button>
                  )}
                </div>
              </div>

              <div className="border-b border-surface-variant pb-md">
                <h3 className="font-label-sm text-label-sm text-outline mb-sm uppercase tracking-wider">
                  Genre
                </h3>
                <div className="flex flex-wrap gap-xs">
                  {data?.filters.genres.map((g) => (
                    <button
                      key={g.label}
                      onClick={() => commit({ genre: g.selected ? null : g.label })}
                      disabled={g.count === 0 && !g.selected}
                      title={`${g.count} film${g.count === 1 ? '' : 's'}`}
                      className={
                        g.selected
                          ? 'bg-primary text-on-primary font-label-sm text-label-sm px-2 py-1 rounded shadow-sm cursor-pointer'
                          : 'bg-surface-container-high text-on-surface font-label-sm text-label-sm px-2 py-1 rounded hover:bg-primary-container/20 cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-default disabled:hover:bg-surface-container-high'
                      }
                    >
                      {g.label}{' '}
                      <span className={g.selected ? 'opacity-70' : 'text-outline'}>{g.count}</span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="border-b border-surface-variant pb-md">
                <h3 className="font-label-sm text-label-sm text-outline mb-sm uppercase tracking-wider">
                  Release Year
                </h3>
                <div className="space-y-2">
                  {data?.filters.years.map((y) => (
                    <label key={y.label} className="flex items-center gap-2 cursor-pointer group">
                      <input
                        className="text-primary border-outline-variant focus:ring-primary h-4 w-4"
                        name="year"
                        type="radio"
                        checked={y.selected}
                        /* Radios can't be unchecked by clicking, so the change
                           handler doubles as a toggle back to "any decade". */
                        onChange={() => commit({ year: y.selected ? null : y.label })}
                        onClick={() => y.selected && commit({ year: null })}
                      />
                      <span className="font-body-md text-body-md text-on-surface-variant group-hover:text-primary transition-colors">
                        {y.label}
                      </span>
                      <span className="font-label-sm text-label-sm text-outline ml-auto">
                        {y.count}
                      </span>
                    </label>
                  ))}
                </div>
              </div>

              <div className="pb-md">
                <h3 className="font-label-sm text-label-sm text-outline mb-sm uppercase tracking-wider">
                  Minimum Rating
                </h3>
                <div className="flex items-center gap-1">
                  {Array.from({ length: 5 }, (_, i) => (
                    <button
                      key={i}
                      /* Clicking the current floor clears it — otherwise there
                         is no way back to "any rating" once one is set. */
                      onClick={() => commit({ min_rating: minRating === i + 1 ? null : i + 1 })}
                      aria-label={`Minimum ${i + 1} stars`}
                      aria-pressed={minRating === i + 1}
                      className={`material-symbols-outlined text-xl cursor-pointer ${
                        i < minRating ? 'text-primary' : 'text-surface-variant'
                      }`}
                      style={FILLED}
                    >
                      star
                    </button>
                  ))}
                  {minRating > 0 && (
                    <span className="font-label-sm text-label-sm text-outline ml-1">& up</span>
                  )}
                </div>
              </div>
            </div>
          </aside>

          {/* Results. `min-w-0` because a flex item defaults to `min-width:auto`,
              which lets any over-wide child stretch the column past the viewport
              instead of being constrained by it — the pager did exactly that. */}
          <section className="flex-grow min-w-0">
            <div className="mb-lg flex justify-between items-end border-b border-surface-variant pb-xs">
              <div>
                <h1 className="font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background">
                  Search Results
                </h1>
                <p className="font-body-md text-body-md text-on-surface-variant mt-1">
                  {data
                    ? `Showing ${data.total_results} result${data.total_results === 1 ? '' : 's'}${
                        data.query ? ` for "${data.query}"` : ''
                      }`
                    : ' '}
                </p>
              </div>
              <div className="hidden sm:flex gap-sm">
                <button
                  onClick={() => setGridView(true)}
                  aria-label="Grid view"
                  aria-pressed={gridView}
                  className={`material-symbols-outlined cursor-pointer ${
                    gridView ? 'text-primary' : 'text-outline'
                  }`}
                  style={gridView ? FILLED : OUTLINED}
                >
                  grid_view
                </button>
                <button
                  onClick={() => setGridView(false)}
                  aria-label="List view"
                  aria-pressed={!gridView}
                  className={`material-symbols-outlined cursor-pointer ${
                    gridView ? 'text-outline' : 'text-primary'
                  }`}
                  style={gridView ? OUTLINED : FILLED}
                >
                  view_list
                </button>
              </div>
            </div>

            {watchlist.error && (
              <div className="mb-lg">
                <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
              </div>
            )}

            {loading && !data && <Loading />}

            {data && data.results.length === 0 && (
              <div className="flex flex-col items-center gap-sm py-xxl text-center">
                <span className="material-symbols-outlined text-4xl text-outline opacity-50">
                  search_off
                </span>
                <p className="font-body-lg text-body-lg text-on-background">No films match.</p>
                <p className="font-label-sm text-label-sm text-on-surface-variant">
                  Try a different genre or lower the rating floor.
                </p>
              </div>
            )}

            {data && data.results.length > 0 && (
              <div
                /* Dim while a refetch is in flight so the stale grid reads as
                   stale rather than as the new result set. */
                className={`${
                  gridView
                    ? 'grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-gutter'
                    : 'grid grid-cols-1 gap-gutter'
                } transition-opacity ${loading ? 'opacity-50' : ''}`}
              >
                {data.results.map((result, i) => (
                  <div
                    key={result.id}
                    /* The export nudges every other card down a step on mobile
                       for an asymmetric rhythm; flat in the list view. */
                    className={gridView && i % 2 === 1 ? 'mt-md md:mt-0' : undefined}
                  >
                    <ResultCard
                      result={result}
                      onToggleWatchlist={watchlist.run}
                      busy={watchlist.busy}
                    />
                  </div>
                ))}
              </div>
            )}

            {data && data.page_count > 1 && (
              <div className="mt-xl flex justify-center items-center gap-md border-t border-surface-variant pt-lg">
                <button
                  onClick={() => commit({ page: Math.max(1, page - 1) }, true)}
                  disabled={page === 1}
                  aria-label="Previous page"
                  className="w-10 h-10 rounded-full border border-surface-variant flex items-center justify-center text-outline hover:bg-surface-container-high hover:text-primary transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent disabled:hover:text-outline"
                >
                  <span className="material-symbols-outlined" style={OUTLINED}>
                    chevron_left
                  </span>
                </button>
                {pageWindow(data.page, data.page_count).map((n, i) =>
                  n === null ? (
                    <span
                      // Index-keyed: there are at most two of these and their
                      // position in the list is the only thing that identifies them.
                      key={`gap-${i}`}
                      aria-hidden="true"
                      className="font-label-sm text-label-sm text-outline-variant select-none"
                    >
                      …
                    </span>
                  ) : (
                    <button
                      key={n}
                      onClick={() => commit({ page: n }, true)}
                      aria-current={n === data.page ? 'page' : undefined}
                      aria-label={`Page ${n}`}
                      className={
                        n === data.page
                          ? 'font-label-sm text-label-sm text-primary font-bold'
                          : 'font-label-sm text-label-sm text-outline hover:text-on-surface cursor-pointer'
                      }
                    >
                      {n}
                    </button>
                  ),
                )}
                <button
                  onClick={() => commit({ page: Math.min(data.page_count, page + 1) }, true)}
                  disabled={page >= data.page_count}
                  aria-label="Next page"
                  className="w-10 h-10 rounded-full border border-surface-variant flex items-center justify-center text-outline hover:bg-surface-container-high hover:text-primary transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-default"
                >
                  <span className="material-symbols-outlined" style={OUTLINED}>
                    chevron_right
                  </span>
                </button>
              </div>
            )}
          </section>
        </main>
      )}

      <BottomNavBar active="movies" />
    </div>
  )
}
