/**
 * The pieces a profile is built from, shared by your own page (`Profile`) and
 * everyone else's (`Person`).
 *
 * They live here rather than in either screen because the two are supposed to be
 * the same kind of page — same header, same poster strips, same grid — and two
 * copies of that drifted apart the first time one of them changed. What differs
 * between the screens is only what each genuinely knows: your page has an editable
 * bio and a Following list, theirs has a follow button and their prose reviews.
 *
 * Every class string is spelled out literally. Tailwind's JIT scans the source for
 * them, so a composed `text-${size}` would compile to nothing.
 */
import type { Movie } from '../api'
import { Link } from 'react-router-dom'

import { Poster } from './PosterTile'

/** Shared by both tile variants, so the linked one can't drift from the plain one. */
const TILE_SHELL =
  'bg-surface-container-low rounded-xl p-md border border-surface-variant flex flex-col gap-sm'

const TILE_LABEL =
  'font-label-sm text-label-sm font-bold uppercase tracking-wider text-outline'

/**
 * A bento tile: an uppercase label, an optional link out, and its content.
 *
 * When `to` is set the *whole tile* is the link, not just the chevron. The chevron was
 * a 24px target on a card the size of a hand, and clicking anywhere else — the label,
 * the posters, the gap — did nothing, which read as the tile being decorative.
 *
 * A `<Link>` rather than an `<a href="#...">`: these go to a collection page now instead
 * of scrolling to a duplicate of themselves further down. Note that react-router
 * resolves a bare `#` against the *current* path, so `to="#"` on `/profile` would be a
 * live-looking link back to the page you're on.
 *
 * The posters inside are links too. Nesting an `<a>` inside an `<a>` is invalid HTML and
 * the browser un-nests it, so the linked variant draws its films as plain images — see
 * `PosterStrip`'s `linked` prop. The tile goes to the collection; the collection page is
 * where a poster goes to its film.
 */
export function Tile({
  label,
  to,
  children,
}: {
  label: string
  to?: string
  children: React.ReactNode
}) {
  if (!to) {
    return (
      <div className={TILE_SHELL}>
        <h2 className={TILE_LABEL}>{label}</h2>
        {children}
      </div>
    )
  }

  return (
    <Link
      to={to}
      // `group` so the chevron responds to a hover anywhere on the tile — otherwise the
      // arrow looks like the only live part of a card that is now entirely live.
      className={`${TILE_SHELL} group hover:border-outline-variant transition-colors`}
    >
      <div className="flex items-center justify-between gap-sm">
        <h2 className={TILE_LABEL}>{label}</h2>
        <span
          className="material-symbols-outlined text-primary group-hover:translate-x-0.5 transition-transform"
          aria-hidden="true"
        >
          chevron_right
        </span>
      </div>
      {children}
    </Link>
  )
}

/** What a strip shows before the thing that fills it has happened. */
export function Empty({ children }: { children: React.ReactNode }) {
  return <p className="font-body-md text-body-md text-on-surface-variant">{children}</p>
}

/** One 64px poster. The same frame either way, so a strip's rows line up. */
const THUMBNAIL =
  'w-16 aspect-[2/3] rounded bg-surface-container overflow-hidden shrink-0 inner-stroke'

/** The strips' 64px thumbnails, linked to the film. */
export function Thumbnail({ film }: { film: Movie }) {
  return (
    <Link
      to={`/movie/${film.id}`}
      title={film.title}
      className={`${THUMBNAIL} hover:opacity-80 transition-opacity`}
    >
      <Poster image={film.poster} className="w-full h-full object-cover" />
    </Link>
  )
}

/**
 * A row of thumbnails inside a tile, or the empty copy in its place.
 *
 * `overflow-hidden` rather than a scroller: the tile is a summary of a collection page,
 * and a second scrollable region inside a bento cell is a lot of affordance for four
 * posters.
 *
 * `linked` is false inside a linked `Tile`, where the whole card is already an `<a>` and
 * nesting another is invalid HTML the browser silently un-nests — leaving the tile's own
 * link broken wherever a poster covered it.
 */
export function PosterStrip({
  films,
  empty,
  linked = true,
}: {
  films: Movie[]
  empty: React.ReactNode
  linked?: boolean
}) {
  if (films.length === 0) return <Empty>{empty}</Empty>
  return (
    <div className="flex gap-sm overflow-hidden">
      {films.map((film) =>
        linked ? (
          <Thumbnail key={film.id} film={film} />
        ) : (
          <div key={film.id} className={THUMBNAIL} title={film.title}>
            <Poster image={film.poster} className="w-full h-full object-cover" />
          </div>
        ),
      )}
    </div>
  )
}

/** A section heading with its count on the right, as the mock drew them. */
export function SectionHeading({ title, count }: { title: string; count?: number }) {
  return (
    <div className="flex items-baseline justify-between mb-sm">
      <h2 className="font-headline-lg-mobile md:font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background">
        {title}
      </h2>
      {count !== undefined && count > 0 && (
        <span className="font-label-sm text-label-sm text-outline">{count}</span>
      )}
    </div>
  )
}

/**
 * The header both pages open with: 96px avatar, name, handle line, bio.
 *
 * `bio` and `action` are slots rather than strings, because that is the whole
 * difference between the two screens at the top of the page — your own bio is an
 * editable field and the action is nothing; theirs is a paragraph and the action is
 * the follow button.
 */
export function ProfileHeader({
  avatar,
  name,
  meta,
  badge,
  bio,
  action,
}: {
  avatar: { src: string; alt: string }
  name: string
  /** "@handle • Cinephile since 2018", or just the handle. */
  meta: string
  badge?: React.ReactNode
  bio?: React.ReactNode
  action?: React.ReactNode
}) {
  return (
    <section className="flex items-start gap-md py-md">
      <div className="w-24 h-24 shrink-0 rounded-full overflow-hidden border border-surface-variant soft-shadow bg-surface-container">
        <img className="w-full h-full object-cover" alt={avatar.alt} src={avatar.src} />
      </div>
      <div className="flex flex-col gap-xs flex-grow min-w-0">
        <div className="flex items-center gap-sm flex-wrap">
          <h1 className="font-headline-md text-headline-md text-on-background">{name}</h1>
          {badge}
        </div>
        <p className="font-label-sm text-label-sm text-outline uppercase tracking-wider">{meta}</p>
        {bio}
      </div>
      {action}
    </section>
  )
}
