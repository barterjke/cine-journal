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

/** A bento tile: an uppercase label, an optional link out, and its content. */
export function Tile({
  label,
  to,
  children,
}: {
  label: string
  to?: string
  children: React.ReactNode
}) {
  return (
    <div className="bg-surface-container-low rounded-xl p-md border border-surface-variant flex flex-col gap-sm">
      <div className="flex items-center justify-between">
        <h2 className="font-label-sm text-label-sm font-bold uppercase tracking-wider text-outline">
          {label}
        </h2>
        {to && (
          <a
            className="material-symbols-outlined text-primary text-md hover:opacity-70 transition-opacity"
            href={to}
            aria-label={`Jump to ${label}`}
          >
            chevron_right
          </a>
        )}
      </div>
      {children}
    </div>
  )
}

/** What a strip shows before the thing that fills it has happened. */
export function Empty({ children }: { children: React.ReactNode }) {
  return <p className="font-body-md text-body-md text-on-surface-variant">{children}</p>
}

/** The strips' 64px thumbnails. */
export function Thumbnail({ film }: { film: Movie }) {
  return (
    <Link
      to={`/movie/${film.id}`}
      title={film.title}
      className="w-16 aspect-[2/3] rounded bg-surface-container overflow-hidden shrink-0 inner-stroke hover:opacity-80 transition-opacity"
    >
      <img className="w-full h-full object-cover" alt={film.poster.alt} src={film.poster.src} />
    </Link>
  )
}

/**
 * A row of thumbnails inside a tile, or the empty copy in its place.
 *
 * `overflow-hidden` rather than a scroller: the tile is a summary that links to the
 * full grid below, and a second scrollable region inside a bento cell is a lot of
 * affordance for four posters.
 */
export function PosterStrip({ films, empty }: { films: Movie[]; empty: React.ReactNode }) {
  if (films.length === 0) return <Empty>{empty}</Empty>
  return (
    <div className="flex gap-sm overflow-hidden">
      {films.map((film) => (
        <Thumbnail key={film.id} film={film} />
      ))}
    </div>
  )
}

/** A poster in the watchlist grid, with its title over a gradient. */
export function WatchlistCard({ film }: { film: Movie }) {
  return (
    <Link
      to={`/movie/${film.id}`}
      className="relative group rounded-lg overflow-hidden inner-stroke aspect-[2/3] bg-surface-container"
    >
      <img
        className="w-full h-full object-cover opacity-80 group-hover:opacity-100 transition-opacity"
        alt={film.poster.alt}
        src={film.poster.src}
      />
      <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-black/20 to-transparent flex flex-col justify-end p-md text-white">
        <span className="font-body-md text-body-md font-bold truncate">{film.title}</span>
      </div>
    </Link>
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
