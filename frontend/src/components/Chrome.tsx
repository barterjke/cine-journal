/**
 * Shared app chrome: the desktop TopAppBar and the mobile BottomNavBar.
 *
 * The export shipped these markup blocks four times with small divergences; the
 * shape kept here is the desktop feed's, parameterized by which tab is active.
 * Links marked `#` were inert in the export and stay inert.
 */
import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'

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
 * The nav is shown on every screen, including the movie detail page — the
 * export's detail mock dropped the links and left only the trailing icons, which
 * stranded you there with no way back except the browser's own button.
 */
export function TopAppBar({
  active,
  showSearch = false,
  showSearchIcon = false,
}: {
  active: Tab
  showSearch?: boolean
  showSearchIcon?: boolean
}) {
  return (
    <header className="w-full top-0 sticky z-50 bg-surface dark:bg-on-background border-b border-surface-variant dark:border-outline-variant hidden md:flex">
      <div className="flex justify-between items-center px-margin-mobile md:px-margin-desktop py-md w-full max-w-[1440px] mx-auto">
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
              const className =
                tab.id === active
                  ? 'text-primary dark:text-primary-fixed font-bold border-b-2 border-primary py-2 hover:text-primary dark:hover:text-primary-fixed transition-colors cursor-pointer active:opacity-70'
                  : 'text-on-surface-variant dark:text-outline py-2 hover:text-primary dark:hover:text-primary-fixed transition-colors cursor-pointer active:opacity-70'

              return tab.to === null ? (
                <span key={tab.id} className="text-outline dark:text-outline py-2 cursor-default">
                  {tab.label}
                </span>
              ) : (
                <Link key={tab.id} to={tab.to} className={className}>
                  {tab.label}
                </Link>
              )
            })}
          </nav>
        </div>
        <div className="flex items-center gap-md">
          {showSearchIcon && (
            <Link
              to="/search"
              className="text-on-surface-variant hover:text-primary transition-colors p-sm cursor-pointer active:opacity-70"
            >
              <span className="material-symbols-outlined">search</span>
            </Link>
          )}
          {showSearch && <SearchBox />}
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
