/**
 * Following and unfollowing from the friend directory.
 *
 * All three lists arrive in one response and a person can be in more than one, so the
 * screen patches every copy of them after a follow. Two rows disagreeing about whether
 * you follow someone is what that code prevents, so that is what these check.
 *
 * Names are counted rather than scoped to a panel: the panels are headings over plain
 * divs with no landmark role to query within.
 */
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { api } from '../api'
import { People } from '../pages/People'
import { aPeopleResponse, aPerson, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

const directory = { path: '/people', at: '/people' }

describe('the friend directory', () => {
  it('adds someone followed from the search results to the Following list', async () => {
    // An empty query returns nobody by design, so the results panel appears only after a
    // search.
    vi.mocked(api.people).mockImplementation((q?: string) =>
      Promise.resolve(q ? aPeopleResponse({ query: q, results: [aPerson()] }) : aPeopleResponse()),
    )
    vi.mocked(api.setFollow).mockResolvedValue({
      person_id: '1136406',
      following: true,
      following_count: 1,
    })

    renderScreen(<People />, directory)

    await userEvent.type(await screen.findByLabelText('Search people by nickname'), 'elena{Enter}')

    expect(await screen.findByText('Elena Vasquez')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Follow' }))

    expect(api.setFollow).toHaveBeenCalledWith('1136406', true)
    // Two rows now: the search result, and the Following panel. Patched rather than
    // refetched, so there is no loading flash.
    await waitFor(() => expect(screen.getAllByText('Elena Vasquez')).toHaveLength(2))
    expect(screen.getAllByRole('button', { name: 'Following' })).toHaveLength(2)
  })

  it('removes someone from the Following list when they are unfollowed', async () => {
    vi.mocked(api.people).mockResolvedValue(
      aPeopleResponse({ following: [aPerson({ following: true })] }),
    )
    vi.mocked(api.setFollow).mockResolvedValue({
      person_id: '1136406',
      following: false,
      following_count: 0,
    })

    renderScreen(<People />, directory)

    await userEvent.click(await screen.findByRole('button', { name: 'Following' }))

    expect(api.setFollow).toHaveBeenCalledWith('1136406', false)
    // The panel is defined by the flag, so the row leaves the list rather than sitting
    // there under a "Follow" button.
    await waitFor(() => expect(screen.queryByText('Elena Vasquez')).not.toBeInTheDocument())
    expect(screen.getByText(/You don't follow anyone yet/)).toBeInTheDocument()
  })
})
