/**
 * Profile. Ported from `reference/profile/`.
 *
 * Everything below the header is the visitor's own data rather than the mock's
 * invented copy: the strips and the grid are their real watchlist and ratings, and
 * "Following" is the two friend rails the other screens draw from. So the screen
 * starts out mostly empty and fills in as you use the app, which is the honest
 * version of a mock whose every tile was pre-populated.
 *
 * Three of the mock's controls are dropped rather than reproduced: "Edit" and
 * `share` (nothing to edit, nothing to share to), and the four `chevron_right`
 * links (each opened nothing). The two that have a real destination — Watchlist and
 * Following — are the headings of the sections further down the same page, so the
 * strips scroll to them instead.
 */
import type { FollowedPerson, Movie, Profile as ProfileData, RatedFilm } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import {
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'
import { StarRating } from '../components/StarRating'
import { Link } from 'react-router-dom'

/** A bento tile: an uppercase label, an optional link out, and its content. */
function Tile({
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

/** The strips' 64px thumbnails. */
function Thumbnail({ film }: { film: Movie }) {
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

/** What a strip shows before the visitor has done the thing that fills it. */
function Empty({ children }: { children: React.ReactNode }) {
  return <p className="font-body-md text-body-md text-on-surface-variant">{children}</p>
}

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
        <StarRating
          halfStars={film.rating_half_stars}
          size="text-sm"
          showEmpty={false}
          className="scale-75 origin-right shrink-0"
        />
      </div>
      {film.blurb && (
        <p className="text-sm text-on-surface-variant line-clamp-1">{film.blurb}</p>
      )}
    </div>
  )
}

/** One "Following" row. Not a link: friends have no screen of their own. */
function FriendRow({ person }: { person: FollowedPerson }) {
  return (
    <div className="flex items-center gap-md">
      <div className="w-12 h-12 rounded-full overflow-hidden bg-surface-container border border-surface-variant shrink-0">
        <img className="w-full h-full object-cover" alt={person.avatar.alt} src={person.avatar.src} />
      </div>
      <div className="flex flex-col flex-grow min-w-0">
        <span className="font-body-md text-body-md text-on-background">{person.name}</span>
        <span className="font-label-sm text-label-sm text-outline truncate">
          {person.subtitle}
        </span>
      </div>
    </div>
  )
}

/** A poster in the watchlist grid, with its title over a gradient. */
function WatchlistCard({ film }: { film: Movie }) {
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

function Body({ data }: { data: ProfileData }) {
  return (
    <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-md">
      <section className="flex items-center gap-md text-left py-md">
        <div className="w-24 h-24 shrink-0 rounded-full overflow-hidden border border-surface-variant soft-shadow bg-surface-container">
          <img className="w-full h-full object-cover" alt={data.avatar.alt} src={data.avatar.src} />
        </div>
        <div className="flex flex-col gap-xs flex-grow min-w-0">
          <h1 className="font-headline-md text-headline-md text-on-background">{data.name}</h1>
          <p className="font-label-sm text-label-sm text-outline uppercase tracking-wider">
            {data.handle} • {data.member_since}
          </p>
          <p className="font-body-md text-body-md text-on-surface-variant line-clamp-2 md:line-clamp-1">
            {data.bio}
          </p>
        </div>
      </section>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-md">
        <Tile label="Favorite Films">
          {data.favorites.length ? (
            <div className="flex gap-sm overflow-hidden">
              {data.favorites.map((film) => (
                <Thumbnail key={film.id} film={film} />
              ))}
            </div>
          ) : (
            <Empty>
              Rate a film on its page and your highest-rated ones collect here.
            </Empty>
          )}
        </Tile>

        <Tile label="Watchlist" to={data.watchlist.length ? '#watchlist' : undefined}>
          {data.watchlist.length ? (
            <div className="flex gap-sm overflow-hidden">
              {data.watchlist.map((film) => (
                <Thumbnail key={film.id} film={film} />
              ))}
            </div>
          ) : (
            <Empty>Nothing logged yet — the "+" over any poster adds one.</Empty>
          )}
        </Tile>

        <Tile label={`Following (${data.following_count})`} to="#following">
          <div className="flex gap-sm">
            {data.following.map((person) => (
              <div
                key={person.id}
                title={person.name}
                className="w-8 h-8 rounded-full overflow-hidden border border-surface-variant"
              >
                <img
                  className="w-full h-full object-cover"
                  alt={person.avatar.alt}
                  src={person.avatar.src}
                />
              </div>
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
            <Empty>Your ratings show up here, newest first.</Empty>
          )}
        </Tile>
      </div>

      <section className="grid grid-cols-1 md:grid-cols-12 gap-gutter">
        <div className="md:col-span-8 flex flex-col gap-md" id="watchlist">
          <div className="flex items-baseline justify-between mb-sm">
            <h2 className="font-headline-lg-mobile md:font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background">
              Watchlist
            </h2>
            {data.watchlist.length > 0 && (
              <span className="font-label-sm text-label-sm text-outline">
                {data.watchlist.length}
              </span>
            )}
          </div>
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
          <div className="flex items-baseline justify-between mb-sm">
            <h2 className="font-headline-lg-mobile md:font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background">
              Following
            </h2>
            <span className="font-label-sm text-label-sm text-outline">
              {data.following_count}
            </span>
          </div>
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
  const { data, error, loading } = useApi(() => api.profile())

  return (
    // `pb-24` clears the 64px mobile nav, which is fixed and would otherwise sit
    // over the last row of the following list.
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="profile" />
      <DemoBanner />
      {loading && <Loading />}
      {error && <ErrorNote error={error} />}
      {data && <Body data={data} />}
      <BottomNavBar active="profile" />
    </div>
  )
}
