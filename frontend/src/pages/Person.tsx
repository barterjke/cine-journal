/**
 * One person's page: who they are, whether the follow is mutual, what they love,
 * what they mean to watch, and everything they've reviewed.
 *
 * Built from the same `ProfileParts` your own profile is, and deliberately in the
 * same order — header, two poster rows, then the long lists. Someone else's page
 * should be the same kind of page as yours, and it used to be reviews and nothing
 * else, which made a friend's page look like a comment history.
 *
 * What differs is only what each page genuinely knows. Theirs has a follow button
 * where your own has the bio's "Edit"; yours has a Following list, because the graph
 * stores only your own edges; theirs has their prose, because they wrote some and
 * the visitor's own reviews live on their journal tile instead.
 *
 * These are the app's own users, seeded once from TMDB's real reviewers. Their
 * reviews are stored rows against real film ids — unlike the export's activity
 * rails, which carry no film and are paired with whatever is trending. So a review
 * here still says the same thing next week. Their favourites are derived from those
 * reviews and their watchlists from what they haven't written about, since TMDB
 * publishes neither (see `db::derive_taste`).
 */
import type { PersonProfile } from '../api'
import { api, isNotFound } from '../api'
import { useApi } from '../useApi'
import {
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'
import { FollowButton, FollowsYouBadge, ReviewCard } from '../components/People'
import { PosterRow, ProfileHeader, SectionHeading, Tile } from '../components/ProfileParts'
import { collectionPath } from './Collection'
import { Link, useParams } from 'react-router-dom'

function Body({
  data,
  onFollowChange,
}: {
  data: PersonProfile
  onFollowChange: (following: boolean) => void
}) {
  const first = data.name.split(' ')[0]

  return (
    <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-md">
      <ProfileHeader
        avatar={data.avatar}
        name={data.name}
        meta={data.handle}
        badge={data.follows_you && <FollowsYouBadge />}
        bio={
          data.bio && (
            <p className="font-body-md text-body-md text-on-surface-variant">{data.bio}</p>
          )
        }
        /* No follower/following counts: the graph only stores the visitor's own
           edges, so any such number would be 0 or 1. The badge and the button say
           everything that is actually known about the relationship. */
        action={
          <FollowButton personId={data.id} following={data.following} onChange={onFollowChange} />
        }
      />

      {/* The same two cards your own profile opens with, linking to the same collection
          page with their nickname on it. They used to scroll to full copies of
          themselves further down, so every film on this page appeared twice.

          `items-start` so a card with one poster isn't drawn as tall as the card
          beside it — see the note in `Profile`. */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-md items-start">
        <Tile label="Favorite Films" to={collectionPath('favorites', data.handle)} viewAll>
          <PosterRow
            films={data.favorites}
            captioned
            empty={`${first} hasn’t raved about anything yet.`}
          />
        </Tile>

        <Tile label="Watchlist" to={collectionPath('watchlist', data.handle)} viewAll>
          <PosterRow films={data.watchlist} empty={`${first}’s watchlist is empty.`} />
        </Tile>
      </div>

      {/* Their reviews stay on this page rather than moving to a collection: prose is
          what they wrote, and this is where you read it. */}
      <section className="flex flex-col gap-md">
        {/* `review_count` rather than `reviews.length`: the count is the true total,
            so it stays honest if the list is ever clamped. */}
        <SectionHeading title="Reviews" count={data.review_count} />

        {data.reviews.length ? (
          <div className="bg-surface-container-low rounded-xl p-lg border border-surface-variant flex flex-col gap-lg">
            {data.reviews.map((review, index) => (
              <div key={review.id} className="flex flex-col gap-lg">
                {index > 0 && <hr className="border-t border-surface-variant w-full" />}
                {/* The film leads here: they wrote all of these, so the author is
                    not the distinguishing part. */}
                <ReviewCard review={review} showFilm />
              </div>
            ))}
          </div>
        ) : (
          <div className="bg-surface-container-low rounded-xl p-lg border border-surface-variant">
            <p className="font-body-md text-body-md text-on-surface-variant">
              {data.name} hasn't reviewed anything yet.
            </p>
          </div>
        )}
      </section>
    </main>
  )
}

/** A nickname nobody has. Distinct from a dead API, which `ErrorNote` reports. */
function NotFound({ handle }: { handle: string }) {
  return (
    <div className="flex flex-col items-center gap-sm py-xxl text-center">
      <span className="material-symbols-outlined text-outline">person_off</span>
      <p className="font-body-md text-body-md text-on-background">No such person.</p>
      <p className="font-label-sm text-label-sm text-on-surface-variant">
        Nobody here goes by @{handle.replace(/^@/, '')}.
      </p>
      <Link
        to="/people"
        className="font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 bg-primary text-on-primary rounded-full hover:opacity-90 transition-opacity mt-sm"
      >
        Search for someone
      </Link>
    </div>
  )
}

export function Person() {
  const { handle = '' } = useParams()
  const { data, error, loading, update, reload } = useApi(() => api.person(handle), [handle])

  // A 404 is a real answer — this nickname doesn't exist — and deserves the empty
  // state rather than "couldn't reach the API", which would send you to restart a
  // server that is running fine. Read off `ApiError.status`, not the message: the
  // message is the backend's own prose ("no person with nickname 'x'") and never
  // contains the code.
  const missing = isNotFound(error)

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="friends" />
      <DemoBanner />
      {loading && <Loading />}
      {missing && <NotFound handle={handle} />}
      {error && !missing && <ErrorNote error={error} onRetry={reload} />}
      {data && (
        <Body
          data={data}
          onFollowChange={(following) => {
            update((current) => ({ ...current, following }))
          }}
        />
      )}
      <BottomNavBar active="friends" />
    </div>
  )
}
