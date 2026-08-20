/**
 * `/profile`: the four cards, and the page with nobody signed in.
 *
 * The 401 case was here first. A visitor with no account is one of the two reads the
 * API refuses, and the page used to render the refusal verbatim — "GET /api/profile
 * failed: sign in to do that". That is an answer, not a fault, so it asks them in.
 *
 * The rest is the redesign. Each case is a thing the cards say that the data doesn't
 * spell out: a title under a poster, a surname shortened to an initial, a date and a
 * like count that are both allowed to be missing, and a page that has to look
 * deliberate whether it holds one film or twenty.
 */
import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { api } from '../api'
import { Profile } from '../pages/Profile'
import {
  aFollowedPerson,
  aMovie,
  aProfile,
  aRatedFilm,
  aUser,
  anAuthFailure,
  anImage,
  renderScreen,
} from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

/** Mount the page over one profile payload, with a session behind it. */
function show(profile = aProfile()) {
  vi.mocked(api.me).mockResolvedValue(aUser())
  vi.mocked(api.profile).mockResolvedValue(profile)
  return renderScreen(<Profile />, { path: '/profile', at: '/profile' })
}

/**
 * One card, found by its heading.
 *
 * Queries are scoped to a card because the same film can appear in two of them —
 * a favourite you have also reviewed — and an unscoped `getByText` would then be
 * ambiguous for reasons that have nothing to do with the case.
 */
async function card(heading: string): Promise<HTMLElement> {
  const title = await screen.findByRole('heading', { name: heading })
  const section = title.closest('section')
  if (section === null) throw new Error(`"${heading}" is not inside a card`)
  return section
}

/**
 * Every address a card links to.
 *
 * For the cases about where a row goes, which are as much about the link that should
 * *not* be there — the whole bug was a review row leading to the film instead.
 */
function hrefs(root: HTMLElement): (string | null)[] {
  return [...root.querySelectorAll('a')].map((link) => link.getAttribute('href'))
}

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

describe('the favourites card', () => {
  it('names every poster under it', async () => {
    // The strip this replaces drew four posters and no titles at all, so the only way
    // to tell which film was which was to hover one.
    show(
      aProfile({
        favorites: [
          aMovie(),
          aMovie({ id: 'stalker', title: 'Stalker', poster: anImage('Stalker poster') }),
        ],
        recent_reviews: [],
      }),
    )

    const favourites = await card('Favorite Films')
    expect(within(favourites).getByText('Solaris')).toBeInTheDocument()
    expect(within(favourites).getByText('Stalker')).toBeInTheDocument()
    // Each title belongs to the poster above it, so both go to the same film.
    expect(within(favourites).getByRole('link', { name: /Solaris/ })).toHaveAttribute(
      'href',
      '/movie/solaris',
    )
  })

  it('keeps the way to the whole list, as the chevron used to', async () => {
    show()

    const favourites = await card('Favorite Films')
    expect(within(favourites).getByRole('link', { name: 'View all' })).toHaveAttribute(
      'href',
      '/collections/favorites',
    )
  })
})

describe('the following card', () => {
  it('abbreviates a surname to an initial', async () => {
    show(aProfile({ following: [aFollowedPerson({ name: 'Sarah Jennings' })] }))

    const following = await card('Following')
    expect(within(following).getByText('Sarah J.')).toBeInTheDocument()
    // The chip is small; the full name is still there to hover.
    expect(within(following).getByTitle('Sarah Jennings')).toBeInTheDocument()
  })

  it('leaves a one-word name alone rather than giving it a stray dot', async () => {
    show(aProfile({ following: [aFollowedPerson({ name: 'Prince' })] }))

    const following = await card('Following')
    expect(within(following).getByText('Prince')).toBeInTheDocument()
    expect(within(following).queryByText(/Prince\s*\./)).not.toBeInTheDocument()
  })
})

describe('a row in the recent reviews card', () => {
  it('carries its date and how many people liked it', async () => {
    show(
      aProfile({
        recent_reviews: [aRatedFilm({ written_on: 'Oct 12', like_count: 3 })],
      }),
    )

    const reviews = await card('Recent Reviews')
    expect(within(reviews).getByText('Oct 12')).toBeInTheDocument()
    expect(within(reviews).getByText('3')).toBeInTheDocument()
    // The heart is decorative, so the count reads as a count without it.
    expect(within(reviews).getByText('likes')).toBeInTheDocument()
    expect(within(reviews).getByAltText('Mirror poster')).toBeInTheDocument()
  })

  it('draws nothing at all for the three fields the API may omit', async () => {
    // No artwork, no stored date, nobody has liked it. Every one of those is normal.
    show(
      aProfile({
        recent_reviews: [
          aRatedFilm({ poster: null, written_on: null, like_count: null }),
        ],
      }),
    )

    const reviews = await card('Recent Reviews')
    // What the visitor wrote is the point of the row, and it is still here.
    expect(within(reviews).getByText('Mirror')).toBeInTheDocument()
    expect(
      within(reviews).getByText('A cold film that stays warm in the memory.'),
    ).toBeInTheDocument()
    // Not "0 likes" and not a heart with nothing beside it.
    expect(within(reviews).queryByText('0')).not.toBeInTheDocument()
    expect(within(reviews).queryByText('favorite')).not.toBeInTheDocument()
    expect(within(reviews).queryByText('likes')).not.toBeInTheDocument()
    // The shared placeholder, not an empty frame and not an `<img>` with no file
    // behind it — a missing poster looks the same here as everywhere else.
    expect(within(reviews).getByRole('img', { name: 'No poster available' })).toBeInTheDocument()
    expect(reviews.querySelectorAll('img')).toHaveLength(0)
  })
})

/**
 * Where a journal row goes.
 *
 * It used to go to the film, both from the thumbnail and from the title, which left
 * your own entries as the one thing on the site you could not open — no full text, no
 * like, no reply. The review page already did all three; nothing linked to it. Every
 * row goes there now, a rating with no words included.
 */
describe('the links on a recent review row', () => {
  it('opens the review, where the whole text and the replies are', async () => {
    show(aProfile({ recent_reviews: [aRatedFilm({ review_id: 'me-mirror' })] }))

    const reviews = await card('Recent Reviews')
    expect(within(reviews).getByRole('link', { name: 'Mirror' })).toHaveAttribute(
      'href',
      '/review/me-mirror',
    )
    // The bug, stated: no link on the row leads to the film any more.
    expect(hrefs(reviews)).not.toContain('/movie/mirror')
  })

  it('opens the review for a bare score too, not the film', async () => {
    show(
      aProfile({
        recent_reviews: [
          aRatedFilm({
            review_id: 'me-mirror',
            body: null,
            blurb: 'A man sifts through his own memory.',
          }),
        ],
      }),
    )

    const reviews = await card('Recent Reviews')
    // A rating with no words is still a review: friends can like it and reply to it,
    // so it has the same page to open. Only the row's excerpt falls back to the
    // film's own sentence.
    expect(within(reviews).getByRole('link', { name: 'Mirror' })).toHaveAttribute(
      'href',
      '/review/me-mirror',
    )
    expect(hrefs(reviews)).not.toContain('/movie/mirror')
  })

  it('links the poster and the title separately, not the row as one anchor', async () => {
    show()

    const reviews = await card('Recent Reviews')
    const poster = within(reviews).getByAltText('Mirror poster').closest('a')
    const title = within(reviews).getByRole('link', { name: 'Mirror' })

    // `Person` wrapped a whole row in one `<a>`, and the posters inside it stopped
    // being links at all — an `<a>` inside an `<a>` is invalid HTML the browser
    // un-nests. Two anchors, neither inside the other.
    expect(poster).toHaveAttribute('href', '/review/me-mirror')
    expect(title).toHaveAttribute('href', '/review/me-mirror')
    expect(poster?.contains(title)).toBe(false)
    expect(title.contains(poster)).toBe(false)
  })

  it('leaves the stars, the date and the like count outside the link', async () => {
    show(aProfile({ recent_reviews: [aRatedFilm({ written_on: 'Oct 12', like_count: 3 })] }))

    const reviews = await card('Recent Reviews')
    // A date is not somewhere to go, and neither is a count or a rating.
    expect(within(reviews).getByText('Oct 12').closest('a')).toBeNull()
    expect(within(reviews).getByText('3').closest('a')).toBeNull()
    expect(within(reviews).getByText('star_half').closest('a')).toBeNull()
  })
})

/**
 * Any class that would pin a card's height instead of letting its content set it.
 *
 * The mock's flaw was cards stretched tall over a few lines of content. There is no
 * layout in jsdom to measure, so the check is on the classes that would cause it:
 * a minimum height anywhere, or a grid that stretches its items to the row.
 */
function heightPinningClasses(root: HTMLElement): string[] {
  return [...root.querySelectorAll('*')]
    .flatMap((element) => [...element.classList])
    .filter(
      (name) =>
        name.includes('min-h-') || name === 'items-stretch' || name === 'self-stretch',
    )
}

describe('a nearly-empty profile', () => {
  it('leaves no card stretched tall over nothing', async () => {
    show(aProfile({ favorites: [aMovie()], watchlist: [], recent_reviews: [], following: [] }))

    const main = await screen.findByRole('main')
    const grid = main.querySelector('.grid')

    // `items-start` is the fix: without it a grid item is as tall as the tallest card
    // in its row, so one favourite beside a full Following list gets a few hundred
    // pixels of nothing under it.
    expect(grid).toHaveClass('items-start')
    expect(heightPinningClasses(main)).toEqual([])

    // The three empty cards say what fills them instead of standing blank.
    expect(screen.getByText(/the "\+" over any poster adds one/)).toBeInTheDocument()
    expect(screen.getByText(/Rate or review a film/)).toBeInTheDocument()
    expect(screen.getByText(/find people on Friends/)).toBeInTheDocument()
  })

  it('still gives each card its full column, rather than shrinking to fit', async () => {
    // The horizontal slack beside one poster is expected — a real account fills it.
    // Sizing a card to its contents would make the page rearrange itself as you use it.
    show(aProfile({ favorites: [aMovie()], watchlist: [], recent_reviews: [], following: [] }))

    const favourites = await card('Favorite Films')
    expect(favourites.parentElement).toHaveClass('md:col-span-3')
    expect(favourites).not.toHaveClass('w-fit')
  })
})

describe('the header card', () => {
  it('opens the bio editor from "Edit" and saves what you write', async () => {
    vi.mocked(api.setBio).mockResolvedValue({ bio: 'Watches too much Tarkovsky.' })
    show()

    await userEvent.click(await screen.findByRole('button', { name: 'Edit' }))

    const field = screen.getByLabelText('Your bio')
    await userEvent.clear(field)
    await userEvent.type(field, 'Watches too much Tarkovsky.')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(api.setBio).toHaveBeenCalledWith('Watches too much Tarkovsky.')
    // The line the server sends back, not the draft — an empty box restores the
    // account's default and only the server knows what that is.
    expect(await screen.findByText('Watches too much Tarkovsky.')).toBeInTheDocument()
    expect(screen.queryByLabelText('Your bio')).not.toBeInTheDocument()
  })

  it('copies the account\'s public address from "Share", not /profile', async () => {
    // jsdom has no clipboard, which is also what an insecure origin looks like — so
    // it is defined here rather than replacing the whole `navigator`.
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    show()

    await userEvent.click(await screen.findByRole('button', { name: 'Share' }))

    // `/profile` is whoever is signed in, so pasting it to somebody else shows them
    // their own page. Their handle's page says the same thing to every reader.
    expect(writeText).toHaveBeenCalledWith(`${window.location.origin}/people/sam`)
    // The label reports what happened. A button that looks inert is worse.
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument()
  })
})
