/**
 * Profile: one header card, then four cards in two rows.
 *
 * Everything below the header is the visitor's own data rather than a mock's
 * invented copy: the poster rows and the review list are their real favourites,
 * watchlist and journal, and "Following" is the people they actually follow. So the
 * screen starts out mostly empty and fills in as you use the app, which is the
 * honest version of a mock whose every tile was pre-populated.
 *
 * "Edit" and "Share" are both real. Edit opens the one field the visitor owns — the
 * bio. Share copies this page's address, which is the only sharing this app can do
 * without taking on a dependency. Their name, handle, avatar and joined line are the
 * account's, and there is nothing here to edit them with.
 *
 * The layout pieces are shared with `Person` through `ProfileParts` — someone
 * else's page is supposed to be the same page, and two copies of this drifted.
 */
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'

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
import { reviewPath } from '../components/People'
import { Empty, PosterRow, ProfileHeader, Tile } from '../components/ProfileParts'
import { Poster } from '../components/PosterTile'
import { StarRating } from '../components/StarRating'

/** How long a bio the server accepts — `routes::MAX_BIO_LEN`. */
const MAX_BIO = 280

/** The header's two pills: "Edit" is the filled one, "Share" the outlined one. */
const EDIT_PILL =
  'font-body-md text-sm font-bold px-4 py-2 rounded-full bg-primary text-on-primary hover:opacity-90 transition-opacity'

const SHARE_PILL =
  'font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 rounded-full border border-outline-variant text-on-surface-variant hover:bg-surface-container-low transition-colors'

/**
 * The grey line under a review: when it was written, and how many people liked it.
 *
 * Both are optional and both are simply absent when the API has neither. "0" beside a
 * heart is a way of saying nobody has pressed it, which is not worth the space, and
 * a date is not something to invent for a rating stored before dates were.
 */
function ReviewMeta({ film }: { film: RatedFilm }) {
  // An empty string is a row with no date, same as null. Neither prints a blank line.
  const date = film.written_on === null || film.written_on === '' ? null : film.written_on
  if (date === null && film.like_count === null) return null

  return (
    <div className="flex items-center gap-md font-label-sm text-label-sm text-outline">
      {date !== null && <span>{date}</span>}
      {film.like_count !== null && (
        <span className="inline-flex items-center gap-xs">
          <span className="material-symbols-outlined text-[14px]" aria-hidden="true">
            favorite
          </span>
          <span>{film.like_count}</span>
          {/* The heart is decorative, so the number needs a word of its own for
              anyone who can't see it. */}
          <span className="sr-only">likes</span>
        </span>
      )}
    </div>
  )
}

/**
 * One journal entry: a rating, something written, or both.
 *
 * The stars are optional — writing about a film you never scored is allowed, and no
 * score draws no stars rather than a zero-star row. What you wrote wins over the
 * film's synopsis blurb: a card headed "Recent Reviews" printing the studio's own
 * copy underneath was the thing that made it not one.
 *
 * The row opens the review, not the film. Both used to lead to `/movie/{id}`, which
 * meant your own entries were the one thing on the site you could not open: no full
 * text, no likes, no replies. Every row goes there, a bare score included — a rating
 * is a review with no words in it, and it can be liked and replied to like any other.
 * The film is only the fallback for a server that sends no id at all.
 *
 * The poster and the title are two links rather than one anchor around the whole row.
 * `Person` wrapped a row that way and the posters inside it stopped being links at
 * all: an `<a>` inside an `<a>` is invalid HTML and the browser un-nests it. The stars
 * and the meta line stay outside both — a date is not somewhere to go.
 *
 * The thumbnail goes through the shared `Poster`, so a film whose artwork the API
 * couldn't resolve gets the same placeholder it gets everywhere else instead of an
 * `<img>` with nothing behind it.
 */
function ReviewRow({ film }: { film: RatedFilm }) {
  const excerpt = film.body ?? film.blurb
  // The film only for a row that arrived without an id. Defensive, not a mode.
  const to = film.review_id === null ? `/movie/${film.id}` : reviewPath(film.review_id)

  return (
    <div className="flex items-start gap-md">
      <Link
        to={to}
        title={film.title}
        className="block w-16 shrink-0 aspect-[2/3] rounded-lg overflow-hidden bg-surface-container inner-stroke hover:opacity-80 transition-opacity"
      >
        <Poster image={film.poster} className="w-full h-full object-cover" />
      </Link>
      <div className="flex flex-col gap-xs min-w-0 flex-grow">
        <div className="flex items-baseline justify-between gap-sm">
          <Link
            to={to}
            className="font-body-md text-body-md font-bold text-on-background truncate hover:text-primary transition-colors"
          >
            {film.title}
          </Link>
          {film.rating_half_stars !== null && (
            <StarRating
              halfStars={film.rating_half_stars}
              size="text-sm"
              showEmpty={false}
              className="shrink-0"
            />
          )}
        </div>
        {/* `line-clamp-2` for your own words: the row links to the review, where the
            whole thing is, but a review clipped to one line is usually clipped
            mid-clause. */}
        {excerpt !== null && (
          <p className="font-body-md text-sm text-on-surface-variant line-clamp-2">{excerpt}</p>
        )}
        <ReviewMeta film={film} />
      </div>
    </div>
  )
}

/**
 * "Sarah Jennings" → "Sarah J.".
 *
 * One word stays as it is. "Prince ." would be a stray initial for a name that
 * hasn't got one, and a chip is too small to be worth that.
 */
function shortName(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length < 2) return parts[0] ?? ''
  return `${parts[0]} ${parts[parts.length - 1][0]}.`
}

/**
 * One person in the Following card: their face and their short name, on a pill.
 *
 * A link where the API gave us a handle. Some followed people were seeded before
 * every row here was an account and have no page to go to; those draw as a plain
 * chip rather than as a link that 404s.
 */
function FollowChip({ person }: { person: FollowedPerson }) {
  const chip =
    'inline-flex items-center gap-sm pl-1 pr-sm py-1 rounded-full bg-surface-container max-w-full'

  const body = (
    <>
      <img
        className="w-6 h-6 rounded-full object-cover shrink-0"
        alt={person.avatar.alt}
        src={person.avatar.src}
      />
      <span className="font-body-md text-sm text-on-surface truncate">
        {shortName(person.name)}
      </span>
    </>
  )

  if (person.handle === null) {
    return (
      <span className={chip} title={person.name}>
        {body}
      </span>
    )
  }

  return (
    <Link
      to={`/people/${person.handle.replace(/^@/, '')}`}
      title={person.name}
      className={`${chip} hover:bg-surface-container-high transition-colors`}
    >
      {body}
    </Link>
  )
}

/**
 * The bio, and the editor "Edit" opens.
 *
 * Reads as one truncated line until you press Edit, rather than as a permanently-open
 * field: it's one line of a header, and a textarea sitting in it makes the page look
 * like a form. Saving an empty box restores the account's default line — the server
 * decides that, and this renders whatever comes back, so the field can't sit showing
 * something the profile doesn't.
 */
function BioField({
  bio,
  editing,
  onDone,
  onSaved,
}: {
  bio: string
  editing: boolean
  onDone: () => void
  onSaved: (bio: string) => void
}) {
  const [draft, setDraft] = useState(bio)

  const save = useAction(async () => {
    const state = await api.setBio(draft)
    onSaved(state.bio)
    setDraft(state.bio)
    onDone()
  })

  if (!editing) {
    // One line, clipped. The bio is a caption under a name here, not a paragraph.
    return (
      <p
        title={bio}
        className="font-body-md text-body-md text-on-surface-variant truncate"
      >
        {bio}
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-sm items-start">
      <textarea
        className="w-full max-w-2xl bg-surface-container-low border border-surface-variant rounded-lg p-sm font-body-md text-body-md text-on-surface focus:ring-1 focus:ring-primary resize-none"
        rows={2}
        maxLength={MAX_BIO}
        autoFocus
        aria-label="Your bio"
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
          onClick={() => {
            setDraft(bio)
            onDone()
          }}
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
 * "Share", which copies the address of the visitor's public page.
 *
 * Not this page's own URL: `/profile` is whoever is signed in, so pasting it to a
 * friend shows them *their* profile, or a sign-in prompt. An account lives in the
 * same table everybody else does, so `/people/{handle}` is the page that says the
 * same thing to any reader — see `Person`.
 *
 * The clipboard rather than a share sheet: `navigator.share` is absent on every
 * desktop browser, and a share *library* is a dependency for one button. The label
 * reports what happened, because a button that looks inert is worse than one that
 * says it failed — an insecure origin or a browser without the API both land there.
 */
function ShareButton({ handle }: { handle: string }) {
  const [result, setResult] = useState<'idle' | 'copied' | 'failed'>('idle')

  // The label goes back to "Share" on its own, so it can be pressed again.
  useEffect(() => {
    if (result === 'idle') return
    const timer = window.setTimeout(() => setResult('idle'), 2000)
    return () => window.clearTimeout(timer)
  }, [result])

  const url = `${window.location.origin}/people/${handle.replace(/^@/, '')}`

  const copy = async () => {
    try {
      // Optional chaining would swallow a missing clipboard silently; this throws
      // into the catch instead, where it is reported like any other failure.
      await navigator.clipboard.writeText(url)
      setResult('copied')
    } catch {
      setResult('failed')
    }
  }

  return (
    <button
      onClick={() => void copy()}
      title={result === 'failed' ? "Your browser wouldn't let us copy it." : url}
      className={SHARE_PILL}
    >
      {result === 'copied' ? 'Copied' : result === 'failed' ? "Couldn't copy" : 'Share'}
    </button>
  )
}

/**
 * Header card, then one bento grid of four. Nothing below it.
 *
 * Two columns from `md` up: the wider card of each pair on the left, the narrower
 * on the right. One column below that.
 *
 * `items-start` is the whole fix for the mock's worst habit. Grid items stretch to
 * their row by default, so Following — three chips — was drawn as tall as the
 * favourites beside it, with a few hundred pixels of nothing under the chips. Each
 * card is now as tall as its own content. The *widths* are unchanged: a card with
 * two posters still fills its column and leaves the space to their right empty,
 * because most accounts will fill it and a row that resizes itself to its contents
 * makes the page jump about as you use it.
 */
function Body({
  data,
  onBioSaved,
}: {
  data: ProfileData
  onBioSaved: (bio: string) => void
}) {
  const [editing, setEditing] = useState(false)

  return (
    <main className="max-w-[1120px] mx-auto px-margin-mobile md:px-margin-desktop py-lg md:py-xl flex flex-col gap-md">
      <ProfileHeader
        avatar={data.avatar}
        name={data.name}
        meta={`${data.handle} • ${data.member_since}`}
        bio={
          <BioField
            bio={data.bio}
            editing={editing}
            onDone={() => setEditing(false)}
            onSaved={onBioSaved}
          />
        }
        action={
          <>
            {/* Hidden while the editor is open: it has its own Save and Cancel, and
                a second way in beside them only invites a click that does nothing. */}
            {!editing && (
              <button onClick={() => setEditing(true)} className={EDIT_PILL}>
                Edit
              </button>
            )}
            <ShareButton handle={data.handle} />
            {/* Only below `md`. There is no app bar on a phone, so this is the only
                way out there. It belongs on your own page anyway. */}
            <SignOutButton className="md:hidden" />
          </>
        }
      />

      <div className="grid grid-cols-1 md:grid-cols-5 gap-md items-start">
        {/* Linked even when empty: the collection page carries the same "how to fill
            this" copy plus a way to act on it. */}
        <div className="md:col-span-3">
          <Tile label="Favorite Films" to="/collections/favorites" viewAll>
            <PosterRow
              films={data.favorites}
              captioned
              empty={'Press the heart on any film’s page and it collects here.'}
            />
          </Tile>
        </div>

        {/* The count is the graph's own total, and every person behind it is drawn,
            so the number and the chips can't disagree. */}
        <div className="md:col-span-2">
          <Tile label="Following" count={data.following_count}>
            {data.following.length ? (
              <div className="flex flex-wrap gap-sm">
                {data.following.map((person) => (
                  <FollowChip key={person.id} person={person} />
                ))}
              </div>
            ) : (
              <Empty>Nobody yet — find people on Friends and follow them.</Empty>
            )}
          </Tile>
        </div>

        <div className="md:col-span-3">
          <Tile label="Watchlist" to="/collections/watchlist">
            <PosterRow
              films={data.watchlist}
              empty={'Nothing logged yet — the "+" over any poster adds one.'}
            />
          </Tile>
        </div>

        <div className="md:col-span-2">
          <Tile label="Recent Reviews" to="/collections/journal">
            {data.recent_reviews.length ? (
              // Hairlines between the rows, none above the first or below the last.
              <div className="flex flex-col divide-y divide-surface-variant">
                {data.recent_reviews.map((film) => (
                  <div key={film.id} className="py-md first:pt-0 last:pb-0">
                    <ReviewRow film={film} />
                  </div>
                ))}
              </div>
            ) : (
              <Empty>Rate or review a film and it shows up here, newest first.</Empty>
            )}
          </Tile>
        </div>
      </div>
    </main>
  )
}

export function Profile() {
  const { data, error, loading, update, reload } = useApi(() => api.profile())

  // This page is the account's own, so a visitor without one gets a 401. That is an
  // answer, not a fault. It asks them in rather than reporting a dead API at somebody
  // who has never signed in.
  const anonymous = isUnauthorized(error)

  return (
    // `pb-24` clears the 64px mobile nav, which is fixed and would otherwise sit
    // over the last card.
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="profile" />
      <DemoBanner />
      {loading && <Loading />}
      {anonymous && <SignInPrompt heading="Sign in to see your profile." />}
      {error && !anonymous && <ErrorNote error={error} onRetry={reload} />}
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
