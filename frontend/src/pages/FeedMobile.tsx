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

/**
 * One circle in the stories rail: somebody you follow, tapping through to their
 * newest review.
 *
 * The export's rail was five fixed avatars with an invented read/unread state and no
 * destination — the ring meant nothing and the tap did nothing. Here the ring means
 * "has a review to show" (the only such state anything can answer) and the tap opens
 * it. Someone you follow who hasn't written anything is still drawn, dimmed and as a
 * plain `div`: who you follow is a fact whether or not they've posted, but there is
 * nothing to link to, and a link that goes nowhere is what this replaced.
 */
function StoryCircle({ story }: { story: Story }) {
  const ring = story.unseen
    ? 'bg-gradient-to-tr from-primary to-inverse-primary'
    : 'bg-surface-variant'
  const avatar = (
    <>
      <div
        className={`w-16 h-16 rounded-full p-[2px] group-active:scale-95 transition-transform ${ring}`}
      >
        <img
          className={`w-full h-full rounded-full object-cover border-2 border-surface ${
            story.unseen ? '' : 'opacity-80'
          }`}
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
    </>
  )

  const shell = 'flex flex-col items-center gap-xs flex-shrink-0 group'

  if (!story.review_id) {
    return (
      <div className={shell} title={`${story.name} hasn't written a review yet`}>
        {avatar}
      </div>
    )
  }

  return (
    <Link
      to={`/review-mobile/${story.review_id}`}
      className={`${shell} cursor-pointer`}
      aria-label={`Read ${story.name}'s newest review`}
    >
      {avatar}
    </Link>
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
        {/* The subtitle is the card's one difference in kind: "Elena rated it" opens
            what Elena wrote, "Because you liked X" explains itself and stays text.
            The poster goes to the film either way. */}
        {item.review_id ? (
          <Link
            to={`/review-mobile/${item.review_id}`}
            className="font-label-sm text-label-sm text-primary mt-1 block truncate hover:opacity-70 transition-opacity"
          >
            {item.subtitle}
          </Link>
        ) : (
          <p className="font-label-sm text-label-sm text-outline mt-1 truncate">{item.subtitle}</p>
        )}
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
          {/* Was a bell and a cast icon: bare `<span>`s with `cursor-pointer`, no
              handler and nothing behind them — there are no notifications and nothing
              to cast to. One link to your own profile instead, which is the only thing
              this corner of a masthead can actually do here. */}
          <Link
            to="/profile"
            aria-label="Your profile"
            className="text-on-surface-variant dark:text-outline hover:text-primary transition-colors active:opacity-70"
          >
            <span className="material-symbols-outlined block">account_circle</span>
          </Link>
        </div>
      </header>
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="flex-grow w-full max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop pt-lg md:pt-xl space-y-xl md:space-y-xxl">
          {/* "Recent Activity" was the export's heading over five avatars that stood
              for nothing. These are the people you follow, so the heading says so.
              The rail is hidden outright when you follow nobody — an empty scroller
              with a title above it looks like content that failed to load. */}
          {data.stories.length > 0 && (
            <section className="w-full">
              <h2 className="font-headline-md text-headline-md mb-sm text-on-surface-variant px-1">
                People you follow
              </h2>
              <div className="flex overflow-x-auto hide-scrollbar gap-sm md:gap-md py-sm px-1">
                {data.stories.map((story) => (
                  <StoryCircle key={story.id} story={story} />
                ))}
              </div>
            </section>
          )}

          {watchlist.error && (
            <ActionError
              message={watchlist.error}
              onDismiss={watchlist.clearError}
              signIn={watchlist.signInRequired}
            />
          )}

          {data.items.length > 0 ? (
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
          ) : (
            /* Both halves of the grid are empty: nobody you follow has written
               anything, and you have no favourites to suggest from. Saying which is
               beside the point — following someone or favouriting a film fixes both. */
            <section className="flex flex-col gap-sm items-start border border-dashed border-surface-variant rounded-lg p-lg">
              <p className="font-body-md text-body-md text-on-surface-variant">
                Nothing here yet. Follow a few people, or favourite a film to get
                suggestions based on it.
              </p>
              <Link
                to="/people"
                className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
              >
                Find people to follow →
              </Link>
            </section>
          )}

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
