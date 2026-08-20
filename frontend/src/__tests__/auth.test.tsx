/**
 * Signing in and out, through the app bar every screen draws.
 *
 * `TopAppBar` on its own rather than a whole screen. The bar is the only part of the
 * app that changes with who you are, and a screen would drag its own requests in.
 *
 * The cookie is HttpOnly, so `api.me` is all the bar knows about you. A rejected `me`
 * is the anonymous case, which is what the API sends a reader with no session.
 */
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { ApiError, api } from '../api'
import { TopAppBar } from '../components/Chrome'
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
