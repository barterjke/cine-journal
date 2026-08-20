/**
 * What a screen says when its request failed.
 *
 * This used to print the API's raw message and then tell the reader to start a
 * server from a shell. Nobody visiting the site can act on either, so these check
 * the copy is written for a visitor — and that the technical half is still on the
 * page, in the `title`, where we can read it and they can't trip over it.
 *
 * `FORBIDDEN` is the assertion that stops the regression. Anything a developer
 * would recognise and a visitor wouldn't belongs in that pattern.
 */
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { ApiError, NothingYetError, api } from '../api'
import { MovieDetail } from '../pages/MovieDetail'
import { Review } from '../pages/Review'
import { aFilm, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

const film = { path: '/movie/:id', at: '/movie/project-kepler' }

/** Words and shapes that mean nothing to a visitor: tools, hosts, ports. */
const FORBIDDEN = /cargo|localhost|127\.0\.0\.1|\bnpm\b|\bvite\b|:\d{2,5}\b|\b(?:3001|5173)\b/i

/** The reviews rail is a second request, and no test here is about it. */
function noReviews() {
  vi.mocked(api.movieReviews).mockResolvedValue([])
}

// Call counts, not implementations. One test below counts how many times the retry
// asked, and the stub is shared by the whole file.
beforeEach(() => {
  vi.clearAllMocks()
})

describe('a film that is not in the catalogue', () => {
  it('says so, rather than blaming the connection', async () => {
    vi.mocked(api.movie).mockRejectedValue(
      new ApiError('GET /api/movies/project-kepler failed: 404 Not Found', 404),
    )
    noReviews()

    renderScreen(<MovieDetail />, film)

    expect(await screen.findByText(/isn't in our catalogue/)).toBeInTheDocument()
    // The other branch would send someone to retry a request that will answer the same.
    expect(screen.queryByText(/trouble connecting/)).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument()
    // A dead end still needs a way onward.
    expect(screen.getByRole('link', { name: 'Back to the feed' })).toHaveAttribute('href', '/')
  })

  it('shows no shell command and no developer wording at all', async () => {
    vi.mocked(api.movie).mockRejectedValue(
      new ApiError('GET /api/movies/project-kepler failed: 404 Not Found', 404),
    )
    noReviews()

    const { container } = renderScreen(<MovieDetail />, film)

    await screen.findByText(/isn't in our catalogue/)
    expect(container.textContent).not.toMatch(FORBIDDEN)
    // The old copy verbatim, and the `<code>` element it lived in.
    expect(screen.queryByText(/Start it with/)).not.toBeInTheDocument()
    expect(container.querySelector('code')).toBeNull()
  })
})

describe('a request that never reached the server', () => {
  it('offers a retry and runs the request again when it is pressed', async () => {
    vi.mocked(api.movie)
      .mockRejectedValueOnce(new TypeError('Failed to fetch'))
      .mockResolvedValue(aFilm())
    noReviews()

    renderScreen(<MovieDetail />, film)

    expect(await screen.findByText('Something went wrong')).toBeInTheDocument()
    expect(screen.getByText(/trouble connecting/)).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: 'Try again' }))

    // The film itself, so the retry refetched rather than only clearing the notice.
    expect(await screen.findByRole('heading', { name: /Neon Reverie/ })).toBeInTheDocument()
    expect(api.movie).toHaveBeenCalledTimes(2)
  })

  it('keeps the real message in the title and out of the copy', async () => {
    vi.mocked(api.movie).mockRejectedValue(
      new ApiError('GET /api/movies/project-kepler failed: 502 Bad Gateway', 502),
    )
    noReviews()

    const { container } = renderScreen(<MovieDetail />, film)

    const heading = await screen.findByText('Something went wrong')
    expect(container.textContent).not.toMatch(FORBIDDEN)
    // Not printed at anybody, but still one hover away for us.
    expect(container.textContent).not.toContain('/api/movies/project-kepler')
    expect(heading.closest('[title]')).toHaveAttribute(
      'title',
      'GET /api/movies/project-kepler failed: 502 Bad Gateway',
    )
  })
})

describe('a site with nothing on it yet', () => {
  it('says it is empty instead of apologising for a failure', async () => {
    vi.mocked(api.reviewOrNewest).mockRejectedValue(
      new NothingYetError('Nobody has written a review yet. Yours could be the first.'),
    )

    const { container } = renderScreen(<Review />, { path: '/review', at: '/review' })

    expect(await screen.findByText('Nothing here yet')).toBeInTheDocument()
    expect(screen.getByText(/Yours could be the first/)).toBeInTheDocument()
    expect(screen.queryByText('Something went wrong')).not.toBeInTheDocument()
    // Nothing to retry: the same request would find the same empty site.
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument()
    expect(container.textContent).not.toMatch(FORBIDDEN)
  })
})
