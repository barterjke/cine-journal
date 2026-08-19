/**
 * The "+" button on a poster tile, tested through a collection grid.
 *
 * `PosterTile` takes the state as a prop; the optimistic flip, the write and the
 * rollback all live in `Collection`. Rendering the tile alone would only check that a
 * boolean renders a check mark.
 *
 * `Collection` and not `Feed`, of the two screens using the tile, because the feed adds
 * an `IntersectionObserver` and a second cache-refresh request that would need stubbing
 * first and have nothing to do with the watchlist.
 */
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { ApiError, api } from '../api'
import { Collection } from '../pages/Collection'
import { aCollection, renderScreen } from './support'

// Hoisted above the imports by Vitest. The real module is spread back in so `ApiError`
// and `isNotFound` stay real — see `api-stub.ts`.
vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

/** The slug is a route param, so the pattern comes with it. */
const at = (slug: string) => ({ path: '/collections/:slug', at: `/collections/${slug}` })

describe('the watchlist button on a poster tile', () => {
  it('marks the film as logged before the server answers', async () => {
    vi.mocked(api.collection).mockResolvedValue(aCollection())
    // Never resolves, so only the optimistic flip can make this pass.
    vi.mocked(api.setWatchlist).mockReturnValue(new Promise(() => {}))

    renderScreen(<Collection />, at('favorites'))

    const button = await screen.findByRole('button', { name: 'Add Solaris to watchlist' })
    await userEvent.click(button)

    expect(button).toHaveAccessibleName('Remove Solaris from watchlist')
    expect(button).toHaveAttribute('aria-pressed', 'true')
  })

  it('sends the state it wants instead of a toggle', async () => {
    vi.mocked(api.collection).mockResolvedValue(aCollection())
    vi.mocked(api.setWatchlist).mockResolvedValue({ movie_id: 'solaris', on_watchlist: true })

    renderScreen(<Collection />, at('favorites'))
    await userEvent.click(await screen.findByRole('button', { name: /Add Solaris/ }))

    // The second argument is what makes a double click harmless.
    expect(api.setWatchlist).toHaveBeenCalledWith('solaris', true)
  })

  it('reverts the button and reports the failure when the write fails', async () => {
    vi.mocked(api.collection).mockResolvedValue(aCollection())
    vi.mocked(api.setWatchlist).mockRejectedValue(
      new ApiError('POST /api/movies/solaris/watchlist failed: 503 Service Unavailable', 503),
    )

    renderScreen(<Collection />, at('favorites'))

    const button = await screen.findByRole('button', { name: 'Add Solaris to watchlist' })
    await userEvent.click(button)

    // The revert and the notice are one behaviour: a button that springs back silently
    // reads as a click that never landed.
    await waitFor(() => expect(button).toHaveAttribute('aria-pressed', 'false'))
    expect(button).toHaveAccessibleName('Add Solaris to watchlist')
    expect(await screen.findByRole('status')).toHaveTextContent('503 Service Unavailable')
  })

  it('offers a way out of an empty watchlist', async () => {
    vi.mocked(api.collection).mockResolvedValue(
      aCollection({ slug: 'watchlist', title: 'Your Watchlist', movies: [] }),
    )

    renderScreen(<Collection />, at('watchlist'))

    expect(await screen.findByText(/over any poster adds one/)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Find something to watch/ })).toHaveAttribute(
      'href',
      '/search',
    )
  })

  it('treats a 404 as an empty address, not a dead API', async () => {
    vi.mocked(api.collection).mockRejectedValue(
      new ApiError('GET /api/collections/mixtapes failed: 404 Not Found', 404),
    )

    renderScreen(<Collection />, at('mixtapes'))

    expect(await screen.findByText('No such collection.')).toBeInTheDocument()
    // The other branch would tell you to restart a server that is running.
    expect(screen.queryByText("Couldn't reach the API.")).not.toBeInTheDocument()
  })
})
