/**
 * Shared app chrome: the desktop TopAppBar and the mobile BottomNavBar.
 *
 * The export shipped these markup blocks four times with small divergences; the
 * shape kept here is the desktop feed's, parameterized by which tab is active.
 * Links marked `#` were inert in the export and stay inert.
 */
import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'

import type { Status } from '../api'
import { api } from '../api'

export type Tab = 'feed' | 'movies' | 'friends' | 'profile'

/**
 * `to: null` marks a tab that was inert in the export and still has no screen
 * behind it. It renders as text rather than as a `<Link to="#">`: react-router
 * resolves a bare `#` against the *current* path, so on `/movie/red-shift` the
 * Profile tab's href became `/movie/red-shift` — a link that looks live and goes
 * nowhere useful.
 */
const TABS: { id: Tab; label: string; icon: string; to: string | null }[] = [
  { id: 'feed', label: 'Feed', icon: 'home', to: '/' },
  { id: 'movies', label: 'Movies', icon: 'movie', to: '/search' },
  { id: 'friends', label: 'Friends', icon: 'group', to: '/review' },
  { id: 'profile', label: 'Profile', icon: 'person', to: null },
]

/**
 * The app bar's search box. Submitting hands the query to `/search`, which owns
 * its state in the URL — so this only has to navigate, never filter.
 *
 * Below `lg` there isn't room for it beside the four nav links, so the bar shows
 * a search *icon* there instead. Both are always rendered and swap on the
 * breakpoint — which one you get must not depend on the route.
 */
function SearchBox() {
  const navigate = useNavigate()
  const [draft, setDraft] = useState('')

  return (
    <form
      className="relative hidden lg:block"
      onSubmit={(e) => {
        e.preventDefault()
        const q = draft.trim()
        navigate(q ? `/search?q=${encodeURIComponent(q)}` : '/search')
      }}
    >
      <span
        className="material-symbols-outlined absolute left-sm top-1/2 -translate-y-1/2 text-outline"
        style={{ fontSize: '20px' }}
      >
        search
      </span>
      <input
        className="bg-surface-container-low border-none rounded-full py-2 pl-xl pr-md font-label-sm text-label-sm text-on-surface focus:ring-1 focus:ring-primary w-64"
        placeholder="Search films..."
        type="search"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
      />
    </form>
  )
}

/**
 * One bar for every desktop screen — no props beyond which tab is lit.
 *
 * It used to take `showSearch` / `showNav` / `showSearchIcon` so each page could
 * reproduce its own export mock, and the result was four subtly different bars:
 * the detail page had no nav links and no search box, and the two screens that
 * omitted the box were 1px shorter than the two that had it, so the whole bar
 * shifted as you navigated. Nothing here is conditional now, which is the only
 * way that stays fixed.
 *
 * Two export behaviours are deliberately dropped: the detail mock's missing nav
 * links (they stranded you on a page every other screen links into) and the
 * inert wordmark, now the home button.
 */
export function TopAppBar({ active }: { active: Tab }) {
  return (
    <header className="w-full top-0 sticky z-50 bg-surface dark:bg-on-background border-b border-surface-variant dark:border-outline-variant hidden md:flex">
      {/* `h-20` rather than `py-md`: the search input is a hair taller than the
          icon buttons beside it, and it's hidden below `lg`, so a
          content-derived height made the bar 79px on some routes and 80px on
          others. Fixing the height decouples it from what's inside. */}
      <div className="flex justify-between items-center h-20 px-margin-mobile md:px-margin-desktop w-full max-w-[1440px] mx-auto">
        <div className="flex items-center gap-xl">
          {/* The wordmark doubles as the home button, which is what a masthead
              is for — the export drew it as inert text. */}
          <Link to="/" aria-label="CinéJournal home">
            <h1 className="font-headline-md text-headline-md font-bold text-primary dark:text-primary-fixed tracking-tight hover:opacity-70 transition-opacity">
              CinéJournal
            </h1>
          </Link>
          <nav className="flex items-center gap-lg">
            {TABS.map((tab) => {
              // `font-body-md` is explicit rather than inherited: the export's
              // search screen spelled it out on each link, and the feed's relied
              // on `<body>`. Inheriting meant the bar rendered in Hanken Grotesk
              // on two routes and the Tailwind default sans on the other two,
              // depending on whether that page's root div happened to set a font.
              const base =
                'font-body-md text-body-md py-2 hover:text-primary dark:hover:text-primary-fixed transition-colors cursor-pointer active:opacity-70'

              return tab.to === null ? (
                <span
                  key={tab.id}
                  className="font-body-md text-body-md text-outline dark:text-outline py-2 cursor-default"
                >
                  {tab.label}
                </span>
              ) : (
                <Link
                  key={tab.id}
                  to={tab.to}
                  className={
                    tab.id === active
                      ? `${base} text-primary dark:text-primary-fixed font-bold border-b-2 border-primary`
                      : `${base} text-on-surface-variant dark:text-outline`
                  }
                >
                  {tab.label}
                </Link>
              )
            })}
          </nav>
        </div>
        <div className="flex items-center gap-md">
          {/* The box and the icon are the same control at two sizes — the box
              from `lg` up, the icon below it, where the nav links leave no room. */}
          <Link
            to="/search"
            aria-label="Search films"
            className="lg:hidden text-on-surface-variant hover:text-primary transition-colors p-sm cursor-pointer active:opacity-70"
          >
            <span className="material-symbols-outlined">search</span>
          </Link>
          <SearchBox />
          <button className="text-on-surface-variant hover:text-primary transition-colors p-sm cursor-pointer active:opacity-70">
            <span className="material-symbols-outlined">notifications</span>
          </button>
          <button className="text-on-surface-variant hover:text-primary transition-colors p-sm cursor-pointer active:opacity-70">
            <span className="material-symbols-outlined">cast</span>
          </button>
        </div>
      </div>
    </header>
  )
}

export function BottomNavBar({ active }: { active: Tab }) {
  return (
    <nav className="fixed bottom-0 w-full z-50 bg-surface/90 backdrop-blur-md border-t border-surface-variant md:hidden">
      <div className="flex justify-around items-center px-4 h-16 w-full">
        {TABS.map((tab) => {
          const isActive = tab.id === active
          const body = (
            <>
              <span
                className="material-symbols-outlined"
                style={isActive ? { fontVariationSettings: "'FILL' 1" } : undefined}
              >
                {tab.icon}
              </span>
              <span className="font-label-sm text-label-sm mt-1 text-[10px]">{tab.label}</span>
            </>
          )

          return tab.to === null ? (
            <span
              key={tab.id}
              className="flex flex-col items-center justify-center text-outline px-4 py-1"
            >
              {body}
            </span>
          ) : (
            <Link
              key={tab.id}
              to={tab.to}
              className={
                isActive
                  ? 'flex flex-col items-center justify-center text-primary font-bold bg-primary-container/10 rounded-xl px-4 py-1 active:scale-95 transition-transform duration-150'
                  : 'flex flex-col items-center justify-center text-on-surface-variant hover:bg-surface-container-high px-4 py-1 rounded-xl active:scale-95 transition-transform duration-150'
              }
            >
              {body}
            </Link>
          )
        })}
      </div>
    </nav>
  )
}

/**
 * Memoized across mounts: `data_source` can only change when the server
 * restarts, so re-asking on every client-side navigation would be six identical
 * requests for one unchanging fact. A rejection isn't cached — if the backend was
 * simply down when the first page mounted, the next navigation retries.
 */
let statusRequest: Promise<Status> | null = null

function fetchStatus(): Promise<Status> {
  statusRequest ??= api.status().catch((error: unknown) => {
    statusRequest = null
    throw error
  })
  return statusRequest
}

/**
 * "These films are made up" band, shown on every screen when the backend has no
 * TMDB token and is serving `data.rs` instead. Renders nothing in TMDB mode, and
 * nothing while the request is in flight — a band that appears a beat after the
 * page would shove the content down as you started reading it.
 *
 * Deliberately *not* inside `TopAppBar`: that bar's height is fixed so it is
 * byte-identical on every route, and a conditional band inside it would undo
 * that. Out here it scrolls away as you read, which is the right weight for a
 * notice you only need to see once.
 *
 * A failed status request is swallowed. It means the API is unreachable, which
 * every screen already reports through `ErrorNote` — and the one thing worse than
 * no warning is a warning about the warning.
 */
export function DemoBanner() {
  const [status, setStatus] = useState<Status | null>(null)

  useEffect(() => {
    let active = true
    fetchStatus().then(
      (next) => {
        if (active) setStatus(next)
      },
      () => {},
    )
    return () => {
      active = false
    }
  }, [])

  if (status === null || status.data_source !== 'demo') return null

  return (
    <div className="w-full bg-secondary/10 border-b border-secondary/40">
      <div
        role="status"
        className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-sm flex items-start gap-sm text-on-surface"
      >
        <span className="material-symbols-outlined text-secondary text-lg shrink-0">
          science
        </span>
        {/* `font-body-md` rather than `font-label-sm`: the label face is
            JetBrains Mono, which is right for a chip but wraps this sentence to
            four hard-to-skim lines on a phone. */}
        <p className="flex-grow font-body-md text-sm">
          {status.message ?? 'Showing demo data — no TMDB token.'}{' '}
          <a
            href={status.docs_url}
            target="_blank"
            rel="noreferrer"
            className="font-label-sm text-label-sm text-primary underline hover:opacity-70 transition-opacity whitespace-nowrap"
          >
            Get a token
          </a>
        </p>
      </div>
    </div>
  )
}

/** Shown while a screen's single request is in flight. */
export function Loading() {
  return (
    <div className="flex items-center justify-center py-xxl text-on-surface-variant font-label-sm text-label-sm uppercase tracking-widest">
      Loading…
    </div>
  )
}

/**
 * Inline notice for a failed action (a like, a post, a watchlist toggle) where
 * the screen itself loaded fine. Distinct from `ErrorNote`, which replaces the
 * screen's content when the initial fetch failed.
 */
export function ActionError({ message, onDismiss }: { message: string; onDismiss?: () => void }) {
  return (
    <div
      role="status"
      className="flex items-start gap-sm rounded-lg border border-secondary/40 bg-secondary/5 px-md py-sm font-label-sm text-label-sm text-on-surface"
    >
      <span className="material-symbols-outlined text-secondary text-lg">error</span>
      <span className="flex-grow">{message}</span>
      {onDismiss && (
        <button
          onClick={onDismiss}
          aria-label="Dismiss"
          className="text-outline hover:text-on-surface transition-colors"
        >
          <span className="material-symbols-outlined text-lg">close</span>
        </button>
      )}
    </div>
  )
}

/** Shown when the API is unreachable — most often the backend isn't running. */
export function ErrorNote({ error }: { error: Error }) {
  return (
    <div className="flex flex-col items-center gap-sm py-xxl text-center">
      <span className="material-symbols-outlined text-secondary">error</span>
      <p className="font-body-md text-body-md text-on-background">Couldn't reach the API.</p>
      <p className="font-label-sm text-label-sm text-on-surface-variant">{error.message}</p>
      <p className="font-label-sm text-label-sm text-outline">
        Start it with <code>cd backend &amp;&amp; cargo run</code>
      </p>
    </div>
  )
}
