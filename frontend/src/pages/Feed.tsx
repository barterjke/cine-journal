/**
 * The feed: one interleaved, infinitely-scrolling column of reviews, suggestions and the
 * visitor's own entries.
 *
 * It used to be three fixed rails — six reviews, eight posters, eight suggestions, and
 * then the page stopped. That is a summary, not a feed. `GET /api/feed?cursor=` now pages
 * through the same three sources mixed together, and this screen appends pages as the
 * sentinel at the bottom comes into view.
 *
 * **Cache first, then revalidate.** The first request is served from Redis when there is
 * anything there, so the screen paints immediately; seeing `from_cache`, it asks again
 * with `refresh` and swaps in the rebuilt page when it lands. So you read last visit's
 * feed while this visit's is being made — and what you build fills the cache for whoever
 * comes next. When Redis is absent every page is built on demand and `from_cache` is
 * always false, so this path simply never runs.
 *
 * Not `useApi`: that hook resets to `{ data: null, loading: true }` whenever its deps
 * change, which is right for a screen that shows one payload and wrong for one that
 * accumulates pages — the first append would blank everything above it.
 *
 * Every poster and film name links to `/movie/:id`. The export's cards were styled as
 * clickable (`cursor-pointer`, hover lifts) and went nowhere.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'

import type { FeedItem, Movie } from '../api'
import { api } from '../api'
import { useAction } from '../useAction'
import {
  ActionError,
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'
import { ReviewCard } from '../components/People'
import { StarRating } from '../components/StarRating'

/**
 * A card's identity, unique across kinds.
 *
 * The same film legitimately appears twice in one feed — a friend reviewed it *and* you
 * logged it — so the film alone can't be the key. Two cards of the same kind about the
 * same film are the duplicate, which is what pages overlapping at a seam produce.
 */
function itemKey(item: FeedItem): string {
  switch (item.kind) {
    case 'review':
      return `review-${item.id}`
    case 'recommendation':
      return `rec-${item.movie.id}`
    case 'entry':
      return `entry-${item.movie.id}`
  }
}

/** The film a card is about, for the watchlist toggle. Reviews have no button. */
function cardMovieId(item: FeedItem): string | null {
  switch (item.kind) {
    case 'recommendation':
    case 'entry':
      return item.movie.id
    case 'review':
      return null
  }
}

/** How far ahead of the viewport a page is fetched, so scrolling never stalls. */
const PREFETCH_MARGIN = '800px'

/**
 * The accumulating feed.
 *
 * Kept here rather than in a generic hook because the two things that make it awkward are
 * specific to this screen: pages must accumulate across fetches, and the *first* page is
 * fetched twice on purpose.
 */
function useFeed() {
  const [items, setItems] = useState<FeedItem[]>([])
  const [cursor, setCursor] = useState<string | null>(null)
  const [done, setDone] = useState(false)
  const [loading, setLoading] = useState(true)
  const [pending, setPending] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  /** Fatal: the first page failed, so there is nothing on screen to keep. */
  const [error, setError] = useState<Error | null>(null)
  /** A failed *append*. Recoverable — everything above it is still good. */
  const [moreError, setMoreError] = useState<string | null>(null)

  /**
   * Whether anything has been appended yet, which decides what a revalidated head does.
   *
   * A ref rather than state: it's read inside the load below and must not re-run it.
   */
  const appended = useRef(false)
  /** So the observer firing twice in a row can't launch two requests for one cursor. */
  const inFlight = useRef(false)
  /**
   * Whether the first load has been started, so it happens exactly once.
   *
   * StrictMode runs every effect twice in dev — mount, cleanup, mount again on the same
   * instance — and this load is two requests, so letting it run twice would quadruple
   * them. A ref survives that, unlike state.
   *
   * There is deliberately no cancellation flag alongside it: a cleanup that set one
   * would cancel the *first* run's fetch and then be skipped by this guard on the
   * second, leaving the screen loading forever. Refs and state survive StrictMode's
   * fake remount, so the in-flight request still has somewhere to land; and after a
   * genuine unmount these setters are no-ops.
   */
  const started = useRef(false)

  useEffect(() => {
    if (started.current) return
    started.current = true

    const accept = (page: { items: FeedItem[]; next_cursor: string | null }) => {
      setCursor(page.next_cursor)
      setDone(page.next_cursor === null)
    }

    void (async () => {
      let first
      try {
        first = await api.feedPage()
      } catch (cause) {
        // Fatal: there is nothing on screen to fall back to.
        setError(cause instanceof Error ? cause : new Error(String(cause)))
        setLoading(false)
        return
      }
      setItems(first.items)
      accept(first)
      setLoading(false)

      // Whatever Redis had is on screen; now build the current one. Nothing to do when
      // the page was built for this request — it is already the fresh copy.
      if (!first.from_cache) return
      setRefreshing(true)
      try {
        const fresh = await api.feedPage(null, true)
        if (appended.current) {
          // Scrolled past the head already: graft the new cards on top and leave the
          // cursor alone, rather than resetting to page two and re-walking what's below.
          setItems((prev) => {
            const arriving = new Set(fresh.items.map(itemKey))
            return [...fresh.items, ...prev.filter((item) => !arriving.has(itemKey(item)))]
          })
        } else {
          setItems(fresh.items)
          accept(fresh)
        }
      } catch {
        // Swallowed, deliberately: the cached page is still on screen and still a
        // working feed. Reporting this would replace it with an error over a screen of
        // perfectly good cards, and there is nothing the reader would do about it.
      } finally {
        setRefreshing(false)
      }
    })()
  }, [])

  const loadMore = useCallback(async () => {
    if (inFlight.current || cursor === null) return
    inFlight.current = true
    setPending(true)
    setMoreError(null)
    try {
      const page = await api.feedPage(cursor)
      appended.current = true
      // Filtered against what's already up: two pages can overlap at their seam when the
      // graph changed between requests, and React would rather not have two of one key.
      setItems((prev) => {
        const seen = new Set(prev.map(itemKey))
        return [...prev, ...page.items.filter((item) => !seen.has(itemKey(item)))]
      })
      setCursor(page.next_cursor)
      setDone(page.next_cursor === null)
    } catch (cause) {
      setMoreError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      inFlight.current = false
      setPending(false)
    }
  }, [cursor])

  /** Flip one film's watchlist flag on every card that shows it. */
  const patchWatchlist = useCallback((movieId: string, on: boolean) => {
    setItems((prev) =>
      prev.map((item) =>
        cardMovieId(item) === movieId ? ({ ...item, on_watchlist: on } as FeedItem) : item,
      ),
    )
  }, [])

  /**
   * Whether a film is on the watchlist, according to whichever card shows it.
   *
   * Read off the list rather than kept in a set of its own, so there is one answer on
   * screen and in state — two cards for the same film always agree, because they were
   * patched together.
   */
  const onWatchlist = useCallback(
    (movieId: string) => {
      const card = items.find((item) => cardMovieId(item) === movieId)
      return card !== undefined && 'on_watchlist' in card && card.on_watchlist
    },
    [items],
  )

  return {
    items,
    done,
    loading,
    pending,
    refreshing,
    error,
    moreError,
    clearMoreError: () => setMoreError(null),
    loadMore,
    patchWatchlist,
    onWatchlist,
  }
}

/**
 * The eyebrow every card wears: what kind of thing this is.
 *
 * The three headings the screen used to have are gone with the rails, and the cards are
 * interleaved now — so each one has to say for itself whether it's somebody's review,
 * a suggestion, or your own entry. Without it the column reads as one undifferentiated
 * list of films.
 */
function Card({
  icon,
  eyebrow,
  children,
}: {
  icon: string
  eyebrow: string
  children: React.ReactNode
}) {
  return (
    <article className="flex flex-col gap-md bg-surface-container-low rounded-xl p-md md:p-lg border border-surface-variant">
      <div className="flex items-center gap-xs">
        <span className="material-symbols-outlined text-[16px] text-outline" aria-hidden="true">
          {icon}
        </span>
        <h2 className="font-label-sm text-label-sm text-outline uppercase tracking-widest">
          {eyebrow}
        </h2>
      </div>
      {children}
    </article>
  )
}

/** The "+ Watchlist" / "✓ On watchlist" text button both poster rows carry. */
function WatchlistButton({
  movie,
  on,
  onToggle,
  busy,
}: {
  movie: Movie
  on: boolean
  onToggle: (id: string) => void
  busy: boolean
}) {
  return (
    <button
      onClick={() => onToggle(movie.id)}
      disabled={busy}
      aria-pressed={on}
      aria-label={on ? `Remove ${movie.title} from watchlist` : `Add ${movie.title} to watchlist`}
      className={`self-start flex items-center gap-1 font-label-sm text-label-sm uppercase tracking-widest transition-opacity hover:opacity-70 disabled:cursor-wait ${
        on ? 'text-on-surface-variant' : 'text-primary'
      }`}
    >
      <span className="material-symbols-outlined text-[16px]" aria-hidden="true">
        {on ? 'check' : 'add'}
      </span>
      {on ? 'On watchlist' : 'Watchlist'}
    </button>
  )
}

/** A 64px poster, linked. The left column of both poster-bearing cards. */
function RowPoster({ movie }: { movie: Movie }) {
  return (
    <Link
      to={`/movie/${movie.id}`}
      title={movie.title}
      className="w-16 md:w-24 aspect-[2/3] shrink-0 rounded bg-surface-container overflow-hidden inner-stroke hover:opacity-80 transition-opacity"
    >
      <img className="w-full h-full object-cover" alt={movie.poster.alt} src={movie.poster.src} />
    </Link>
  )
}

/**
 * One suggested film, with the film of yours it came from.
 *
 * The "because you liked X" line is the whole card: it's a claim the data backs, and X
 * links to the film that made it, so a suggestion can be traced rather than taken on
 * faith.
 */
function RecommendationCard({
  item,
  onToggleWatchlist,
  busy,
}: {
  item: Extract<FeedItem, { kind: 'recommendation' }>
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <Card icon="recommend" eyebrow="Recommended for you">
      <div className="flex gap-md">
        <RowPoster movie={item.movie} />
        <div className="flex flex-col gap-xs min-w-0 flex-grow">
          <Link
            to={`/movie/${item.movie.id}`}
            className="font-headline-md text-[16px] leading-tight font-bold truncate hover:text-primary transition-colors"
          >
            {item.movie.title}
          </Link>
          {/* The crowd average, not anybody's rating — `star_rating` is fractional, so
              it's halved into half-stars as every other crowd score is. Nothing at all
              for a film nobody has voted on, rather than zero stars. */}
          {item.star_rating !== null && (
            <StarRating
              halfStars={Math.round(item.star_rating * 2)}
              size="text-[12px]"
              color="text-tertiary"
              showEmpty={false}
            />
          )}
          {/* "liked" only for a favourite. A watchlisted film is one you mean to watch
              and may never have seen, so claiming you liked it is exactly the kind of
              invented statement this rail replaced. */}
          <p className="font-label-sm text-label-sm text-outline">
            {item.because_favorite ? 'Because you liked ' : 'Because you want to watch '}
            <Link
              to={`/movie/${item.because_movie_id}`}
              className="text-on-surface-variant hover:text-primary transition-colors"
            >
              {item.because}
            </Link>
          </p>
          <WatchlistButton
            movie={item.movie}
            on={item.on_watchlist}
            onToggle={onToggleWatchlist}
            busy={busy}
          />
        </div>
      </div>
    </Card>
  )
}

/** One of the visitor's own logged films. */
function EntryCard({
  item,
  onToggleWatchlist,
  busy,
}: {
  item: Extract<FeedItem, { kind: 'entry' }>
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <Card icon="bookmark_added" eyebrow="You logged this">
      <div className="flex gap-md">
        <RowPoster movie={item.movie} />
        <div className="flex flex-col gap-xs min-w-0 flex-grow">
          <div className="flex items-baseline justify-between gap-sm">
            <Link
              to={`/movie/${item.movie.id}`}
              className="font-headline-md text-[16px] leading-tight font-bold truncate hover:text-primary transition-colors"
            >
              {item.movie.title}
            </Link>
            <span className="font-label-sm text-label-sm text-on-surface-variant shrink-0">
              {item.movie.year}
            </span>
          </div>
          {/* Zero half-stars means written-about-but-never-scored, which the API sends as
              0 — so nothing is drawn rather than five empty glyphs. */}
          {item.rating_half_stars > 0 && (
            <StarRating
              halfStars={item.rating_half_stars}
              size="text-[14px]"
              color="text-primary"
            />
          )}
          <WatchlistButton
            movie={item.movie}
            on={item.on_watchlist}
            onToggle={onToggleWatchlist}
            busy={busy}
          />
        </div>
      </div>
    </Card>
  )
}

/** Dispatch on `kind`. The whole point of the tagged union. */
function FeedCard({
  item,
  onToggleWatchlist,
  busy,
}: {
  item: FeedItem
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  switch (item.kind) {
    case 'review':
      return (
        <Card icon="rate_review" eyebrow="From someone you follow">
          <ReviewCard review={item} showFilm />
        </Card>
      )
    case 'recommendation':
      return (
        <RecommendationCard item={item} onToggleWatchlist={onToggleWatchlist} busy={busy} />
      )
    case 'entry':
      return <EntryCard item={item} onToggleWatchlist={onToggleWatchlist} busy={busy} />
  }
}

/**
 * A feed with nothing in it, saying why and where to go.
 *
 * Every card is derived from something you or the people you follow did, so on a new
 * account there is genuinely nothing — and filling the gap with whatever is popular would
 * be the invented content this screen replaced.
 */
function EmptyFeed() {
  return (
    <div className="flex flex-col gap-sm items-start border border-dashed border-surface-variant rounded-lg p-lg">
      <p className="font-body-md text-body-md text-on-surface-variant">
        Nothing here yet. Follow someone whose taste you trust, or rate a film of your own —
        both fill this page.
      </p>
      <div className="flex gap-md flex-wrap">
        <Link
          to="/people"
          className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
        >
          Find people to follow →
        </Link>
        <Link
          to="/search"
          className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
        >
          Find a film →
        </Link>
      </div>
    </div>
  )
}

export function Feed() {
  const feed = useFeed()
  const sentinel = useRef<HTMLDivElement | null>(null)

  const watchlist = useAction(async (id: string) => {
    const target = !feed.onWatchlist(id)

    // Optimistic: flip immediately, then reconcile with what the server stored.
    feed.patchWatchlist(id, target)
    try {
      const state = await api.setWatchlist(id, target)
      feed.patchWatchlist(id, state.on_watchlist)
    } catch (cause) {
      feed.patchWatchlist(id, !target)
      throw cause
    }
  })

  // Fetch the next page as the bottom comes near, rather than on a "Load more" button:
  // the request is in flight well before the sentinel is on screen, so the column just
  // keeps going. Re-registered whenever `loadMore` changes identity, which it does with
  // the cursor — an observer holding a stale cursor would re-fetch one page forever.
  const { loadMore, done, loading, error, moreError } = feed
  useEffect(() => {
    const node = sentinel.current
    if (!node || done || loading || error || moreError) return

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) void loadMore()
      },
      { rootMargin: PREFETCH_MARGIN },
    )
    observer.observe(node)
    return () => observer.disconnect()
  }, [loadMore, done, loading, error, moreError])

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="feed" />
      <DemoBanner />

      {feed.loading && <Loading />}
      {feed.error && <ErrorNote error={feed.error} />}

      {!feed.loading && !feed.error && (
        <main className="max-w-3xl mx-auto px-margin-mobile md:px-0 py-lg md:py-xl flex flex-col gap-md">
          {/* Quiet, and only while it lasts: the cards below are real, just from the
              last request. A blocking spinner over a full screen of content would be
              the opposite of what caching it bought. */}
          {feed.refreshing && (
            <p className="font-label-sm text-label-sm text-outline uppercase tracking-widest">
              Refreshing your feed…
            </p>
          )}

          {watchlist.error && (
            <ActionError
              message={watchlist.error}
              onDismiss={watchlist.clearError}
              signIn={watchlist.signInRequired}
            />
          )}

          {feed.items.length === 0 ? (
            <EmptyFeed />
          ) : (
            feed.items.map((item) => (
              <FeedCard
                key={itemKey(item)}
                item={item}
                onToggleWatchlist={(id) => void watchlist.run(id)}
                busy={watchlist.busy}
              />
            ))
          )}

          {/* The observer's target. Always mounted while there is more, so its first
              intersection can happen during the initial paint on a tall screen. */}
          {!feed.done && <div ref={sentinel} aria-hidden="true" className="h-px" />}

          {feed.pending && (
            <p className="font-label-sm text-label-sm text-outline uppercase tracking-widest text-center py-md">
              Loading more…
            </p>
          )}

          {/* A failed append stops the observer — otherwise it would retry on every
              scroll event against a server that just said no — so the retry is a button. */}
          {feed.moreError && (
            <div className="flex flex-col gap-sm items-start">
              <ActionError message={feed.moreError} onDismiss={feed.clearMoreError} />
              <button
                onClick={() => void feed.loadMore()}
                className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
              >
                Try again
              </button>
            </div>
          )}

          {/* The feed genuinely ends — the graph is finite — so it says so instead of
              spinning on an empty page forever. */}
          {feed.done && feed.items.length > 0 && (
            <p className="font-label-sm text-label-sm text-outline uppercase tracking-widest text-center py-lg">
              That’s everything for now
            </p>
          )}
        </main>
      )}

      <BottomNavBar active="feed" />
    </div>
  )
}
