/**
 * `/profile` for a visitor with no account.
 *
 * It is one of the two reads the API refuses anonymously, and it used to render the
 * refusal verbatim: "GET /api/profile failed: sign in to do that". A 401 there is an
 * answer, not a fault, so this checks the page asks the visitor in.
 */
import { screen } from '@testing-library/react'

import { api } from '../api'
import { Profile } from '../pages/Profile'
import { anAuthFailure, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

describe('the profile page with nobody signed in', () => {
  it('prompts for a sign in instead of reporting a failed request', async () => {
    vi.mocked(api.me).mockRejectedValue(anAuthFailure('GET', '/api/auth/me'))
    vi.mocked(api.profile).mockRejectedValue(anAuthFailure('GET', '/api/profile'))

    renderScreen(<Profile />, { path: '/profile', at: '/profile' })

    expect(await screen.findByText('Sign in to see your profile.')).toBeInTheDocument()
    // The bug this replaces: the API's own line, printed at somebody who has simply
    // never signed in.
    expect(screen.queryByText(/GET \/api\/profile failed/)).not.toBeInTheDocument()
    // And not the other branch either, which would blame a connection that is fine.
    expect(screen.queryByText('Something went wrong')).not.toBeInTheDocument()
    // Two: the prompt's, and the app bar's.
    expect(screen.getAllByRole('button', { name: 'Sign in with Google' })).toHaveLength(2)
  })
})
