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
 * at all. The stills came back, but as the Media carousel rather than a hero:
 * one 16:9 stage that plays the videos and zooms the frames, with a thumbnail
 * rail under it. The mock drew a single tile there, which showed one of the
 * 45–170 videos and none of the 72–192 stills TMDB has per film.
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
 * The heart and the "Write a review" pill sit in the same row for the same
 * reason: three things you can say about a film, said in one place.
 *
 * The mock's one-line "Friends' Activity" card in the right column is now a
 * full-width Reviews section fed by `GET /api/movies/:id/reviews` — real users'
 * prose, the ones you follow first and then the best-rated strangers.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link, useParams } from 'react-router-dom'

import type {
  CastMember,
  DetailFact,
  Image,
  Still,
  Trailer,
  UserReview,
  WatchOption,
} from '../api'
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
import { RatePicker } from '../components/RatePicker'

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
 * One slide of the Media carousel. A video is played where it sits; a frame
 * opens full size.
 *
 * A tagged union rather than one shape with optional fields, so the stage can't
 * be asked to embed a still or to zoom a video: each branch carries only what
 * its own affordance needs.
 */
type MediaItem =
  | { sort: 'video'; key: string; label: string; badge: string; thumbnail: Image; embed: string }
  | { sort: 'still'; key: string; label: string; badge: string; thumbnail: Image; full: Image }

/**
 * The widest a YouTube thumbnail can be and still be their "no such video" image.
 *
 * TMDB keeps listing videos YouTube has since removed, and asking for a removed
 * one's thumbnail does *not* fail in a way the browser reports: the response is a
 * 404 carrying a valid 120×90 grey JPEG, which decodes fine, so `onError` never
 * fires and the slide renders as a grey smudge over a play button that plays
 * "Video unavailable".
 *
 * A dimension check is what's left. Measured across 70 videos from five films:
 * every live `hqdefault` is exactly 480×360 and the only other size seen is the
 * 120×90 placeholder, which is byte-identical for a removed id and for a
 * nonsense one. The threshold sits at the placeholder rather than just below
 * 480, so a genuinely small-but-real frame would still be shown.
 */
const DEAD_THUMBNAIL_WIDTH = 120

/**
 * Every slide the block offers: the videos first, then the frames from the film.
 *
 * Flattened into one list rather than drawn as two rails, because there is one
 * stage and the visitor arrows through it — two rails would mean two "current"
 * slides and no answer to what the next arrow does. Videos lead because a
 * trailer is what someone opening Media came for; a TMDB film carries up to six
 * of them and ten stills behind that.
 *
 * Non-YouTube videos are dropped rather than drawn: nothing else is embeddable
 * here, and the alternative is a play button that can't play. The mapper already
 * filters them upstream, so in practice this removes nothing — it's the second
 * half of that contract, kept here so a change on either side stays safe.
 */
function mediaItems({ trailers, stills }: { trailers: Trailer[]; stills: Still[] }): MediaItem[] {
  return [
    ...trailers
      .filter((trailer) => trailer.site === 'YouTube')
      .map(
        (trailer): MediaItem => ({
          sort: 'video',
          key: trailer.key,
          label: trailer.name,
          badge: trailer.kind,
          thumbnail: trailer.thumbnail,
          // `rel=0` keeps YouTube's end screen to this channel, so a finished
          // trailer doesn't advertise another film on our page.
          embed: `https://www.youtube.com/embed/${trailer.key}?autoplay=1&rel=0`,
        }),
      ),
    ...stills.map(
      (still, index): MediaItem => ({
        sort: 'still',
        key: still.image.src,
        label: `Still ${index + 1}`,
        badge: 'Still',
        thumbnail: still.image,
        full: still.full,
      }),
    ),
  ]
}

/**
 * The Media block: one 16:9 stage, a thumbnail rail under it, and a lightbox for
 * the frames.
 *
 * The export drew a single tile — one video, chosen for you, with no sign that
 * anything else existed. TMDB has 45–170 videos and 72–192 stills per film, so
 * the tile was showing a fraction of a percent of what's there; the stage keeps
 * its shape and the rail is what makes the rest reachable.
 *
 * Videos still play in place, since the stage is already the right size and
 * shape for a player. Stills open a lightbox instead: the rail image is `w780`,
 * which is a thumbnail of a frame rather than the frame, and "look closer" is
 * the whole reason to show a still at all.
 *
 * Mounted with `key={movie id}`, so navigating to another film resets the stage
 * instead of leaving the previous film's trailer playing.
 */
function MediaCarousel({ items }: { items: MediaItem[] }) {
  const [index, setIndex] = useState(0)
  const [playing, setPlaying] = useState(false)
  const [zoomed, setZoomed] = useState(false)
  const [broken, setBroken] = useState<string[]>([])
  const rail = useRef<HTMLDivElement>(null)

  /**
   * Slides whose thumbnail came back real — see `DEAD_THUMBNAIL_WIDTH` for how a
   * removed video gives itself away, and why it has to be caught on load rather
   * than on error.
   *
   * Detected here rather than upstream because this is where it's observable: a
   * server-side liveness probe would be six extra requests per film and would
   * still go a day stale behind the detail cache. The cost is that a thumbnail
   * lost to a flaky connection also drops its slide — acceptable, since the rail
   * loads them all at once and a whole-rail failure means the page has bigger
   * problems than one absent frame.
   */
  const shown = items.filter((item) => !broken.includes(item.key))
  // Clamped rather than reset to 0: when the slide you're on drops out, the next
  // one takes its place, and at the end you land on the new last one.
  const at = Math.min(index, shown.length - 1)
  const current: MediaItem | undefined = shown[at]

  /**
   * Keep the active thumbnail in view. Scrolls the rail's own overflow and
   * nothing else — `scrollIntoView` would drag the *page* to the rail whenever
   * the block sits below the fold, which on this screen it usually does.
   */
  useEffect(() => {
    const track = rail.current
    const active = track?.children[at] as HTMLElement | undefined
    if (!track || !active) return
    track.scrollTo({
      left: active.offsetLeft - (track.clientWidth - active.clientWidth) / 2,
      behavior: 'smooth',
    })
  }, [at])

  // Escape closes the lightbox. Bound only while it's open, so the key keeps
  // whatever meaning it has elsewhere on the page the rest of the time.
  useEffect(() => {
    if (!zoomed) return
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setZoomed(false)
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [zoomed])

  /**
   * Move the stage. Wraps both ways, which is why neither arrow is ever
   * disabled — a carousel of sixteen slides whose right arrow greys out at the
   * end reads as broken more often than it reads as finished.
   */
  const show = (next: number) => {
    setIndex((next + shown.length) % shown.length)
    // Arrowing away from a playing video stops it rather than leaving audio
    // running behind a still.
    setPlaying(false)
    setZoomed(false)
  }

  // Functional, because the rail's images load in parallel and report back in
  // the same tick — `[...broken, key]` off a stale read would keep only the last.
  const markBroken = (key: string) =>
    setBroken((previous) => (previous.includes(key) ? previous : [...previous, key]))

  /**
   * Judge one thumbnail as it loads. Videos only: a TMDB still that resolves at
   * all is the real frame, and its `w780` rendition is legitimately narrower on a
   * source image that small.
   */
  const judge = (item: MediaItem, img: HTMLImageElement) => {
    if (item.sort === 'video' && img.naturalWidth <= DEAD_THUMBNAIL_WIDTH) markBroken(item.key)
  }

  // Every slide's image failed, which in practice means the whole block is
  // unreachable. Nothing to show and nothing to say about it: the film's other
  // sections are unaffected. Below the hooks so their order never changes.
  if (!current) return null

  return (
    <>
      <div className="relative w-full aspect-video rounded-lg inner-stroke overflow-hidden bg-surface-container-highest">
        {current.sort === 'video' && playing ? (
          <iframe
            className="w-full h-full"
            src={current.embed}
            title={current.label}
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen
          />
        ) : (
          <button
            type="button"
            onClick={() => (current.sort === 'video' ? setPlaying(true) : setZoomed(true))}
            aria-label={
              current.sort === 'video' ? `Play ${current.label}` : `View ${current.label} full size`
            }
            className="w-full h-full group cursor-pointer"
          >
            {/* `object-cover`: YouTube's `hqdefault` frame is 480×360 with the
                16:9 image letterboxed inside it, so it has to be cropped rather
                than fitted or the stage grows black bars. */}
            <img
              className="w-full h-full object-cover"
              alt={current.thumbnail.alt}
              src={current.thumbnail.src}
              // The ref catches a decode that already finished — a cached image
              // can complete before React attaches `onLoad` — and the handler
              // catches one that hasn't. `onError` covers a real network failure,
              // which is a different thing from YouTube's 404-with-a-JPEG.
              ref={(img) => {
                if (img?.complete) judge(current, img)
              }}
              onLoad={(event) => judge(current, event.currentTarget)}
              onError={() => markBroken(current.key)}
            />
            <div className="absolute inset-0 bg-black/20 group-hover:bg-black/10 transition-colors flex items-center justify-center">
              <span className="material-symbols-outlined text-white text-[48px]" style={FILLED}>
                {current.sort === 'video' ? 'play_circle' : 'zoom_in'}
              </span>
            </div>
          </button>
        )}

        {/* Hidden while a video plays: the arrows sit where the player's own
            surface is, and the rail below is still there to leave by. */}
        {shown.length > 1 && !playing && (
          <>
            <button
              type="button"
              onClick={() => show(at - 1)}
              aria-label="Previous"
              className="absolute left-sm top-1/2 -translate-y-1/2 w-9 h-9 rounded-full bg-black/50 text-white flex items-center justify-center hover:bg-black/70 transition-colors"
            >
              <span className="material-symbols-outlined text-[20px]">chevron_left</span>
            </button>
            <button
              type="button"
              onClick={() => show(at + 1)}
              aria-label="Next"
              className="absolute right-sm top-1/2 -translate-y-1/2 w-9 h-9 rounded-full bg-black/50 text-white flex items-center justify-center hover:bg-black/70 transition-colors"
            >
              <span className="material-symbols-outlined text-[20px]">chevron_right</span>
            </button>
          </>
        )}
      </div>

      {/* The caption says which of how many, because the stage alone can't: six
          videos captioned by their own titles don't say which is the trailer. */}
      <div className="flex items-baseline justify-between gap-sm">
        <span className="font-headline-md text-sm font-semibold text-on-surface truncate">
          {current.label}
        </span>
        <span className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-wider shrink-0">
          {current.badge} · {at + 1}/{shown.length}
        </span>
      </div>

      {/* `min-w-0` below is load-bearing, not tidying: a scroller's default
          `min-width: auto` is its content's width, so this rail reported a
          1464px minimum for sixteen 84px thumbnails, the grid column honoured
          it as `1fr`'s floor, and the whole hero overflowed the page — the
          `overflow-x-auto` never got the chance to scroll. */}
      {shown.length > 1 && (
        <div
          ref={rail}
          className="flex gap-sm overflow-x-auto hide-scrollbar snap-x pb-xs min-w-0"
        >
          {shown.map((item, position) => (
            <button
              key={item.key}
              type="button"
              onClick={() => show(position)}
              aria-current={position === at}
              aria-label={`Show ${item.label}`}
              className={`relative shrink-0 w-[84px] aspect-video rounded overflow-hidden snap-start transition-opacity ${
                position === at ? 'ring-2 ring-primary' : 'opacity-60 hover:opacity-100'
              }`}
            >
              {/* Empty alt: the same picture is described on the stage, and the
                  button already carries a label. Announcing it twice makes the
                  rail read as sixteen unlabelled images. */}
              <img
                className="w-full h-full object-cover"
                alt=""
                src={item.thumbnail.src}
                ref={(img) => {
                  if (img?.complete) judge(item, img)
                }}
                onLoad={(event) => judge(item, event.currentTarget)}
                onError={() => markBroken(item.key)}
              />
              {item.sort === 'video' && (
                <span
                  className="absolute inset-0 flex items-center justify-center text-white material-symbols-outlined text-[18px]"
                  style={FILLED}
                >
                  play_circle
                </span>
              )}
            </button>
          ))}
        </div>
      )}

      {/* Above the sticky header and the bottom nav, both of which are `z-50`. */}
      {zoomed && current.sort === 'still' && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={current.thumbnail.alt}
          onClick={() => setZoomed(false)}
          className="fixed inset-0 z-[60] bg-black/90 flex items-center justify-center p-md cursor-zoom-out"
        >
          <img
            className="max-w-full max-h-full object-contain rounded"
            alt={current.full.alt}
            src={current.full.src}
          />
          <button
            type="button"
            aria-label="Close"
            className="absolute top-md right-md w-10 h-10 rounded-full bg-black/50 text-white flex items-center justify-center hover:bg-black/70 transition-colors"
          >
            <span className="material-symbols-outlined">close</span>
          </button>
        </div>
      )}
    </>
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

/**
 * What this film's reviews are: the people you follow, then the strangers who
 * rated it highest. Said out loud, because the ordering is the feature — otherwise
 * a friend's 3½ stars appearing above a stranger's 5 looks like a sorting bug.
 */
function reviewsCaption(reviews: UserReview[]): string {
  const friends = reviews.filter((review) => review.author_followed).length
  if (friends === reviews.length) return friends === 1 ? 'From someone you follow' : 'From people you follow'
  if (friends === 0) return 'Best rated first — nobody you follow has reviewed it'
  return `${friends} from ${friends === 1 ? 'someone' : 'people'} you follow, then the best rated`
}

/**
 * The visitor's own review of this film: write it, rewrite it, or clear it.
 *
 * Its own component so the draft is local state, and mounted with
 * `key={movie id}` so navigating to another film starts a fresh box rather than
 * carrying the previous film's prose across — the same trick `MediaTile` uses,
 * and cheaper than an effect that re-seeds on every save.
 *
 * `stored` is what the server holds; the draft starts there. Clearing the box and
 * saving deletes the review, which is why there's no separate delete button — the
 * empty field already means "no review", and `PUT` with an empty body says so.
 */
function ReviewComposer({
  stored,
  onSave,
  busy,
}: {
  stored: string | null
  onSave: (body: string) => void
  busy: boolean
}) {
  const [draft, setDraft] = useState(stored ?? '')
  const changed = draft.trim() !== (stored ?? '')

  return (
    <div className="flex flex-col gap-sm w-full max-w-3xl">
      <textarea
        className="w-full bg-surface-container-low border-none rounded-lg p-4 font-body-md text-body-md text-on-surface focus:ring-1 focus:ring-primary placeholder:text-on-surface-variant resize-none"
        placeholder="What did you make of it?"
        rows={4}
        maxLength={2000}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      <div className="flex items-center gap-md flex-wrap">
        <button
          onClick={() => onSave(draft)}
          disabled={busy || !changed}
          className="font-label-sm text-label-sm px-6 py-3 rounded-full bg-primary text-on-primary hover:bg-primary-container hover:text-on-primary-container transition-colors disabled:opacity-40 disabled:cursor-default"
        >
          {busy ? 'Saving…' : stored ? 'Update review' : 'Post review'}
        </button>
        {/* Only offered once there's something stored to remove — before that the
            box is already empty and the button would do nothing. */}
        {stored && (
          <button
            onClick={() => {
              setDraft('')
              onSave('')
            }}
            disabled={busy}
            className="font-label-sm text-label-sm px-6 py-3 rounded-full border border-outline-variant text-on-surface hover:bg-surface-container-low transition-colors disabled:cursor-wait"
          >
            Delete
          </button>
        )}
        <span className="font-label-sm text-label-sm text-outline ml-auto">
          {draft.trim().length} / 2000
        </span>
      </div>
    </div>
  )
}

/**
 * One portrait in the horizontally scrolling cast rail.
 *
 * The whole tile is the link, portrait included, rather than just the name — a
 * 80px circle is the easiest thing on the rail to hit, and a photograph you can't
 * click beside a name you can reads as two different affordances for one person.
 *
 * Unlinked when `searchable` is false: the demo dataset's cast have no filmography,
 * so the link would land on an empty grid.
 */
function CastCard({ member }: { member: CastMember }) {
  const portrait = (
    <>
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
    </>
  )

  const shape = 'flex flex-col items-center gap-sm min-w-[100px] snap-start'
  if (!member.searchable) return <div className={shape}>{portrait}</div>

  return (
    <Link
      to={`/search?person=${encodeURIComponent(member.id)}`}
      className={`${shape} group`}
      title={`Films with ${member.name}`}
    >
      {portrait}
    </Link>
  )
}

/**
 * A credits-grid value with each person in it linked to their other films.
 *
 * The row arrives as finished text plus the people it names, in order — so this
 * walks the names through the string rather than re-deriving the join, which would
 * guess wrong about a name containing a comma. A name the backend named but that
 * isn't findable in the text is skipped rather than appended; the row's wording is
 * what the visitor reads, and it stays intact either way.
 *
 * Rows that name nobody — "Production", and every row in demo mode — fall through
 * to plain text.
 */
function CreditedNames({ fact }: { fact: DetailFact }) {
  const parts: ReactNode[] = []
  let rest = fact.value

  for (const person of fact.people) {
    const at = rest.indexOf(person.name)
    if (at < 0) continue
    if (at > 0) parts.push(rest.slice(0, at))
    parts.push(
      <Link
        key={`${person.id}-${parts.length}`}
        to={`/search?person=${encodeURIComponent(person.id)}`}
        className="hover:text-primary hover:underline transition-colors"
        title={`Films with ${person.name}`}
      >
        {person.name}
      </Link>,
    )
    rest = rest.slice(at + person.name.length)
  }

  if (parts.length === 0) return <span className="font-medium">{fact.value}</span>
  return (
    <span className="font-medium">
      {parts}
      {rest}
    </span>
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
      // Whatever the feed leads with — one page is plenty, and its first card is the
      // most defensible "some film" this route can pick. An empty feed is an empty
      // account, which is the honest reason for having nothing to show.
      const page = await api.feedPage()
      const first = page.items[0]
      const filmId = first && (first.kind === 'review' ? first.movie_id : first.movie.id)
      if (!filmId) throw new Error('No films to show yet.')
      return api.movie(filmId)
    }
  }, [id])

  // The id the mutations target. `data.id` rather than the route param, so the
  // fallback above writes against the film actually on screen; every control is
  // rendered inside `{data && …}`, so it's never the empty string when one fires.
  const movieId = data?.id ?? ''

  /**
   * This film's reviews: written by real users against this real film id, the ones
   * you follow first and then the best-rated strangers (the ordering is the
   * backend's — see `db::reviews_for_movie`).
   *
   * A separate request from the film itself, and keyed on the resolved `movieId` so
   * the `/movie`-with-no-id fallback asks about the film that actually rendered. Its
   * error is swallowed: the section is supplementary, and the film's own fetch
   * already reports a dead API through `ErrorNote`.
   */
  const reviews = useApi(
    () => (movieId ? api.movieReviews(movieId) : Promise.resolve([])),
    [movieId],
  )

  const [rateOpen, setRateOpen] = useState(false)
  const [reviewOpen, setReviewOpen] = useState(false)

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

  const favorite = useAction(async () => {
    const target = !data?.is_favorite
    update((current) => ({ ...current, is_favorite: target }))
    try {
      const state = await api.setFavorite(movieId, target)
      update((current) => ({ ...current, is_favorite: state.is_favorite }))
    } catch (cause) {
      update((current) => ({ ...current, is_favorite: !target }))
      throw cause
    }
  })

  /**
   * Not optimistic, unlike the three buttons: the composer stays open while this
   * runs, and flipping `your_review` early would re-key nothing but would make the
   * Delete button appear before the write landed. The Saving… label covers the wait.
   */
  const review = useAction(async (body: string) => {
    const state = await api.writeReview(movieId, body)
    update((current) => ({ ...current, your_review: state.your_review }))
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
  const filmReviews = reviews.data ?? []

  /**
   * The carousel's slides, built once here and handed to both copies of the
   * block — the narrow one under the poster and the wide one in the middle
   * column. Two mounts of the same list rather than one moved by CSS, because
   * the two live in different grid columns; each keeps its own place in the
   * carousel, which is fine since only one is ever visible.
   */
  const media = data ? mediaItems(data) : []

  return (
    <div className="bg-surface text-on-surface font-body-md min-h-screen pb-xxl">
      <TopAppBar active="movies" />
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop pt-xl">
          {/* Hero: poster | details | ratings. Two columns from `md`, three from
              `xl`, where the ratings column splits off from the details.

              Three columns at `lg` and not `xl` was the mock's proportions read
              literally, but 300px + 250px of fixed track plus two gutters leaves
              the middle at 311px on a 1100px screen — narrower than the poster
              next to it. The split waits for the width that affords it.

              `minmax(0,…)` rather than a bare `1fr`, which means `minmax(auto,1fr)`
              — a column that can't shrink below its content. One horizontally
              scrolling rail inside it then sets the floor for the whole page and
              the layout overflows sideways instead of the rail scrolling. */}
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)] xl:grid-cols-[300px_minmax(0,1fr)_250px] gap-gutter md:gap-xl items-start">
            {/* Left column: the poster.
                Capped on mobile — the mock is desktop-only, where this column is
                a third of the width; at `grid-cols-1` an uncapped 2:3 poster is
                585px tall on a 390px screen and the title falls below the fold. */}
            <div className="flex flex-col gap-md max-w-[240px] md:max-w-none">
              <img
                className="w-full h-auto rounded-lg poster-shadow inner-stroke object-cover aspect-[2/3]"
                alt={data.poster.alt}
                src={data.poster.src}
              />
            </div>

            {/* Middle column */}
            <div className="flex flex-col gap-lg">
              <div>
                <h1 className="font-display-lg text-display-lg md:text-headline-lg-mobile lg:text-display-lg text-on-surface mb-xs">
                  {data.title}{' '}
                  {/* Omitted for an announced film rather than printed as "(0)". */}
                  {data.year !== null && (
                    <span className="text-on-surface-variant font-normal">({data.year})</span>
                  )}
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

                {/* The heart is a separate act from the rating beside it: a five-star
                    rating says the film is good, a favourite says it's yours. Outlined
                    rather than filled like the Watchlist button, so the row doesn't
                    read as two primary actions. */}
                <button
                  onClick={() => favorite.run()}
                  disabled={favorite.busy}
                  aria-pressed={data.is_favorite}
                  className={`font-label-sm text-label-sm px-6 py-3 rounded-full flex items-center gap-xs border transition-colors disabled:cursor-wait ${
                    data.is_favorite
                      ? 'bg-surface-container-low border-primary text-primary'
                      : 'bg-surface border-outline-variant text-on-surface hover:bg-surface-container-low'
                  }`}
                >
                  <span
                    className="material-symbols-outlined"
                    style={data.is_favorite ? FILLED : undefined}
                  >
                    favorite
                  </span>
                  {data.is_favorite ? 'Favorite' : 'Add to Favorites'}
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

                <button
                  onClick={() => setReviewOpen((open) => !open)}
                  aria-expanded={reviewOpen}
                  className={`font-label-sm text-label-sm px-6 py-3 rounded-full flex items-center gap-xs border transition-colors ${
                    reviewOpen || data.your_review
                      ? 'bg-surface-container-low border-primary text-primary'
                      : 'bg-surface border-outline-variant text-on-surface hover:bg-surface-container-low'
                  }`}
                >
                  <span
                    className="material-symbols-outlined"
                    style={data.your_review ? FILLED : undefined}
                  >
                    rate_review
                  </span>
                  {data.your_review ? 'Your review' : 'Write a review'}
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

                {/* Below the pills rather than beside them: a 4-row textarea in a
                    `flex-wrap` row would sit in the gutter left of it. `w-full`
                    forces its own line. */}
                {reviewOpen && (
                  <div className="w-full pt-sm">
                    <ReviewComposer
                      key={movieId}
                      stored={data.your_review}
                      onSave={review.run}
                      busy={review.busy}
                    />
                  </div>
                )}
              </div>

              {(rating.error ?? watchlist.error ?? favorite.error ?? review.error) && (
                <div className="flex flex-col gap-sm">
                  {rating.error && (
                    <ActionError message={rating.error} onDismiss={rating.clearError} />
                  )}
                  {watchlist.error && (
                    <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
                  )}
                  {favorite.error && (
                    <ActionError message={favorite.error} onDismiss={favorite.clearError} />
                  )}
                  {review.error && (
                    <ActionError message={review.error} onDismiss={review.clearError} />
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
                      <CreditedNames fact={fact} />
                    </div>
                  ))}
                </div>
              )}

              {/* One copy, in the middle column at every width — the mock's
                  `w-2/3` is dropped because this column is already bounded by the
                  grid, and two-thirds of it is a 207px stage at 1100px, narrower
                  than the poster beside it. Below `md` the grid is one column, so
                  full width here is the full page. */}
              {media.length > 0 && (
                <div className="flex flex-col gap-sm mt-md">
                  <h3 className="font-headline-md text-headline-md mb-xs">Media</h3>
                  {/* Keyed by film: the carousel holds the selected index, and
                      without this, navigating to another film keeps slide 9 of a
                      gallery that may only have three. */}
                  <MediaCarousel key={data.id} items={media} />
                </div>
              )}
            </div>

            {/* Right column: score and availability. Below `xl` there is no third
                column, so it wraps to a row of its own and spans both — otherwise
                it lands under the poster in a 251px track and the provider rows
                wrap mid-name. Side by side there, since the score block is short. */}
            <div className="flex flex-col gap-xl md:col-span-2 md:flex-row md:gap-xxl xl:col-span-1 xl:flex-col xl:gap-xl">
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

            </div>
          </div>

          {/* Reviews, full width rather than in the 250px right column the mock's
              one-line "Friends' Activity" card sat in: these are paragraphs of real
              prose, and several run past a thousand words. */}
          {filmReviews.length > 0 && (
            <>
              <div className="h-xl"></div>
              <hr className="border-t border-surface-variant" />
              <div className="h-xl"></div>

              <section className="flex flex-col gap-lg">
                <div className="flex items-baseline justify-between gap-md flex-wrap px-xs">
                  <h2 className="font-headline-lg text-headline-lg text-on-surface">Reviews</h2>
                  <span className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-wider">
                    {reviewsCaption(filmReviews)}
                  </span>
                </div>
                {/* Two columns from `lg`, where the hero is three: a clamped review
                    is short enough that one per row leaves the page mostly gutter. */}
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-x-xl gap-y-lg">
                  {filmReviews.map((review) => (
                    <div
                      key={review.id}
                      className="bg-surface-container-lowest p-md rounded-xl inner-stroke"
                    >
                      <ReviewCard review={review} />
                    </div>
                  ))}
                </div>
              </section>
            </>
          )}

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
