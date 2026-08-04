# CinéJournal
[google stitch project](https://stitch.withgoogle.com/projects/8536155539744862)
## Intended stack: 
- rust for the backend
- tmdb as open-source (non-commercial) API for movies
- react for desktop
- either PWA or native app for iPhone

A film journal / social platform, split into a **Rust API** and a **React frontend**.

The design is a 1:1 re-creation of a **Google Stitch** export — "Lumi Cinema Social", in an
*Editorial Minimalism* style. The first four screens were built as static HTML; that version
is kept under `reference/` as the source of truth for both the markup and the data.

```
backend/     Rust + Axum API. Serves the demo dataset as JSON, plus the images.
frontend/    React + Vite + TypeScript. Renders the six screens from the API.
reference/   The original static HTML re-creation and the Stitch exports it came from.
```

The backend holds no database. Content lives in three layers that never mix:

- `data.rs` — a static module transcribed verbatim from the demo markup, so every string,
  rating, year, and alt text matches the original. Immutable.
- `state.rs` — what the visitor changed this session (watchlist, ratings, likes, posts),
  in memory only. Lost on restart.
- `hydrate.rs` — folds the second into the first on the way out, so `data` never has to
  know a store exists.

## Run it

Two terminals:

```bash
cd backend && cargo run          # API on http://127.0.0.1:3001
cd frontend && npm install && npm run dev   # UI on http://localhost:5173
```

Then open http://localhost:5173. Vite proxies `/api` and `/img` to the backend, so the
browser only ever talks to one origin.

Override the API port with `PORT=4000 cargo run` (and `API_URL=http://127.0.0.1:4000
npm run dev` to match).

## Screens

| Screen | Route | Ported from |
| --- | --- | --- |
| Movie Feed — Desktop | `/` | `reference/cine-journal/index.html` |
| Friend Review — Desktop | `/review` | `reference/cine-journal/review.html` |
| Movie Feed — Mobile | `/feed-mobile` | `reference/cine-journal/feed-mobile.html` |
| Friend Review — Mobile | `/review-mobile` | `reference/cine-journal/review-mobile.html` |
| Movie Detail | `/movie/:id` | `reference/stitch_lumi_cinema_social 2/movie_detail_desktop/` |
| Search & Filter | `/search` | `reference/stitch_lumi_cinema_social 2/movie_search_desktop/` |

The two `*-mobile` routes are the export's separate mobile-only designs, kept as distinct
screens rather than merged into the responsive desktop ones. View them narrow, or with
device emulation (Chrome: ⌥⌘I → ⌘⇧M).

The two newer screens came from a second Stitch export and had no static-HTML stage — they
were built straight from its markup.

## What you can do

Every poster and film name links to `/movie/:id`, from all of the feed, search, and the
friends-activity rail.

- **Watchlist** — the "+" over any poster, and the button on the detail page. One shared
  list, so a film logged on the mobile feed shows as watchlisted on its detail page.
- **Rating** — click a star on the detail page; click the same one again to clear it.
  Half-stars come from the left/right half of each glyph.
- **Search** — text, genre, decade, and minimum rating, with pagination. The state lives in
  the URL, so `/search?q=shift&genre=Sci-Fi&min_rating=4` is shareable and the back button
  works. Text input is debounced 250ms; the filters apply immediately.
- **Filter counts** are leave-one-out: a genre chip's count ignores the current genre
  selection but respects the query and the other filters, so a chip never reads "4" and
  then yields nothing when you click it.
- **Likes, comments, replies** on the review screens.

Mutations are optimistic — the button flips first, then reconciles with what the server
stored, and rolls back with an inline error if the request failed.

## API

`GET` endpoints are pure reads. Mutations write to the in-memory visitor state and are
reflected by every subsequent read.

| Endpoint | Returns |
| --- | --- |
| `GET /api/health` | `{"status":"ok"}` |
| `GET /api/feed` | Live discussions, recent entries, friends activity |
| `GET /api/feed/mobile` | Stories rail + poster cards |
| `GET /api/reviews` | Both long-form reviews |
| `GET /api/reviews/{id}` | One review; `404` with `{"error":…}` if unknown |
| `GET /api/movies` | The catalogue as detail pages |
| `GET /api/movies/{id}` | One detail page — **any** id resolves (see below) |
| `GET /api/search?q=&genre=&year=&min_rating=&page=` | Results, facet counts, page count |
| `GET /api/watchlist` | The visitor's watchlist as movie ids |
| `POST /api/movies/{id}/watchlist` | `{on_watchlist}` — body `{"on_watchlist":bool}`, or omit it to toggle |
| `PUT /api/movies/{id}/rating` | `{your_rating_half_stars}` — body `{"rating_half_stars":0..=10}`, `0` clears |
| `POST /api/reviews/{id}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments` | The whole review, with the comment appended |
| `POST /api/reviews/{id}/comments/{cid}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments/{cid}/replies` | The whole review, with the reply attached |
| `/img/*` | The export's posters, avatars, and stills |

The two comment endpoints return the whole review rather than just the new row, so the
thread, the "Conversation (n)" heading, and the row itself all update from one response.

`GET /api/movies/{id}` never 404s. The catalogue only has details for a handful of films,
and only one of them (Neon Reverie) was actually designed, so every film borrows its
synopsis, cast, gallery, and credits — the title is the only thing that varies. An id that
isn't in the catalogue at all gets a title guessed from the slug (`/movie/some-quiet-film`
→ "Some Quiet Film") rather than a 404, so a hand-typed URL doesn't look broken either.
This is demo scaffolding, not a design decision — the endpoint should 404 once there is
real content behind it.

Ratings travel as `rating_half_stars` — an integer 0–10 rather than a float. The screens
draw discrete full/half/empty star glyphs, and integers keep that exact with no rounding
ambiguity. `frontend/src/components/StarRating.tsx` is the only place that decodes it.
`SearchResult.star_rating` is the exception: it's a crowd average shown as a number, so it
is genuinely fractional.

### No authentication

There is one visitor, shared by everyone who can reach the port, and the mutation endpoints
take no credentials. That is fine for a local demo and is why the backend binds
`127.0.0.1`. **Do not expose it publicly** without putting real auth in front of the writes
— see the note at the top of `backend/src/main.rs`.

## Frontend notes

Tailwind's theme block (47 color tokens plus the radius, spacing, and type scales) is
copied verbatim from the export's config into `frontend/tailwind.config.js`; only the
`content` globs differ. The custom utilities the export defined — `soft-shadow`,
`poster-inner-stroke`, `hairline-bottom`, `hide-scrollbar`, and the Material Symbols axis
defaults — are carried over in `frontend/src/index.css`.

Two export quirks are preserved on purpose and documented in the files that keep them: the
mobile feed's square poster corners (the markup used `rounded-DEFAULT`, which emits no CSS)
and its 20px titles.

Three are deliberately *not* preserved, all for the same reason — in a static mock a dead
end is a still image, but in an SPA it reads as a bug:

- the mobile review's `md:hidden` on `<body>`, which made that screen render blank at ≥768px
- the movie detail bar's missing nav links, which stranded you on a page every other screen
  links into, with no way out but the browser's back button
- the inert `CinéJournal` wordmark, now the home button on all four bars

The `Profile` tab has no screen behind it and renders as dimmed text rather than a link. It
was `<Link to="#">` until the detail page exposed the flaw: react-router resolves a bare `#`
against the current path, so on `/movie/red-shift` its href became `/movie/red-shift`.

Two things the wire format deliberately does *not* carry: Tailwind class strings, and
absolute-vs-relative image paths.

Tailwind's JIT only emits CSS for classes it finds literally in the source it scans, so a
class name arriving over the wire generates nothing. The gallery's asymmetric bento grid
therefore sends a semantic `shape` (`hero` / `companion` / `compact` / `panorama`) and the
frontend owns the class vocabulary — see `SHAPE_CLASSES` in `MovieDetail.tsx`.

Image `src`es are normalized to root-relative in `Image::new`. The export was a flat
directory, so `img/poster.jpg` resolved from every page; in an SPA it doesn't — on
`/movie/red-shift` the browser asks for `/movie/img/poster.jpg` and the dev server answers
with `index.html`. Fixing it in one constructor keeps all ~60 call sites verbatim.

Fidelity was verified by rendering each route in headless Chrome and comparing against the
export's reference screenshots in `reference/stitch_lumi_cinema_social*/*/screen.png`. Use
CDP's `Emulation.setDeviceMetricsOverride` for the mobile screens rather than Chrome's
`--window-size` — the latter misreports layout and makes correct pages look clipped.

The interactions were verified the same way, by real clicks against the running dev servers:
watchlist toggles on both feeds, search, and the detail page; half-star rating and clearing
it; the review and per-comment like buttons; both comment composers; and the inline reply.

See [`reference/cine-journal/README.md`](reference/cine-journal/README.md) for the full
design system and the rest of the export quirks.
