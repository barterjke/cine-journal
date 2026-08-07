/**
 * Movie Feed — Desktop. Layout and class lists ported from
 * `reference/cine-journal/index.html`; the sections are `GET /api/feed`.
 *
 * Two of the export's three are gone. "Live Now" listed discussion rooms with member
 * counts and participant avatars, and "Friends Activity" listed verbs with
 * timestamps — there are no rooms, and nothing records when anyone watched anything,
 * so both were furniture. In their place: what the people you follow have actually
 * written, and films suggested from your own favourites and watchlist. The middle
 * section, "Recent Entries", stays and is now your journal rather than four fixed
 * posters.
 *
 * Every poster and every film name links to `/movie/:id`. The export's cards were
 * styled as clickable (`cursor-pointer`, hover lifts) but went nowhere.
 */
import { useState } from 'react'
import { Link } from 'react-router-dom'

import type { Feed as FeedData, FeedEntry, Recommendation } from '../api'
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
import { ReviewCard } from '../components/People'
import { StarRating } from '../components/StarRating'

/**
 * One suggested film in the sidebar, with the film of yours it came from.
 *
 * The "because you liked X" line is the whole card: it's a claim the data backs, and
 * X links to the film that made the claim, so a suggestion can be traced rather than
 * taken on faith. Keeps the "+" button every other poster in the app has.
 */
function RecommendationRow({
  item,
  onToggleWatchlist,
  busy,
}: {
  item: Recommendation
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="flex gap-md group">
      <Link
        to={`/movie/${item.movie.id}`}
        title={item.movie.title}
        className="w-16 aspect-[2/3] shrink-0 rounded bg-surface-container overflow-hidden inner-stroke hover:opacity-80 transition-opacity"
      >
        <img
          className="w-full h-full object-cover"
          alt={item.movie.poster.alt}
          src={item.movie.poster.src}
        />
      </Link>
      <div className="flex flex-col gap-xs min-w-0 flex-grow">
        <Link
          to={`/movie/${item.movie.id}`}
          className="font-body-md text-body-md font-bold truncate hover:text-primary transition-colors"
        >
          {item.movie.title}
        </Link>
        {/* The crowd average, not anybody's rating — `star_rating` is fractional, so
            it's halved into half-stars the same way every other crowd score is.
            Nothing is drawn for a film nobody has voted on, rather than zero stars. */}
        {item.star_rating !== null && (
          <StarRating
            halfStars={Math.round(item.star_rating * 2)}
            size="text-[12px]"
            color="text-tertiary"
            showEmpty={false}
          />
        )}
        {/* "liked" only for a favourite. A watchlisted film is one the visitor
            means to watch and may never have seen, so claiming they liked it is
            exactly the kind of invented statement this rail replaced. */}
        <p className="font-label-sm text-label-sm text-outline">
          {item.because_favorite ? 'Because you liked ' : 'Because you want to watch '}
          <Link
            to={`/movie/${item.because_movie_id}`}
            className="text-on-surface-variant hover:text-primary transition-colors"
          >
            {item.because}
          </Link>
        </p>
        <button
          onClick={() => onToggleWatchlist(item.movie.id)}
          disabled={busy}
          aria-pressed={item.on_watchlist}
          aria-label={
            item.on_watchlist
              ? `Remove ${item.movie.title} from watchlist`
              : `Add ${item.movie.title} to watchlist`
          }
          className={`self-start flex items-center gap-1 font-label-sm text-label-sm uppercase tracking-widest transition-opacity hover:opacity-70 disabled:cursor-wait ${
            item.on_watchlist ? 'text-on-surface-variant' : 'text-primary'
          }`}
        >
          <span className="material-symbols-outlined text-[16px]" aria-hidden="true">
            {item.on_watchlist ? 'check' : 'add'}
          </span>
          {item.on_watchlist ? 'On watchlist' : 'Watchlist'}
        </button>
      </div>
    </div>
  )
}

function PosterTile({
  entry,
  onToggleWatchlist,
  busy,
}: {
  entry: FeedEntry
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="flex flex-col gap-sm group">
      <div className="aspect-[2/3] w-full rounded-lg overflow-hidden inner-stroke soft-shadow relative bg-surface-container-low">
        <Link to={`/movie/${entry.movie.id}`} aria-label={entry.movie.title}>
          <img
            className="w-full h-full object-cover transition-transform duration-700 group-hover:scale-105"
            alt={entry.movie.poster.alt}
            src={entry.movie.poster.src}
          />
        </Link>
        {/* Overlay ignores pointer events so it never eats the poster's click;
            only the button inside takes one. It stays up on a watchlisted film,
            otherwise the state would be invisible until you hover. */}
        <div
          className={`absolute inset-0 bg-black/40 transition-opacity duration-300 flex items-center justify-center backdrop-blur-sm pointer-events-none ${
            entry.on_watchlist
              ? 'opacity-100'
              : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'
          }`}
        >
          <button
            onClick={() => onToggleWatchlist(entry.movie.id)}
            disabled={busy}
            aria-pressed={entry.on_watchlist}
            aria-label={
              entry.on_watchlist
                ? `Remove ${entry.movie.title} from watchlist`
                : `Add ${entry.movie.title} to watchlist`
            }
            className={`pointer-events-auto w-12 h-12 rounded-full border flex items-center justify-center transition-colors disabled:cursor-wait ${
              entry.on_watchlist
                ? 'bg-white text-black border-white'
                : 'bg-white/20 border-white/50 text-white hover:bg-white hover:text-black'
            }`}
          >
            <span
              className="material-symbols-outlined"
              style={{ fontVariationSettings: "'FILL' 1" }}
            >
              {entry.on_watchlist ? 'check' : 'add'}
            </span>
          </button>
        </div>
      </div>
      <div className="flex flex-col">
        <div className="flex justify-between items-baseline gap-2">
          <Link to={`/movie/${entry.movie.id}`} className="truncate">
            <h3 className="font-headline-md text-[16px] leading-tight text-on-background font-bold truncate hover:text-primary transition-colors">
              {entry.movie.title}
            </h3>
          </Link>
          <span className="font-label-sm text-label-sm text-on-surface-variant shrink-0">
            {entry.movie.year}
          </span>
        </div>
        <StarRating
          halfStars={entry.rating_half_stars}
          size="text-[14px]"
          color="text-primary"
          className="mt-1"
        />
      </div>
    </div>
  )
}

/**
 * A section with nothing in it yet, saying why and where to go.
 *
 * Every section of this screen is derived from something the visitor or the people
 * they follow did, so all three can legitimately be empty — on a new account, all
 * three are. An empty column with no explanation reads as a broken page, and filling
 * it with whatever is popular would be exactly the invented content this screen
 * replaced.
 */
function EmptySection({ children, to, cta }: { children: string; to: string; cta: string }) {
  return (
    <div className="flex flex-col gap-sm items-start border border-dashed border-surface-variant rounded-lg p-lg">
      <p className="font-body-md text-body-md text-on-surface-variant">{children}</p>
      <Link
        to={to}
        className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
      >
        {cta} →
      </Link>
    </div>
  )
}

export function Feed() {
  const { data, error, loading, update } = useApi(() => api.feed())
  const [gridView, setGridView] = useState(true)

  const watchlist = useAction(async (id: string) => {
    // The same film can be in both the journal grid and the suggestion rail, and both
    // draw a "+" — so the target comes from whichever holds it and both are flipped.
    const current =
      data?.recent.find((e) => e.movie.id === id)?.on_watchlist ??
      data?.recommended.find((r) => r.movie.id === id)?.on_watchlist ??
      false
    const target = !current
    const setFlag = (on: boolean) => (current: FeedData) => ({
      ...current,
      recent: current.recent.map((e) => (e.movie.id === id ? { ...e, on_watchlist: on } : e)),
      recommended: current.recommended.map((r) =>
        r.movie.id === id ? { ...r, on_watchlist: on } : r,
      ),
    })

    // Optimistic: flip immediately, then reconcile with what the server stored.
    update(setFlag(target))
    try {
      const state = await api.setWatchlist(id, target)
      update(setFlag(state.on_watchlist))
    } catch (cause) {
      update(setFlag(!target))
      throw cause
    }
  })

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="feed" />
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl grid grid-cols-1 md:grid-cols-12 gap-gutter">
          <section className="md:col-span-8 lg:col-span-9 flex flex-col gap-xxl">
            {/* What the people you follow have written. Two columns, since a review
                is a paragraph of prose rather than a poster — at one column on a wide
                screen the lines run past comfortable reading length. */}
            <div className="flex flex-col gap-lg">
              <div className="flex items-center gap-sm border-b border-surface-variant pb-sm">
                <span className="material-symbols-outlined text-on-surface-variant" aria-hidden="true">
                  rate_review
                </span>
                <h2 className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-widest">
                  From people you follow
                </h2>
              </div>
              {data.friend_reviews.length > 0 ? (
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-x-gutter gap-y-xl">
                  {data.friend_reviews.map((review) => (
                    <ReviewCard key={review.id} review={review} showFilm />
                  ))}
                </div>
              ) : (
                <EmptySection to="/people" cta="Find people to follow">
                  Nobody you follow has written a review yet.
                </EmptySection>
              )}
            </div>

            {/* Recent Entries */}
            <div className="flex flex-col gap-lg">
              <div className="flex justify-between items-end border-b border-surface-variant pb-sm">
                <h2 className="font-headline-lg text-headline-lg text-on-background">
                  Recent Entries
                </h2>
                <div className="flex gap-sm">
                  <button
                    onClick={() => setGridView(true)}
                    aria-label="Grid view"
                    aria-pressed={gridView}
                    className={`p-xs hover:text-primary transition-colors ${
                      gridView ? 'text-primary' : 'text-outline'
                    }`}
                  >
                    <span
                      className="material-symbols-outlined"
                      style={gridView ? { fontVariationSettings: "'FILL' 1" } : undefined}
                    >
                      grid_view
                    </span>
                  </button>
                  <button
                    onClick={() => setGridView(false)}
                    aria-label="List view"
                    aria-pressed={!gridView}
                    className={`p-xs hover:text-primary transition-colors ${
                      gridView ? 'text-outline' : 'text-primary'
                    }`}
                  >
                    <span
                      className="material-symbols-outlined"
                      style={gridView ? undefined : { fontVariationSettings: "'FILL' 1" }}
                    >
                      view_list
                    </span>
                  </button>
                </div>
              </div>

              {watchlist.error && (
                <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
              )}

              {data.recent.length > 0 ? (
                <div
                  className={
                    gridView
                      ? 'grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-x-gutter gap-y-xl'
                      : 'grid grid-cols-1 sm:grid-cols-2 gap-x-gutter gap-y-lg'
                  }
                >
                  {data.recent.map((entry) => (
                    <PosterTile
                      key={entry.id}
                      entry={entry}
                      onToggleWatchlist={watchlist.run}
                      busy={watchlist.busy}
                    />
                  ))}
                </div>
              ) : (
                <EmptySection to="/search" cta="Find a film">
                  Rate or review a film and it will show up here.
                </EmptySection>
              )}
            </div>
          </section>

          {/* Suggestions from the visitor's own favourites and watchlist. Where the
              export put a "Friends Activity" sidebar of invented verbs — same
              position, same sticky column, but every row here traces back to a film
              the visitor chose. */}
          <aside className="hidden md:flex md:col-span-4 lg:col-span-3 flex-col gap-xl">
            <div className="sticky top-32 flex flex-col gap-lg">
              <div className="flex items-center gap-sm border-b border-surface-variant pb-sm">
                <span className="material-symbols-outlined text-on-surface-variant" aria-hidden="true">
                  recommend
                </span>
                <h2 className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-widest">
                  Recommended for you
                </h2>
              </div>
              {data.recommended.length > 0 ? (
                <div className="flex flex-col gap-lg">
                  {data.recommended.map((item) => (
                    <RecommendationRow
                      key={item.movie.id}
                      item={item}
                      onToggleWatchlist={watchlist.run}
                      busy={watchlist.busy}
                    />
                  ))}
                </div>
              ) : (
                /* Two reasons this is empty and one message, because the screen can't
                   tell them apart: no seeds yet, or no token to ask upstream with.
                   Either way, favouriting a film is what fills it. */
                <EmptySection to="/search" cta="Browse films">
                  Favourite a film or add one to your watchlist, and suggestions based
                  on it will appear here.
                </EmptySection>
              )}
            </div>
          </aside>
        </main>
      )}

      <BottomNavBar active="feed" />
    </div>
  )
}
