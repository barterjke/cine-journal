/**
 * The pieces every screen that shows a person needs: the follow button, a
 * directory row, and a review card.
 *
 * Shared because four screens draw them — the directory, one person's page, the
 * profile's Following list and a film's reviews — and a follow button that looked
 * or behaved differently on one of them would read as a different button.
 *
 * The people here are the app's own users, seeded once at startup from TMDB's real
 * reviewers (see `content::harvest_graph`). They are not the export's decorative
 * cast on the stories and activity rails, who have no page and no follow button —
 * `FollowedPerson.handle` is `null` for those.
 */
import { Link } from 'react-router-dom'

import type { PersonCard, UserReview } from '../api'
import { api } from '../api'
import { useAction } from '../useAction'
import { Poster } from './PosterTile'
import { StarRating } from './StarRating'

/** Where a person's page lives. The `@` is dropped: it's punctuation, not an id. */
export function personPath(handle: string): string {
  return `/people/${handle.replace(/^@/, '')}`
}

/**
 * Where one review lives, given its id.
 *
 * A function like `personPath` rather than the path spelled out at each call site.
 * Three screens link here now — the feed's cards, a person's page, and your own
 * journal rows — and the route is what they have to agree about.
 */
export function reviewPath(id: string): string {
  return `/review/${id}`
}

/**
 * What to call the author of a comment or a reply.
 *
 * The server always sends the real name, so "You" is derived here. Shared by both
 * review screens so they can't label the same row differently.
 */
export function authorLabel(author: { author_name: string; is_you: boolean }): string {
  return author.is_you ? 'You' : author.author_name
}

/**
 * The score in words: "Rated 4.5 / 5". `null` when there is no score.
 *
 * This is interface text, not prose. It fills the space a rating with no words
 * would otherwise leave empty. "4.5 / 5" is the form the mobile review screen
 * already prints, so a score reads the same way everywhere.
 */
export function ratingLine(halfStars: number | null): string | null {
  return halfStars === null ? null : `Rated ${halfStars / 2} / 5`
}

/**
 * What a review page shows when nothing was written.
 *
 * A rating on its own is a post. So the score is the content here. Nothing is
 * invented: the server sends no prose, and neither do we.
 *
 * Both review screens draw the like button and the composer under this, which is
 * why the second line points at them.
 */
export function RatingOnlyNote({ halfStars }: { halfStars: number | null }) {
  const line = ratingLine(halfStars)

  return (
    <div className="flex flex-col gap-xs items-start w-full border border-dashed border-surface-variant rounded-lg p-lg">
      {line !== null && (
        <p className="font-headline-md text-headline-md text-on-surface">{line}</p>
      )}
      <p className="font-label-sm text-label-sm text-outline">
        Nothing written. Like it or reply below.
      </p>
    </div>
  )
}

/**
 * Follow / Following, as a toggle.
 *
 * Optimistic, like the watchlist button: the state flips immediately and reverts
 * if the write fails, because a follow that waits for a round trip feels broken.
 * The request states its target rather than saying "flip it", so a double-click
 * can't leave the button and the database on opposite answers.
 *
 * The two states are deliberately different weights — filled for "Follow" (an
 * action offered), outlined for "Following" (a state you can undo) — so which one
 * you are looking at is legible without reading it.
 */
export function FollowButton({
  personId,
  following,
  onChange,
  size = 'md',
}: {
  personId: string
  following: boolean
  /** Called with the new state so the owning screen can patch its own copy. */
  onChange: (following: boolean) => void
  size?: 'sm' | 'md'
}) {
  const follow = useAction(async () => {
    const target = !following
    onChange(target)
    try {
      const state = await api.setFollow(personId, target)
      onChange(state.following)
    } catch (cause) {
      onChange(!target)
      throw cause
    }
  })

  const padding = size === 'sm' ? 'px-3 py-1' : 'px-4 py-2'
  const shell = `font-label-sm text-label-sm uppercase tracking-wider rounded-full shrink-0 transition-opacity hover:opacity-80 active:opacity-60 disabled:opacity-50 ${padding}`

  // A failed write has already reverted the button, so the label is truthful
  // again — but a button that silently springs back looks like a bug rather than
  // an error. The button becomes its own error surface, since it is the only thing
  // on screen the failure is about.
  const label = follow.error ? 'Retry' : following ? 'Following' : 'Follow'

  return (
    <button
      onClick={() => void follow.run()}
      disabled={follow.busy}
      // The label reads as a state; the title says what clicking does, or why the
      // last click didn't.
      title={follow.error ?? (following ? 'Unfollow' : 'Follow')}
      className={
        follow.error
          ? `${shell} border border-secondary text-secondary`
          : following
            ? `${shell} border border-outline text-on-surface-variant`
            : `${shell} bg-primary text-on-primary`
      }
    >
      {label}
    </button>
  )
}

/** "Follows you" — the answer to whether a follow is mutual. */
export function FollowsYouBadge() {
  return (
    <span className="font-label-sm text-label-sm text-outline border border-surface-variant rounded-full px-2 py-0.5 shrink-0">
      Follows you
    </span>
  )
}

/** One row of a people list: avatar, name, nickname, badge, follow button. */
export function PersonRow({
  person,
  onFollowChange,
}: {
  person: PersonCard
  onFollowChange: (following: boolean) => void
}) {
  return (
    <div className="flex items-center gap-md">
      <Link
        to={personPath(person.handle)}
        className="w-12 h-12 rounded-full overflow-hidden bg-surface-container border border-surface-variant shrink-0 hover:opacity-80 transition-opacity"
      >
        <img
          className="w-full h-full object-cover"
          alt={person.avatar.alt}
          src={person.avatar.src}
        />
      </Link>
      <div className="flex flex-col flex-grow min-w-0">
        <div className="flex items-center gap-sm min-w-0">
          <Link
            to={personPath(person.handle)}
            className="font-body-md text-body-md text-on-background truncate hover:text-primary transition-colors"
          >
            {person.name}
          </Link>
          {person.follows_you && <FollowsYouBadge />}
        </div>
        <span className="font-label-sm text-label-sm text-outline truncate">
          {person.handle}
          {person.review_count > 0 &&
            ` • ${person.review_count} ${person.review_count === 1 ? 'review' : 'reviews'}`}
        </span>
      </div>
      <FollowButton
        personId={person.id}
        following={person.following}
        onChange={onFollowChange}
        size="sm"
      />
    </div>
  )
}

/**
 * One review, as a film's page draws it: who wrote it, their rating, their prose.
 *
 * `showFilm` swaps which end is the subject. On a film's page every review is of
 * the same film, so the author leads; on a person's page they wrote all of them, so
 * the film leads. One component either way, because the two would otherwise drift.
 *
 * A rating with nothing written gets a card too. One quiet line states the score
 * where the prose would be, so the gap reads as deliberate. The link stays: that
 * page is where the likes and the replies are. It stops saying "Read full review",
 * which would promise text nobody wrote.
 */
export function ReviewCard({ review, showFilm = false }: { review: UserReview; showFilm?: boolean }) {
  const score = ratingLine(review.rating_half_stars)
  // "Rated 4.5 / 5 · nothing written". A score is always there in practice; the
  // shorter line is for a row that somehow has neither.
  const nothingWritten = score === null ? 'Nothing written' : `${score} · nothing written`

  return (
    <article className="flex gap-md">
      {showFilm ? (
        <Link
          to={`/movie/${review.movie_id}`}
          title={review.movie_title}
          className="w-16 aspect-[2/3] rounded bg-surface-container overflow-hidden shrink-0 inner-stroke hover:opacity-80 transition-opacity"
        >
          {/* Was `{review.poster && …}`, which left an empty box on a film with no
              poster — a different answer from every other screen's. */}
          <Poster image={review.poster} className="w-full h-full object-cover" />
        </Link>
      ) : (
        <Link
          to={personPath(review.author_handle)}
          className="w-10 h-10 rounded-full overflow-hidden bg-surface-container shrink-0 border border-surface-variant hover:opacity-80 transition-opacity"
        >
          <img
            className="w-full h-full object-cover"
            alt={review.author_avatar.alt}
            src={review.author_avatar.src}
          />
        </Link>
      )}

      <div className="flex flex-col gap-xs min-w-0 flex-grow">
        <div className="flex items-baseline gap-sm flex-wrap">
          {showFilm ? (
            <Link
              to={`/movie/${review.movie_id}`}
              className="font-body-md text-body-md font-bold hover:text-primary transition-colors"
            >
              {review.movie_title}
            </Link>
          ) : (
            <Link
              to={personPath(review.author_handle)}
              className="font-body-md text-body-md font-bold hover:text-primary transition-colors"
            >
              {review.author_name}
            </Link>
          )}
          {/* No score, no stars. A film written about but never rated is not a film
              rated zero. */}
          {review.rating_half_stars !== null && (
            <StarRating
              halfStars={review.rating_half_stars}
              size="text-sm"
              showEmpty={false}
              className="shrink-0"
            />
          )}
          {/* Why this review sorted where it did, on the screen that sorts by it. */}
          {!showFilm && review.author_followed && (
            <span className="font-label-sm text-label-sm text-primary">Following</span>
          )}
          {/* An old row has no stored date, and the API sends "" for it. No blank
              line for that. */}
          {review.written_on !== '' && (
            <span className="font-label-sm text-label-sm text-outline">{review.written_on}</span>
          )}
        </div>
        {review.body === null ? (
          // Label type, not body type. This is the interface saying there is nothing
          // to read, so it must not look like the author's own words.
          <p className="font-label-sm text-label-sm text-outline">{nothingWritten}</p>
        ) : (
          /* Clamped: several of the real reviews run to a thousand words, and the
             rail is a sidebar. The link below opens the whole thing. */
          <p className="font-body-md text-body-md text-on-surface-variant line-clamp-4 whitespace-pre-line">
            {review.body}
          </p>
        )}
        {/* Always offered, even for a review that fits: the page it opens isn't
            only the rest of the text, it's where the likes and the conversation
            are. Without it a clamped review was a dead end.
            A bare score has no text to read, so the label names what the page
            actually offers instead. */}
        <Link
          to={reviewPath(review.id)}
          className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity self-start"
        >
          {review.body === null ? 'Like or reply →' : 'Read full review →'}
        </Link>
      </div>
    </article>
  )
}
