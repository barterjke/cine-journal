/**
 * Movie Detail. Ported from `reference/movie page/code.html`.
 *
 * Fed by `GET /api/movies/:id`. In TMDB mode that's a real film and an unknown id
 * 404s; in demo mode *any* id resolves, since only Neon Reverie was designed and
 * every other film borrows its synopsis, cast and credits.
 *
 * This replaces the first export's version of the screen, which led with a
 * full-bleed 70vh backdrop and a four-tile bento gallery. The new reference is an
 * editorial three-column layout — poster, details, ratings — with no hero image
 * at all and the stills gone entirely; the film's backdrop now only appears
 * behind the Media block's play button, which is the one thing TMDB has no
 * artwork for.
 *
 * Departures from the reference, all for the same reason the rest of the app
 * departs from its mocks — in a still image a dead end is a design detail, in an
 * SPA it reads as a bug:
 *  - the `more_horiz` overflow button and the "Full Cast & Crew" link are gone.
 *    Both were inert, and there is no screen behind either.
 *  - the mock's static "In Theaters" row is real streaming availability, and the
 *    section hides itself when the film isn't anywhere.
 *  - the app bar is the shared `TopAppBar`, with the nav links and search box the
 *    mock omitted. A detail page is reached from every other screen.
 *
 * The mock also drew no rating control, only a "Rate" pill. The pill is kept and
 * reveals the half-star picker beside it, so "give a starred rating" has
 * somewhere to live without a second card competing with the aggregate score.
 */
import { useState } from 'react'
import { useParams } from 'react-router-dom'

import type { CastMember, FriendActivity, Trailer, WatchOption } from '../api'
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
import { RatePicker } from '../components/RatePicker'
import { StarRating } from '../components/StarRating'

/**
 * Where `/movie` with no id lands in demo mode — the export's own detail page.
 * In TMDB mode the id is resolved from the feed instead, since this slug 404s
 * there; see the fetcher.
 */
const DEFAULT_MOVIE_ID = 'neon-reverie'

const FILLED = { fontVariationSettings: "'FILL' 1" }

/**
 * The mock's aggregate-score green, `#2E7D32`, which is not one of the export's
 * 47 theme tokens — the score is the one place the design steps outside the
 * palette, and matching it means spelling the hex out.
 */
const SCORE_COLOR = 'text-[#2E7D32]'

/**
 * "R • Drama, Sci-Fi • 1h 58m", minus whatever the source doesn't have.
 *
 * Assembled rather than templated because all three parts are genuinely
 * optional: TMDB publishes no certification for plenty of films, an unreleased
 * one has no runtime, and a sparse record has no genres. Joining a `null` in
 * would print a bullet with nothing between it and the next.
 */
function metaLine({
  certification,
  genres,
  runtime,
}: {
  certification: string | null
  genres: string[]
  runtime: string
}): string {
  return [certification, genres.length > 0 ? genres.join(', ') : null, runtime !== '—' ? runtime : null]
    .filter((part): part is string => Boolean(part))
    .join(' • ')
}

/**
 * The Media block: a still with a play button that becomes the player.
 *
 * Swapped in place rather than opening a modal — the tile is already 16:9 and
 * the right size, and a lightbox would be a second dismissal affordance for a
 * page that has none. Mounted with `key={movie id}`, so navigating to another
 * film resets it to the still instead of leaving the previous trailer playing.
 */
function MediaTile({ trailer }: { trailer: Trailer }) {
  const [playing, setPlaying] = useState(false)

  // Only YouTube is embeddable, and the mapper already drops anything else; this
  // is the second half of that contract rather than a redundant check — a Vimeo
  // key in a YouTube embed URL renders a player that can't play.
  if (playing && trailer.site === 'YouTube') {
    return (
      <iframe
        className="w-full aspect-video rounded-lg inner-stroke"
        src={`https://www.youtube.com/embed/${trailer.key}?autoplay=1&rel=0`}
        title={trailer.name}
        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
        allowFullScreen
      />
    )
  }

  return (
    <button
      type="button"
      onClick={() => setPlaying(true)}
      aria-label={`Play ${trailer.name}`}
      className="bg-cover bg-center w-full aspect-video rounded-lg inner-stroke relative group cursor-pointer overflow-hidden"
      style={{ backgroundImage: `url('${trailer.thumbnail.src}')` }}
    >
      <div className="absolute inset-0 bg-black/20 group-hover:bg-black/10 transition-colors flex items-center justify-center">
        <span className="material-symbols-outlined text-white text-[48px]" style={FILLED}>
          play_circle
        </span>
      </div>
    </button>
  )
}

/**
 * One "Where to Watch" row.
 *
 * A link only when there's somewhere to go: TMDB's terms permit linking their
 * own watch page and nothing else, so the demo dataset's invented services get
 * the same row as a `<div>`. `kind` ("Stream", "Rent") rides on the right where
 * the mock put its arrow, since a row that only says "Hulu" doesn't say whether
 * you already pay for it.
 */
function WatchRow({ option, href }: { option: WatchOption; href: string | null }) {
  const inner = (
    <>
      <div className="flex items-center gap-sm min-w-0">
        <div className="w-8 h-8 bg-surface-container-highest rounded flex items-center justify-center overflow-hidden shrink-0">
          {option.logo ? (
            <img className="w-full h-full object-cover" alt={option.logo.alt} src={option.logo.src} />
          ) : (
            /* No artwork upstream — the mock's own generic glyph. */
            <span className="material-symbols-outlined text-on-surface-variant text-sm">
              theaters
            </span>
          )}
        </div>
        {/* Wraps rather than truncates: in a 250px column "Paramount Plus
            Premium" and "Paramount Plus Essential" both clip to "Paramount Plus
            P…", and two rows that read identically are worse than two lines. */}
        <span className="font-body-md text-body-md text-on-surface group-hover:text-primary transition-colors leading-tight">
          {option.provider}
        </span>
      </div>
      <span className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-wider shrink-0 ml-sm">
        {option.kind}
      </span>
    </>
  )

  const shell = 'flex items-center justify-between p-sm rounded-lg group'

  return href ? (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className={`${shell} hover:bg-surface-container-low transition-colors`}
    >
      {inner}
    </a>
  ) : (
    <div className={shell}>{inner}</div>
  )
}

/** One row of the "Friends' Activity" card. */
function FriendRow({ entry }: { entry: FriendActivity }) {
  return (
    <div className="flex gap-sm items-start">
      <img
        className="w-8 h-8 rounded-full object-cover shrink-0"
        alt={entry.author_avatar.alt}
        src={entry.author_avatar.src}
      />
      <div className="flex flex-col min-w-0">
        <div className="flex items-center gap-xs">
          <span className="font-label-sm text-label-sm font-bold">{entry.author_name}</span>
          <span className="text-outline text-xs">
            {entry.kind === 'watched' ? 'watched' : 'added it'}
          </span>
        </div>
        {entry.rating_half_stars !== null && (
          <StarRating
            halfStars={entry.rating_half_stars}
            size="text-sm"
            showEmpty={false}
            className="mt-1"
          />
        )}
      </div>
    </div>
  )
}

/** One portrait in the horizontally scrolling cast rail. */
function CastCard({ member }: { member: CastMember }) {
  return (
    <div className="flex flex-col items-center gap-sm min-w-[100px] snap-start">
      <img
        className="w-20 h-20 rounded-full object-cover inner-stroke"
        alt={member.portrait.alt}
        src={member.portrait.src}
      />
      <div className="text-center">
        <p className="font-body-md text-body-md font-medium text-on-surface leading-tight">
          {member.name}
        </p>
        {/* The mapper writes an em dash for an unnamed part, and the mock shows
            cast with no role line at all rather than a placeholder. */}
        {member.role !== '—' && (
          <p className="font-label-sm text-label-sm text-on-surface-variant mt-1">{member.role}</p>
        )}
      </div>
    </div>
  )
}

export function MovieDetail() {
  const { id } = useParams()
  const { data, error, loading, update } = useApi(async () => {
    if (id) return api.movie(id)
    // `/movie` with no id: the demo slug only resolves in demo mode, so fall back
    // to whatever the feed is showing first rather than to a guaranteed 404.
    try {
      return await api.movie(DEFAULT_MOVIE_ID)
    } catch {
      const feed = await api.feed()
      const first = feed.recent[0] ?? feed.live[0]
      if (!first) throw new Error('No films to show yet.')
      return api.movie(first.movie.id)
    }
  }, [id])

  /**
   * The friends rail, filtered to this film.
   *
   * A separate request so a failed feed can't blank the page, and its error is
   * swallowed for the same reason — the card is supplementary, and the film's own
   * fetch already reports a dead API through `ErrorNote`.
   *
   * There is no per-film activity endpoint and inventing one would mean inventing
   * the activity: the seeded rows carry no film id and are paired with trending
   * films at request time (see `content::feed`). So this shows a real row when a
   * friend really did touch this film, and no card otherwise.
   */
  const activity = useApi(() => api.feed().then((feed) => feed.friend_activity), [])

  // The id the mutations target. `data.id` rather than the route param, so the
  // fallback above writes against the film actually on screen; every control is
  // rendered inside `{data && …}`, so it's never the empty string when one fires.
  const movieId = data?.id ?? ''

  const [rateOpen, setRateOpen] = useState(false)

  const watchlist = useAction(async () => {
    const target = !data?.on_watchlist
    // Optimistic: flip immediately, then reconcile with what the server stored.
    update((current) => ({ ...current, on_watchlist: target }))
    try {
      const state = await api.setWatchlist(movieId, target)
      update((current) => ({ ...current, on_watchlist: state.on_watchlist }))
    } catch (cause) {
      update((current) => ({ ...current, on_watchlist: !target }))
      throw cause
    }
  })

  const rating = useAction(async (halfStars: number) => {
    const previous = data?.your_rating_half_stars ?? null
    update((current) => ({
      ...current,
      your_rating_half_stars: halfStars === 0 ? null : halfStars,
    }))
    try {
      const state = await api.rate(movieId, halfStars)
      update((current) => ({
        ...current,
        your_rating_half_stars: state.your_rating_half_stars,
      }))
    } catch (cause) {
      update((current) => ({ ...current, your_rating_half_stars: previous }))
      throw cause
    }
  })

  const yourRating = data?.your_rating_half_stars ?? 0
  const friends = (activity.data ?? []).filter((entry) => entry.movie_id === movieId)

  return (
    <div className="bg-surface text-on-surface font-body-md min-h-screen pb-xxl">
      <TopAppBar active="movies" />
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop pt-xl">
          {/* Hero: poster | details | ratings. Two columns from `md`, three from
              `lg`, where the ratings column splits off from the details. */}
          <div className="grid grid-cols-1 md:grid-cols-[1fr_2fr] lg:grid-cols-[300px_1fr_250px] gap-gutter md:gap-xl items-start">
            {/* Left column: poster, and the Media block below `lg` where the
                middle column is too narrow for a 16:9 tile beside the text.
                Capped on mobile — the mock is desktop-only, where this column is
                a third of the width; at `grid-cols-1` an uncapped 2:3 poster is
                585px tall on a 390px screen and the title falls below the fold. */}
            <div className="flex flex-col gap-md max-w-[240px] md:max-w-none">
              <img
                className="w-full h-auto rounded-lg poster-shadow inner-stroke object-cover aspect-[2/3]"
                alt={data.poster.alt}
                src={data.poster.src}
              />
              {data.trailer && (
                <div className="lg:hidden flex flex-col gap-sm">
                  <MediaTile key={`${data.id}-narrow`} trailer={data.trailer} />
                  <span className="font-headline-md text-sm font-semibold">
                    {data.trailer.name}
                  </span>
                </div>
              )}
            </div>

            {/* Middle column */}
            <div className="flex flex-col gap-lg">
              <div>
                <h1 className="font-display-lg text-display-lg md:text-headline-lg-mobile lg:text-display-lg text-on-surface mb-xs">
                  {data.title} <span className="text-on-surface-variant font-normal">({data.year})</span>
                </h1>
                <p className="font-body-lg text-body-lg text-on-surface-variant">
                  {metaLine(data)}
                </p>
              </div>

              <div className="text-body-md font-body-md text-on-surface max-w-3xl leading-relaxed">
                {data.synopsis}
              </div>

              {/* Actions. The picker joins this row when the Rate pill is on, so
                  it wraps with the buttons instead of pushing the grid down. */}
              <div className="flex flex-wrap gap-md items-center pt-sm pb-md border-b border-surface-variant">
                <button
                  onClick={() => watchlist.run()}
                  disabled={watchlist.busy}
                  aria-pressed={data.on_watchlist}
                  className={`font-label-sm text-label-sm px-6 py-3 rounded-full flex items-center gap-xs transition-colors shadow-sm disabled:cursor-wait ${
                    data.on_watchlist
                      ? 'bg-primary-fixed text-on-primary-fixed'
                      : 'bg-primary text-on-primary hover:bg-primary-container hover:text-on-primary-container'
                  }`}
                >
                  <span className="material-symbols-outlined" style={FILLED}>
                    {data.on_watchlist ? 'check' : 'add'}
                  </span>
                  {data.on_watchlist ? 'On Watchlist' : 'Watchlist'}
                </button>

                <button
                  onClick={() => setRateOpen((open) => !open)}
                  aria-expanded={rateOpen}
                  className={`font-label-sm text-label-sm px-6 py-3 rounded-full flex items-center gap-xs border transition-colors ${
                    rateOpen || yourRating > 0
                      ? 'bg-surface-container-low border-primary text-primary'
                      : 'bg-surface border-outline-variant text-on-surface hover:bg-surface-container-low'
                  }`}
                >
                  <span
                    className="material-symbols-outlined"
                    style={yourRating > 0 ? FILLED : undefined}
                  >
                    star
                  </span>
                  {yourRating > 0 ? `${yourRating / 2} / 5` : 'Rate'}
                </button>

                {rateOpen && (
                  <div className="flex items-center gap-sm">
                    <RatePicker
                      value={yourRating}
                      onRate={rating.run}
                      busy={rating.busy}
                      size="text-[24px]"
                    />
                    <span className="font-label-sm text-label-sm text-outline">
                      {yourRating > 0 ? 'click again to clear' : 'your rating'}
                    </span>
                  </div>
                )}
              </div>

              {(rating.error ?? watchlist.error) && (
                <div className="flex flex-col gap-sm">
                  {rating.error && (
                    <ActionError message={rating.error} onDismiss={rating.clearError} />
                  )}
                  {watchlist.error && (
                    <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
                  )}
                </div>
              )}

              {/* Credits grid. Rows the source doesn't have are simply absent —
                  the mapper omits them rather than sending "Unknown". */}
              {data.details.length > 0 && (
                /* `auto` below `sm` rather than the mock's fixed 100px:
                   "Cinematography" measures 118px at this size and would spill
                   into the value column. The fixed track returns at `sm`, where
                   it's wide enough and the labels line up as designed. */
                <div className="grid grid-cols-[auto_1fr] sm:grid-cols-[120px_1fr] gap-y-sm gap-x-md text-body-md font-body-md">
                  {data.details.map((fact) => (
                    /* A fragment per row, so the two spans stay siblings of the
                       grid — a wrapper div would collapse each pair into one
                       cell and the labels would stop lining up. */
                    <div key={fact.label} className="contents">
                      <span className="text-on-surface-variant">{fact.label}</span>
                      <span className="font-medium">{fact.value}</span>
                    </div>
                  ))}
                </div>
              )}

              {data.trailer && (
                <div className="hidden lg:flex flex-col gap-sm mt-md w-2/3">
                  <h3 className="font-headline-md text-headline-md mb-xs">Media</h3>
                  <MediaTile key={`${data.id}-wide`} trailer={data.trailer} />
                  <span className="font-label-sm text-label-sm text-on-surface-variant">
                    {data.trailer.name}
                  </span>
                </div>
              )}
            </div>

            {/* Right column: score, availability, friends */}
            <div className="flex flex-col gap-xl">
              {/* An average over no votes is not a 0.0 — with nothing behind it
                  the block hides rather than reporting the film as terrible. */}
              {data.vote_count > 0 && (
                <div className="flex flex-col gap-xs">
                  <div className="flex items-end gap-sm">
                    <span className={`font-display-lg text-display-lg ${SCORE_COLOR}`}>
                      {data.score.toFixed(1)}
                    </span>
                    <span className="text-body-md font-body-md text-on-surface-variant pb-2">
                      / 10
                    </span>
                  </div>
                  <p className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-wider">
                    CinéJournal Score
                  </p>
                  {/* "ratings", not the mock's "reviews": this is TMDB's vote
                      count, and most of those votes carry no written review. */}
                  <p className="text-body-md font-body-md text-on-surface-variant mt-xs">
                    Based on {data.vote_count.toLocaleString()} ratings
                  </p>
                </div>
              )}

              {data.watch_options.length > 0 && (
                <div className="flex flex-col gap-sm">
                  <h3 className="font-headline-md text-headline-md border-b border-surface-variant pb-xs mb-xs">
                    Where to Watch
                  </h3>
                  {data.watch_options.map((option) => (
                    <WatchRow key={option.provider} option={option} href={data.watch_link} />
                  ))}
                  {data.watch_link && (
                    <a
                      href={data.watch_link}
                      target="_blank"
                      rel="noreferrer"
                      className="font-label-sm text-label-sm text-primary uppercase tracking-wider hover:underline px-sm mt-xs"
                    >
                      All options
                    </a>
                  )}
                </div>
              )}

              {friends.length > 0 && (
                <div className="flex flex-col gap-sm bg-surface-container-lowest p-md rounded-xl inner-stroke">
                  <h3 className="font-headline-md text-headline-md">Friends' Activity</h3>
                  <div className="flex flex-col gap-md mt-sm">
                    {friends.map((entry) => (
                      <FriendRow key={entry.id} entry={entry} />
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          {data.cast.length > 0 && (
            <>
              <div className="h-xl"></div>
              <hr className="border-t border-surface-variant" />
              <div className="h-xl"></div>

              <section>
                <div className="flex justify-between items-end mb-lg px-xs">
                  <h2 className="font-headline-lg text-headline-lg text-on-surface">Top Cast</h2>
                </div>
                {/* Scrolls rather than wraps: the rail is the reason the mapper
                    sends ten and not four. */}
                <div className="flex overflow-x-auto gap-md pb-md snap-x hide-scrollbar px-xs">
                  {data.cast.map((member) => (
                    <CastCard key={member.id} member={member} />
                  ))}
                </div>
              </section>
            </>
          )}
        </main>
      )}

      <BottomNavBar active="movies" />
    </div>
  )
}
