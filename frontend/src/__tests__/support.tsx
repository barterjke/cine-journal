/**
 * Test helpers: one way to mount a screen, and one fixture per payload shape.
 *
 * Every screen reads its route and renders `TopAppBar`, so all of them need a router
 * above them. `renderScreen` is the only way in.
 *
 * Fixtures return a whole payload with an overrides argument, so a test spells out only
 * the fields it is about. The defaults are the boring case.
 */
import { render } from '@testing-library/react'
import type { ReactElement } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { vi } from 'vitest'

import type {
  Collection,
  Comment,
  Image,
  Movie,
  MovieDetail,
  PeopleResponse,
  PersonCard,
  Reply,
  Review,
  SearchResponse,
  SearchResult,
  User,
} from '../api'
import { ApiError, api } from '../api'
import { resetAuth } from '../useAuth'

/**
 * Mount one screen at one address. `path` is the route pattern, because that is what
 * `useParams` reads; `at` is the URL.
 *
 * Also answers `api.status`, which every screen's `DemoBanner` calls. "tmdb" keeps the
 * banner off the page.
 *
 * Auth is shared in a module, so it is forgotten here instead of carried from the
 * last case into this one. `api.me` is left to the test. Unstubbed it rejects, and
 * the chrome reads any failure as "nobody is signed in" — the right default for a
 * case that isn't about accounts.
 */
export function renderScreen(element: ReactElement, { path, at }: { path: string; at: string }) {
  resetAuth()

  vi.mocked(api.status).mockResolvedValue({
    data_source: 'tmdb',
    message: null,
    docs_url: 'https://example.invalid/tmdb',
  })

  return render(
    <MemoryRouter initialEntries={[at]}>
      <Routes>
        <Route path={path} element={element} />
      </Routes>
    </MemoryRouter>,
  )
}

/** A poster or avatar. jsdom never loads `src`, so `alt` is all a test can see. */
export function anImage(alt: string): Image {
  return { src: `/img/${alt.toLowerCase().replace(/\W+/g, '-')}.jpg`, alt }
}

/** Whoever `GET /api/auth/me` says is signed in. */
export function aUser(overrides: Partial<User> = {}): User {
  return {
    id: 'me',
    name: 'Sam Reyes',
    handle: '@sam',
    avatar: anImage('Portrait of Sam'),
    ...overrides,
  }
}

/** The one 401 the API answers with, for a read or a write. */
export function anAuthFailure(method: string, path: string): ApiError {
  return new ApiError(`${method} ${path} failed: sign in to do that`, 401)
}

export function aMovie(overrides: Partial<Movie> = {}): Movie {
  return {
    id: 'solaris',
    title: 'Solaris',
    year: 1972,
    poster: anImage('Solaris poster'),
    ...overrides,
  }
}

/** One film, not on the watchlist. `owner: null` means it is the visitor's own. */
export function aCollection(overrides: Partial<Collection> = {}): Collection {
  return {
    slug: 'favorites',
    title: 'Your Favourites',
    description: 'The ones you press the heart on.',
    owner: null,
    movies: [{ movie: aMovie(), rating_half_stars: null, on_watchlist: false }],
    ...overrides,
  }
}

/**
 * A film with nothing set on it: unrated, unreviewed, not favourited, not logged.
 *
 * No trailers and no stills, so the Media carousel stays hidden. It drops any slide
 * whose thumbnail comes back too small, and jsdom loads no images at all.
 */
export function aFilm(overrides: Partial<MovieDetail> = {}): MovieDetail {
  return {
    id: 'neon-reverie',
    title: 'Neon Reverie',
    year: 2024,
    certification: 'R',
    runtime: '1h 58m',
    genres: ['Sci-Fi', 'Drama'],
    poster: anImage('Neon Reverie poster'),
    backdrop: anImage('Neon Reverie backdrop'),
    synopsis: 'A courier runs a dead city’s last errand.',
    score: 7.8,
    vote_count: 1204,
    details: [],
    trailers: [],
    stills: [],
    watch_options: [],
    watch_link: null,
    cast: [],
    on_watchlist: false,
    is_favorite: false,
    your_rating_half_stars: null,
    your_review: null,
    ...overrides,
  }
}

export function aSearchResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: 'blade-runner',
    title: 'Blade Runner',
    year: 1982,
    star_rating: 4.5,
    poster: anImage('Blade Runner poster'),
    genres: ['Sci-Fi'],
    on_watchlist: false,
    ...overrides,
  }
}

/**
 * One page of results, plus the facets the sidebar draws. `query` is the term the
 * server echoed back, which is the one the screen prints.
 */
export function aSearchResponse(overrides: Partial<SearchResponse> = {}): SearchResponse {
  return {
    query: '',
    total_results: 1,
    results: [aSearchResult()],
    filters: {
      genres: [{ label: 'Sci-Fi', selected: false, count: 1 }],
      years: [{ label: '1980s', selected: false, count: 1 }],
      minimum_rating_stars: 0,
    },
    page: 1,
    page_count: 1,
    person: null,
    ...overrides,
  }
}

/**
 * Someone you don't follow yet. The avatar's `alt` is not the name on purpose: a row
 * renders the name twice, as a portrait link and as text, and one test counts those.
 */
export function aPerson(overrides: Partial<PersonCard> = {}): PersonCard {
  return {
    id: '1136406',
    name: 'Elena Vasquez',
    handle: '@elena',
    avatar: anImage('Portrait'),
    bio: null,
    following: false,
    follows_you: false,
    review_count: 5,
    ...overrides,
  }
}

/** All three lists in one answer, as the screen gets them. */
export function aPeopleResponse(overrides: Partial<PeopleResponse> = {}): PeopleResponse {
  return { query: '', results: [], following: [], followers: [], ...overrides }
}

/**
 * One reply, by somebody other than the viewer.
 *
 * `is_you` defaults to false, and `author_name` is the real name either way — the
 * server never sends "You", so a test for that word overrides the flag, not the name.
 */
export function aReply(overrides: Partial<Reply> = {}): Reply {
  return {
    id: 'reply-1',
    author_id: 'account-1002',
    author_name: 'Theo Marchetti',
    author_handle: '@theo',
    author_avatar: anImage('Portrait of Theo'),
    is_you: false,
    timestamp: 'August 19, 2026',
    body: 'The ending gets me every time.',
    ...overrides,
  }
}

/** One comment by somebody else: nobody has liked it, and it has no replies. */
export function aComment(overrides: Partial<Comment> = {}): Comment {
  return {
    id: 'comment-1',
    author_id: 'account-1001',
    author_name: 'Nadia Halim',
    author_handle: '@nadia',
    author_avatar: anImage('Portrait of Nadia'),
    is_you: false,
    timestamp: 'August 20, 2026',
    body: 'Fifty years on and it still lands.',
    like_count: null,
    replies: [],
    liked: false,
    ...overrides,
  }
}

/** One review in full, scored, with an empty thread. */
export function aReview(overrides: Partial<Review> = {}): Review {
  return {
    id: 'elena-solaris',
    movie: aMovie(),
    backdrop: null,
    director: 'Andrei Tarkovsky',
    genres: ['Sci-Fi'],
    author_id: '1136406',
    author_name: 'Elena Vasquez',
    author_handle: '@elena',
    author_avatar: anImage('Portrait of Elena'),
    author_followed: false,
    watched_on: 'Reviewed on March 15, 2024',
    rating_half_stars: 9,
    paragraphs: ['A cold film that stays warm in the memory.'],
    like_count: null,
    comments: [],
    liked: false,
    ...overrides,
  }
}
