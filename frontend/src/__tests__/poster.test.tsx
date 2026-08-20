/**
 * What a film with no poster looks like.
 *
 * A missing poster is normal, so the frame that stands in for one has to be the same
 * on every screen — and it must not be artwork. The bug this covers showed a real
 * demo film's poster in place of the missing one, which credits that art to the
 * wrong film.
 *
 * Two screens, because they used to disagree: the search grid drew its own tile and a
 * review card drew nothing at all.
 */
import { screen } from '@testing-library/react'

import { api } from '../api'
import { Person } from '../pages/Person'
import { Search } from '../pages/Search'
import {
  aPersonProfile,
  aSearchResponse,
  aSearchResult,
  aUserReview,
  renderScreen,
} from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

/**
 * Every `<img>` the screen drew.
 *
 * The placeholder must not be one of these: an `<img>` here means a file was
 * fetched, and the only files this app has are real films' posters.
 */
function imageSources(container: HTMLElement): string[] {
  return [...container.querySelectorAll('img')].map((img) => img.getAttribute('src') ?? '')
}

describe('a film with no poster', () => {
  it("draws the shared frame in the search grid, not a real film's artwork", async () => {
    vi.mocked(api.search).mockResolvedValue(
      aSearchResponse({ results: [aSearchResult({ poster: null })] }),
    )

    const { container } = renderScreen(<Search />, { path: '/search', at: '/search' })

    const frame = await screen.findByRole('img', { name: 'No poster available' })
    // Drawn, not fetched. An `<img>` would be some other film's poster file.
    expect(frame.tagName).not.toBe('IMG')
    expect(imageSources(container).some((src) => src.includes('poster'))).toBe(false)
  })

  it('draws the same frame on a review card, which used to draw nothing', async () => {
    vi.mocked(api.person).mockResolvedValue(
      aPersonProfile({ reviews: [aUserReview({ poster: null })] }),
    )

    const { container } = renderScreen(<Person />, {
      path: '/people/:handle',
      at: '/people/elena',
    })

    const frame = await screen.findByRole('img', { name: 'No poster available' })
    expect(frame.tagName).not.toBe('IMG')
    expect(imageSources(container).some((src) => src.includes('poster'))).toBe(false)
  })

  it("draws the film's own poster when there is one", async () => {
    vi.mocked(api.search).mockResolvedValue(aSearchResponse())

    renderScreen(<Search />, { path: '/search', at: '/search' })

    expect(await screen.findByRole('img', { name: 'Blade Runner poster' })).toBeInTheDocument()
    expect(screen.queryByRole('img', { name: 'No poster available' })).not.toBeInTheDocument()
  })

  it("replaces the API's own stand-in file with the same frame", async () => {
    // The API sends this path for a film it has no poster for. Recognised so there is
    // one missing-poster treatment rather than two.
    vi.mocked(api.search).mockResolvedValue(
      aSearchResponse({
        results: [
          aSearchResult({ poster: { src: 'img/poster-missing.svg', alt: 'No poster.' } }),
        ],
      }),
    )

    const { container } = renderScreen(<Search />, { path: '/search', at: '/search' })

    expect(await screen.findByRole('img', { name: 'No poster available' })).toBeInTheDocument()
    expect(imageSources(container).some((src) => src.includes('poster'))).toBe(false)
  })
})
