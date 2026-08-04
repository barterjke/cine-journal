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

export interface LiveDiscussion {
  id: string
  movie: Movie
  rating_half_stars: number
  blurb: string
  participants: Image[]
  overflow_count: number | null
}

export interface FeedEntry {
  id: string
  movie: Movie
  rating_half_stars: number
  on_watchlist: boolean
}

/** Serde emits the unit variants of `ActivityKind` as bare strings. */
export type ActivityKind = 'watched' | 'added_to_watchlist'

export interface FriendActivity {
  id: string
  author_name: string
  author_avatar: Image
  timestamp: string
  kind: ActivityKind
  movie_id: string
  movie_title: string
  rating_half_stars: number | null
  quote: string | null
}

export interface Story {
  id: string
  name: string
  avatar: Image
  unseen: boolean
}

export interface MobileFeedItem {
  id: string
  movie: Movie
  subtitle: string
  rating_half_stars: number | null
  on_watchlist: boolean
}

export interface Reply {
  id: string
  author_name: string
  author_avatar: Image
  body: string
}

export interface Comment {
  id: string
  author_name: string
  author_avatar: Image
  timestamp: string
  body: string
  like_count: number | null
  replies: Reply[]
  liked: boolean
}

export interface Review {
  id: string
  movie: Movie
  backdrop: Image | null
  director: string | null
  genres: string[]
  author_name: string
  author_avatar: Image
  watched_on: string
  rating_half_stars: number
  paragraphs: string[]
  like_count: number | null
  comments: Comment[]
  hashtags: string[]
  liked: boolean
}

export interface CastMember {
  id: string
  name: string
  role: string
  portrait: Image
}

/**
 * Slot in the detail screen's asymmetric bento grid. Semantic rather than a
 * class string because Tailwind's JIT only emits CSS for classes it finds in
 * scanned source — see `SHAPE_CLASSES` in `pages/MovieDetail.tsx`.
 */
export type StillShape = 'hero' | 'companion' | 'compact' | 'panorama'

export interface GalleryStill {
  id: string
  image: Image
  shape: StillShape
}

export interface DetailFact {
  label: string
  value: string
}

export interface MovieDetail {
  id: string
  title: string
  year: number
  director: string
  runtime: string
  genres: string[]
  poster: Image
  backdrop: Image
  synopsis: string
  cast: CastMember[]
  /** May exceed `gallery.length` — the export claims 12 and shows 4. */
  still_count: number
  gallery: GalleryStill[]
  details: DetailFact[]
  watch_progress_percent: number
  watch_progress_label: string
  on_watchlist: boolean
  /** The visitor's own rating in half-stars; `null` if they haven't rated it. */
  your_rating_half_stars: number | null
}

export interface SearchResult {
  id: string
  title: string
  year: number
  /** A fractional 0.0–5.0 crowd average, shown as a number — not glyphs. */
  star_rating: number
  /** `null` renders the "Poster Missing" placeholder. */
  poster: Image | null
  grayscale: boolean
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
}

/** What the search screen's controls hold. All optional; all default to "any". */
export interface SearchParams {
  q?: string
  genre?: string | null
  year?: string | null
  minRating?: number
  page?: number
}

export interface Feed {
  live: LiveDiscussion[]
  recent: FeedEntry[]
  friend_activity: FriendActivity[]
}

export interface MobileFeed {
  stories: Story[]
  items: MobileFeedItem[]
}

export interface WatchlistState {
  movie_id: string
  on_watchlist: boolean
}

export interface RatingState {
  movie_id: string
  your_rating_half_stars: number | null
}

export interface LikeState {
  id: string
  liked: boolean
  /** Already includes the visitor's own like, so render it as-is. */
  like_count: number | null
}

/**
 * Prefers the API's own `{ error }` message over the bare status line — the
 * backend explains rejected writes ("body must not be empty"), and that text is
 * what the UI shows the user.
 */
async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })

  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`
    try {
      const payload: unknown = await res.json()
      if (payload && typeof payload === 'object' && 'error' in payload) {
        message = String((payload as { error: unknown }).error)
      }
    } catch {
      // Non-JSON error body — the status line is all we have.
    }
    throw new Error(`${method} ${path} failed: ${message}`)
  }

  return res.json() as Promise<T>
}

const get = <T>(path: string) => request<T>('GET', path)

/** Only sends the params that are actually set, keeping dev-tools URLs readable. */
function searchQuery({ q, genre, year, minRating, page }: SearchParams): string {
  const params = new URLSearchParams()
  if (q) params.set('q', q)
  if (genre) params.set('genre', genre)
  if (year) params.set('year', year)
  if (minRating) params.set('min_rating', String(minRating))
  if (page && page > 1) params.set('page', String(page))
  const query = params.toString()
  return query ? `?${query}` : ''
}

export const api = {
  feed: () => get<Feed>('/api/feed'),
  mobileFeed: () => get<MobileFeed>('/api/feed/mobile'),
  reviews: () => get<Review[]>('/api/reviews'),
  review: (id: string) => get<Review>(`/api/reviews/${id}`),
  movie: (id: string) => get<MovieDetail>(`/api/movies/${id}`),
  search: (params: SearchParams = {}) =>
    get<SearchResponse>(`/api/search${searchQuery(params)}`),
  watchlist: () => get<string[]>('/api/watchlist'),

  /** Omit `onWatchlist` to toggle; pass it to make the call idempotent. */
  setWatchlist: (id: string, onWatchlist?: boolean) =>
    request<WatchlistState>('POST', `/api/movies/${id}/watchlist`, {
      on_watchlist: onWatchlist ?? null,
    }),

  /** `0` clears the rating. */
  rate: (id: string, ratingHalfStars: number) =>
    request<RatingState>('PUT', `/api/movies/${id}/rating`, {
      rating_half_stars: ratingHalfStars,
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
