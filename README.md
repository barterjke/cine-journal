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
frontend/    React + Vite + TypeScript. Renders the nine screens from the API.
reference/   The original static HTML re-creation and the Stitch exports it came from.
```

## Where the data comes from

Three layers that never mix:

- **Films** — [TMDB](https://www.themoviedb.org/), live, via `tmdb/` and the `content.rs`
  seam. Titles, posters, backdrops, runtimes, cast, credits, trailers, age ratings,
  streaming availability, crowd ratings and the long-form reviews are all real. With no
  token configured, `data.rs` stands in — see *Demo mode* below.
- **The social layer + the visitor's own actions** — SQLite (`db.rs`). People, the follow
  graph, their reviews and comment threads have no upstream equivalent (TMDB's `/reviews` is
  flat prose with no replies), so they're seeded there; the visitor's watchlist, favourites,
  ratings, written reviews, bio, likes, follows and posted comments live in the same file and
  survive a restart.
- **`hydrate.rs`** — folds the second into the first on the way out, so neither layer has to
  know about the other.

### The people

Everyone in `people` is a user: someone with a handle, a page at `/people/:handle`, reviews of
their own, and a follow button that works.

That was not always true. The export also drew eleven decorative people — a stories rail of
avatars with an invented read/unread state, and a "Friends Activity" sidebar of verbs and
timestamps ("Watched Interstellar · 2h ago"). Nothing recorded any of it, and the film each
row named was whichever trending title the request happened to pair it with, so two screens
could credit the same person with different films. Both rails are gone, and with them the
`is_user = 0` population, the pairing trick, and `PersonRow` / `ActivityRow` / `DiscussionRow`
/ `CaptionRow`. Anyone on screen is now somebody the visitor follows, and what's drawn beside
them is what they actually wrote.

`FollowedPerson.handle` is still nullable, for rows a pre-users build left behind. Those are
drawn unlinked and reach no screen; the column stays because SQLite has no `DROP COLUMN`
before 3.35 and rewriting `people` would risk a visitor's own follows for tidiness.

The users are seeded once at startup by `content::harvest_graph`, which reads
`/movie/{id}/reviews` for the twelve top trending films and keeps every reviewer who left a
rating — 24 of them, 60 reviews, in about a second. Their nicknames are TMDB's real
`author_details.username` (`@msbreviews`, `@Geronimo1967`), which is a *different* string from
the display `author` ("Manuel São Bento", "CinemaSerf"), so both are independently searchable.
Their bios are derived from their own ratings ("5 films reviewed · generous ratings") rather
than invented, because TMDB publishes no bio to borrow.

After that first seed the users are **ours**. Nothing re-fetches them, following one writes to
our own `follows` table, and their stored reviews are keyed to real app film ids — unlike the
rails, a review is *of a specific film* and would say something different every week under the
pairing trick above. `db::needs_graph_seed` is checked *before* the harvest, so every restart
after the first makes zero HTTP calls. A `BTreeMap` keeps the harvest ordered, so the same
TMDB data always yields the same ids in the same order.

`follows` (what the visitor did) is deliberately asymmetric with `people.follows_visitor`
(a fact about them). Conflating them would make unfollowing someone rewrite whether they had
ever followed you.

There are **no follower/following counts on a person's page**, and that's on purpose: the graph
stores only the visitor's own edges — there is no person-to-person following — so any such
count would be 0 or 1, and a page reading "1 followers" under someone's name is worse than a
page that doesn't claim to know. `following` and `follows_you` are the two relationships that
actually exist, and the button and the "Follows you" badge say both.

### Demo mode

Without `TMDB_TOKEN` the backend does not fail to start — it serves the invented dataset in
`data.rs` (a film catalogue and one designed detail page, transcribed from the Stitch export)
and every screen shows a banner saying the films are made up, with a link to get a free token.
`GET /api/status` reports which mode is live; `DemoBanner` in
`frontend/src/components/Chrome.tsx` renders it.

`data.rs` used to transcribe the two feeds as well. It no longer does, and there is nothing to
replace them with: every section of both feeds is the visitor's own follows, journal or taste,
all of which live in SQLite and so read the same in either mode. A missing token can't empty
those screens — only the recommendation rail needs TMDB, and it degrades to nothing rather than
to invented films.

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
duplicates the follow graph nor clobbers anything you changed. Delete the file to start over.

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
| Profile | `/profile` | `reference/profile/` |
| Friends | `/people` | no mock — borrows the profile's panels |
| One person | `/people/:handle` | no mock — the profile's own layout, minus what only you have |

The two `*-mobile` routes are the export's separate mobile-only designs, kept as distinct
screens rather than merged into the responsive desktop ones. View them narrow, or with
device emulation (Chrome: ⌥⌘I → ⌘⇧M).

The two newer screens came from a second Stitch export and had no static-HTML stage — they
were built straight from its markup.

The two friend screens have no mock at all: the export drew friends only as avatars on rails.
Rather than invent a visual language for one corner of the app, they reuse the profile's
bordered `surface-container-low` panels, its 96px avatar header and its row treatment. A
person's page isn't merely *like* your own — it's the same components in the same order, so it
shows their favourites and their watchlist alongside their reviews. `/people/:handle` is keyed
on the nickname rather than the id, so the URL is the thing you'd have searched for.

The detail screen has been through two designs. The first (`reference/stitch_lumi_cinema_social 2/movie_detail_desktop/`)
led with a full-bleed 70vh backdrop and a four-tile bento gallery of stills; the current one
is editorial — three columns, no hero image, no stills — and the backdrop survives only
behind the Media block's play button. Both references are kept, since the older one is still
what the search and feed screens were drawn against.

## What you can do

Every poster and film name links to `/movie/:id`, from the feed, search, a person's page and a
review.

- **The feed** — three sections, none of which shows you anything nobody did. *From people you
  follow* is a strict `JOIN` on the follow graph rather than a friends-first *ordering*, so a
  stranger can't appear under a heading that says "people you follow"; *Recent Entries* is your
  own journal; *Recommended for you* is TMDB's `/movie/{id}/recommendations` asked about your
  three most recent favourites and watchlist entries. Each suggestion names the film it came
  from and links to it — "because you liked Obsession" for a favourite, "because you want to
  watch X" for a watchlist entry, since bookmarking a film isn't the same as having liked it.
  Ranked by how many of your films point at the same suggestion, then by vote count. On mobile
  the same three become a stories rail (tap a circle to read that person's newest review) and
  a grid of poster cards. All five sections can legitimately be empty, so each says why and
  where to go rather than filling itself with whatever is popular.
- **Watchlist** — the "+" over any poster, and the button on the detail page. One shared
  list, so a film logged on the mobile feed shows as watchlisted on its detail page.
- **Favourite** — the heart beside it. A separate list from the watchlist and from your
  ratings: what you'd recommend isn't what you mean to watch, and it isn't your top scores
  either. (It used to be — the profile's Favorite Films strip was the four highest-rated
  films, which meant you couldn't put a 3-star film you love on it, or leave a 5-star one off.)
- **Rating** — "Rate" on the detail page reveals a five-star picker; click the same star
  again to clear it. Half-stars come from the left/right half of each glyph.
- **Write a review** — the same page's "Write a review" pill opens a composer, prefilled if
  you've written one before; Save updates it and Delete removes it. Rating and prose are
  independent, so a film can carry either, both, or neither.
- **Trailer** — the detail page's Media tile swaps itself for a YouTube embed. Non-YouTube
  videos are dropped upstream rather than shown as a play button that can't play.
- **Search** — text, genre, decade, and minimum rating, with pagination. The state lives in
  the URL, so `/search?q=shift&genre=Sci-Fi&min_rating=4` is shareable and the back button
  works. Text input is debounced 250ms; the filters apply immediately.
- **Browse one person's films** — every cast portrait and every credits-grid name on a film's
  page links to `/search?person=<tmdb id>`, which narrows the grid to that person's whole
  filmography, acting and crewing alike. A removable pill in the sidebar names them, so
  arriving from a film doesn't trap you in one filmography. The text box and the other chips
  still apply *within* it — which no upstream endpoint can do, see below.
- **Filter counts** are leave-one-out: a genre chip's count ignores the current genre
  selection but respects the query and the other filters, so a chip never reads "4" and
  then yields nothing when you click it.
- **Likes, comments, replies** on the review screens.
- **Follow people** — the Friends tab (`/people`) lists everyone, who you follow, and who
  follows you. Search matches nickname *or* display name (`serf` finds `@Geronimo1967`,
  "CinemaSerf"). Every row and avatar opens that person's page, where the same button and a
  "Follows you" badge answer whether the follow is mutual. The button states its target rather
  than saying "flip it", so a double-click can't leave it and the database on opposite answers.
- **Read a film's reviews** — a film's page shows real users' prose, **the people you follow
  first, then the strangers who rated it highest**. One SQL `ORDER BY followed DESC,
  half_stars DESC, created_at DESC` does both, so the fallback isn't a second code path that
  could disagree. A "4 from people you follow, then the best rated" caption says so out loud:
  a friend's 3½ stars above a stranger's 5 would otherwise read as a sorting bug.
- **Profile** — `/profile` is where all of that becomes visible: the Favorite Films strip is
  what you hearted, the Watchlist grid is the list itself, and Recent Reviews is one journal
  ordered by whichever of the rating or the review happened later — an entry shows its stars,
  or the word "Written" when there's prose but no score. So it starts out empty and fills in as
  you use the app; each tile says what to do rather than showing borrowed posters. "Following"
  is the real graph, and its count comes from the graph rather than the list's length — the
  profile and the friend directory printing different numbers for the same fact is the one
  thing that must not happen. (It briefly did: the list also counted the export's two rails, so
  the profile said 12 while `/people` said 5.)
- **Edit your bio** — "Edit" beside it on your own profile; saving an empty one restores the
  default line rather than leaving a blank gap under your name.
- **See anyone's profile the way you see your own** — a person's page carries their favourites
  and their watchlist as well as their reviews. Both pages draw from
  `frontend/src/components/ProfileParts.tsx`, and `ProfileHeader` takes the bio and the header
  action as slots, so the only thing that differs is what each page genuinely knows: yours has
  an editable bio and a Following list, theirs has a follow button and their prose. Copy-pasting
  the layout instead is what let the two drift apart in the first place.

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
- **`person=` set** → `/person/{id}?append_to_response=movie_credits`, and it takes over the
  whole search: that person's filmography *is* the candidate set, narrower than anything the
  other two routes would return. Everything else — text included — filters locally over it.

In the second case `total_results` and the facet counts describe matches *within the
60-candidate window*, not all of TMDB. That's the deliberate trade: one window means a chip's
count and the results it yields come from the same set, so the leave-one-out promise above
holds. Counting against all of TMDB while filtering against a window would make the chips lie.

The person route has no window at all, which is why it's `/person/{id}` rather than
`/discover?with_people=`: one cached response carries the *entire* filmography — 63 films for
Nolan, 342 for Spielberg, 257 for Samuel L. Jackson — each entry already holding the title,
date, poster, rating and genre ids a result card needs (verified across four people, no missing
keys). So the counts are exact over everything they were credited on, pagination costs no extra
requests, and **text and person compose**, which nothing upstream offers: `/search/movie` has
no person parameter and `/discover` has no text one.

Two things that filter to nothing rather than being ignored, because ignoring them would put a
name over films that aren't theirs: a `person=` that isn't a number, and one TMDB 404s on
(`tmdb::Error` carries a `Kind` for exactly this — a missing record and an unreachable TMDB
have different right answers, an empty grid versus the demo dataset). Demo mode does the same
for *every* `person=`: the export's twelve films and four cast members were invented, so nobody
in it has a filmography, and its cast names are drawn unlinked rather than leading to an empty
grid.

## API

`GET` endpoints are pure reads. Mutations write to SQLite and are reflected by every
subsequent read, including after a restart.

| Endpoint | Returns |
| --- | --- |
| `GET /api/health` | `{"status":"ok"}` |
| `GET /api/status` | `{data_source: "tmdb"｜"demo", message, docs_url}` — drives the demo banner |
| `GET /api/feed` | Reviews by people you follow, your own journal, films suggested from your favourites and watchlist |
| `GET /api/feed/mobile` | The same three, as a stories rail of the people you follow plus poster cards |
| `GET /api/reviews` | The long-form reviews of one trending film |
| `GET /api/reviews/{id}` | One review; `404` with `{"error":…}` if unknown |
| `GET /api/movies` | Detail pages for the current trending films |
| `GET /api/movies/{id}` | One detail page; `404` if unknown (see below) |
| `GET /api/movies/{id}/reviews` | Real users' reviews of that film, followed first then best-rated. Never 404s — an empty list is the honest answer for a film nobody reviewed |
| `GET /api/search?q=&genre=&year=&min_rating=&page=&person=` | Results, facet counts, page count. `person` is a TMDB person id and narrows to their filmography; the response names them so the heading can say who |
| `GET /api/watchlist` | The visitor's watchlist as movie ids |
| `GET /api/profile` | The whole profile screen: identity, bio, favourites, watchlist, journal, following |
| `GET /api/people?q=` | The friend directory: results, following, followers, in one response. Omit `q` for everyone |
| `GET /api/people/{handle}` | One person by nickname (with or without `@`) and their favourites, watchlist and reviews; `404` if no such nickname |
| `POST /api/people/{id}/follow` | `{person_id, following, following_count}` — body `{"following":bool}`, or omit it to toggle. Keyed on the **id**, not the handle |
| `POST /api/movies/{id}/watchlist` | `{on_watchlist}` — body `{"on_watchlist":bool}`, or omit it to toggle |
| `POST /api/movies/{id}/favorite` | `{is_favorite}` — same shape, same toggle. Independent of the rating |
| `PUT /api/movies/{id}/rating` | `{your_rating_half_stars}` — body `{"rating_half_stars":0..=10}`, `0` clears |
| `PUT /api/movies/{id}/review` | `{your_review}` — body `{"body":string}` up to 2000 chars; **blank deletes it** |
| `PUT /api/profile` | `{bio}` — body `{"bio":string}` up to 280 chars; blank restores the default |
| `POST /api/reviews/{id}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments` | The whole review, with the comment appended |
| `POST /api/reviews/{id}/comments/{cid}/like` | `{liked, like_count}` — toggles |
| `POST /api/reviews/{id}/comments/{cid}/replies` | The whole review, with the reply attached |
| `/img/*` | The export's avatars and the demo dataset's posters and backdrops |

The two comment endpoints return the whole review rather than just the new row, so the
thread, the "Conversation (n)" heading, and the row itself all update from one response.

`GET /api/people` returns all three lists together for the same reason: a person can appear in
the results, in Following, *and* in Followers, and two panels on one screen showing opposite
follow states would be worse than a refetch — so the frontend patches all three from one
response instead.

The follow endpoint's three outcomes are distinct on purpose. `200` is the new state; `404`
means the id names nobody, so the button was never going to work; `500` means the write failed
and it's worth retrying. Collapsing the last two would have the UI either hide a real failure
or invite you to retry something that can't succeed.

The two text endpoints treat **blank as delete** rather than storing an empty string, matching
`PUT …/rating` with `0`: there is one way to have no review, so nothing downstream has to ask
whether `Some("")` means the same as `None`. Both trim first, so a body of spaces deletes too.
They cap length in the handler, not the schema — 2000 characters for a review, 280 for a bio,
which is where the profile header stops being able to show it.

### Ids

Film ids are `<tmdb_id>-<slug>` — `157336-interstellar`. Only the leading integer is parsed,
so `/movie/157336` works too, and the slug is there to make a pasted URL readable. A demo
slug can never collide with one: those never start with a digit.

`GET /api/movies/{id}` returns a real `404` for an id TMDB doesn't know. In demo mode it
still resolves any id — the catalogue has details for only a handful of films, only one of
which (Neon Reverie) was actually designed, so every film borrows its synopsis, cast and
credits, and an unknown slug gets a title guessed from it (`/movie/some-quiet-film` →
"Some Quiet Film"). That's scaffolding for a dataset with nothing behind it, and it goes
away as soon as there is.

Review ids are `<tmdb_movie_id>-<tmdb_review_id>`. Which reviews exist depends on what's
trending, so the two review screens fetch `GET /api/reviews` and take the first and second
entry rather than naming an id.

### The detail payload

`GET /api/movies/{id}` is one upstream round trip:
`/movie/{id}?append_to_response=credits,images,videos,release_dates,watch/providers`.
Appending is free — the response is one document — where five calls would multiply the page's
latency for the same bytes. The path doubles as the cache key, so it has to stay byte-stable.

Four fields are optional in a way the screen has to handle, because upstream really is
missing them, not as a placeholder for work not done:

- **`certification`** (`"PG-13"`) — TMDB publishes no global age rating, only per-country
  `release_dates`, so this is the US entry. Blank strings are pervasive there: 47 of
  Interstellar's 92 countries are entirely empty, and France's list reads
  `['', '', 'TP', …]`, so the mapper takes the first *non-empty* one. `null` omits the
  segment from the metadata line rather than printing an empty bullet.
- **`trailer`** — ranked official → `Trailer` > `Teaser` > other → newest, and non-YouTube
  videos are dropped. Recency alone picks wrong: Interstellar's two most recent videos are
  *Clips* from 2026. `null` hides the Media block entirely; the demo dataset sets it `null`
  on purpose, since a play button that plays nothing is exactly the dead end an SPA can't
  afford.
- **`watch_options`** — read in visitor-priority order (stream, free, ad-supported, rent,
  buy), sorted by TMDB's `display_priority`, deduped by name and capped at four. The dedupe
  is load-bearing: Amazon Video appears under both `rent` and `buy`. There is no per-row URL
  because TMDB's attribution terms permit linking only their own watch page — hence the
  single `watch_link`, dropped along with the rows when there are none.
- **`score` / `vote_count`** — `score` stays a raw 0–10 float, unlike every other rating here,
  because half-stars would round 7.8 to 8.0 and the design prints the decimal. `vote_count: 0`
  hides the whole block: an average over no votes is not a 0.0.

Everything else the mock shows is derived rather than fetched separately. The credits grid's
"Writers" row walks a priority chain (`Writer` → `Screenplay` → `Story` → `Novel` → …) and
takes only the first title anyone holds — three modern films expose only `Writer`, while
*Absolute Power* credits `Screenplay` + `Novel`; appending the novelist would credit them for
the screenplay.

Each row carries the people it names alongside its finished text, in the order they appear
there, which is what lets the frontend link each name to `/search?person=`. Parallel rather
than derived, because neither direction works on its own: splitting the joined string back
apart would break on a name containing a comma, and rendering from the people alone would lose
the "Production" row, whose value is a studio and therefore nobody. An empty list means the row
is plain text — "Production", and every row in demo mode.

The mock's one-line "Friends' Activity" card in the right column is now a full-width **Reviews**
section fed by `GET /api/movies/{id}/reviews` — a separate request from the film itself, because
the film's facts are cached for a day upstream while these change the moment you follow someone.
Full width rather than in the 250px column: these are paragraphs of real prose, several of them
past a thousand words, clamped to four lines with the rest on the author's page.

Ratings travel as `rating_half_stars` — an integer 0–10 rather than a float. The screens
draw discrete full/half/empty star glyphs, and integers keep that exact with no rounding
ambiguity. `frontend/src/components/StarRating.tsx` is the only place that decodes it.
`SearchResult.star_rating` is the exception: it's a crowd average shown as a number, so it
is genuinely fractional.

It is also **nullable**, as is `year` on both `SearchResult` and `MovieDetail` — for the same
reason `vote_count: 0` hides the detail screen's score block. A filmography is where the two
sentinels showed: it lists announced films, which have no release date, and obscure ones nobody
has voted on. `year: 0` printed "(0)" beside a title and `star_rating: 0.0` printed "★ 0.0",
each stating a fact that doesn't exist. `vote_count` is what tells "rated zero" from "unrated"
(they coincide exactly across 111 credits in `person-525.json`), and the frontend drops the
separator dot with whichever half is missing rather than leaving it dangling. Demo mode keeps
its own `0.0` — Project: Kepler's, which the export drew deliberately, and which the rating
floor is meant to drop.

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

### Two mirrored families of tables

Everything a person can have, the visitor has too — in a *separate* table, because the visitor
has no `people` row to hang a foreign key off:

| | Real users | The visitor |
| --- | --- | --- |
| Favourites | `user_favorites(person_id, movie_id)` | `favorites(movie_id, added_at)` |
| Watchlist | `user_watchlist(person_id, movie_id)` | `watchlist(movie_id, added_at)` |
| Reviews | `user_reviews(person_id, movie_id, body, half_stars)` | `visitor_reviews(movie_id, body)` + `ratings(movie_id, half_stars)` |
| Bio | `people.bio` | `settings('bio')` |

Giving the visitor a `people` row instead would look tidier and be wrong: they'd then turn up
in the friend directory, in a film's reviews, and in their own Following list, and every query
over real users would need an exclusion clause that one of them would eventually forget.

The visitor splits prose from score across two tables while a user's review holds both, because
the visitor writes them at different moments through different endpoints and either can exist
alone. `journal_recent_first` unions them and orders by whichever timestamp is later, which is
what makes one Recent Reviews list out of two tables.

Users' favourites and watchlists come from `db::derive_taste`, which deals each harvested
reviewer four favourites from the films they rated 7+ half-stars and six watchlist films from
the trending pool, offset by their seat so neighbours don't get the same strip. TMDB publishes
neither list, and a page with two empty strips wouldn't have shown that the feature works.

### Schema changes

The schema is applied with `CREATE TABLE IF NOT EXISTS`, which is a no-op on a table that
already exists — so a column added after the fact needs its own `ALTER TABLE`, in `migrate`,
guarded by a `PRAGMA table_info` check. `ratings.rated_at` is the first of those (it orders the
profile's Recent Reviews). It's nullable rather than `NOT NULL DEFAULT CURRENT_TIMESTAMP`
because SQLite's `ADD COLUMN` only accepts a *constant* default; rows written before it existed
sort last, and a test builds a pre-migration table to prove they still load.

The social graph's four columns on `people` (`handle`, `bio`, `is_user`, `follows_visitor`) go
through the same path, and `handle` shows the other half of that restriction: `ADD COLUMN`
**rejects `UNIQUE`**, so nickname uniqueness is a separate `CREATE UNIQUE INDEX
people_by_handle`. A test builds a pre-graph `people` table and asserts the migration lands.

Timestamps have one-second resolution, so every ordering query adds `movie_id` as a
deterministic tiebreak — otherwise three films rated in the same second come back in
whatever order SQLite feels like. The seeded reviews use a fixed table of dates for the same
reason in reverse: deleting the DB has to bring back an identical graph, so nothing in the
seed path reads the clock.

Removal is the harder direction: SQLite has no `DROP COLUMN` before 3.35, so `people` still
carries `unseen`, `in_stories` and `position` from the deleted rails. Nothing reads them, and
rewriting the table to be rid of them would put a visitor's own follows at risk for tidiness.
`position` is still *written* at seed time, though — a database from an earlier build declares
it `NOT NULL` with no default, and `CREATE TABLE IF NOT EXISTS` cannot retrofit the default
this schema now gives it. Omit it there and `INSERT OR IGNORE` silently swallows a `NOT NULL`
violation for every person, seeding nobody, and then the follow insert fails on a foreign key.

One trap worth knowing when resetting state by hand: the DB runs in WAL mode, so `rm x.db`
alone leaves `x.db-wal` and `x.db-shm` behind and SQLite recovers the old rows into the
"fresh" file. Remove all three.

### No authentication

There is one visitor, shared by everyone who can reach the port, and the mutation endpoints
take no credentials. That is fine for a local demo and is why the backend binds
`127.0.0.1`. **Do not expose it publicly** without putting real auth in front of the writes
— see the note at the top of `backend/src/main.rs`. This matters more now than it did when
the visitor state was in memory: writes land in a real file on disk, and nothing prunes it.

The visitor's identity — Alex Mercer, `@alexm_cinema`, and their avatar — is transcribed in
`hydrate.rs` rather than stored, since there is nothing to sign in to. Their comment byline
stays "You" (a relation, as the export drew it); the *name* only appears on the profile. The
avatar had been Elena's small portrait, which was invisible until the profile existed and then
became load-bearing: Elena was one of the export's people, so the visitor wore her photo on
their own comments and their own name on their profile, reading as two different people.

## Frontend notes

Tailwind's theme block (47 color tokens plus the radius, spacing, and type scales) is
copied verbatim from the export's config into `frontend/tailwind.config.js`; only the
`content` globs differ. The custom utilities the export defined — `soft-shadow`,
`poster-inner-stroke`, `hairline-bottom`, `hide-scrollbar`, and the Material Symbols axis
defaults — are carried over in `frontend/src/index.css`.

Two export quirks are preserved on purpose and documented in the files that keep them: the
mobile feed's square poster corners (the markup used `rounded-DEFAULT`, which emits no CSS)
and its 20px titles.

Several are deliberately *not* preserved, all for the same reason — in a static mock a dead
end is a still image, but in an SPA it reads as a bug:

- the mobile review's `md:hidden` on `<body>`, which made that screen render blank at ≥768px
- the movie detail bar's missing nav links and search box, which stranded you on a page every
  other screen links into, with no way out but the browser's back button
- the inert `CinéJournal` wordmark, now the home button on every bar
- the detail page's `more_horiz` overflow button and its "Full Cast & Crew" link, both of
  which opened nothing, and its static "In Theaters" row, now real streaming availability
- the profile's `share` button (nothing to share to) and its four `chevron_right` links; the two
  with a real destination — Watchlist and Following — are headings further down the same page,
  so those two scroll to them instead. The mock's header "Edit" was dropped for the same reason
  and has since come back as a real one, scoped to the bio — the only part of the identity block
  there is anything to edit, since the name and avatar have nothing to sign in to
- the profile's `Following (124)` beside a list of three. The count is the graph's own, and the
  list is what the graph holds, since a count the list contradicts is the kind of decoration an
  SPA can't afford
- the profile's Following rows as unclickable faces, now links to those people's pages. Their
  subtitles were the activity rail's verbs ("Watched Interstellar • 2h ago") and are now built
  from the bio and a live review count, both facts about what that person wrote
- the desktop bar's bell and cast icons and the mobile masthead's pair, four buttons with no
  handler — there are no notifications and nothing to cast to. One `account_circle` link to
  your own page in their place
- the feed's "Live Now" rail of discussion rooms with member counts, and its "Friends Activity"
  sidebar of verbs and timestamps. There are no rooms, and nothing records when anyone watched
  anything. In their place: what the people you follow have written, and films suggested from
  your own favourites and watchlist, each card naming the film of yours it came from
- the mobile feed's stories rail as five fixed avatars with an invented read/unread state and
  no destination. It is the people you follow now, the ring means "has a review to show", and
  tapping a circle opens it
- one search card's desaturated poster (`grayscale`, carried per result). It described one
  invented film's art direction and was `false` for every real one

Two of the detail screen's departures are responsive rather than editorial, since its mock is
desktop-only: the poster is capped at 240px below `md` (uncapped, a 2:3 poster is 585px tall
on a 390px screen and pushes the title off the fold), and the credits grid's label column is
`auto` below `sm` (the fixed 100px track clips "Cinematography", which measures 118px).

`TopAppBar` takes no props but the active tab, and that is deliberate. It briefly took
`showNav` / `showSearch` / `showSearchIcon` so each page could reproduce its own export mock,
which produced four subtly different bars — the detail page lost its nav and search box, the
two screens without the box were 1px shorter than the two with it (so the bar jumped as you
navigated), and the nav text rendered in a different font on the two screens whose root div
didn't set one. Font smoothing moved to `<body>` in `index.css` for the same reason. Across all
five desktop routes the bar is now identical but for which tab is lit — same 81px height, same
link positions, same search-box rect — verified by cropping and measuring it on each.

All four tabs now have a screen behind them, so all four are real links, and each points at the
screen it names — Friends went to `/review` while a single review was the only place another
person appeared, and now goes to `/people`. `Profile` was the exception until `/profile`
existed, and it rendered as dimmed text rather than as `<Link to="#">` — because react-router
resolves a bare `#` against the *current* path, so on `/movie/red-shift` its href became
`/movie/red-shift`: a link that looks live and goes somewhere wrong. Worth remembering before
adding a fifth tab ahead of its screen.

`ApiError` carries the HTTP status as a number alongside the message. A person's page needs to
tell "this nickname doesn't exist" (show the empty state) from "the server is down" (show the
error), and it can't find the code by grepping the text: the message is deliberately the API's
own prose, so a 404 reads `no person with nickname 'x'` and contains no "404" at all. The first
version tested `error.message.includes('404')` and silently never matched.

Two things the wire format deliberately does *not* carry: Tailwind class strings, and
absolute-vs-relative image paths.

Tailwind's JIT only emits CSS for classes it finds literally in the source it scans, so a
class name arriving over the wire generates nothing. Where a payload needs to influence
layout it sends a semantic value and the frontend owns the class vocabulary — the trailer's
`site` ("YouTube") picks the embed, `WatchOption.kind` picks the row's trailing label.

Image `src`es are normalized to root-relative in `Image::new`. The export was a flat
directory, so `img/poster.jpg` resolved from every page; in an SPA it doesn't — on
`/movie/red-shift` the browser asks for `/movie/img/poster.jpg` and the dev server answers
with `index.html`. Fixing it in one constructor keeps all ~60 call sites verbatim. Anything
with a scheme passes through untouched, which is how TMDB's CDN URLs and the monogram
`data:` URI below both survive it.

Review authors whose TMDB profile has no picture — more than half of them — get an initials
monogram rather than a stock photograph. Putting one of the export's faces on a majority of
real reviews would attribute them to someone who didn't write them.

`components/ProfileParts.tsx` exists for the same reason one layer up: your profile and someone
else's are the same page, so the tile, the poster strip, the watchlist card, the section heading
and the header all live in one file and both screens import them. `ProfileHeader` takes `bio` and
`action` as slots — that pair *is* the difference between the two screens, and expressing it as
two props keeps every other pixel impossible to diverge. Before this, a person's page had only
their reviews, and drifted from the profile it was supposedly modelled on.

`components/People.tsx` holds the follow button, the person row and the review card because
four screens draw them — the directory, one person's page, the profile's Following list and a
film's reviews — and a follow button that looked or behaved differently on one of them would
read as a different button. `ReviewCard`'s `showFilm` swaps which end is the subject (the film
leads on a person's page, the author on a film's) rather than forking into two components that
would drift. The follow button doubles as its own error surface, turning into **Retry**: a
failed write reverts it, which leaves the label truthful but makes it look like a bug unless it
says why it sprang back.

Fidelity was verified by rendering each route in headless Chrome and comparing against the
export's reference screenshots in `reference/stitch_lumi_cinema_social*/*/screen.png`. Use
CDP's `Emulation.setDeviceMetricsOverride` for the mobile screens rather than Chrome's
`--window-size` — the latter misreports layout and makes correct pages look clipped.

The interactions were verified the same way, by real clicks against the running dev servers:
watchlist toggles on both feeds, search, and the detail page; half-star rating and clearing
it; the review and per-comment like buttons; both comment composers; and the inline reply.
Then, in both modes, the favourite heart in both directions, the review composer's save and
delete, and the bio editor — each checked against what the API returned afterwards rather than
against the button's own label, since an optimistic button will happily lie about a failed write.

The composer is mounted with `key={movieId}` so it remounts per film, rather than seeding its
draft in an effect. The case that distinguishes them is navigating *between two films* inside
the SPA, where the component never unmounts: a mount-time seed leaves one film's prose in the
next film's box. Both are in the sweep — reached through in-app journal and search links, not
reloads, because a reload passes either way.

The feed's own sweep asks where things *go*, since that is what separates it from the rails it
replaced: a review card has to open the review it named, a story circle has to open that
person's newest one, and a suggestion's "because you liked X" has to land on X. The watchlist
button there is checked against `GET /api/watchlist` rather than the reloaded rail, because a
watchlisted film is dropped from the rail — so its absence can't tell a stored flip from an
unstored one. Layout is swept at thirteen widths from 1440 down to 390, asserting nothing's box
crosses the viewport's right edge; the run that mattered was 945px, between the desktop grid and
the mobile one, where the export's three columns used to be squeezed. And the empty case is
tested by unfollowing everyone on a throwaway database, since a new account sees exactly that.

See [`reference/cine-journal/README.md`](reference/cine-journal/README.md) for the full
design system and the rest of the export quirks.
