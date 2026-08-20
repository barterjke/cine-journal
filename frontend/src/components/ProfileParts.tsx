/**
 * The pieces a profile is built from, shared by your own page (`Profile`) and
 * everyone else's (`Person`).
 *
 * They live here rather than in either screen because the two are supposed to be
 * the same kind of page — same header, same cards, same poster rows — and two
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

/**
 * The shell every block on a profile sits in.
 *
 * White on the near-white page, with one hairline border and no fill of its own —
 * the border is what separates a card from the background here.
 *
 * Nothing sets a height. A card is as tall as what is in it, which is the whole
 * reason the grid can hold a full profile and an empty one without either looking
 * like a mistake. See the `items-start` note in `Profile`.
 */
const CARD =
  'bg-surface-container-lowest rounded-xl border border-surface-variant p-md md:p-lg flex flex-col gap-md'

export function Card({
  className = '',
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return <section className={`${CARD} ${className}`.trim()}>{children}</section>
}

/** A card's heading: sentence case and bold, not the mono label it used to be. */
const CARD_TITLE = 'font-headline-md text-body-md font-bold text-on-background'

/** The small blue link out of a card, top right. */
const VIEW_ALL =
  'font-label-sm text-label-sm text-primary uppercase tracking-wider hover:underline shrink-0'

/**
 * A card with a heading, and its content below.
 *
 * `to` is where the card's full list lives — a collection page. It makes the
 * *heading* a link, and with `viewAll` also draws "View all" opposite it. The whole
 * card used to be one `<a>`, which meant nothing inside it could be a link either:
 * an `<a>` inside an `<a>` is invalid HTML the browser un-nests, so a poster on top
 * of the card broke the card's own link. Now the card is a plain container and the
 * posters and rows inside it link to their films.
 *
 * `count` is the alternative right-hand slot: a total, small and grey. Following
 * uses it, because there is no collection page for the people you follow.
 */
export function Tile({
  label,
  to,
  viewAll = false,
  count,
  children,
}: {
  label: string
  to?: string
  /** Draw the "View all" link as well as linking the heading. */
  viewAll?: boolean
  /** The total, drawn opposite the heading. */
  count?: number
  children: React.ReactNode
}) {
  return (
    <Card>
      <div className="flex items-baseline justify-between gap-sm">
        <h2 className={CARD_TITLE}>
          {to ? (
            <Link to={to} className="hover:text-primary transition-colors">
              {label}
            </Link>
          ) : (
            label
          )}
        </h2>
        {to && viewAll && (
          <Link to={to} className={VIEW_ALL}>
            View all
          </Link>
        )}
        {count !== undefined && (
          <span className="font-label-sm text-label-sm text-outline shrink-0">{count}</span>
        )}
      </div>
      {children}
    </Card>
  )
}

/** What a card shows before the thing that fills it has happened. */
export function Empty({ children }: { children: React.ReactNode }) {
  return <p className="font-body-md text-body-md text-on-surface-variant">{children}</p>
}

/** The frame a poster sits in. The same one either size, so the two rows match. */
const POSTER_FRAME =
  'block aspect-[2/3] rounded-lg overflow-hidden bg-surface-container inner-stroke'

/**
 * A row of posters that wraps onto further rows, or the empty copy in its place.
 *
 * The widths are fixed and the row is left-aligned. A card with two films keeps its
 * full column width and leaves the space to the right of them empty, rather than
 * growing two posters to fill it — the row is a summary, and its scale should not
 * depend on how many films happen to be in it. Wrapping is what makes it responsive:
 * four across in a wide card, fewer on a phone, never overflowing.
 *
 * `captioned` is the favourites row: smaller posters with the film's title under
 * each. The watchlist grid is larger and bare, which is the only difference.
 */
export function PosterRow({
  films,
  empty,
  captioned = false,
}: {
  films: Movie[]
  empty: React.ReactNode
  captioned?: boolean
}) {
  if (films.length === 0) return <Empty>{empty}</Empty>

  return (
    <div className="flex flex-wrap gap-md">
      {films.map((film) => (
        <Link
          key={film.id}
          to={`/movie/${film.id}`}
          title={film.title}
          className={
            captioned
              ? 'group w-[90px] shrink-0 flex flex-col gap-xs'
              : 'group w-[105px] shrink-0'
          }
        >
          <span className={`${POSTER_FRAME} group-hover:opacity-80 transition-opacity`}>
            <Poster image={film.poster} className="w-full h-full object-cover" />
          </span>
          {captioned && (
            <span className="font-body-md text-sm text-on-surface leading-snug line-clamp-2 group-hover:text-primary transition-colors">
              {film.title}
            </span>
          )}
        </Link>
      ))}
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
 * The card both pages open with: a 52px face, the name with the handle beside it,
 * one line of bio, and the page's own controls on the right.
 *
 * `bio` and `action` are slots rather than strings, because that is the whole
 * difference between the two screens at the top of the page — your own bio is an
 * editable field and the action is Edit and Share; theirs is a paragraph and the
 * action is the follow button.
 *
 * `flex-wrap` with a `basis-48` text column is what keeps this readable on a phone:
 * when the name, the handle and the buttons can't share a line, the buttons take the
 * next one instead of being squeezed to nothing.
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
    <Card>
      <div className="flex flex-wrap items-start gap-md">
        <div className="w-[52px] h-[52px] shrink-0 rounded-full overflow-hidden border border-surface-variant bg-surface-container">
          <img className="w-full h-full object-cover" alt={avatar.alt} src={avatar.src} />
        </div>
        <div className="flex flex-col gap-xs flex-grow basis-48 min-w-0">
          {/* Name and handle share a baseline, so the handle reads as part of the
              name rather than as a line of its own. */}
          <div className="flex items-baseline gap-sm flex-wrap">
            <h1 className="font-headline-md text-headline-md font-bold text-on-background">
              {name}
            </h1>
            <p className="font-label-sm text-label-sm text-outline uppercase tracking-wider">
              {meta}
            </p>
            {badge}
          </div>
          {bio}
        </div>
        {action && (
          <div className="flex items-center gap-sm shrink-0 ml-auto flex-wrap justify-end">
            {action}
          </div>
        )}
      </div>
    </Card>
  )
}
