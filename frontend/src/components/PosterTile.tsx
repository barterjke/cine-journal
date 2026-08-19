/**
 * The poster card with a watchlist "+" over it, drawn by the feed and by every
 * collection grid.
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

import type { Movie } from '../api'
import { StarRating } from './StarRating'

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
        <Link to={`/movie/${movie.id}`} aria-label={movie.title}>
          <img
            className="w-full h-full object-cover transition-transform duration-700 group-hover:scale-105"
            alt={movie.poster.alt}
            src={movie.poster.src}
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
