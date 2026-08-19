/**
 * Searching for a film.
 *
 * The screen keeps only the draft text locally; the query and filters live in the URL and
 * the request is derived from them. So the assertions check what `api.search` was handed,
 * plus what got drawn.
 *
 * Real timers: typing is debounced by 250ms and Testing Library's polling covers that,
 * while fake timers would have to be wired to `userEvent` for no gain.
 */
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import type { SearchParams } from '../api'
import { api } from '../api'
import { Search } from '../pages/Search'
import { aSearchResponse, aSearchResult, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

describe('searching for films', () => {
  it('asks the API for the typed text and draws the results', async () => {
    // Answers per request, so the grid before and after the search can't be the same one.
    vi.mocked(api.search).mockImplementation((params: SearchParams = {}) =>
      Promise.resolve(
        params.q
          ? aSearchResponse({ query: 'Blade Runner', results: [aSearchResult()] })
          : aSearchResponse({ query: '', total_results: 0, results: [] }),
      ),
    )

    renderScreen(<Search />, { path: '/search', at: '/search' })

    expect(await screen.findByText('No films match.')).toBeInTheDocument()

    await userEvent.type(screen.getByPlaceholderText('Title or genre…'), 'blade')

    expect(await screen.findByRole('heading', { name: 'Blade Runner' })).toBeInTheDocument()
    expect(api.search).toHaveBeenCalledWith(expect.objectContaining({ q: 'blade' }))
    // The count and the term come from the response, not from the box.
    expect(screen.getByText('Showing 1 result for "Blade Runner"')).toBeInTheDocument()
  })

  it('filters by a genre chip without waiting for the typing pause', async () => {
    vi.mocked(api.search).mockResolvedValue(
      aSearchResponse({
        filters: {
          genres: [{ label: 'Sci-Fi', selected: false, count: 1 }],
          years: [],
          minimum_rating_stars: 0,
        },
      }),
    )

    renderScreen(<Search />, { path: '/search', at: '/search' })

    // The chip prints its match count beside the label, so the count is part of its name.
    await userEvent.click(await screen.findByRole('button', { name: 'Sci-Fi 1' }))

    expect(api.search).toHaveBeenCalledWith(expect.objectContaining({ genre: 'Sci-Fi' }))
  })

  it('says when a person filter matched nobody, rather than blaming the other filters', async () => {
    // A `null` person with the filter still in the URL means the id named nobody.
    vi.mocked(api.search).mockResolvedValue(
      aSearchResponse({ total_results: 0, results: [], person: null }),
    )

    renderScreen(<Search />, { path: '/search', at: '/search?person=999999999' })

    expect(await screen.findByText('No films match.')).toBeInTheDocument()
    expect(
      screen.getByText('That person is not on file. Remove the filter to search everything.'),
    ).toBeInTheDocument()
    // The pill is keyed off the URL, so it is still there to press.
    expect(screen.getByRole('button', { name: /Remove the .* filter/ })).toBeInTheDocument()
  })
})
