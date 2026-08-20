/**
 * Profile. Ported from `reference/profile/`.
 *
 * Everything below the header is the visitor's own data rather than the mock's
 * invented copy: the strips and the grid are their real favourites, watchlist and
 * journal, and "Following" is the two friend rails the other screens draw from. So
 * the screen starts out mostly empty and fills in as you use the app, which is the
 * honest version of a mock whose every tile was pre-populated.
 *
 * The mock's `share` button is dropped (nothing to share to). Its four `chevron_right`
 * links opened nothing; each tile is now a link in its entirety — three to a collection
 * page and Following to Friends. "Edit" is real too, and edits the one field the visitor
 * owns: the bio. Their name, handle, avatar and joined line are still the export's, held
 * in `hydrate` as constants, because there is no account system behind them.
 *
 * The layout pieces are shared with `Person` through `ProfileParts` — someone
 * else's page is supposed to be the same page, and two copies of this drifted.
 */
import { useState } from 'react'
import type { FollowedPerson, Profile as ProfileData, RatedFilm } from '../api'
import { api, isUnauthorized } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import {
  ActionError,
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  SignInPrompt,
  SignOutButton,
  TopAppBar,
} from '../components/Chrome'
import { Empty, PosterStrip, ProfileHeader, Tile } from '../components/ProfileParts'
import { StarRating } from '../components/StarRating'

/** How long a bio the server accepts — `routes::MAX_BIO_LEN`. */
const MAX_BIO = 280

/**
 * One journal entry: a rating, something written, or both.
 *
 * The stars are optional now — writing about a film you never scored is allowed,
 * and the row then carries prose where the rating would have been. What you wrote
 * wins over the film's synopsis blurb: a tile headed "Recent Reviews" printing the
 * studio's own copy underneath was the thing that made it not one.
 *
 * The title is text rather than a link to the film: this row lives inside a tile that
 * is itself a link to the journal collection, and an `<a>` inside an `<a>` is invalid
 * HTML the browser un-nests — which broke the tile wherever a title covered it. The
 * collection page is where a row goes to its film.
 */
function ReviewLine({ film }: { film: RatedFilm }) {
  return (
    <div className="flex flex-col gap-xs">
      <div className="flex items-center justify-between gap-sm">
        <span className="font-body-md text-body-md font-bold truncate">{film.title}</span>
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
      {save.error && (
        <ActionError
          message={save.error}
          onDismiss={save.clearError}
          signIn={save.signInRequired}
        />
      )}
    </div>
  )
}

/**
 * The Following tile's 32px face.
 *
 * Not a link, though every face in this app otherwise is: the tile around it goes to
 * Friends, where each of these people has a linked row of their own. `title` keeps the
 * name reachable on hover, which is all the strip was ever saying.
 */
function Avatar({ person }: { person: FollowedPerson }) {
  return (
    <div
      title={person.name}
      className="w-8 h-8 rounded-full overflow-hidden border border-surface-variant shrink-0"
    >
      <img className="w-full h-full object-cover" alt={person.avatar.alt} src={person.avatar.src} />
    </div>
  )
}

/**
 * The overflow pill beside the Following faces: "+121".
 *
 * The mock drew three avatars and a count of 124, and the count is the real number the
 * API sends — so the pill says how many aren't pictured rather than repeating the total.
 * Nothing when the strip already shows everyone.
 */
function MorePill({ count }: { count: number }) {
  if (count <= 0) return null
  return (
    <span className="h-8 px-sm inline-flex items-center rounded-full bg-surface-container font-label-sm text-label-sm text-on-surface-variant">
      +{count}
    </span>
  )
}

/**
 * Header, then one bento grid. Nothing below it.
 *
 * The grid used to be a summary of three full-width sections repeated underneath it, so
 * every film on this page appeared twice — once as a 64px thumbnail and again as a
 * poster, under the same heading. Each tile is now a link to its collection page instead,
 * which is what a summary is for.
 *
 * The two-column shape follows `reference/profile 2/`: favourites wide beside a tall
 * Following cell, then reviews beside the watchlist. `md:col-span-*` rather than four
 * equal cells, because the strips hold four posters and the avatars hold three.
 */
function Body({
  data,
  onBioSaved,
}: {
  data: ProfileData
  onBioSaved: (bio: string) => void
}) {
  return (
    <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-lg">
      <ProfileHeader
        avatar={data.avatar}
        name={data.name}
        meta={`${data.handle} • ${data.member_since}`}
        bio={<BioField bio={data.bio} onSaved={onBioSaved} />}
        // Only below `md`. There is no app bar on a phone, so this is the only way
        // out there. It belongs on your own page anyway.
        action={<SignOutButton className="md:hidden" />}
      />

      <div className="grid grid-cols-1 md:grid-cols-5 gap-md">
        {/* Linked even when empty, unlike before: the collection page carries the same
            "how to fill this" copy plus a way to act on it, so an empty tile that
            couldn't be clicked was the one dead end left on this screen. */}
        <div className="md:col-span-3">
          <Tile label="Favorite Films" to="/collections/favorites">
            <PosterStrip
              films={data.favorites}
              linked={false}
              empty={'Press the heart on any film’s page and it collects here.'}
            />
          </Tile>
        </div>

        <div className="md:col-span-2">
          <Tile label={`Following (${data.following_count})`} to="/people">
            {data.following.length ? (
              <div className="flex items-center gap-sm">
                {data.following.map((person) => (
                  <Avatar key={person.id} person={person} />
                ))}
                <MorePill count={data.following_count - data.following.length} />
              </div>
            ) : (
              <Empty>Nobody yet — find people on Friends and follow them.</Empty>
            )}
          </Tile>
        </div>

        <div className="md:col-span-2">
          <Tile label="Recent Reviews" to="/collections/journal">
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

        <div className="md:col-span-3">
          <Tile label="Watchlist" to="/collections/watchlist">
            <PosterStrip
              films={data.watchlist}
              linked={false}
              empty={'Nothing logged yet — the "+" over any poster adds one.'}
            />
          </Tile>
        </div>
      </div>
    </main>
  )
}

export function Profile() {
  const { data, error, loading, update } = useApi(() => api.profile())

  // This page is the account's own, so a visitor without one gets a 401. That is an
  // answer, not a fault. It asks them in rather than reporting a dead API at somebody
  // who has never signed in.
  const anonymous = isUnauthorized(error)

  return (
    // `pb-24` clears the 64px mobile nav, which is fixed and would otherwise sit
    // over the last row of the following list.
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="profile" />
      <DemoBanner />
      {loading && <Loading />}
      {anonymous && <SignInPrompt heading="Sign in to see your profile." />}
      {error && !anonymous && <ErrorNote error={error} />}
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
