/**
 * Shared app chrome: the desktop TopAppBar and the mobile BottomNavBar.
 *
 * The export shipped these markup blocks four times with small divergences; the
 * shape kept here is the desktop feed's, parameterized by which tab is active.
 */
import { useEffect, useState } from 'react'
import { Link, useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import type { Status } from '../api'
import { NothingYetError, api, isNotFound, visitorMessage } from '../api'
import { useAction } from '../useAction'
import { signOut, useAuth } from '../useAuth'

export type Tab = 'feed' | 'movies' | 'friends' | 'profile'

/**
 * Every tab now has a screen behind it, so every one is a real `<Link>`.
 *
 * Profile was the exception until `/profile` existed, and it rendered as dimmed
 * text rather than as `<Link to="#">` — because react-router resolves a bare `#`
 * against the *current* path, so on `/movie/red-shift` its href became
 * `/movie/red-shift`: a link that looks live and goes somewhere wrong. Worth
 * remembering before adding a fifth tab ahead of its screen.
 */
const TABS: { id: Tab; label: string; icon: string; to: string }[] = [
  { id: 'feed', label: 'Feed', icon: 'home', to: '/' },
  { id: 'movies', label: 'Movies', icon: 'movie', to: '/search' },
  // Friends pointed at `/review` while a single review screen was the only place
  // another person appeared. `/people` is the actual directory now.
  { id: 'friends', label: 'Friends', icon: 'group', to: '/people' },
  { id: 'profile', label: 'Profile', icon: 'person', to: '/profile' },
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

/** The pill both auth buttons wear. Written once so the two can't drift. */
const PILL =
  'font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 rounded-full inline-flex items-center gap-xs transition-opacity disabled:cursor-wait'

/**
 * The one way in. Always drawn while nobody is signed in.
 *
 * Whether this server has Google credentials is a fact about today's deployment, not
 * about the app. Hiding the button on a server that lacks them is how "there is no
 * sign-in" ships. So the 503 is reported on the click instead. `api.signIn` asks the
 * endpoint before navigating to it, so a misconfigured server can't dump JSON at you.
 *
 * `float` puts a failure in a card under the button instead of in the flow. That is
 * for the app bar, whose height is fixed so it doesn't shift between routes.
 */
export function SignInButton({ float = false }: { float?: boolean }) {
  const signIn = useAction(() => api.signIn())

  return (
    <div className={float ? 'relative' : 'flex flex-col items-center gap-sm'}>
      <button
        onClick={() => void signIn.run()}
        disabled={signIn.busy}
        className={`${PILL} bg-primary text-on-primary hover:opacity-90`}
      >
        <span className="material-symbols-outlined text-[18px]" aria-hidden="true">
          login
        </span>
        {signIn.busy ? 'Signing in…' : 'Sign in with Google'}
      </button>
      {signIn.error && (
        <p
          role="status"
          className={
            float
              ? 'absolute top-full right-0 mt-sm w-64 rounded-lg border border-secondary/40 bg-surface px-md py-sm font-label-sm text-label-sm text-secondary soft-shadow z-50'
              : 'max-w-xs font-label-sm text-label-sm text-secondary'
          }
        >
          {signIn.error}
        </p>
      )}
    </div>
  )
}

/**
 * The way out. Signing out refreshes the auth state, so the bar updates with no
 * reload — see `signOut`.
 *
 * Logout answers 204 either way, so the only failure is not reaching the API. The
 * label stays short because this sits in a bar of fixed height, and the message goes
 * in `title`, the way `FollowButton` handles its own.
 */
export function SignOutButton({ className = '' }: { className?: string }) {
  const out = useAction(signOut)

  return (
    <button
      onClick={() => void out.run()}
      disabled={out.busy}
      title={out.error ?? 'Sign out'}
      className={`${PILL} border ${
        out.error
          ? 'border-secondary text-secondary'
          : 'border-outline-variant text-on-surface-variant hover:bg-surface-container-low'
      } ${className}`.trim()}
    >
      {out.error ? 'Retry sign out' : out.busy ? 'Signing out…' : 'Sign out'}
    </button>
  )
}

/**
 * The right end of the app bar: your own page, and the way in or out of it.
 *
 * The profile link is the one that was always there. It wears your face once there
 * is one to draw.
 */
function AccountControl({ active }: { active: Tab }) {
  const { user, loading } = useAuth()

  return (
    <div className="flex items-center gap-sm">
      <Link
        to="/profile"
        aria-label="Your profile"
        title={user?.name ?? 'Your profile'}
        className={
          active === 'profile'
            ? 'text-primary dark:text-primary-fixed p-sm active:opacity-70'
            : 'text-on-surface-variant hover:text-primary transition-colors p-sm active:opacity-70'
        }
      >
        {user ? (
          <img
            className="w-8 h-8 rounded-full object-cover border border-surface-variant block"
            alt={user.avatar.alt}
            src={user.avatar.src}
          />
        ) : (
          <span
            className="material-symbols-outlined block"
            style={active === 'profile' ? { fontVariationSettings: "'FILL' 1" } : undefined}
          >
            account_circle
          </span>
        )}
      </Link>
      {/* Nothing until the first `/api/auth/me` answers. A sign-in button that turns
          into your avatar a beat later reads as a session that dropped. */}
      {loading ? null : user ? <SignOutButton /> : <SignInButton float />}
    </div>
  )
}

/**
 * What a screen shows in place of `ErrorNote` when the API asked for an account.
 *
 * A 401 from `/api/profile` is an answer, not a failure. So this reads as an
 * invitation, and says what stays readable without signing in.
 */
export function SignInPrompt({ heading }: { heading: string }) {
  return (
    <div className="flex flex-col items-center gap-sm py-xxl px-margin-mobile text-center">
      <span className="material-symbols-outlined text-primary" aria-hidden="true">
        account_circle
      </span>
      <p className="font-headline-md text-headline-md text-on-background">{heading}</p>
      <p className="font-body-md text-body-md text-on-surface-variant max-w-md">
        Your films, your ratings and your watchlist live on your account. The feed,
        every film's page and everybody's reviews are readable without one.
      </p>
      <div className="pt-sm">
        <SignInButton />
      </div>
    </div>
  )
}

/**
 * What a sign-in that didn't finish says, per slug.
 *
 * The backend sends the browser back to `/?auth_error=<slug>` instead of answering
 * with JSON, so a cancelled or expired sign-in lands on the feed rather than on a
 * page of raw `{"error":…}`.
 */
const AUTH_ERRORS: Record<string, string> = {
  cancelled: "Sign-in cancelled. You're still signed out.",
  expired: 'That sign-in took too long and expired. Please try again.',
  denied: "Google didn't grant access, so you're still signed out.",
  failed: 'Sign-in failed. Please try again.',
}

/**
 * For a slug this build doesn't know. The set can grow on the server first, and
 * printing the slug itself would be showing the reader a variable name.
 */
const AUTH_ERROR_FALLBACK = "Sign-in didn't finish. Please try again."

/**
 * The notice for a failed sign-in, drawn on the feed when `?auth_error=` is set.
 *
 * The URL is the only state. Dismissing drops the parameter, so the notice can't
 * come back on a refresh — and it is a `replace`, so Back doesn't return to it
 * either. `ActionError` is the app's notice, reused rather than restyled here.
 */
export function AuthErrorNotice() {
  const [params, setParams] = useSearchParams()
  const slug = params.get('auth_error')

  if (slug === null) return null

  const dismiss = () => {
    const next = new URLSearchParams(params)
    next.delete('auth_error')
    setParams(next, { replace: true })
  }

  return (
    <div className="max-w-3xl mx-auto px-margin-mobile md:px-0 pt-lg">
      <ActionError message={AUTH_ERRORS[slug] ?? AUTH_ERROR_FALLBACK} onDismiss={dismiss} signIn />
    </div>
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

              return (
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
          {/* Was a bell and a cast icon: two `<button>`s with no `onClick`, no
              notifications behind them and nothing to cast to. Your own page and the
              way in or out instead — the Profile tab goes to the same place, but this
              corner of a masthead is where "you" belongs, and it is reachable below
              `lg` where the nav collapses. */}
          <AccountControl active={active} />
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

          return (
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
 * "These films are made up" band, shown on every screen when the server is serving
 * sample data instead of the real catalogue. Renders nothing otherwise, and nothing
 * while the request is in flight — a band that appears a beat after the page would
 * shove the content down as you started reading it.
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
        {/* The fallback line said "no TMDB token", which names a credential a
            visitor has never heard of. It says what is true of the films instead;
            the link is where anyone curious can read why. */}
        <p className="flex-grow font-body-md text-sm">
          {status.message ?? 'These films are made up — this site is running on sample data.'}{' '}
          <a
            href={status.docs_url}
            target="_blank"
            rel="noreferrer"
            className="font-label-sm text-label-sm text-primary underline hover:opacity-70 transition-opacity whitespace-nowrap"
          >
            Find out why
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
 *
 * Pass `signIn={action.signInRequired}` for a write the server refused for want of
 * an account. The notice then carries the way out instead of only naming the problem.
 */
export function ActionError({
  message,
  onDismiss,
  signIn = false,
}: {
  message: string
  onDismiss?: () => void
  signIn?: boolean
}) {
  // The API's messages carry the method and path in front of the sentence. That
  // half is ours to read, so it moves to `title` and out of the visible copy.
  const visible = visitorMessage(message)

  return (
    <div
      role="status"
      title={visible === message ? undefined : message}
      className="flex items-start gap-sm rounded-lg border border-secondary/40 bg-secondary/5 px-md py-sm font-label-sm text-label-sm text-on-surface"
    >
      <span className="material-symbols-outlined text-secondary text-lg">
        {signIn ? 'account_circle' : 'error'}
      </span>
      <span className="flex-grow">{visible}</span>
      {signIn && <SignInButton />}
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

/**
 * The illustration on the error screen: a strip of film with one blank frame.
 *
 * Drawn in markup rather than fetched. An error screen that loads an image can
 * fail the same way as the thing it is reporting, and a broken picture is a poor
 * way to say "we're on it". Decorative, so it is hidden from screen readers —
 * the copy below carries the meaning.
 */
function ErrorArt() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 200 120"
      className="w-40 h-auto text-outline-variant"
      fill="none"
      stroke="currentColor"
    >
      {/* The strip, with a sprocket hole every 24px down both edges. */}
      <rect x="1" y="1" width="198" height="118" rx="8" strokeWidth="2" />
      <path
        strokeWidth="2"
        d="M9 14h11v10H9zM9 38h11v10H9zM9 62h11v10H9zM9 86h11v10H9zM180 14h11v10h-11zM180 38h11v10h-11zM180 62h11v10h-11zM180 86h11v10h-11z"
      />
      {/* The frame with nothing in it. Dashed, so it reads as absence. */}
      <rect x="40" y="20" width="120" height="80" rx="4" strokeWidth="2" strokeDasharray="7 6" />
    </svg>
  )
}

/**
 * What a screen shows in place of its content when its first request failed.
 *
 * Written for a visitor. It used to print the raw message and then tell the
 * reader to start a server from a shell, which is meaningless to anyone who
 * didn't write this. The real message is still here, in the `title` — an
 * attribute rather than text, so it can't leak back into the copy on screen.
 *
 * Three cases, because a visitor can do different things about them:
 *  - a 404 is a dead end, so it offers the way onward and no retry;
 *  - nothing to show yet is not a failure at all, and says so in its own words;
 *  - anything else is worth trying again.
 */
export function ErrorNote({
  error,
  onRetry,
  missing,
}: {
  error: Error
  /** Runs the screen's request again. Without it, only the way onward is offered. */
  onRetry?: () => void
  /** This screen's 404 line, e.g. "This film isn't in our catalogue." */
  missing?: string
}) {
  const { pathname } = useLocation()
  const notFound = isNotFound(error)
  const nothingYet = error instanceof NothingYetError

  const heading = notFound
    ? "We couldn't find that"
    : nothingYet
      ? 'Nothing here yet'
      : 'Something went wrong'

  const line = notFound
    ? (missing ?? "That page isn't in our catalogue. It may have moved, or the link may be wrong.")
    : nothingYet
      ? error.message
      : "We're having trouble connecting. Nothing you did — please try again in a moment."

  const pill =
    'font-label-sm text-label-sm uppercase tracking-wider px-4 py-2 rounded-full transition-opacity hover:opacity-80'

  return (
    <div
      title={error.message}
      className="flex flex-col items-center gap-md py-xxl px-margin-mobile text-center"
    >
      <ErrorArt />
      <h2 className="font-headline-md text-headline-md text-on-background">{heading}</h2>
      <p className="font-body-md text-body-md text-on-surface-variant max-w-md">{line}</p>
      <div className="flex items-center gap-sm flex-wrap justify-center">
        {/* No retry on a 404 or an empty site: the same request would give the same
            answer, and a button that changes nothing is worse than no button. */}
        {onRetry && !notFound && !nothingYet && (
          <button onClick={onRetry} className={`${pill} bg-primary text-on-primary`}>
            Try again
          </button>
        )}
        {/* Skipped on the feed itself, where it would be a link to this page. */}
        {pathname !== '/' && (
          <Link to="/" className={`${pill} border border-outline-variant text-on-surface-variant`}>
            Back to the feed
          </Link>
        )}
      </div>
    </div>
  )
}
