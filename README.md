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
is kept under `reference/` as the source of truth for the markup, and for the demo dataset
that stands in when TMDB isn't configured.

```
backend/     Rust + Axum API. Films from TMDB, social layer in SQLite, plus the images.
frontend/    React + Vite + TypeScript. Renders the six screens from the API.
reference/   The original static HTML re-creation and the Stitch exports it came from.
```

## Where the data comes from

Three layers that never mix:

- **Films** — [TMDB](https://www.themoviedb.org/), live, via `tmdb/` and the `content.rs`
  seam. Titles, posters, backdrops, runtimes, cast, galleries, crowd ratings and the
  long-form reviews are all real. With no token configured, `data.rs` stands in — see
  *Demo mode* below.
- **The social layer + the visitor's own actions** — SQLite (`db.rs`). Friends, the stories
  rail, live-discussion rooms and comment threads have no upstream equivalent (TMDB's
  `/reviews` is flat prose with no replies), so they're seeded there; the visitor's
  watchlist, ratings, likes and posted comments live in the same file and survive a
  restart.
- **`hydrate.rs`** — folds the second into the first on the way out, so neither layer has to
  know about the other.

Rows in the social layer carry no film ids: at request time template *i* is paired with
trending film *i*, so the rail can't go stale and the DB holds no ids that could rot.

### Demo mode

Without `TMDB_TOKEN` the backend does not fail to start — it serves the invented dataset in
`data.rs` (1015 lines transcribed verbatim from the Stitch export) and every screen shows a
banner saying the films are made up, with a link to get a free token. `GET /api/status`
reports which mode is live; `DemoBanner` in `frontend/src/components/Chrome.tsx` renders it.

The same banner appears with a different message when a token *is* present but TMDB rejected
it or is unreachable — a flaky upstream degrades to the demo data rather than to a blank
screen.

## Run it

Copy `.env.example` to `.env` and paste in a TMDB **API Read Access Token** (the long v4
JWT), free from [themoviedb.org/settings/api](https://www.themoviedb.org/settings/api). Skip
this to run in demo mode.

Two terminals:

```bash
cd backend && cargo run          # API on http://127.0.0.1:3001
cd frontend && npm install && npm run dev   # UI on http://localhost:5173
```

Then open http://localhost:5173. Vite proxies `/api` and `/img` to the backend, so the
browser only ever talks to one origin.

The SQLite file is created on first run at `backend/cine-journal.db` (gitignored;
`DATABASE_PATH` overrides it) and seeded only when it's empty, so a restart neither
duplicates the friends rail nor clobbers anything you changed. Delete the file to start over.

Override the API port with `PORT=4000 cargo run` (and `API_URL=http://127.0.0.1:4000
npm run dev` to match).

The token is read once at startup and never logged — the log line is `tmdb: enabled` or
`tmdb: disabled` and nothing more. It travels in an `Authorization: Bearer` header rather
than as TMDB's `?api_key=` query parameter, which the request-tracing layer would print.
`.env` is gitignored; `.env.example` is the committed template.

## Screens

| Screen | Route | Ported from |
| --- | --- | --- |
| Movie Feed — Desktop | `/` | `reference/cine-journal/index.html` |
| Friend Review — Desktop | `/review` | `reference/cine-journal/review.html` |
| Movie Feed — Mobile | `/feed-mobile` | `reference/cine-journal/feed-mobile.html` |
| Friend Review — Mobile | `/review-mobile` | `reference/cine-journal/review-mobile.html` |
| Movie Detail | `/movie/:id` | `reference/movie page/` |
| Search & Filter | `/search` | `reference/stitch_lumi_cinema_social 2/movie_search_desktop/` |

The two `*-mobile` routes are the export's separate mobile-only designs, kept as distinct
screens rather than merged into the responsive desktop ones. View them narrow, or with
device emulation (Chrome: ⌥⌘I → ⌘⇧M).

The two newer screens came from a second Stitch export and had no static-HTML stage — they
were built straight from its markup.

The detail screen has been through two designs. The first (`reference/stitch_lumi_cinema_social 2/movie_detail_desktop/`)
led with a full-bleed 70vh backdrop and a four-tile bento gallery of stills; the current one
is editorial — three columns, no hero image, no stills — and the backdrop survives only
behind the Media block's play button. Both references are kept, since the older one is still
what the search and feed screens were drawn against.

## What you can do

Every poster and film name links to `/movie/:id`, from all of the feed, search, and the
friends-activity rail.

- **Watchlist** — the "+" over any poster, and the button on the detail page. One shared
  list, so a film logged on the mobile feed shows as watchlisted on its detail page.
- **Rating** — "Rate" on the detail page reveals a five-star picker; click the same star
  again to clear it. Half-stars come from the left/right half of each glyph.
- **Trailer** — the detail page's Media tile swaps itself for a YouTube embed. Non-YouTube
  videos are dropped upstream rather than shown as a play button that can't play.
- **Search** — text, genre, decade, and minimum rating, with pagination. The state lives in
  the URL, so `/search?q=shift&genre=Sci-Fi&min_rating=4` is shareable and the back button
  works. Text input is debounced 250ms; the filters apply immediately.
- **Filter counts** are leave-one-out: a genre chip's count ignores the current genre
  selection but respects the query and the other filters, so a chip never reads "4" and
  then yields nothing when you click it.
- **Likes, comments, replies** on the review screens.

Mutations are optimistic — the button flips first, then reconciles with what the server
stored, and rolls back with an inline error if the request failed.

### One caveat on text search

TMDB's `/search/movie` **ignores `with_genres`** (verified: `query=dune` and
`query=dune&with_genres=878` both report 1095 results), and `/discover/movie` — which does
filter — has no free-text parameter. So the backend takes two routes:

- **No text** → `/discover/movie` with the genre, decade and rating pushed upstream. Counts
  and pagination are exact, across all of TMDB — except that TMDB serves no page past 500, so
  the paginator only ever offers the reachable prefix (500 × 20 upstream items = 1250 of our
  pages of 8). It won't offer a page the next click would fail on, even when `total_results`
  is larger. The sidebar's own paginator shows the first page, the last, and a window around
  the current one; at 1250 pages, a button each is a row that can't wrap.
- **Text** → `/search/movie`, three pages (60 candidates), with the genre, decade and rating
  applied locally over that window.

In the second case `total_results` and the facet counts describe matches *within the
60-candidate window*, not all of TMDB. That's the deliberate trade: one window means a chip's
count and the results it yields come from the same set, so the leave-one-out promise above
holds. Counting against all of TMDB while filtering against a window would make the chips lie.

## API

`GET` endpoints are pure reads. Mutations write to SQLite and are reflected by every
subsequent read, including after a restart.

| Endpoint | Returns |
| --- | --- |
| `GET /api/health` | `{"status":"ok"}` |
| `GET /api/status` | `{data_source: "tmdb"｜"demo", message, docs_url}` — drives the demo banner |
| `GET /api/feed` | Live discussions, recent entries, friends activity |
| `GET /api/feed/mobile` | Stories rail + poster cards |
| `GET /api/reviews` | The long-form reviews of one trending film |
| `GET /api/reviews/{id}` | One review; `404` with `{"error":…}` if unknown |
| `GET /api/movies` | Detail pages for the current trending films |
| `GET /api/movies/{id}` | One detail page; `404` if unknown (see below) |
| `GET /api/search?q=&genre=&year=&min_rating=&page=` | Results, facet counts, page count |
| `GET /api/watchlist` | The visitor's watchlist as movie ids |
| `POST /api/movies/{id}/watchlist` | `{on_watchlist}` — body `{"on_watchlist":bool}`, or omit it to toggle |
| `PUT /api/movies/{id}/rating` | `{your_rating_half_stars}` — body `{"rating_half_stars":0..=10}`, `0` clears |
| `POST /api/reviews/{id}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments` | The whole review, with the comment appended |
| `POST /api/reviews/{id}/comments/{cid}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments/{cid}/replies` | The whole review, with the reply attached |
| `/img/*` | The export's avatars and the demo dataset's posters and stills |

The two comment endpoints return the whole review rather than just the new row, so the
thread, the "Conversation (n)" heading, and the row itself all update from one response.

### Ids

Film ids are `<tmdb_id>-<slug>` — `157336-interstellar`. Only the leading integer is parsed,
so `/movie/157336` works too, and the slug is there to make a pasted URL readable. A demo
slug can never collide with one: those never start with a digit.

`GET /api/movies/{id}` returns a real `404` for an id TMDB doesn't know. In demo mode it
still resolves any id — the catalogue has details for only a handful of films, only one of
which (Neon Reverie) was actually designed, so every film borrows its synopsis, cast and
gallery, and an unknown slug gets a title guessed from it (`/movie/some-quiet-film` →
"Some Quiet Film"). That's scaffolding for a dataset with nothing behind it, and it goes
away as soon as there is.

Review ids are `<tmdb_movie_id>-<tmdb_review_id>`. Which reviews exist depends on what's
trending, so the two review screens fetch `GET /api/reviews` and take the first and second
entry rather than naming an id.

Ratings travel as `rating_half_stars` — an integer 0–10 rather than a float. The screens
draw discrete full/half/empty star glyphs, and integers keep that exact with no rounding
ambiguity. `frontend/src/components/StarRating.tsx` is the only place that decodes it.
`SearchResult.star_rating` is the exception: it's a crowd average shown as a number, so it
is genuinely fractional.

### Backend layout

```
tmdb/mod.rs   HTTP client: bearer auth, a TTL cache, one method per endpoint
tmdb/dto.rs   serde structs for the TMDB payloads — only the fields we render
tmdb/map.rs   dto -> models::*, pure and unit-tested against recorded fixtures
db.rs         SQLite: schema, seed, and the read/write helpers
content.rs    the seam — every screen, backed by TMDB or by `data`
data.rs       the demo dataset, as it was when it was the only dataset
hydrate.rs    folds the visitor's deltas into the content, unchanged
routes.rs     handlers; they call `content::` and never touch either source
```

Tests run offline: `tmdb/map.rs` deserializes recorded responses from
`backend/tests/fixtures/`, and the DB tests use `Connection::open_in_memory()`. One of them
checks the transcribed genre table against TMDB's real list by id, so a renamed genre
upstream fails a test rather than a request.

### No authentication

There is one visitor, shared by everyone who can reach the port, and the mutation endpoints
take no credentials. That is fine for a local demo and is why the backend binds
`127.0.0.1`. **Do not expose it publicly** without putting real auth in front of the writes
— see the note at the top of `backend/src/main.rs`. This matters more now than it did when
the visitor state was in memory: writes land in a real file on disk, and nothing prunes it.

## Frontend notes

Tailwind's theme block (47 color tokens plus the radius, spacing, and type scales) is
copied verbatim from the export's config into `frontend/tailwind.config.js`; only the
`content` globs differ. The custom utilities the export defined — `soft-shadow`,
`poster-inner-stroke`, `hairline-bottom`, `hide-scrollbar`, and the Material Symbols axis
defaults — are carried over in `frontend/src/index.css`.

Two export quirks are preserved on purpose and documented in the files that keep them: the
mobile feed's square poster corners (the markup used `rounded-DEFAULT`, which emits no CSS)
and its 20px titles.

Two are per-poster art direction with no upstream equivalent, so they only appear in demo
mode: the third search card's desaturated poster (`grayscale`) and the Gallery heading's
"12 Stills" over a 4-tile grid. `still_count` is still carried separately from
`gallery.length` — in TMDB mode it's the film's real backdrop count, which also exceeds the
four tiles shown, so the field means the same thing in both modes.

Three are deliberately *not* preserved, all for the same reason — in a static mock a dead
end is a still image, but in an SPA it reads as a bug:

- the mobile review's `md:hidden` on `<body>`, which made that screen render blank at ≥768px
- the movie detail bar's missing nav links and search box, which stranded you on a page every
  other screen links into, with no way out but the browser's back button
- the inert `CinéJournal` wordmark, now the home button on all four bars

`TopAppBar` takes no props but the active tab, and that is deliberate. It briefly took
`showNav` / `showSearch` / `showSearchIcon` so each page could reproduce its own export mock,
which produced four subtly different bars — the detail page lost its nav and search box, the
two screens without the box were 1px shorter than the two with it (so the bar jumped as you
navigated), and the nav text rendered in a different font on the two screens whose root div
didn't set one. Font smoothing moved to `<body>` in `index.css` for the same reason. The bar
is now byte-identical across all four routes, verified by cropping and hashing it.

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
with `index.html`. Fixing it in one constructor keeps all ~60 call sites verbatim. Anything
with a scheme passes through untouched, which is how TMDB's CDN URLs and the monogram
`data:` URI below both survive it.

Review authors whose TMDB profile has no picture — more than half of them — get an initials
monogram rather than a stock photograph. Putting one of the export's faces on a majority of
real reviews would attribute them to someone who didn't write them.

Fidelity was verified by rendering each route in headless Chrome and comparing against the
export's reference screenshots in `reference/stitch_lumi_cinema_social*/*/screen.png`. Use
CDP's `Emulation.setDeviceMetricsOverride` for the mobile screens rather than Chrome's
`--window-size` — the latter misreports layout and makes correct pages look clipped.

The interactions were verified the same way, by real clicks against the running dev servers:
watchlist toggles on both feeds, search, and the detail page; half-star rating and clearing
it; the review and per-comment like buttons; both comment composers; and the inline reply.

See [`reference/cine-journal/README.md`](reference/cine-journal/README.md) for the full
design system and the rest of the export quirks.
