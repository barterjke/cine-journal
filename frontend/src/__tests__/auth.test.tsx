/**
 * Signing in and out, through the app bar every screen draws — and what the feed says
 * when a sign-in came back having failed.
 *
 * `TopAppBar` on its own rather than a whole screen. The bar is the only part of the
 * app that changes with who you are, and a screen would drag its own requests in. The
 * `auth_error` cases below are the exception: the notice is drawn on `/`, and its whole
 * behaviour is about that URL, so they mount the real feed.
 *
 * The cookie is HttpOnly, so `api.me` is all the bar knows about you. A rejected `me`
 * is the anonymous case, which is what the API sends a reader with no session.
 */
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useLocation } from 'react-router-dom'

import { ApiError, api } from '../api'
import { TopAppBar } from '../components/Chrome'
import { Feed } from '../pages/Feed'
import { aUser, anAuthFailure, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

/** The bar is identical on every route, so any one of them will do. */
const bar = { path: '/', at: '/' }

const signedOut = () => vi.mocked(api.me).mockRejectedValue(anAuthFailure('GET', '/api/auth/me'))

// Call counts are asserted below, so each case starts from none. Implementations
// survive this — only the history is dropped.
beforeEach(() => {
  vi.clearAllMocks()
})

describe('the app bar with nobody signed in', () => {
  it('offers a sign in', async () => {
    signedOut()

    renderScreen(<TopAppBar active="feed" />, bar)

    expect(await screen.findByRole('button', { name: 'Sign in with Google' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Sign out' })).not.toBeInTheDocument()
  })

  it('offers it without first asking whether this server can sign anyone in', async () => {
    signedOut()

    renderScreen(<TopAppBar active="feed" />, bar)

    await screen.findByRole('button', { name: 'Sign in with Google' })
    // The button's presence is a property of the app. Hiding it on the server that
    // happens to have no Google credentials is how a missing sign-in ships.
    expect(api.signIn).not.toHaveBeenCalled()
  })

  it('says why when the server has no Google credentials, rather than navigating into the 503', async () => {
    signedOut()
    // What `api.signIn` throws when its probe came back 503 instead of a redirect.
    vi.mocked(api.signIn).mockRejectedValue(
      new ApiError('google sign-in is not configured on this server', 503),
    )

    renderScreen(<TopAppBar active="feed" />, bar)
    await userEvent.click(await screen.findByRole('button', { name: 'Sign in with Google' }))

    expect(await screen.findByRole('status')).toHaveTextContent(
      'google sign-in is not configured on this server',
    )
    // Still there to press: the failure is about this server, not about the visitor.
    expect(screen.getByRole('button', { name: 'Sign in with Google' })).toBeInTheDocument()
  })
})

describe('the app bar with somebody signed in', () => {
  it('draws their face and a way out', async () => {
    vi.mocked(api.me).mockResolvedValue(aUser())

    renderScreen(<TopAppBar active="feed" />, bar)

    expect(await screen.findByRole('button', { name: 'Sign out' })).toBeInTheDocument()
    expect(await screen.findByAltText('Portrait of Sam')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Sign in with Google' })).not.toBeInTheDocument()
  })

  it('goes back to offering a sign in once signed out, with no reload', async () => {
    // Signed in for the first ask, anonymous for the one after signing out.
    signedOut()
    vi.mocked(api.me).mockResolvedValueOnce(aUser())
    vi.mocked(api.logout).mockResolvedValue(undefined)

    renderScreen(<TopAppBar active="feed" />, bar)
    await userEvent.click(await screen.findByRole('button', { name: 'Sign out' }))

    expect(api.logout).toHaveBeenCalled()
    // The cookie is HttpOnly, so the state is re-read rather than reasoned about.
    await waitFor(() => expect(api.me).toHaveBeenCalledTimes(2))
    expect(await screen.findByRole('button', { name: 'Sign in with Google' })).toBeInTheDocument()
    expect(screen.queryByAltText('Portrait of Sam')).not.toBeInTheDocument()
  })
})

/**
 * Prints the current query string, so a case can assert the URL rather than only what
 * is on screen. `role="note"` keeps it queryable without a test id.
 */
function CurrentQuery() {
  const { search } = useLocation()
  return <p role="note">{search === '' ? '(none)' : search}</p>
}

/**
 * A sign-in that ended at Google — cancelled, expired, refused — comes back to `/` as
 * `?auth_error=<slug>`. It used to come back as a page of raw JSON.
 */
describe('a sign-in that did not finish', () => {
  const feedAt = (search: string) => ({ path: '/', at: `/${search}` })

  /** One empty page and no more, so the feed's own requests stay out of the way. */
  const emptyFeed = () =>
    vi.mocked(api.feedPage).mockResolvedValue({
      items: [],
      next_cursor: null,
      from_cache: false,
    })

  it.each([
    ['cancelled', "Sign-in cancelled. You're still signed out."],
    ['expired', 'That sign-in took too long and expired. Please try again.'],
    ['denied', "Google didn't grant access, so you're still signed out."],
    ['failed', 'Sign-in failed. Please try again.'],
  ])('says what happened for %s', async (slug, sentence) => {
    signedOut()
    emptyFeed()

    renderScreen(<Feed />, feedAt(`?auth_error=${slug}`))

    expect(await screen.findByText(sentence)).toBeInTheDocument()
  })

  it('falls back to a generic sentence for a slug it has never heard of', async () => {
    signedOut()
    emptyFeed()

    renderScreen(<Feed />, feedAt('?auth_error=teapot'))

    expect(await screen.findByText("Sign-in didn't finish. Please try again.")).toBeInTheDocument()
    // The slug is a name for us, not copy for the reader. The server can add one
    // before this build knows about it.
    expect(screen.queryByText(/teapot/)).not.toBeInTheDocument()
  })

  it('drops the parameter from the URL when dismissed, so a refresh cannot bring it back', async () => {
    signedOut()
    emptyFeed()

    renderScreen(
      <>
        <Feed />
        <CurrentQuery />
      </>,
      feedAt('?auth_error=cancelled'),
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Dismiss' }))

    expect(screen.getByRole('note')).toHaveTextContent('(none)')
    expect(screen.queryByText(/Sign-in cancelled/)).not.toBeInTheDocument()
  })

  it('says nothing at all to a visitor who simply opened the feed', async () => {
    signedOut()
    emptyFeed()

    renderScreen(<Feed />, feedAt(''))

    // Awaited, so this is asserted against a feed that has finished loading.
    expect(await screen.findByText(/Nothing here yet/)).toBeInTheDocument()
    expect(screen.queryByText(/Sign-in/)).not.toBeInTheDocument()
  })
})
