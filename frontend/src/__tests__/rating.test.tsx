/**
 * Rating a film and writing a review, on the film's page.
 *
 * Ratings travel as half-star counts, so most of what matters is that the star clicked
 * becomes the right number. The rating and the review are separate writes and are tested
 * separately: the stars flip optimistically, the review waits behind "Saving…".
 *
 * The pills' accessible names include their icon glyph ("star Rate"), because the icon
 * font renders as text. Matched on the end of the name so a swapped glyph doesn't fail a
 * rating assertion.
 */
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { api } from '../api'
import { MovieDetail } from '../pages/MovieDetail'
import { aFilm, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

const film = { path: '/movie/:id', at: '/movie/neon-reverie' }

/** The reviews rail below the film is a second request, and no test here is about it. */
function noReviews() {
  vi.mocked(api.movieReviews).mockResolvedValue([])
}

describe('rating a film', () => {
  it('sends four stars as eight half-stars and shows the score on the pill', async () => {
    vi.mocked(api.movie).mockResolvedValue(aFilm())
    vi.mocked(api.rate).mockResolvedValue({
      movie_id: 'neon-reverie',
      your_rating_half_stars: 8,
    })
    noReviews()

    renderScreen(<MovieDetail />, film)

    // The picker is behind the pill and isn't on the page until it is pressed.
    const pill = await screen.findByRole('button', { name: /Rate$/ })
    expect(screen.queryByRole('group', { name: 'Your rating' })).not.toBeInTheDocument()
    await userEvent.click(pill)

    await userEvent.click(screen.getByRole('button', { name: 'Rate 4 out of 5' }))

    expect(api.rate).toHaveBeenCalledWith('neon-reverie', 8)
    await waitFor(() => expect(pill).toHaveAccessibleName(/4 \/ 5$/))
  })

  it('clears the rating when the star already set is clicked again', async () => {
    vi.mocked(api.movie).mockResolvedValue(aFilm({ your_rating_half_stars: 8 }))
    vi.mocked(api.rate).mockResolvedValue({
      movie_id: 'neon-reverie',
      your_rating_half_stars: null,
    })
    noReviews()

    renderScreen(<MovieDetail />, film)

    const pill = await screen.findByRole('button', { name: /4 \/ 5$/ })
    await userEvent.click(pill)
    await userEvent.click(screen.getByRole('button', { name: 'Rate 4 out of 5' }))

    // `0` means unrated, and this is the only control that can get back there.
    expect(api.rate).toHaveBeenCalledWith('neon-reverie', 0)
    await waitFor(() => expect(pill).toHaveAccessibleName(/Rate$/))
  })
})

describe('writing a review', () => {
  it('posts the typed review and then offers to update it', async () => {
    vi.mocked(api.movie).mockResolvedValue(aFilm())
    vi.mocked(api.writeReview).mockResolvedValue({
      movie_id: 'neon-reverie',
      your_review: 'Gorgeous, and colder than it looks.',
    })
    noReviews()

    renderScreen(<MovieDetail />, film)

    await userEvent.click(await screen.findByRole('button', { name: /Write a review$/ }))

    // Nothing typed yet. An empty body would mean "delete my review".
    const post = screen.getByRole('button', { name: 'Post review' })
    expect(post).toBeDisabled()

    await userEvent.type(
      screen.getByPlaceholderText('What did you make of it?'),
      'Gorgeous, and colder than it looks.',
    )
    expect(post).toBeEnabled()
    await userEvent.click(post)

    expect(api.writeReview).toHaveBeenCalledWith(
      'neon-reverie',
      'Gorgeous, and colder than it looks.',
    )
    // A film only ever holds one review of yours, so both labels have to change.
    expect(await screen.findByRole('button', { name: 'Update review' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Your review$/ })).toBeInTheDocument()
  })

  it('shows Delete only when a review is stored', async () => {
    vi.mocked(api.movie).mockResolvedValue(aFilm({ your_review: 'Watched it twice.' }))
    noReviews()

    renderScreen(<MovieDetail />, film)

    await userEvent.click(await screen.findByRole('button', { name: /Your review$/ }))
    // The stored text is the draft's starting point.
    expect(screen.getByPlaceholderText('What did you make of it?')).toHaveValue('Watched it twice.')
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument()
  })
})
