/**
 * Movie Feed — Mobile. Ported from `reference/cine-journal/feed-mobile.html`.
 *
 * Two export quirks are preserved deliberately (see the reference README):
 *  - poster titles are 20px `text-headline-md`, larger than the desktop grid's 16px
 *  - posters have square corners — the markup used `rounded-DEFAULT`, which is
 *    not a real Tailwind class, so it emits no CSS. Kept verbatim.
 *
 * The overlay "+" now toggles the watchlist, and posters link to `/movie/:id`.
 * The overlay only appeared on `hover:` in the export, which never fires on a
 * touch screen — it also opens on `focus-within` so the button is reachable.
 */
import { Link } from 'react-router-dom'

import type { MobileFeed as MobileFeedData, MobileFeedItem, Story } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import { ActionError, BottomNavBar, DemoBanner, ErrorNote, Loading } from '../components/Chrome'
import { StarRating } from '../components/StarRating'

function StoryCircle({ story }: { story: Story }) {
  return (
    <div className="flex flex-col items-center gap-xs flex-shrink-0 cursor-pointer group">
      <div
        className={
          story.unseen
            ? 'w-16 h-16 rounded-full p-[2px] bg-gradient-to-tr from-primary to-inverse-primary group-active:scale-95 transition-transform'
            : 'w-16 h-16 rounded-full p-[2px] bg-surface-variant group-active:scale-95 transition-transform'
        }
      >
        <img
          className={
            story.unseen
              ? 'w-full h-full rounded-full object-cover border-2 border-surface'
              : 'w-full h-full rounded-full object-cover border-2 border-surface opacity-80'
          }
          alt={story.avatar.alt}
          src={story.avatar.src}
        />
      </div>
      <span
        className={`font-label-sm text-label-sm truncate w-16 text-center ${
          story.unseen ? 'text-on-surface-variant' : 'text-outline'
        }`}
      >
        {story.name}
      </span>
    </div>
  )
}

function FeedCard({
  item,
  onToggleWatchlist,
  busy,
}: {
  item: MobileFeedItem
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <article className="flex flex-col gap-sm">
      <div className="relative w-full aspect-[2/3] rounded-DEFAULT overflow-hidden poster-shadow poster-inner-stroke bg-surface-container">
        <Link to={`/movie/${item.movie.id}`} aria-label={item.movie.title}>
          <img
            className="w-full h-full object-cover"
            alt={item.movie.poster.alt}
            src={item.movie.poster.src}
          />
        </Link>
        {/* Ignores pointer events so the poster stays tappable; the button
            inside opts back in. On a watchlisted film the overlay stays up. */}
        <div
          className={`absolute inset-0 bg-black/10 transition-opacity flex items-center justify-center backdrop-blur-[2px] pointer-events-none ${
            item.on_watchlist ? 'opacity-100' : 'opacity-0 hover:opacity-100 focus-within:opacity-100'
          }`}
        >
          <button
            onClick={() => onToggleWatchlist(item.movie.id)}
            disabled={busy}
            aria-pressed={item.on_watchlist}
            aria-label={
              item.on_watchlist
                ? `Remove ${item.movie.title} from watchlist`
                : `Add ${item.movie.title} to watchlist`
            }
            className={`pointer-events-auto rounded-full p-2 backdrop-blur-sm active:scale-95 transition-transform disabled:cursor-wait ${
              item.on_watchlist ? 'bg-on-primary/90 text-primary' : 'bg-primary/90 text-on-primary'
            }`}
          >
            <span
              className="material-symbols-outlined block"
              style={{ fontVariationSettings: "'FILL' 1" }}
            >
              {item.on_watchlist ? 'check' : 'add'}
            </span>
          </button>
        </div>
      </div>
      <div className="px-1">
        <Link to={`/movie/${item.movie.id}`} className="block">
          <h3 className="font-headline-md text-headline-md leading-tight text-on-surface truncate">
            {item.movie.title}
          </h3>
        </Link>
        <p className="font-label-sm text-label-sm text-outline mt-1">{item.subtitle}</p>
        {item.rating_half_stars !== null && (
          <StarRating
            halfStars={item.rating_half_stars}
            size="text-[16px]"
            color="text-primary"
            emptyClassName="text-surface-variant"
            className="mt-2 gap-1"
          />
        )}
      </div>
    </article>
  )
}

export function FeedMobile() {
  const { data, error, loading, update } = useApi(() => api.mobileFeed())

  const watchlist = useAction(async (id: string) => {
    const target = !data?.items.find((i) => i.movie.id === id)?.on_watchlist
    const setFlag = (on: boolean) => (current: MobileFeedData) => ({
      ...current,
      items: current.items.map((i) => (i.movie.id === id ? { ...i, on_watchlist: on } : i)),
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
    <div className="bg-background text-on-background font-body-md min-h-screen flex flex-col pb-[80px] md:pb-0">
      <header className="w-full top-0 sticky bg-surface dark:bg-on-background z-40 border-b border-surface-variant dark:border-outline-variant">
        <div className="flex justify-between items-center px-margin-mobile md:px-margin-desktop py-md w-full max-w-7xl mx-auto">
          {/* Home link, as on the desktop bar — see `TopAppBar`. */}
          <Link to="/" aria-label="CinéJournal home">
            <h1 className="font-headline-md text-headline-md font-bold text-primary dark:text-primary-fixed hover:opacity-70 transition-opacity">
              CinéJournal
            </h1>
          </Link>
          <div className="flex gap-4">
            <span className="material-symbols-outlined text-on-surface-variant dark:text-outline cursor-pointer active:opacity-70">
              notifications
            </span>
            <span className="material-symbols-outlined text-on-surface-variant dark:text-outline cursor-pointer active:opacity-70">
              cast
            </span>
          </div>
        </div>
      </header>
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="flex-grow w-full max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop pt-lg md:pt-xl space-y-xl md:space-y-xxl">
          <section className="w-full">
            <h2 className="font-headline-md text-headline-md mb-sm text-on-surface-variant px-1">
              Recent Activity
            </h2>
            <div className="flex overflow-x-auto hide-scrollbar gap-sm md:gap-md py-sm px-1">
              {data.stories.map((story) => (
                <StoryCircle key={story.id} story={story} />
              ))}
            </div>
          </section>

          {watchlist.error && (
            <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
          )}

          <section className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-md md:gap-lg">
            {data.items.map((item) => (
              <FeedCard
                key={item.id}
                item={item}
                onToggleWatchlist={watchlist.run}
                busy={watchlist.busy}
              />
            ))}
          </section>

          <div className="flex justify-center w-full py-lg">
            <Link
              to="/review-mobile"
              className="bg-primary text-on-primary font-body-md px-lg py-sm rounded-full active:scale-95 transition-transform flex items-center gap-sm shadow-sm hover:shadow-md"
            >
              <span className="material-symbols-outlined">edit_square</span>
              Write a Review
            </Link>
          </div>
        </main>
      )}

      <BottomNavBar active="feed" />
    </div>
  )
}
