/**
 * One collection in full: the page behind a profile tile.
 *
 * The profile's tiles are summaries capped at four and six posters. They used to link to
 * duplicate full-width sections *on the same page*, so every film appeared twice under the
 * same heading; now they link here, which is what makes a tile worth clicking.
 *
 * One screen for three slugs and for anybody's collection, because they are the same page:
 * the server resolves the title, the description and whose it is, so nothing here branches
 * on which one it drew. `journal` is the one with ratings behind it and the only one whose
 * posters carry stars — the server sends `null` for the rest.
 */
import { Link, useParams, useSearchParams } from 'react-router-dom'

import type { Collection as CollectionData } from '../api'
import { api, isNotFound } from '../api'
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
import { personPath } from '../components/People'
import { PosterTile } from '../components/PosterTile'

/** The route a profile tile links to. One place, so the two pages can't disagree. */
export function collectionPath(slug: string, handle?: string | null): string {
  const person = handle?.replace(/^@/, '')
  return `/collections/${slug}${person ? `?person=${encodeURIComponent(person)}` : ''}`
}

/**
 * What each empty collection says, and where to go about it.
 *
 * All three of the visitor's start empty on a new account, so this is the first thing the
 * page says rather than an edge case. Somebody else's gets no call to action — there is
 * nothing you can do about what they haven't favourited.
 */
function emptyState(slug: string, owner: string | null): { copy: string; to: string; cta: string } {
  if (owner) {
    const copy =
      slug === 'favorites'
        ? `${owner} hasn’t raved about anything yet.`
        : `${owner}’s watchlist is empty.`
    return { copy, to: '/search', cta: 'Find something to watch' }
  }
  switch (slug) {
    case 'favorites':
      return {
        copy: 'Press the heart on any film’s page and it collects here.',
        to: '/search',
        cta: 'Find something to love',
      }
    case 'watchlist':
      return {
        copy: 'Nothing logged yet — the "+" over any poster adds one.',
        to: '/search',
        cta: 'Find something to watch',
      }
    default:
      return {
        copy: 'Rate or review a film and it shows up here, newest first.',
        to: '/movie',
        cta: 'Pick a film to rate',
      }
  }
}

function Body({
  data,
  onToggleWatchlist,
  busy,
}: {
  data: CollectionData
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  const first = data.owner ? data.owner.name.split(' ')[0] : null
  const empty = emptyState(data.slug, first)

  return (
    <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-lg">
      <header className="flex flex-col gap-sm">
        {/* Back to whoever's collection this is. The profile is where you came from and
            the only place this page is linked from, so it's a real destination rather
            than a decorative breadcrumb. */}
        <Link
          to={data.owner ? personPath(data.owner.handle) : '/profile'}
          className="self-start flex items-center gap-xs font-label-sm text-label-sm text-outline uppercase tracking-wider hover:text-primary transition-colors"
        >
          <span className="material-symbols-outlined text-[16px]" aria-hidden="true">
            arrow_back
          </span>
          {data.owner ? data.owner.name : 'Your profile'}
        </Link>

        <div className="flex items-end justify-between gap-md border-b border-surface-variant pb-sm">
          <div className="flex items-center gap-md min-w-0">
            {data.owner && (
              <div className="w-12 h-12 shrink-0 rounded-full overflow-hidden border border-surface-variant bg-surface-container">
                <img
                  className="w-full h-full object-cover"
                  alt={data.owner.avatar.alt}
                  src={data.owner.avatar.src}
                />
              </div>
            )}
            <h1 className="font-headline-lg-mobile md:font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background truncate">
              {data.title}
            </h1>
          </div>
          {/* `movies.length` is the count here, unlike the profile's tiles: this page is
              the whole collection, so the number can't be a clamped view of a longer one. */}
          {data.movies.length > 0 && (
            <span className="font-label-sm text-label-sm text-outline shrink-0">
              {data.movies.length}
            </span>
          )}
        </div>

        <p className="font-body-md text-body-md text-on-surface-variant">{data.description}</p>
      </header>

      {data.movies.length ? (
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-md md:gap-gutter">
          {data.movies.map((item) => (
            <PosterTile
              key={item.movie.id}
              movie={item.movie}
              onWatchlist={item.on_watchlist}
              rating={item.rating_half_stars}
              onToggleWatchlist={onToggleWatchlist}
              busy={busy}
            />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-sm items-start border border-dashed border-surface-variant rounded-lg p-lg">
          <p className="font-body-md text-body-md text-on-surface-variant">{empty.copy}</p>
          <Link
            to={empty.to}
            className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
          >
            {empty.cta} →
          </Link>
        </div>
      )}
    </main>
  )
}

/** A slug or a nickname that names nothing. Distinct from a dead API. */
function NotFound() {
  return (
    <div className="flex flex-col items-center gap-sm py-xxl text-center">
      <span className="material-symbols-outlined text-outline">collections_bookmark</span>
      <p className="font-body-md text-body-md text-on-background">No such collection.</p>
      <p className="font-label-sm text-label-sm text-on-surface-variant">
        There is nothing at this address.
      </p>
      <Link
        to="/profile"
        className="font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 bg-primary text-on-primary rounded-full hover:opacity-90 transition-opacity mt-sm"
      >
        Your profile
      </Link>
    </div>
  )
}

export function Collection() {
  const { slug = '' } = useParams()
  const person = useSearchParams()[0].get('person')
  const { data, error, loading, update, reload } = useApi(
    () => api.collection(slug, person),
    [slug, person],
  )

  // Somebody else's collection still has *your* watchlist buttons on it — the poster is
  // about them, the button is about you.
  const watchlist = useAction(async (id: string) => {
    const current = data?.movies.find((item) => item.movie.id === id)?.on_watchlist ?? false
    const target = !current
    const setFlag = (on: boolean) => (current: CollectionData) => ({
      ...current,
      movies: current.movies.map((item) =>
        item.movie.id === id ? { ...item, on_watchlist: on } : item,
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

  // A 404 is a real answer — this URL names nothing — and gets the empty state rather
  // than "couldn't reach the API", which would send you to restart a healthy server.
  const missing = isNotFound(error)

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active={person ? 'friends' : 'profile'} />
      <DemoBanner />
      {watchlist.error && (
        <div className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop pt-md">
          <ActionError
            message={watchlist.error}
            onDismiss={watchlist.clearError}
            signIn={watchlist.signInRequired}
          />
        </div>
      )}
      {loading && <Loading />}
      {missing && <NotFound />}
      {error && !missing && <ErrorNote error={error} onRetry={reload} />}
      {data && (
        <Body
          data={data}
          onToggleWatchlist={(id) => void watchlist.run(id)}
          busy={watchlist.busy}
        />
      )}
      <BottomNavBar active={person ? 'friends' : 'profile'} />
    </div>
  )
}
