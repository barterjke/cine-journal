/**
 * Typed client for the Rust API. These types mirror `backend/src/models.rs`
 * field-for-field — if you change one side, change the other.
 *
 * Ratings arrive as half-star counts (0..=10) rather than floats, matching the
 * discrete full/half/empty glyphs the design uses. See `StarRating`.
 */

export interface Image {
  src: string
  alt: string
}

export interface Movie {
  id: string
  title: string
  year: number | null
  poster: Image
}

export interface FeedEntry {
  id: string
  movie: Movie
  rating_half_stars: number
  on_watchlist: boolean
}

/**
 * One suggested film, with the film of the visitor's own that prompted it.
 *
 * `because` is what separates this from a shelf of posters: the card says "because
 * you liked Interstellar", and `because_movie_id` links to that film.
 */
export interface Recommendation {
  movie: Movie
  /** 0.0–5.0 crowd average, `null` for a film nobody has voted on. */
  star_rating: number | null
  because: string
  because_movie_id: string
  /**
   * Whether the seed was a favourite, or only on the watchlist.
   *
   * Picks the verb: "because you liked X" is false about a film the visitor has
   * bookmarked and may not have watched, and both kinds of seed reach this rail.
   */
  because_favorite: boolean
  on_watchlist: boolean
}

/**
 * A circle in the mobile feed's stories rail: one person the visitor follows.
 *
 * `review_id` is what makes it a link — tapping opens their newest review. `null`
 * for someone who hasn't written anything, drawn dimmed and unlinked rather than
 * dropped, since who you follow is a fact whether or not they've posted.
 */
export interface Story {
  id: string
  name: string
  avatar: Image
  review_id: string | null
  handle: string
  /** Whether they have something to show; drives the ring. */
  unseen: boolean
}

export interface MobileFeedItem {
  id: string
  movie: Movie
  /**
   * "Elena rated it", "Elena reviewed it" for a review with no score, or "Because you
   * liked Interstellar". Printed as-is.
   */
  subtitle: string
  /** The author's stars where this is somebody's review; `null` for a suggestion. */
  rating_half_stars: number | null
  /** The review this card opens. `null` for a suggestion, which opens the film. */
  review_id: string | null
  on_watchlist: boolean
}

/**
 * One reply under a comment. Same author fields as `Comment`, and nothing else:
 * replies carry no likes and no replies of their own.
 */
export interface Reply {
  id: string
  /** What the follow button posts to. */
  author_id: string
  /** Always the real name. Print "You" from `is_you` instead — never from here. */
  author_name: string
  /** "@sam". `personPath` drops the sigil. */
  author_handle: string
  author_avatar: Image
  /**
   * Whether the viewer wrote it.
   *
   * The one thing the client can't work out: the session cookie is HttpOnly, so the
   * browser never learns its own account id.
   */
  is_you: boolean
  /** Pre-formatted, e.g. "August 20, 2026". */
  timestamp: string
  body: string
}

/**
 * One comment on a review, with its replies.
 *
 * Shared content, not a per-viewer list. Everybody sees the same thread with real
 * authors on it, so the author fields are always usable — the avatar and handle link
 * to a page even on the viewer's own row.
 */
export interface Comment {
  id: string
  author_id: string
  /** Always the real name, as on `Reply`. */
  author_name: string
  author_handle: string
  author_avatar: Image
  is_you: boolean
  /** Pre-formatted, e.g. "August 20, 2026". */
  timestamp: string
  body: string
  /**
   * How many people liked it, or `null` for none.
   *
   * A shared total with the viewer's own like already in it. Render it as-is; adding
   * `liked` on top would count that person twice.
   */
  like_count: number | null
  /** Oldest first, as the thread draws them. */
  replies: Reply[]
  /** Whether *the viewer* liked it. */
  liked: boolean
}

/**
 * One review in full, plus its conversation.
 *
 * The expanded form of `UserReview` — same row, same id, same author. A card in a
 * list links to `/review/{id}` and this is what that page reads.
 */
export interface Review {
  /** `<person_id>-<movie_id>`, the id `UserReview` carries. */
  id: string
  movie: Movie
  backdrop: Image | null
  director: string | null
  genres: string[]
  author_id: string
  author_name: string
  author_handle: string
  author_avatar: Image
  author_followed: boolean
  watched_on: string
  /** `null` for prose written without a score — see `UserReview`. Draws no stars. */
  rating_half_stars: number | null
  paragraphs: string[]
  like_count: number | null
  comments: Comment[]
  liked: boolean
}

export interface CastMember {
  id: string
  name: string
  role: string
  portrait: Image
  /**
   * Whether this id means anything to `/search?person=`.
   *
   * `false` for the demo dataset's invented cast — real names and portraits with no
   * filmography behind them — so the screen draws those names unlinked rather than
   * sending you to an empty grid.
   */
  searchable: boolean
}

/** One label/value row of the detail screen's credits grid. */
export interface DetailFact {
  label: string
  value: string
  /**
   * The people named in `value`, in the order they appear there.
   *
   * Parallel to the text rather than replacing it, because a row's value is
   * sometimes not a person: "Production" names a studio. Empty means nothing here
   * links anywhere.
   */
  people: CreditedPerson[]
}

/** One name inside a `DetailFact`, and the person `/search?person=` will filter by. */
export interface CreditedPerson {
  name: string
  id: string
}

/**
 * One video in the detail screen's Media carousel.
 *
 * `key` and `site` rather than a finished URL: the `<iframe>` src is built from
 * them here, and only YouTube is embeddable, so `site` has to be checked at the
 * point of rendering.
 */
export interface Trailer {
  name: string
  /** Site-scoped id — on YouTube, what follows `watch?v=`. */
  key: string
  /** "YouTube" or "Vimeo". */
  site: string
  /** "Trailer", "Teaser", "Clip", "Featurette", "Behind the Scenes". */
  kind: string
  /** YouTube's own frame for this video, not the film's backdrop. */
  thumbnail: Image
}

/** One frame from the film in the same carousel: a rail thumbnail and a full-size copy. */
export interface Still {
  image: Image
  /** The same frame at full resolution, for the lightbox. */
  full: Image
}

/**
 * One "Where to Watch" row. No per-row URL: TMDB's terms permit linking only to
 * their own watch page, which `MovieDetail.watch_link` carries for all of them.
 */
export interface WatchOption {
  provider: string
  /** "Stream", "Rent", "Buy" or "Free". */
  kind: string
  /** `null` for a service with no artwork upstream — drawn as a generic glyph. */
  logo: Image | null
}

export interface MovieDetail {
  id: string
  title: string
  /** `null` for an announced film with no release date; the title line omits it. */
  year: number | null
  /** "PG-13", "R". `null` where there's no rating, and the segment is omitted. */
  certification: string | null
  runtime: string
  genres: string[]
  poster: Image
  /** One wide frame. Read by the review screen's faded header, not the Media block. */
  backdrop: Image
  synopsis: string
  /** The crowd average on a 0–10 scale, printed to one decimal beside "/ 10". */
  score: number
  /** How many votes that average is over. 0 hides the attribution line. */
  vote_count: number
  details: DetailFact[]
  /** Every video the carousel offers, best first. Empty for a film with none. */
  trailers: Trailer[]
  /** Frames from the film, for the same carousel. */
  stills: Still[]
  /** Empty is normal; the section hides itself. */
  watch_options: WatchOption[]
  watch_link: string | null
  cast: CastMember[]
  on_watchlist: boolean
  /** Whether the visitor pressed the heart. Independent of the rating below it. */
  is_favorite: boolean
  /** The visitor's own rating in half-stars; `null` if they haven't rated it. */
  your_rating_half_stars: number | null
  /** What the visitor wrote about it; `null` if they haven't. Prefills the composer. */
  your_review: string | null
}

export interface SearchResult {
  id: string
  title: string
  /** `null` for an unreleased film — a filmography lists those too. */
  year: number | null
  /**
   * A fractional 0.0–5.0 crowd average, shown as a number — not glyphs.
   *
   * `null` for a film nobody has voted on: "★ 0.0" would state an average that
   * doesn't exist. The demo dataset always has one, including a real 0.0.
   */
  star_rating: number | null
  /** `null` draws the shared missing-poster frame — see `MissingPoster`. */
  poster: Image | null
  genres: string[]
  on_watchlist: boolean
}

export interface GenreFacet {
  label: string
  selected: boolean
  /** Matches this facet ignoring the current genre selection — see the backend. */
  count: number
}

export interface YearFacet {
  label: string
  selected: boolean
  count: number
}

export interface SearchFilters {
  genres: GenreFacet[]
  years: YearFacet[]
  /** Whole stars out of 5; 0 means "any". */
  minimum_rating_stars: number
}

export interface SearchResponse {
  query: string
  total_results: number
  results: SearchResult[]
  filters: SearchFilters
  page: number
  page_count: number
  /**
   * Who the `person` filter names, resolved by the backend so the screen can say
   * "Christopher Nolan" rather than echoing `525`.
   *
   * `null` when nobody is filtered on, and also when the id resolved to nobody — an
   * unknown id yields no films, and labelling that grid would be inventing a name.
   */
  person: CreditedPerson | null
}

/** What the search screen's controls hold. All optional; all default to "any". */
export interface SearchParams {
  q?: string
  genre?: string | null
  year?: string | null
  minRating?: number
  page?: number
  /**
   * A TMDB person id — everything they were credited on, acting or crewing.
   *
   * Where a cast portrait and a credits-grid name lead. It narrows to that person's
   * filmography, over which the text box and the other chips still apply.
   */
  person?: string | null
}

/**
 * One card in the feed — a discriminated union on `kind`.
 *
 * The three kinds arrive interleaved in one list rather than as three arrays, which is
 * what makes the feed a feed: the screen renders them in the order they came and never
 * decides how to merge rails.
 */
export type FeedItem =
  | ({ kind: 'review' } & UserReview)
  | ({ kind: 'recommendation' } & Recommendation)
  | ({ kind: 'entry' } & FeedEntry)

/**
 * One page of the feed.
 *
 * `next_cursor` is opaque — it encodes how far into each of the three sources the page
 * reached, and a client that parsed it would break when a fourth is added. `null` means
 * the end: stop observing and say so, rather than fetching an empty page per scroll.
 */
export interface FeedPage {
  items: FeedItem[]
  next_cursor: string | null
  /**
   * Whether this came out of Redis rather than being built for this request.
   *
   * Drives the revalidation: a page flagged `true` is last request's feed, so the screen
   * paints it and immediately asks again with `refresh`, then swaps in what comes back.
   */
  from_cache: boolean
}

/** One film in a collection grid, with whatever that collection knows about it. */
export interface CollectionMovie {
  movie: Movie
  /** The owner's rating, where the collection has ratings behind it. `null` draws no stars. */
  rating_half_stars: number | null
  /** Whether *you* have it on your watchlist — the button is yours even on their page. */
  on_watchlist: boolean
}

/** Whose collection is on screen, when it isn't yours. */
export interface CollectionOwner {
  name: string
  handle: string
  avatar: Image
}

/**
 * One named set of films, in full — the page behind a profile tile.
 *
 * The tiles are summaries capped at four and six posters; this is uncapped, which is
 * what makes a tile worth clicking. `title` and `description` arrive in the right
 * person's voice, so the screen prints them as-is.
 */
export interface Collection {
  slug: string
  title: string
  description: string
  /** `null` for your own. */
  owner: CollectionOwner | null
  movies: CollectionMovie[]
}

export interface MobileFeed {
  stories: Story[]
  items: MobileFeedItem[]
}

export interface FollowedPerson {
  id: string
  name: string
  avatar: Image
  /** A pre-formatted line, e.g. "5 films reviewed · generous ratings". */
  subtitle: string
  /**
   * `null` for a person seeded before every row here was a user — the row is drawn
   * unlinked. Otherwise their handle links to `/people/:handle`.
   */
  handle: string | null
}

/** One person in a list: enough to draw a row and its follow button. */
export interface PersonCard {
  id: string
  name: string
  handle: string
  avatar: Image
  bio: string | null
  /** Whether you follow them. */
  following: boolean
  /** Whether they follow you — the "Follows you" badge. */
  follows_you: boolean
  review_count: number
}

/** One person's review of one film. Serves both a person's page and a film's. */
export interface UserReview {
  id: string
  author_id: string
  author_name: string
  author_handle: string
  author_avatar: Image
  /** Whether you follow the author, which is also why this row sorted where it did. */
  author_followed: boolean
  movie_id: string
  movie_title: string
  /** `null` draws the shared missing-poster frame — see `MissingPoster`. */
  poster: Image | null
  /**
   * `null` for prose written without a score.
   *
   * A rating and a review are separate things here: clearing your score must not
   * delete what you wrote, so a review can exist with no number beside it.
   */
  rating_half_stars: number | null
  body: string
  /** Pre-formatted, e.g. "12 November 2014". */
  written_on: string
}

/**
 * One person's page. No follower counts: the graph only stores your own edges, so
 * any such number would be 0 or 1 — `following` and `follows_you` are the two
 * relationships that actually exist.
 */
/**
 * Someone else's page. Deliberately the same shape as `Profile` from `favorites`
 * down, so `ProfileBody` draws both and the two screens can't drift apart.
 */
export interface PersonProfile {
  id: string
  name: string
  handle: string
  avatar: Image
  bio: string | null
  following: boolean
  follows_you: boolean
  favorites: Movie[]
  watchlist: Movie[]
  reviews: UserReview[]
  review_count: number
}

/**
 * The friend directory. All three lists in one response, so the search results
 * and the Following list can't disagree about whether a follow landed.
 */
export interface PeopleResponse {
  query: string
  results: PersonCard[]
  following: PersonCard[]
  followers: PersonCard[]
}

export interface FollowState {
  person_id: string
  following: boolean
  /** How many people you follow now — the heading updates from this. */
  following_count: number
}

/** One entry in the visitor's journal: a rating, a written review, or both. */
export interface RatedFilm {
  id: string
  title: string
  /** `null` for a film they wrote about without scoring. */
  rating_half_stars: number | null
  /** What they wrote, if anything. */
  body: string | null
  /** The film's own first sentence, sent only when `body` is null. */
  blurb: string | null
}

export interface Profile {
  name: string
  handle: string
  avatar: Image
  member_since: string
  bio: string
  favorites: Movie[]
  watchlist: Movie[]
  recent_reviews: RatedFilm[]
  following: FollowedPerson[]
  following_count: number
}

export interface WatchlistState {
  movie_id: string
  on_watchlist: boolean
}

export interface FavoriteState {
  movie_id: string
  is_favorite: boolean
}

export interface RatingState {
  movie_id: string
  your_rating_half_stars: number | null
}

export interface ReviewState {
  movie_id: string
  /** The stored text, trimmed by the server; `null` once cleared. */
  your_review: string | null
}

/** The bio now on the profile — the export's line again if the field was cleared. */
export interface BioState {
  bio: string
}

export interface LikeState {
  id: string
  liked: boolean
  /** Already includes the visitor's own like, so render it as-is. */
  like_count: number | null
}

/**
 * Who is signed in, as `GET /api/auth/me` sends them.
 *
 * No email — the backend keeps it off the wire. The session cookie is HttpOnly, so
 * this request is the only way to know who you are.
 */
export interface User {
  id: string
  name: string
  /** "@sam", with the sigil, like every other handle in the API. */
  handle: string
  avatar: Image
}

/** Where the films came from. `demo` means they're invented — see `DemoBanner`. */
export type DataSource = 'tmdb' | 'demo'

export interface Status {
  data_source: DataSource
  /** Why the data is fake and what to do about it; `null` in TMDB mode. */
  message: string | null
  docs_url: string
}

/**
 * A failed request, carrying the HTTP status alongside the message.
 *
 * The status has to survive as a number: the message is deliberately the API's own
 * prose, so a screen that wants to tell "this nickname doesn't exist" apart from
 * "the server is down" can't find the code by grepping the text.
 */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * A request that worked but had nothing to show yet.
 *
 * Thrown where a screen opens on "the newest thing" and the site is still empty.
 * That is not a fault, so the error screen prints this message as-is instead of
 * apologising for a failure — which means the message has to be written for a
 * visitor.
 */
export class NothingYetError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'NothingYetError'
  }
}

/** Whether a thrown value is an API 404 — a real answer rather than a failure. */
export function isNotFound(error: unknown): boolean {
  return error instanceof ApiError && error.status === 404
}

/**
 * The half of a failure message a visitor should read.
 *
 * `request` puts the method and path in front of every error so a log line says
 * which call broke. That prefix is for us; it looks like a bug report to anyone
 * else. This drops it and leaves the server's own sentence.
 */
export function visitorMessage(message: string): string {
  return message.replace(/^[A-Z]+ \/\S+ failed: /, '')
}

/**
 * Whether a thrown value is an API 401. That means "sign in", not "something broke".
 *
 * The status is what to read, not the message. Every 401 in the API sends the same
 * sentence. `/api/profile` and `/api/watchlist` answer 401 anonymously, and so does
 * every write.
 */
export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}

/**
 * The API's own `{ error }` message, or the status line when there isn't one.
 *
 * The backend explains rejected writes ("body must not be empty"). That text is what
 * the UI shows the user.
 */
async function errorMessage(res: Response): Promise<string> {
  try {
    const payload: unknown = await res.json()
    if (payload && typeof payload === 'object' && 'error' in payload) {
      return String((payload as { error: unknown }).error)
    }
  } catch {
    // Non-JSON error body — the status line is all we have.
  }
  return `${res.status} ${res.statusText}`
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })

  if (!res.ok) {
    throw new ApiError(`${method} ${path} failed: ${await errorMessage(res)}`, res.status)
  }

  return res.json() as Promise<T>
}

/** The same, for an endpoint that answers 204 with no body to parse. */
async function requestEmpty(method: string, path: string): Promise<void> {
  const res = await fetch(path, { method })
  if (!res.ok) {
    throw new ApiError(`${method} ${path} failed: ${await errorMessage(res)}`, res.status)
  }
}

const get = <T>(path: string) => request<T>('GET', path)

/** Only sends the params that are actually set, keeping dev-tools URLs readable. */
function searchQuery({ q, genre, year, minRating, page, person }: SearchParams): string {
  const params = new URLSearchParams()
  if (q) params.set('q', q)
  if (genre) params.set('genre', genre)
  if (year) params.set('year', year)
  if (minRating) params.set('min_rating', String(minRating))
  if (page && page > 1) params.set('page', String(page))
  if (person) params.set('person', person)
  const query = params.toString()
  return query ? `?${query}` : ''
}

/** Where a sign-in starts. A 302 to Google, or a 503 on a server with no credentials. */
const SIGN_IN_PATH = '/api/auth/google'

export const api = {
  status: () => get<Status>('/api/status'),

  /** Who is signed in. Throws a 401 `ApiError` when nobody is — see `isUnauthorized`. */
  me: () => get<User>('/api/auth/me'),

  /** Ends the session and clears the cookie. 204 either way, so there is no body. */
  logout: () => requestEmpty('POST', '/api/auth/logout'),

  /**
   * Send the browser to Google's consent screen.
   *
   * This has to be a real navigation. The endpoint answers 302 to another origin, and
   * `fetch` cannot follow that. But a server with no Google credentials answers 503
   * with JSON, and navigating into that would leave the user reading `{"error":…}`.
   *
   * So ask first, then navigate. `redirect: 'manual'` stops the browser chasing the
   * 302 cross-origin; it reports an opaque response instead, which is the yes. A 503
   * is thrown with the server's own sentence, and the button prints it.
   *
   * The cost is one wasted CSRF state per click, which the next sign-in sweeps. A
   * `sign_in` flag on `/api/status` would remove the extra round trip.
   */
  signIn: async (): Promise<void> => {
    const res = await fetch(SIGN_IN_PATH, { redirect: 'manual' })
    // A browser gives an opaque response with status 0. Node's fetch gives the 3xx
    // itself. Either one means this leads to Google.
    const redirects = res.type === 'opaqueredirect' || (res.status >= 300 && res.status < 400)
    if (!redirects) {
      throw new ApiError(await errorMessage(res), res.status)
    }
    window.location.assign(SIGN_IN_PATH)
  },

  /**
   * One page of the feed. Omit `cursor` for the first.
   *
   * `refresh` skips the server's cache and rebuilds the page, which is also what fills
   * the cache for the next visitor. The screen calls this twice on load — once without,
   * to paint whatever Redis has, then once with, to replace it — see `FeedPage.from_cache`.
   */
  feedPage: (cursor?: string | null, refresh = false) => {
    const params = new URLSearchParams()
    if (cursor) params.set('cursor', cursor)
    if (refresh) params.set('refresh', 'true')
    const query = params.toString()
    return get<FeedPage>(`/api/feed${query ? `?${query}` : ''}`)
  },

  /**
   * One collection in full: `favorites`, `watchlist` or `journal`.
   *
   * `person` is a nickname, and omitting it means the visitor's own. A slug nobody has,
   * or a nickname nobody has, is a 404 rather than an empty grid.
   */
  collection: (slug: string, person?: string | null) => {
    const handle = person?.replace(/^@/, '')
    return get<Collection>(
      `/api/collections/${encodeURIComponent(slug)}${
        handle ? `?person=${encodeURIComponent(handle)}` : ''
      }`,
    )
  },

  mobileFeed: () => get<MobileFeed>('/api/feed/mobile'),
  reviews: () => get<Review[]>('/api/reviews'),
  review: (id: string) => get<Review>(`/api/reviews/${id}`),

  /**
   * The review a screen opens on: the one `id` names, or the newest in the graph
   * when nothing names one.
   *
   * Both review routes take an optional id, because both are reachable two ways —
   * from a card, which knows exactly which review it is, and from the feed's
   * "featured review" link, which doesn't. Composed here rather than in each
   * screen so the two can't disagree about what "featured" means.
   */
  reviewOrNewest: async (id?: string): Promise<Review> => {
    if (id) return api.review(id)
    const newest = await api.reviews()
    // Thrown rather than returned as null: a screen with no loader, no error and
    // no content reads as a bug, and an empty graph is a cause worth naming.
    // `NothingYetError` so the screen says that instead of "something went wrong".
    if (newest.length === 0) {
      throw new NothingYetError('Nobody has written a review yet. Yours could be the first.')
    }
    return newest[0]
  },
  movie: (id: string) => get<MovieDetail>(`/api/movies/${id}`),
  search: (params: SearchParams = {}) =>
    get<SearchResponse>(`/api/search${searchQuery(params)}`),
  watchlist: () => get<string[]>('/api/watchlist'),
  profile: () => get<Profile>('/api/profile'),

  /**
   * The friend directory. An empty `q` searches for nobody — the screen then draws only
   * Following and Followers, which is the point: a list of every account isn't friends.
   */
  people: (q?: string) =>
    get<PeopleResponse>(`/api/people${q ? `?q=${encodeURIComponent(q)}` : ''}`),

  /** One person by nickname, with or without the leading `@`. */
  person: (handle: string) =>
    get<PersonProfile>(`/api/people/${encodeURIComponent(handle.replace(/^@/, ''))}`),

  /**
   * A film's reviews, from the people you follow first, then the best-rated
   * strangers. A separate call from `movie()` because the film's facts are cached
   * for a day upstream while these change the moment you follow someone.
   */
  movieReviews: (id: string) => get<UserReview[]>(`/api/movies/${id}/reviews`),

  /** Omit `onWatchlist` to toggle; pass it to make the call idempotent. */
  setWatchlist: (id: string, onWatchlist?: boolean) =>
    request<WatchlistState>('POST', `/api/movies/${id}/watchlist`, {
      on_watchlist: onWatchlist ?? null,
    }),

  /** Omit `isFavorite` to toggle; pass it to make the call idempotent. */
  setFavorite: (id: string, isFavorite?: boolean) =>
    request<FavoriteState>('POST', `/api/movies/${id}/favorite`, {
      is_favorite: isFavorite ?? null,
    }),

  /** `0` clears the rating. */
  rate: (id: string, ratingHalfStars: number) =>
    request<RatingState>('PUT', `/api/movies/${id}/rating`, {
      rating_half_stars: ratingHalfStars,
    }),

  /** Write or rewrite the visitor's review. An empty body deletes it. */
  writeReview: (id: string, body: string) =>
    request<ReviewState>('PUT', `/api/movies/${id}/review`, { body }),

  /** Edit the profile bio. An empty string restores the default line. */
  setBio: (bio: string) => request<BioState>('PUT', '/api/profile', { bio }),

  /** Omit `following` to toggle; pass it to make the call idempotent. */
  setFollow: (personId: string, following?: boolean) =>
    request<FollowState>('POST', `/api/people/${personId}/follow`, {
      following: following ?? null,
    }),

  likeReview: (id: string) => request<LikeState>('POST', `/api/reviews/${id}/like`),

  likeComment: (reviewId: string, commentId: string) =>
    request<LikeState>('POST', `/api/reviews/${reviewId}/comments/${commentId}/like`),

  /** Both return the whole review, so the thread re-renders from one response. */
  postComment: (reviewId: string, body: string) =>
    request<Review>('POST', `/api/reviews/${reviewId}/comments`, { body }),

  postReply: (reviewId: string, commentId: string, body: string) =>
    request<Review>('POST', `/api/reviews/${reviewId}/comments/${commentId}/replies`, { body }),
}
