/**
 * Everything to do with drawing a film's poster.
 *
 * `Poster` is the one every screen calls: it draws the image, or the placeholder
 * when a film has no poster. `PosterTile` is the card that wraps it in a watchlist
 * "+", drawn by the feed and by every collection grid.
 *
 * One component rather than one per screen because the button's behaviour is the part
 * that matters and it is fiddly: the scrim has to ignore pointer events so it never eats
 * the poster's own click, and it has to stay up on a film already on the watchlist —
 * otherwise the state is invisible until you hover, which reads as the click not having
 * landed.
 *
 * `rating` is optional and drawn under the title where a collection has ratings behind it
 * (the visitor's journal does, their favourites don't). `null` draws nothing, not zero
 * stars: a film logged without a score is a different thing from a film scored zero.
 *
 * Every class string is spelled out literally — Tailwind's JIT scans the source, so a
 * composed `text-${size}` would compile to nothing.
 */
import { Link } from 'react-router-dom'

import type { Image, Movie } from '../api'
import { StarRating } from './StarRating'

/**
 * The file the API sends for a film it has no poster for.
 *
 * Recognised here so there is only ever one missing-poster treatment on screen.
 * A frame drawn in markup also can't 404, and can't be cached as some other
 * film's artwork.
 */
const API_STAND_IN = 'img/poster-missing.svg'

/** Whether we were given a real poster, rather than none or the API's stand-in. */
export function hasPoster(image: Image | null): image is Image {
  return image !== null && !image.src.endsWith(API_STAND_IN)
}

/**
 * What a film with no poster gets: a plain 2:3 frame, drawn rather than loaded.
 *
 * Never a real film's artwork. A placeholder that looks like somebody's poster
 * credits it to the wrong film, which is worse than showing nothing at all. And
 * missing posters are normal, so this is meant to read as deliberate rather than
 * broken.
 *
 * The frame and its caption are one `<svg>`, so both scale together — the same
 * component works at 64px in a profile strip and full size on a film's page.
 *
 * `className` is the one the poster `<img>` would have worn, so the placeholder
 * lands in the same box with the same corners. Spelled out at each call site,
 * because Tailwind's JIT scans the source.
 */
export function MissingPoster({ className = 'w-full aspect-[2/3]' }: { className?: string }) {
  return (
    <div
      role="img"
      aria-label="No poster available"
      className={`bg-surface-container-high text-outline font-label-sm overflow-hidden ${className}`}
    >
      <svg aria-hidden="true" className="w-full h-full" viewBox="0 0 200 300">
        {/* A film frame, stroked to match the outlined icon set the app uses. */}
        <g
          fill="none"
          stroke="currentColor"
          strokeWidth="4"
          opacity="0.5"
          transform="translate(100 130)"
        >
          <rect x="-34" y="-26" width="68" height="52" rx="4" />
          <path d="M-34 -12h68M-34 12h68M-16 -26v52M16 -26v52" />
        </g>
        <text
          x="100"
          y="194"
          fill="currentColor"
          fontSize="14"
          letterSpacing="1.5"
          textAnchor="middle"
          opacity="0.7"
        >
          NO POSTER
        </text>
      </svg>
    </div>
  )
}

/**
 * A film's poster, or the placeholder when there isn't one.
 *
 * Every screen draws its posters through this, so a film without one looks the
 * same everywhere. It used to be per-screen: the search grid had its own tile, a
 * review card rendered an empty box, and the rest showed whatever the API sent.
 */
export function Poster({ image, className }: { image: Image | null; className: string }) {
  if (!hasPoster(image)) return <MissingPoster className={className} />
  return <img className={className} alt={image.alt} src={image.src} />
}

export function PosterTile({
  movie,
  onWatchlist,
  rating = null,
  onToggleWatchlist,
  busy,
}: {
  movie: Movie
  onWatchlist: boolean
  /** Half-stars, or `null` for a collection with no ratings behind it. */
  rating?: number | null
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="flex flex-col gap-sm group">
      <div className="aspect-[2/3] w-full rounded-lg overflow-hidden inner-stroke soft-shadow relative bg-surface-container-low">
        <Link to={`/movie/${movie.id}`} aria-label={movie.title} className="block w-full h-full">
          <Poster
            image={movie.poster}
            className="w-full h-full object-cover transition-transform duration-700 group-hover:scale-105"
          />
        </Link>
        <div
          className={`absolute inset-0 bg-black/40 transition-opacity duration-300 flex items-center justify-center backdrop-blur-sm pointer-events-none ${
            onWatchlist
              ? 'opacity-100'
              : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'
          }`}
        >
          <button
            onClick={() => onToggleWatchlist(movie.id)}
            disabled={busy}
            aria-pressed={onWatchlist}
            aria-label={
              onWatchlist
                ? `Remove ${movie.title} from watchlist`
                : `Add ${movie.title} to watchlist`
            }
            className={`pointer-events-auto w-12 h-12 rounded-full border flex items-center justify-center transition-colors disabled:cursor-wait ${
              onWatchlist
                ? 'bg-white text-black border-white'
                : 'bg-white/20 border-white/50 text-white hover:bg-white hover:text-black'
            }`}
          >
            <span
              className="material-symbols-outlined"
              style={{ fontVariationSettings: "'FILL' 1" }}
            >
              {onWatchlist ? 'check' : 'add'}
            </span>
          </button>
        </div>
      </div>
      <div className="flex flex-col">
        <div className="flex justify-between items-baseline gap-2">
          <Link to={`/movie/${movie.id}`} className="truncate">
            <h3 className="font-headline-md text-[16px] leading-tight text-on-background font-bold truncate hover:text-primary transition-colors">
              {movie.title}
            </h3>
          </Link>
          <span className="font-label-sm text-label-sm text-on-surface-variant shrink-0">
            {movie.year}
          </span>
        </div>
        {rating !== null && (
          <StarRating halfStars={rating} size="text-[14px]" color="text-primary" className="mt-1" />
        )}
      </div>
    </div>
  )
}
