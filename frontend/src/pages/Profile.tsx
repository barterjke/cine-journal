/**
 * Profile. Ported from `reference/profile/`.
 *
 * Everything below the header is the visitor's own data rather than the mock's
 * invented copy: the strips and the grid are their real favourites, watchlist and
 * journal, and "Following" is the two friend rails the other screens draw from. So
 * the screen starts out mostly empty and fills in as you use the app, which is the
 * honest version of a mock whose every tile was pre-populated.
 *
 * The mock's `share` button is dropped (nothing to share to) and so are its four
 * `chevron_right` links, which each opened nothing — the two with a real destination,
 * Watchlist and Following, are headings further down this page, so those strips
 * scroll to them. "Edit" is now real, and edits the one field the visitor owns: the
 * bio. Their name, handle, avatar and joined line are still the export's, held in
 * `hydrate` as constants, because there is no account system behind them.
 *
 * The layout pieces are shared with `Person` through `ProfileParts` — someone
 * else's page is supposed to be the same page, and two copies of this drifted.
 */
import { useState } from 'react'
import type { FollowedPerson, Profile as ProfileData, RatedFilm } from '../api'
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
import { personPath } from '../components/People'
import {
  Empty,
  PosterStrip,
  ProfileHeader,
  SectionHeading,
  Tile,
  WatchlistCard,
} from '../components/ProfileParts'
import { StarRating } from '../components/StarRating'
import { Link } from 'react-router-dom'

/** How long a bio the server accepts — `routes::MAX_BIO_LEN`. */
const MAX_BIO = 280

/**
 * One journal entry: a rating, something written, or both.
 *
 * The stars are optional now — writing about a film you never scored is allowed,
 * and the row then carries prose where the rating would have been. What you wrote
 * wins over the film's synopsis blurb: a tile headed "Recent Reviews" printing the
 * studio's own copy underneath was the thing that made it not one.
 */
function ReviewLine({ film }: { film: RatedFilm }) {
  return (
    <div className="flex flex-col gap-xs">
      <div className="flex items-center justify-between gap-sm">
        <Link
          to={`/movie/${film.id}`}
          className="font-body-md text-body-md font-bold truncate hover:text-primary transition-colors"
        >
          {film.title}
        </Link>
        {/* The mock drew only the filled stars, at 75% and right-aligned. */}
        {film.rating_half_stars !== null ? (
          <StarRating
            halfStars={film.rating_half_stars}
            size="text-sm"
            showEmpty={false}
            className="scale-75 origin-right shrink-0"
          />
        ) : (
          <span className="font-label-sm text-label-sm text-outline uppercase tracking-wider shrink-0">
            Written
          </span>
        )}
      </div>
      {/* `line-clamp-2` for your own words against the blurb's 1: the tile links
          to the film, where the whole thing is, but a review clipped to one line is
          usually clipped mid-clause. */}
      {film.body ? (
        <p className="text-sm text-on-surface line-clamp-2">{film.body}</p>
      ) : (
        film.blurb && <p className="text-sm text-on-surface-variant line-clamp-1">{film.blurb}</p>
      )}
    </div>
  )
}

/**
 * The bio, and the "Edit" the mock drew with nothing behind it.
 *
 * Reads as a paragraph until you click Edit, rather than as a permanently-open
 * field: it's one line of a header, and a textarea sitting in it makes the page look
 * like a form. Saving an empty box restores the export's line — the server decides
 * that, and this renders whatever comes back, so the field can't sit showing
 * something the profile doesn't.
 */
function BioField({ bio, onSaved }: { bio: string; onSaved: (bio: string) => void }) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(bio)

  const save = useAction(async () => {
    const state = await api.setBio(draft)
    onSaved(state.bio)
    setDraft(state.bio)
    setEditing(false)
  })

  if (!editing) {
    return (
      <div className="flex items-start gap-sm">
        <p className="font-body-md text-body-md text-on-surface-variant line-clamp-2 md:line-clamp-1">
          {bio}
        </p>
        <button
          onClick={() => {
            setDraft(bio)
            setEditing(true)
          }}
          className="font-label-sm text-label-sm text-primary uppercase tracking-wider hover:underline shrink-0"
        >
          Edit
        </button>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-sm items-start">
      <textarea
        className="w-full max-w-2xl bg-surface-container-low border border-surface-variant rounded-lg p-sm font-body-md text-body-md text-on-surface focus:ring-1 focus:ring-primary resize-none"
        rows={2}
        maxLength={MAX_BIO}
        autoFocus
        placeholder="Say something about what you watch."
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      <div className="flex items-center gap-md flex-wrap">
        <button
          onClick={() => void save.run()}
          disabled={save.busy}
          className="font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 bg-primary text-on-primary rounded-full hover:opacity-90 transition-opacity disabled:cursor-wait"
        >
          {save.busy ? 'Saving…' : 'Save'}
        </button>
        <button
          onClick={() => setEditing(false)}
          disabled={save.busy}
          className="font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 border border-outline-variant rounded-full text-on-surface hover:bg-surface-container-low transition-colors"
        >
          Cancel
        </button>
        <span className="font-label-sm text-label-sm text-outline">
          {draft.trim().length} / {MAX_BIO} — empty restores the default
        </span>
      </div>
      {save.error && <ActionError message={save.error} onDismiss={save.clearError} />}
    </div>
  )
}

/** The Following tile's 32px face. Linked on the same terms as `FriendRow`. */
function Avatar({ person }: { person: FollowedPerson }) {
  const face = (
    <img className="w-full h-full object-cover" alt={person.avatar.alt} src={person.avatar.src} />
  )
  const shape = 'w-8 h-8 rounded-full overflow-hidden border border-surface-variant'

  return person.handle ? (
    <Link
      to={personPath(person.handle)}
      title={person.name}
      className={`${shape} hover:opacity-80 transition-opacity`}
    >
      {face}
    </Link>
  ) : (
    <div title={person.name} className={shape}>
      {face}
    </div>
  )
}

/**
 * One "Following" row. A link to their page when they're one of the app's own
 * users; a plain row for the export's decorative cast, who have no `handle` and so
 * no page — a link there would go to a 404 that reads as a bug rather than as "this
 * person was always scenery".
 */
function FriendRow({ person }: { person: FollowedPerson }) {
  const body = (
    <>
      <div className="w-12 h-12 rounded-full overflow-hidden bg-surface-container border border-surface-variant shrink-0">
        <img className="w-full h-full object-cover" alt={person.avatar.alt} src={person.avatar.src} />
      </div>
      <div className="flex flex-col flex-grow min-w-0">
        <span className="font-body-md text-body-md text-on-background">{person.name}</span>
        <span className="font-label-sm text-label-sm text-outline truncate">
          {person.subtitle}
        </span>
      </div>
    </>
  )

  return person.handle ? (
    <Link
      to={personPath(person.handle)}
      className="flex items-center gap-md group hover:opacity-90 transition-opacity"
    >
      {body}
    </Link>
  ) : (
    <div className="flex items-center gap-md">{body}</div>
  )
}

function Body({
  data,
  onBioSaved,
}: {
  data: ProfileData
  onBioSaved: (bio: string) => void
}) {
  return (
    <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-md">
      <ProfileHeader
        avatar={data.avatar}
        name={data.name}
        meta={`${data.handle} • ${data.member_since}`}
        bio={<BioField bio={data.bio} onSaved={onBioSaved} />}
      />

      <div className="grid grid-cols-1 md:grid-cols-2 gap-md">
        <Tile label="Favorite Films" to={data.favorites.length ? '#favorites' : undefined}>
          <PosterStrip
            films={data.favorites}
            empty={'Press the heart on any film’s page and it collects here.'}
          />
        </Tile>

        <Tile label="Watchlist" to={data.watchlist.length ? '#watchlist' : undefined}>
          <PosterStrip
            films={data.watchlist}
            empty={'Nothing logged yet — the "+" over any poster adds one.'}
          />
        </Tile>

        <Tile label={`Following (${data.following_count})`} to="#following">
          <div className="flex gap-sm">
            {data.following.map((person) => (
              <Avatar key={person.id} person={person} />
            ))}
          </div>
        </Tile>

        <Tile label="Recent Reviews">
          {data.recent_reviews.length ? (
            <div className="flex flex-col gap-sm">
              {data.recent_reviews.map((film) => (
                <ReviewLine key={film.id} film={film} />
              ))}
            </div>
          ) : (
            <Empty>Rate or review a film and it shows up here, newest first.</Empty>
          )}
        </Tile>
      </div>

      {/* The favourites in full, when there are any. No empty state: the tile above
          already says how to fill it, and an empty section with its own heading
          says it twice. */}
      {data.favorites.length > 0 && (
        <section className="flex flex-col gap-md" id="favorites">
          <SectionHeading title="Favorite Films" count={data.favorites.length} />
          <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-6 gap-md">
            {data.favorites.map((film) => (
              <WatchlistCard key={film.id} film={film} />
            ))}
          </div>
        </section>
      )}

      <section className="grid grid-cols-1 md:grid-cols-12 gap-gutter">
        <div className="md:col-span-8 flex flex-col gap-md" id="watchlist">
          <SectionHeading title="Watchlist" count={data.watchlist.length} />
          {data.watchlist.length ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-md">
              {data.watchlist.map((film) => (
                <WatchlistCard key={film.id} film={film} />
              ))}
            </div>
          ) : (
            <div className="bg-surface-container-low rounded-xl p-lg border border-surface-variant flex flex-col gap-sm items-start">
              <Empty>
                Your watchlist is empty. Add a film from the feed, the search grid, or its own
                page.
              </Empty>
              <Link
                to="/search"
                className="font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 bg-primary text-on-primary rounded-full hover:opacity-90 transition-opacity"
              >
                Find something
              </Link>
            </div>
          )}
        </div>

        <div className="md:col-span-4 flex flex-col gap-md" id="following">
          <SectionHeading title="Following" count={data.following_count} />
          <div className="bg-surface-container-low rounded-xl p-lg border border-surface-variant flex flex-col gap-md">
            {data.following.map((person, index) => (
              <div key={person.id} className="flex flex-col gap-md">
                {index > 0 && <hr className="border-t border-surface-variant w-full" />}
                <FriendRow person={person} />
              </div>
            ))}
          </div>
        </div>
      </section>
    </main>
  )
}

export function Profile() {
  const { data, error, loading, update } = useApi(() => api.profile())

  return (
    // `pb-24` clears the 64px mobile nav, which is fixed and would otherwise sit
    // over the last row of the following list.
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="profile" />
      <DemoBanner />
      {loading && <Loading />}
      {error && <ErrorNote error={error} />}
      {data && (
        <Body
          data={data}
          onBioSaved={(bio) => {
            update((current) => ({ ...current, bio }))
          }}
        />
      )}
      <BottomNavBar active="profile" />
    </div>
  )
}
