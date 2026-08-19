//! The seam between the two datasets.
//!
//! Every screen has one function here, and each dispatches on `Source`: real films
//! from TMDB, or the transcribed export in `data`. `routes` only knows about this
//! module, so neither the handlers nor `hydrate` learns that TMDB exists.
//!
//! Failure never blanks a screen. If TMDB is unreachable mid-session the request
//! falls back to `data` and says so through `/api/status`, because a feed of
//! invented films with a banner over it is a better answer than an error page.
//!
//! The social layer (friends, stories, live rooms) always comes from SQLite in
//! TMDB mode, since TMDB has none of it. Which film a rail entry is about is
//! decided here, by pairing row *i* with trending film *i*.

use std::sync::{Arc, Mutex};

use crate::db;
use crate::hydrate::{
    visitor_avatar, VISITOR_BIO, VISITOR_HANDLE, VISITOR_NAME, VISITOR_SINCE,
};
use crate::models::*;
use crate::tmdb::{self, map, DiscoverFilters, Tmdb};
use crate::{data, hydrate};

/// How many candidate films a text search pulls before filtering locally.
///
/// Three pages of 20. `/search/movie` can't filter by genre (verified: "dune" and
/// "dune" + Sci-Fi both report 1095 results), so genre, decade and rating are
/// applied here — and both the facet counts and the results are computed over this
/// one window, which is what keeps a chip from reading "4" and then yielding
/// nothing. The cost, stated rather than hidden: with a text query
/// `total_results` counts matches within the window, not across all of TMDB.
const SEARCH_PAGES: u32 = 3;

/// Matches `data::PAGE_SIZE` — eight cards fill the export's four-column grid
/// exactly twice.
const PAGE_SIZE: usize = 8;

/// How many trending films the search screen's empty state draws from.
const TRENDING_NEEDED: usize = 8;

/// How many films each profile tile resolves. Each tile is a summary that links to
/// `/collections/:slug` for the same list uncapped, so these are the counts that *fit*
/// rather than a limit on what the visitor has: four posters fill the favourites strip
/// and six the watchlist's at every breakpoint the design has.
const FAVORITES_SHOWN: usize = 4;
const WATCHLIST_SHOWN: usize = 6;
const REVIEWS_SHOWN: usize = 3;

/// How many reviews `GET /api/reviews` returns. Each one resolves its film, so this
/// is a bound on upstream calls as much as on payload size — and the screen that
/// reads it wants the newest few, not a history.
const RECENT_REVIEWS: u32 = 12;

/// Where the films come from.
///
/// The client is in an `Arc` rather than a `Box` so the facet counts can be fetched
/// concurrently — those run as spawned tasks, which need an owned handle.
pub enum Source {
    Tmdb(Arc<Tmdb>),
    /// No token, or a token TMDB rejected. `reason` is shown to the user.
    Demo { reason: String },
}

impl Source {
    /// Build from the environment. Never logs or returns the token itself.
    ///
    /// A present-but-rejected token is `Demo` with a different reason, so the
    /// banner can tell "you have no token" from "your token doesn't work" —
    /// different fixes.
    pub async fn from_env() -> Self {
        let token = std::env::var("TMDB_TOKEN").unwrap_or_default().trim().to_string();
        if token.is_empty() {
            tracing::info!("tmdb: disabled (no TMDB_TOKEN) — serving the demo dataset");
            return Self::Demo {
                reason: "No TMDB_TOKEN is set, so these films, reviews and ratings are invented."
                    .into(),
            };
        }

        let client = match Tmdb::new(token) {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, "tmdb: disabled");
                return Self::Demo { reason: format!("TMDB is unavailable: {error}") };
            }
        };

        // Checked once at startup, so a bad token shows up in the banner rather
        // than as six broken screens.
        if let Err(error) = client.authenticate().await {
            tracing::warn!(%error, "tmdb: token rejected — serving the demo dataset");
            return Self::Demo {
                reason: format!("TMDB rejected the configured token ({error}), so these films are invented."),
            };
        }

        tracing::info!("tmdb: enabled");
        Self::Tmdb(Arc::new(client))
    }

    fn client(&self) -> Option<&Arc<Tmdb>> {
        match self {
            Self::Tmdb(client) => Some(client),
            Self::Demo { .. } => None,
        }
    }
}

/// `GET /api/status` — whether what's on screen is real.
pub fn status(source: &Source) -> Status {
    const DOCS: &str = "https://www.themoviedb.org/settings/api";
    match source {
        Source::Tmdb(_) => Status { data_source: DataSource::Tmdb, message: None, docs_url: DOCS },
        Source::Demo { reason } => Status {
            data_source: DataSource::Demo,
            message: Some(format!(
                "{reason} Get a free API read access token from TMDB and put it in .env as TMDB_TOKEN."
            )),
            docs_url: DOCS,
        },
    }
}

/// The shared SQLite handle.
///
/// `rusqlite::Connection` is `!Sync`, hence the mutex rather than a `RwLock` over
/// something cloneable. **Never hold this guard across an `.await`** — every
/// caller below does its DB work in a scoped block and awaits outside it. A
/// `std::sync::Mutex` held across a yield point blocks the whole executor thread.
pub type Db = Arc<Mutex<rusqlite::Connection>>;

/// A poisoned mutex means a handler panicked mid-transaction. Recovering the
/// guard is right here: SQLite's own state is consistent (a panic can't leave a
/// half-applied statement), and the alternative is every later request panicking
/// too.
fn lock(db: &Db) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read the visitor's deltas. Falls back to an empty store rather than failing the
/// request — a screen missing its likes beats no screen.
pub fn store(db: &Db) -> crate::state::Store {
    let conn = lock(db);
    db::load_store(&conn).unwrap_or_else(|error| {
        tracing::error!(%error, "could not read the visitor's state");
        crate::state::Store::default()
    })
}

// --- Reads --------------------------------------------------------------------

/// How many of the visitor's own films seed the recommendation rail.
///
/// One upstream call each, so this is the rail's latency budget. Favourites are
/// consulted before the watchlist and only enough of each to fill this — three seeds
/// produced 54 distinct suggestions when measured, which is far more than the rail
/// draws, so the limit binds on calls rather than on variety.
const RECOMMEND_SEEDS: usize = 3;

/// How many suggestions the rail shows.
const RECOMMENDED_SHOWN: usize = 8;

/// How many of the graph's newest reviews the feed lists.
const FRIEND_REVIEWS_SHOWN: u32 = 6;

/// How many circles the mobile stories rail holds.
const STORIES_SHOWN: usize = 8;

/// How many cards one page of the infinite feed holds.
///
/// Twelve: enough to overflow a tall viewport, so the scroll observer at the bottom
/// isn't already visible when the page lands (which would fetch page two before the
/// user did anything), and few enough that a page is a handful of upstream calls
/// rather than a stall.
const FEED_PAGE_SIZE: usize = 12;

/// How the three sources are interleaved within a page, cycled over.
///
/// Two reviews, then one of the visitor's own entries, then one suggestion. Reviews
/// dominate because they're the only cards with something written on them — a feed
/// that alternated one-for-one read as a list of posters with occasional prose. The
/// ratio is only a preference: `feed_page` takes from whichever sources still have
/// something, so a visitor who follows nobody gets a feed of their own films and
/// suggestions rather than a third of a page.
const FEED_MIX: [FeedSource; 4] =
    [FeedSource::Review, FeedSource::Review, FeedSource::Entry, FeedSource::Recommendation];

/// Which of the three underlying lists a card comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedSource {
    Review,
    Entry,
    Recommendation,
}

/// How far into each source the feed has already been read.
///
/// Three offsets rather than one, because the sources are independent lists consumed
/// at different rates — the mix takes two reviews per suggestion, so a single "page
/// number" could not say where to resume. Serialized as `r.e.c` and handed to the
/// client as an opaque string; `parse` rejects anything else, and the caller treats a
/// rejection as "start from the beginning".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FeedCursor {
    reviews: usize,
    entries: usize,
    recommendations: usize,
}

impl FeedCursor {
    /// Round-trips through `Display`. `None` for anything that doesn't parse, which
    /// the handler turns into a first page rather than an error — see `models::FeedQuery`.
    fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split('.');
        let mut next = || parts.next()?.parse::<usize>().ok();
        let cursor = Self { reviews: next()?, entries: next()?, recommendations: next()? };
        // A trailing segment means this isn't our cursor at all.
        parts.next().is_none().then_some(cursor)
    }

    /// How many cards were consumed in total — what decides whether the feed is done.
    fn consumed(&self) -> usize {
        self.reviews + self.entries + self.recommendations
    }
}

impl std::fmt::Display for FeedCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.reviews, self.entries, self.recommendations)
    }
}

/// The cursor a client should be given for the page *after* this one.
///
/// Exposed as a string rather than the struct, so nothing outside this module can
/// construct one by arithmetic.
pub fn feed_cursor(raw: Option<&str>) -> Option<String> {
    raw.and_then(FeedCursor::parse).map(|cursor| cursor.to_string())
}

/// How deep the infinite feed goes before it admits it has nothing left.
///
/// The three sources are finite — a graph of two dozen people, the visitor's own
/// journal, eight suggestions — so the feed *does* end, and this is the ceiling on how
/// much of it a client can pull. Not endless-by-repetition: a feed that started
/// looping would be lying about having more.
const FEED_MAX_ITEMS: usize = 120;

/// How many reviews the paginated feed reads out of SQLite per request.
///
/// The whole window is read and then sliced, rather than issuing a `LIMIT/OFFSET`
/// query per page, because `db` has no offset-taking reads and the graph is dozens of
/// rows: the query is cheap and the slice is exact. The expensive part is resolving
/// each film upstream, which only happens for the cards a page actually emits.
const FEED_REVIEW_WINDOW: u32 = 60;

/// One page of the infinite feed, and the cursor for the next.
///
/// The three sources are read once, sliced at the cursor, then interleaved by
/// `FEED_MIX` — so a page is a fixed number of cards regardless of which sources still
/// have something. Films are resolved lazily, only for the cards this page emits,
/// which is what keeps a page's cost proportional to a page rather than to the graph.
///
/// Every card is derived from something the visitor or the people they follow did.
/// There is no filler when a source runs dry; the feed simply ends, and the screen says
/// so.
pub async fn feed_page(source: &Source, db: &Db, cursor: Option<&str>) -> FeedPage {
    let cursor = cursor.and_then(FeedCursor::parse).unwrap_or_default();

    // One lock, one scope, no `.await` inside it.
    let (reviews, journal, favorite_ids, watchlist_ids) = {
        let conn = lock(db);
        (
            db::reviews_from_followed(&conn, FEED_REVIEW_WINDOW).unwrap_or_default(),
            db::journal_recent_first(&conn).unwrap_or_default(),
            db::favorites_recent_first(&conn).unwrap_or_default(),
            db::watchlist_recent_first(&conn).unwrap_or_default(),
        )
    };

    // Recommendations are one upstream call per seed and don't paginate at the source,
    // so the whole rail is built once and then sliced. Skipped entirely once the cursor
    // is past its end, which is where the calls would otherwise be wasted.
    let seeds = seeds(&favorite_ids, &watchlist_ids);
    let recommendations = if cursor.recommendations < RECOMMENDED_SHOWN {
        recommended(source, &seeds, &watchlist_ids).await
    } else {
        Vec::new()
    };

    let mut next = cursor;
    let mut items: Vec<FeedItem> = Vec::new();
    // Which kinds still have rows behind them. A source drops out when its slice is
    // exhausted, and the loop ends when all three have.
    let available = |cursor: &FeedCursor, kind: FeedSource| match kind {
        FeedSource::Review => cursor.reviews < reviews.len(),
        FeedSource::Entry => cursor.entries < journal.len(),
        FeedSource::Recommendation => cursor.recommendations < recommendations.len(),
    };

    // Start the mix where the previous page left off, so page two doesn't open with the
    // same two-reviews-then-an-entry shape page one did — the seam would be visible as
    // a repeating rhythm.
    let mut turn = cursor.consumed() % FEED_MIX.len();

    while items.len() < FEED_PAGE_SIZE && next.consumed() < FEED_MAX_ITEMS {
        // Whichever of the next few slots has something. Rotating rather than falling
        // back to a fixed order keeps the ratio when every source is full and degrades
        // to "anything left" when they aren't.
        let Some(kind) = (0..FEED_MIX.len())
            .map(|offset| FEED_MIX[(turn + offset) % FEED_MIX.len()])
            .find(|kind| available(&next, *kind))
        else {
            break;
        };
        turn = (turn + 1) % FEED_MIX.len();

        match kind {
            FeedSource::Review => {
                let row = &reviews[next.reviews];
                next.reviews += 1;
                // One row at a time rather than `user_reviews` over the slice: a page
                // emits a handful of reviews out of a window of sixty, and resolving the
                // films for rows this page won't reach would be the whole graph's worth
                // of upstream calls per scroll.
                if let Some(review) = user_reviews(source, std::slice::from_ref(row)).await.pop() {
                    items.push(FeedItem::Review(review));
                }
            }
            FeedSource::Entry => {
                let row = &journal[next.entries];
                next.entries += 1;
                if let Some(detail) = movie_detail_by_id(source, &row.movie_id).await {
                    items.push(FeedItem::Entry(FeedEntry {
                        id: format!("entry-{}", detail.id),
                        // A film written about but never scored shows no stars rather
                        // than a zero, as `journal_entries` does.
                        rating_half_stars: row.half_stars.unwrap_or(0),
                        movie: Movie {
                            id: detail.id,
                            title: detail.title,
                            year: detail.year,
                            poster: detail.poster,
                        },
                        on_watchlist: false,
                    }));
                }
            }
            FeedSource::Recommendation => {
                let rec = recommendations[next.recommendations].clone();
                next.recommendations += 1;
                items.push(FeedItem::Recommendation(rec));
            }
        }
    }

    // Exhausted when nothing is left in any source, or when the ceiling is reached.
    // `None` rather than a cursor that would answer with an empty page: the client
    // stops observing on `None`, and one that had to discover the end by fetching
    // nothing would fire a request per scroll forever.
    let more = next.consumed() < FEED_MAX_ITEMS
        && FEED_MIX.iter().any(|kind| available(&next, *kind));

    FeedPage {
        items,
        next_cursor: more.then(|| next.to_string()),
        // Set by the caching layer in `routes`, which is the only thing that knows
        // whether this was built or read.
        from_cache: false,
    }
}

/// The mobile feed: a stories rail of the people you follow, then their reviews and
/// your recommendations as poster cards.
///
/// The same three facts the desktop feed draws, in the shape this screen has: one
/// rail and one grid. Tapping a circle opens that person's newest review, which is
/// what makes the rail a rail rather than a row of decoration.
pub async fn mobile_feed(source: &Source, db: &Db) -> MobileFeed {
    let (followed, reviews, favorite_ids, watchlist_ids) = {
        let conn = lock(db);
        (
            db::followed_with_newest_review(&conn, STORIES_SHOWN as u32).unwrap_or_default(),
            db::reviews_from_followed(&conn, FRIEND_REVIEWS_SHOWN).unwrap_or_default(),
            db::favorites_recent_first(&conn).unwrap_or_default(),
            db::watchlist_recent_first(&conn).unwrap_or_default(),
        )
    };

    let stories = followed
        .into_iter()
        .map(|row| Story {
            id: format!("story-{}", row.id),
            name: row.name,
            avatar: row.avatar,
            // The ring means "has something to show", which is the only read/unread
            // state anything here can actually answer.
            unseen: row.newest_review.is_some(),
            review_id: row.newest_review,
            handle: row.handle,
        })
        .collect();

    let seeds = seeds(&favorite_ids, &watchlist_ids);

    // Friends' reviews first, then recommendations — the grid reads top to bottom and
    // what someone you follow said outranks what an algorithm suggests.
    let mut items: Vec<MobileFeedItem> = user_reviews(source, &reviews)
        .await
        .into_iter()
        .map(|review| MobileFeedItem {
            id: format!("card-{}", review.id),
            movie: Movie {
                id: review.movie_id,
                title: review.movie_title,
                // The export's mobile cards print no year under the poster.
                year: None,
                poster: review.poster.unwrap_or_else(missing_poster),
            },
            subtitle: format!("{} rated it", first_name(&review.author_name)),
            rating_half_stars: Some(review.rating_half_stars),
            review_id: Some(review.id),
            on_watchlist: false,
        })
        .collect();

    items.extend(recommended(source, &seeds, &watchlist_ids).await.into_iter().map(|rec| {
        MobileFeedItem {
            id: format!("rec-{}", rec.movie.id),
            subtitle: because_line(&rec),
            movie: Movie { year: None, ..rec.movie },
            // Nobody the visitor follows has rated this, and printing the crowd
            // average where the other cards print a person's stars would make the
            // two kinds of card claim the same thing about different subjects.
            rating_half_stars: None,
            review_id: None,
            on_watchlist: rec.on_watchlist,
        }
    }));

    MobileFeed { stories, items }
}

/// "Because you liked Obsession" / "Because Obsession is on your watchlist".
///
/// One string rather than the two fields, because this screen's card has a single line
/// of subtitle and no room for a link. The desktop rail composes its own version from
/// `because` and `because_movie_id` so the film stays clickable there.
fn because_line(rec: &Recommendation) -> String {
    if rec.because_favorite {
        format!("Because you liked {}", rec.because)
    } else {
        format!("Because {} is on your watchlist", rec.because)
    }
}

/// Which of the visitor's films the recommendation rail asks about, best signal first.
///
/// Favourites before watchlist entries, because a favourite is a stronger statement
/// about taste than a film you merely mean to watch — and because the attribution on
/// each card is worded from `favorite`, so which kind a seed was has to survive as far
/// as the card. Ordering also decides the attribution when two seeds suggest the same
/// film: the first one wins, and that should be the favourite.
fn seeds(favorites: &[String], watchlist: &[String]) -> Vec<Seed> {
    favorites
        .iter()
        .map(|id| Seed { id: id.clone(), favorite: true })
        .chain(watchlist.iter().map(|id| Seed { id: id.clone(), favorite: false }))
        .take(RECOMMEND_SEEDS)
        .collect()
}

/// One film of the visitor's own that the rail was built from.
struct Seed {
    id: String,
    /// Whether they favourited it, as opposed to only watchlisting it.
    favorite: bool,
}

/// "Elena Rostova" -> "Elena". The card has room for one word.
fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// The stand-in for a review whose film has no poster.
///
/// `UserReview::poster` is optional because a person's page lists the review either
/// way, but a poster *card* is nothing but its image, so this fills the frame rather
/// than collapsing the grid. The path is the export's own placeholder art.
fn missing_poster() -> Image {
    Image::new("img/poster-missing.svg", "No poster available for this film.")
}

/// Films recommended from the visitor's own, each carrying the seed it came from.
///
/// Ranked by how many seeds recommended the film, then by vote count: a film two of
/// your favourites both point at is a better suggestion than one only a single
/// favourite does, and among equals the better-known film is the safer bet. Anything
/// the visitor already has — a seed itself, or something on their watchlist — is
/// dropped, since recommending a film back to the person who chose it is noise.
///
/// Empty in demo mode and on an upstream failure. Nothing invented fills the gap:
/// this rail's whole claim is that it was derived from your taste.
async fn recommended(
    source: &Source,
    seeds: &[Seed],
    watchlist: &[String],
) -> Vec<Recommendation> {
    let Some(client) = source.client() else {
        return Vec::new();
    };
    if seeds.is_empty() {
        return Vec::new();
    }

    let images = client.images().await;
    let already: std::collections::HashSet<&str> = seeds
        .iter()
        .map(|seed| seed.id.as_str())
        .chain(watchlist.iter().map(String::as_str))
        .collect();

    /// A candidate mid-ranking, before the two sort keys are applied.
    struct Candidate {
        rec: Recommendation,
        /// How many seeds suggested this film — the first sort key.
        seed_count: usize,
        /// The tie-break, and not on the wire: `Recommendation::star_rating` is the
        /// *average*, and ranking a film with three glowing votes above one with
        /// thousands would be reading a small sample as a strong signal.
        vote_count: u32,
    }

    // Insertion-ordered so the tie-break below is deterministic: `HashMap` iteration
    // order is not, and a feed that reshuffles between two refreshes of the same
    // data reads as broken.
    let mut ranked: Vec<Candidate> = Vec::new();

    for seed in seeds {
        let Some(tmdb_id) = map::tmdb_id(&seed.id) else {
            continue;
        };
        // The seed's own title, for the attribution line. Skipped rather than guessed
        // if the film no longer resolves — an unattributed recommendation is the fake
        // claim this rail exists to avoid.
        let Some(detail) = movie_detail_by_id(source, &seed.id).await else {
            continue;
        };
        let suggestions = match client.recommendations(tmdb_id).await {
            Ok(films) => films,
            Err(error) => {
                tracing::warn!(%error, seed = %seed.id, "no recommendations for this film");
                continue;
            }
        };

        for summary in &suggestions {
            let Some(movie) = map::movie(summary, &images) else {
                continue;
            };
            if already.contains(movie.id.as_str()) {
                continue;
            }
            match ranked.iter_mut().find(|c| c.rec.movie.id == movie.id) {
                // Recommended by a second seed. The attribution stays with the first
                // one that suggested it, which is the strongest — favourites are
                // consulted before the watchlist.
                Some(candidate) => candidate.seed_count += 1,
                None => ranked.push(Candidate {
                    rec: Recommendation {
                        movie,
                        star_rating: map::star_rating(summary.vote_average, summary.vote_count),
                        because: detail.title.clone(),
                        because_movie_id: detail.id.clone(),
                        because_favorite: seed.favorite,
                        on_watchlist: false,
                    },
                    seed_count: 1,
                    vote_count: summary.vote_count,
                }),
            }
        }
    }

    // Most-recommended first, then best-known. `sort_by` is stable, so films tied on
    // both keep the order the seeds produced them in.
    ranked.sort_by(|a, b| {
        b.seed_count.cmp(&a.seed_count).then(b.vote_count.cmp(&a.vote_count))
    });

    ranked.into_iter().take(RECOMMENDED_SHOWN).map(|c| c.rec).collect()
}

/// The profile screen.
///
/// Unlike every other function here there is no `data::` counterpart to fall back
/// to, and that is the point: the whole screen below the header is the visitor's
/// own rows out of SQLite, which exist in both modes. Only the film *titles and
/// posters* behind their stored ids need a source, and each is resolved
/// independently — a film TMDB has forgotten drops out of the grid rather than
/// blanking it.
pub async fn profile(source: &Source, db: &Db) -> Profile {
    // One lock, one scope, no `.await` inside it.
    let (follows, follow_count, favorite_ids, watchlist_ids, journal, bio) = {
        let conn = lock(db);
        (
            db::following(&conn).unwrap_or_default(),
            db::follow_count(&conn).unwrap_or(0),
            db::favorites_recent_first(&conn).unwrap_or_default(),
            db::watchlist_recent_first(&conn).unwrap_or_default(),
            db::journal_recent_first(&conn).unwrap_or_default(),
            db::visitor_bio(&conn).unwrap_or_default(),
        )
    };

    let following: Vec<FollowedPerson> = follows
        .iter()
        .map(|row| FollowedPerson {
            id: row.id.clone(),
            name: row.name.clone(),
            avatar: row.avatar.clone(),
            subtitle: follow_subtitle(row),
            handle: row.handle.clone(),
        })
        .collect();

    Profile {
        name: VISITOR_NAME.into(),
        handle: VISITOR_HANDLE.into(),
        avatar: visitor_avatar(),
        member_since: VISITOR_SINCE.into(),
        // Their own line if they've written one, the export's otherwise. See
        // `db::visitor_bio` for why the default isn't stored eagerly.
        bio: bio.unwrap_or_else(|| VISITOR_BIO.into()),
        favorites: movies_for(source, favorite_ids.iter().map(String::as_str), FAVORITES_SHOWN)
            .await,
        watchlist: movies_for(source, watchlist_ids.iter().map(String::as_str), WATCHLIST_SHOWN)
            .await,
        recent_reviews: rated_films(source, &journal).await,
        // The graph's own count, not `following.len()`: the same number the friend
        // directory prints, and the two screens saying different things about how
        // many people you follow is the one thing this must not do.
        following_count: follow_count,
        following,
    }
}

/// How many films a collection page resolves.
///
/// Far above `FAVORITES_SHOWN` and `WATCHLIST_SHOWN` — the point of the page is that
/// it isn't a summary — but still a ceiling, because each film is an upstream call in
/// TMDB mode. Nobody in this app has a hundred favourites, so in practice this binds
/// on nothing and exists so a pathological watchlist can't make one request take a
/// minute.
const COLLECTION_MAX: usize = 100;

/// The collections a slug can name. Rejecting anything else is what makes the URL a
/// closed set rather than a place to guess.
const COLLECTION_SLUGS: [&str; 3] = ["favorites", "watchlist", "journal"];

/// Whether a slug names a collection at all — the handler's 404 check.
pub fn is_collection(slug: &str) -> bool {
    COLLECTION_SLUGS.contains(&slug)
}

/// One collection in full: the page the profile's tiles link to.
///
/// `None` — a real 404 — for an unknown slug, or a nickname nobody has. The two are
/// deliberately the same answer: both mean the URL names nothing, and distinguishing
/// them would tell a client which half it got wrong about a page that doesn't exist
/// either way.
///
/// `person` selects whose collection: absent is the visitor's own, a nickname is
/// somebody else's. The visitor's journal is the one collection with ratings behind it,
/// so it's the only one whose grid draws stars.
pub async fn collection(
    source: &Source,
    db: &Db,
    slug: &str,
    person: Option<&str>,
) -> Option<Collection> {
    if !is_collection(slug) {
        return None;
    }

    match person {
        Some(handle) => person_collection(source, db, slug, handle).await,
        None => Some(visitor_collection(source, db, slug).await),
    }
}

/// The visitor's own favourites, watchlist or journal.
async fn visitor_collection(source: &Source, db: &Db, slug: &str) -> Collection {
    // One lock, one scope, no `.await` inside it — and all three lists are read
    // whichever slug was asked for, because the read is one cheap query each and
    // branching inside the guard would put a `match` between the lock and its drop.
    let (favorite_ids, watchlist_ids, journal) = {
        let conn = lock(db);
        (
            db::favorites_recent_first(&conn).unwrap_or_default(),
            db::watchlist_recent_first(&conn).unwrap_or_default(),
            db::journal_recent_first(&conn).unwrap_or_default(),
        )
    };

    let (title, description, movies) = match slug {
        "favorites" => (
            "Favorite Films",
            "Every film you've pressed the heart on, most recent first.",
            rated_collection(source, favorite_ids.iter().map(|id| (id.as_str(), None))).await,
        ),
        "watchlist" => (
            "Watchlist",
            "Films you mean to watch, most recently added first.",
            rated_collection(source, watchlist_ids.iter().map(|id| (id.as_str(), None))).await,
        ),
        // The only collection with ratings behind it, so the only one whose grid can
        // draw stars.
        _ => (
            "Your Journal",
            "Every film you've rated or written about, newest first.",
            rated_collection(source, journal.iter().map(|row| (row.movie_id.as_str(), row.half_stars)))
                .await,
        ),
    };

    Collection {
        slug: slug.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        owner: None,
        movies,
    }
}

/// Somebody else's favourites or watchlist.
///
/// No journal: the visitor's journal is their own ratings and prose, and the
/// equivalent for another person is their *reviews*, which their page already lists in
/// full. A "journal" page for them would be a second, worse copy of that.
async fn person_collection(
    source: &Source,
    db: &Db,
    slug: &str,
    handle: &str,
) -> Option<Collection> {
    let (row, favorite_ids, watchlist_ids) = {
        let conn = lock(db);
        let row = db::person_by_handle(&conn, handle).ok().flatten()?;
        let favorites = db::favorites_by_person(&conn, &row.id).unwrap_or_default();
        let watchlist = db::watchlist_by_person(&conn, &row.id).unwrap_or_default();
        (row, favorites, watchlist)
    };

    let first = first_name(&row.name);
    let (title, description, ids) = match slug {
        "favorites" => (
            "Favorite Films",
            format!("The films {first} raves about, derived from their own reviews."),
            favorite_ids,
        ),
        "watchlist" => (
            "Watchlist",
            format!("What {first} means to watch next."),
            watchlist_ids,
        ),
        // "journal" reaches here only because `is_collection` allows it for the
        // visitor. Their reviews are on their own page, so this is a 404 rather than an
        // empty grid pretending the collection exists but is empty.
        _ => return None,
    };

    Some(Collection {
        slug: slug.to_string(),
        title: title.to_string(),
        description,
        owner: Some(CollectionOwner {
            name: row.name.clone(),
            handle: row.handle.clone(),
            avatar: row.avatar.clone(),
        }),
        movies: rated_collection(source, ids.iter().map(|id| (id.as_str(), None))).await,
    })
}

/// Resolve `(film id, rating)` pairs into grid cards, dropping the ids that no longer
/// resolve.
///
/// `movies_for`'s counterpart for a collection: the same "drop what TMDB has forgotten
/// rather than blank the grid" rule, but carrying the rating through, which a bare
/// `Movie` has no room for.
async fn rated_collection<'a>(
    source: &Source,
    ids: impl Iterator<Item = (&'a str, Option<u8>)>,
) -> Vec<CollectionMovie> {
    let mut out = Vec::new();
    for (id, rating_half_stars) in ids {
        if out.len() == COLLECTION_MAX {
            break;
        }
        if let Some(detail) = movie_detail_by_id(source, id).await {
            out.push(CollectionMovie {
                movie: Movie {
                    id: detail.id,
                    title: detail.title,
                    year: detail.year,
                    poster: detail.poster,
                },
                rating_half_stars,
                // `hydrate::collection` fills this in — it's the visitor's flag, not the
                // owner's, even on somebody else's page.
                on_watchlist: false,
            });
        }
    }
    out
}

// --- The social graph ---------------------------------------------------------

/// How many trending films the graph harvest reads reviews from. Twelve yielded 52
/// distinct reviewers when measured, 16 of whom had reviewed more than one — enough
/// that a followed person has opinions on several films rather than exactly one.
const HARVEST_FILMS: usize = 12;

/// How many of those reviewers become users. Capped so the friend directory is
/// browsable rather than a wall, and so startup is a bounded number of calls.
const HARVEST_USERS: usize = 24;

/// How many the visitor already follows on first run, so the app opens with friends
/// rather than an empty graph the user has to populate before anything is testable.
const SEEDED_FOLLOWS: usize = 5;

/// Collect people and reviews to seed the graph with, once, at startup.
///
/// The people that come out of this are **ours** from the moment they land in
/// SQLite: nothing re-fetches them, following them writes to our own `follows`
/// table, and their reviews are rows we own. TMDB is the quarry, not the backing
/// store — which is why this returns `SeedUser` rather than a DTO, and why it is
/// called once by `main` instead of per request.
///
/// The prose *is* real TMDB review text, kept rather than invented because
/// generated filler reads as generated filler the moment you look at two of it.
///
/// Returns an empty vec if TMDB is unreachable; startup then leaves the graph empty
/// and the friend screens say so, which beats failing to boot.
pub async fn harvest_graph(source: &Source) -> Vec<db::SeedUser> {
    let Some(client) = source.client() else {
        return db::demo_graph();
    };

    let images = client.images().await;
    let Ok(trending) = client.trending().await else {
        tracing::warn!("graph seed: trending unavailable, leaving the social graph empty");
        return Vec::new();
    };

    // person -> their reviews. `BTreeMap` so the same TMDB data always produces the
    // same ids and the same order; a `HashMap` here would shuffle the directory on
    // every fresh database.
    let mut found: std::collections::BTreeMap<String, HarvestedUser> = Default::default();

    // Every film the harvest read, in trending order — the pool the seeded people's
    // watchlists are drawn from, since TMDB has no watchlists to borrow.
    let mut pool: Vec<String> = Vec::new();

    for summary in trending.iter().take(HARVEST_FILMS) {
        let Some(film) = map::movie(summary, &images) else {
            continue;
        };
        pool.push(film.id.clone());
        let Ok(reviews) = client.reviews(summary.id).await else {
            continue;
        };

        for record in reviews {
            // A review with no rating can't fill a star row, and the film page ranks
            // on it. More than half of real reviews have none, so this is a filter
            // rather than a fallback to an invented number.
            let Some(rating) = record.author_details.rating else {
                continue;
            };
            let handle = record
                .author_details
                .username
                .as_deref()
                .unwrap_or(&record.author)
                .trim()
                .to_string();
            if handle.is_empty() {
                continue;
            }

            let entry = found.entry(handle.clone()).or_insert_with(|| HarvestedUser {
                name: record.author.trim().to_string(),
                avatar: images.avatar(record.author_details.avatar_path.as_deref(), &record.author),
                reviews: Vec::new(),
            });
            entry.reviews.push((
                film.id.clone(),
                map::half_stars(rating),
                record.content.trim().to_string(),
                record.created_at.clone(),
            ));
        }
    }

    // The most prolific first, so the ones who make the cut are the ones with
    // opinions on several films — those are what make a friend's page worth opening.
    let mut ranked: Vec<(String, HarvestedUser)> = found.into_iter().collect();
    ranked.sort_by(|(a_handle, a), (b_handle, b)| {
        b.reviews.len().cmp(&a.reviews.len()).then_with(|| a_handle.cmp(b_handle))
    });

    ranked
        .into_iter()
        .take(HARVEST_USERS)
        .enumerate()
        .map(|(index, (handle, user))| {
            // Both derived from what they wrote, by the same function demo mode uses,
            // so a harvested person's page and a demo person's page are the same
            // shape rather than two guesses at one.
            let (favorites, watchlist) = db::derive_taste(&user.reviews, &pool, index);
            db::SeedUser {
                id: format!("user-{}", map::slug(&handle)),
                bio: Some(review_bio(&user.reviews)),
                // Every other person follows the visitor, and the first few are
                // already followed. Both are positional rather than random so a fresh
                // database is always seeded the same way — a test can rely on it, and
                // so can you when you delete the file and start over.
                follows_visitor: index % 2 == 1,
                followed_by_visitor: index < SEEDED_FOLLOWS,
                handle,
                name: user.name,
                avatar: user.avatar,
                reviews: user.reviews,
                favorites,
                watchlist,
            }
        })
        .collect()
}

/// A reviewer mid-harvest, before they have an id or a place in the graph.
struct HarvestedUser {
    name: String,
    avatar: Image,
    reviews: Vec<(String, u8, String, String)>,
}

/// A one-line bio derived from what they've written, since TMDB has no bio field.
///
/// Says something true about the person rather than inventing a personality: how
/// much they write and how generously they score. A fabricated "Amateur critic,
/// full-time dreamer" on two dozen people would read as filler immediately.
fn review_bio(reviews: &[(String, u8, String, String)]) -> String {
    let count = reviews.len();
    let mean = reviews.iter().map(|(_, stars, _, _)| *stars as f32).sum::<f32>() / count as f32;
    let leaning = match mean {
        m if m >= 8.0 => "generous ratings",
        m if m >= 6.0 => "middling ratings",
        _ => "hard to please",
    };
    let films = if count == 1 { "film" } else { "films" };
    format!("{count} {films} reviewed · {leaning}")
}

/// The friend screen: whoever the search term matches, plus both sides of the
/// visitor's graph.
///
/// An empty query returns **no results**, rather than the whole directory it used to.
/// The screen's two standing lists are Following and Followers — who you know — and a
/// third panel listing every account on the server was neither of those: it made the
/// page a user directory that happened to have your friends in a sidebar. Search is
/// how you find somebody you don't already follow, so it answers only when asked.
pub fn people(db: &Db, query: &str) -> PeopleResponse {
    let conn = lock(db);
    let results = if query.trim().is_empty() {
        Vec::new()
    } else {
        db::search_people(&conn, query).unwrap_or_default().iter().map(card).collect()
    };
    PeopleResponse {
        query: query.to_string(),
        results,
        following: db::followed_users(&conn).unwrap_or_default().iter().map(card).collect(),
        followers: db::followers(&conn).unwrap_or_default().iter().map(card).collect(),
    }
}

fn card(row: &db::UserRow) -> PersonCard {
    PersonCard {
        id: row.id.clone(),
        name: row.name.clone(),
        handle: row.handle.clone(),
        avatar: row.avatar.clone(),
        bio: row.bio.clone(),
        following: row.following,
        follows_you: row.follows_you,
        review_count: row.review_count,
    }
}

/// One person's page. `None` — a real 404 — when no such nickname exists.
///
/// The same sections the visitor's own profile draws, from the same shapes: their
/// favourites, their watchlist and their reviews. A page that showed only reviews
/// while yours showed four tiles made two kinds of person out of one — and the
/// difference that is real is the header, not the body.
pub async fn person(source: &Source, db: &Db, handle: &str) -> Option<PersonProfile> {
    let (row, reviews, favorite_ids, watchlist_ids) = {
        let conn = lock(db);
        let row = db::person_by_handle(&conn, handle).ok().flatten()?;
        let reviews = db::reviews_by_person(&conn, &row.id).unwrap_or_default();
        let favorites = db::favorites_by_person(&conn, &row.id).unwrap_or_default();
        let watchlist = db::watchlist_by_person(&conn, &row.id).unwrap_or_default();
        (row, reviews, favorites, watchlist)
    };

    Some(PersonProfile {
        id: row.id.clone(),
        name: row.name.clone(),
        handle: row.handle.clone(),
        avatar: row.avatar.clone(),
        bio: row.bio.clone(),
        following: row.following,
        follows_you: row.follows_you,
        favorites: movies_for(source, favorite_ids.iter().map(String::as_str), FAVORITES_SHOWN)
            .await,
        watchlist: movies_for(source, watchlist_ids.iter().map(String::as_str), WATCHLIST_SHOWN)
            .await,
        review_count: row.review_count,
        reviews: user_reviews(source, &reviews).await,
    })
}

/// The reviews of one film, friends first — what the detail page's rail draws.
///
/// The film's title and poster are already on the page that asks for this, so this
/// resolves them anyway rather than trusting the caller: a person's page needs them
/// too, and one shape for both means the detail page can't drift from the profile.
pub async fn reviews_of_movie(source: &Source, db: &Db, movie_id: &str) -> Vec<UserReview> {
    let rows = {
        let conn = lock(db);
        db::reviews_for_movie(&conn, movie_id).unwrap_or_default()
    };
    user_reviews(source, &rows).await
}

/// Turn stored review rows into wire shape, resolving each film once.
///
/// Films are looked up through a local cache keyed on id, because a person's page
/// often reviews the same film twice over (it doesn't) but more importantly because
/// `movie_detail_by_id` is a network call in TMDB mode and the same id recurs across
/// a person's reviews of a series.
async fn user_reviews(source: &Source, rows: &[db::UserReviewRow]) -> Vec<UserReview> {
    let mut films: std::collections::HashMap<String, Option<(String, Option<Image>)>> =
        Default::default();
    let mut out = Vec::with_capacity(rows.len());

    for row in rows {
        if !films.contains_key(&row.movie_id) {
            let resolved = movie_detail_by_id(source, &row.movie_id)
                .await
                .map(|detail| (detail.title, Some(detail.poster)));
            films.insert(row.movie_id.clone(), resolved);
        }
        // A film the source has forgotten still shows its review — the prose is the
        // point, and dropping it would silently shrink someone's page.
        let (title, poster) = films
            .get(&row.movie_id)
            .and_then(|f| f.clone())
            .unwrap_or_else(|| (data::title_from_slug(&row.movie_id), None));

        out.push(UserReview {
            id: db::review_id(&row.person_id, &row.movie_id),
            author_id: row.person_id.clone(),
            author_name: row.name.clone(),
            author_handle: row.handle.clone(),
            author_avatar: row.avatar.clone(),
            author_followed: row.followed,
            movie_id: row.movie_id.clone(),
            movie_title: title,
            poster,
            rating_half_stars: row.half_stars,
            body: row.body.clone(),
            written_on: map::long_date(&row.created_at)
                .unwrap_or_else(|| row.created_at.clone()),
        });
    }

    out
}

/// Follow or unfollow.
///
/// `Ok(None)` means the id names nobody — a 404. An `Err` is a failed write, which
/// has to stay distinguishable from that: a 404 tells the client the button was
/// never going to work, a 500 tells it to leave the button alone and try again.
pub fn set_follow(
    db: &Db,
    person_id: &str,
    target: Option<bool>,
) -> rusqlite::Result<Option<FollowState>> {
    let conn = lock(db);
    let Some(following) = db::set_follow(&conn, person_id, target)? else {
        return Ok(None);
    };
    Ok(Some(FollowState {
        person_id: person_id.to_string(),
        following,
        following_count: db::follow_count(&conn)?,
    }))
}

/// Store the visitor's bio and return the line their profile now shows.
///
/// Clearing it restores `VISITOR_BIO` rather than leaving the header blank, so the
/// fallback lives here — one place — rather than in the handler and the profile
/// builder separately.
pub fn set_bio(db: &Db, bio: &str) -> rusqlite::Result<String> {
    let conn = lock(db);
    Ok(db::set_visitor_bio(&conn, bio)?.unwrap_or_else(|| VISITOR_BIO.into()))
}

/// "5 films reviewed · generous ratings" — a followed person's line on the profile.
///
/// This used to read "Watched Interstellar • 2h ago", from the activity rail, which
/// is gone: neither the verb nor the timestamp corresponded to anything recorded, and
/// the film named was whichever trending title the rail happened to pair them with
/// that request. Their bio is written at harvest from their actual reviews, so it is a
/// claim the rows behind it support.
fn follow_subtitle(row: &db::FollowRow) -> String {
    if let Some(bio) = row.bio.as_deref().filter(|bio| !bio.trim().is_empty()) {
        return bio.to_string();
    }
    // Someone the harvest wrote no bio for. Their review count is the one thing this
    // can still say truthfully, and a follow with nothing to show says so.
    match row.review_count {
        0 => "No reviews yet".into(),
        1 => "1 film reviewed".into(),
        n => format!("{n} films reviewed"),
    }
}

/// Resolve stored film ids to titles and posters, dropping the ones that no longer
/// resolve and stopping once `wanted` have been found.
///
/// Sequential rather than concurrent: `wanted` is single digits, every detail call
/// is cached for 24h, and the ids arrive in an order the screen renders in.
async fn movies_for<'a>(
    source: &Source,
    ids: impl Iterator<Item = &'a str>,
    wanted: usize,
) -> Vec<Movie> {
    let mut out = Vec::new();
    for id in ids {
        if out.len() == wanted {
            break;
        }
        if let Some(detail) = movie_detail_by_id(source, id).await {
            out.push(Movie {
                id: detail.id,
                title: detail.title,
                year: detail.year,
                poster: detail.poster,
            });
        }
    }
    out
}

/// What the visitor has logged most recently, as the "Recent Reviews" tile draws it.
///
/// A row they wrote shows their own words; a row they only rated falls back to the
/// synopsis' first sentence, which is what the tile did before there was anywhere to
/// write.
async fn rated_films(source: &Source, journal: &[db::JournalRow]) -> Vec<RatedFilm> {
    let mut out = Vec::new();
    for row in journal {
        if out.len() == REVIEWS_SHOWN {
            break;
        }
        if let Some(detail) = movie_detail_by_id(source, &row.movie_id).await {
            out.push(RatedFilm {
                id: detail.id,
                title: detail.title,
                rating_half_stars: row.half_stars,
                body: row.body.clone(),
                // Only when there is no review to show, so the row never prints the
                // synopsis under prose that already says something better.
                blurb: match row.body {
                    Some(_) => None,
                    None => first_sentence(&detail.synopsis),
                },
            });
        }
    }
    out
}

/// The synopsis' opening sentence, for the one-line blurb under a rated film.
///
/// Truncating mid-word and appending an ellipsis is what the mock printed ("A
/// mesmerizing descent into paranoia..."), but a real synopsis has a sentence
/// break to cut at, and cutting there reads as prose rather than as clipped text.
fn first_sentence(synopsis: &str) -> Option<String> {
    let trimmed = synopsis.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find(". ") {
        Some(end) => Some(trimmed[..=end].trim_end().to_string()),
        None => Some(trimmed.to_string()),
    }
}

/// The newest reviews in the graph, the people the visitor follows first.
///
/// These are **our users' reviews**, out of SQLite, in both modes. There used to be
/// a second review system here that mapped TMDB's own prose into `Review` on the
/// fly, and it was the wrong shape twice over: its authors were bare strings
/// attached to no profile, so a review you could read led nowhere and a person whose
/// page you could open had reviews you couldn't; and the harvest had already
/// imported those very reviewers *as our users*, so it duplicated data we own. One
/// system now, and every review on screen belongs to somebody with a page.
pub async fn reviews(source: &Source, db: &Db) -> Vec<Review> {
    let rows = {
        let conn = lock(db);
        db::recent_reviews(&conn, RECENT_REVIEWS).unwrap_or_default()
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        // Sequential rather than concurrent: consecutive rows are often about the
        // same film, and `Tmdb::movie` caches on the path, so the second one is a
        // hit. Spawning would race them into duplicate upstream calls instead.
        if let Some(review) = full_review(source, row).await {
            out.push(review);
        }
    }
    out
}

/// One review by its `<person>-<film>` id.
pub async fn review_by_id(source: &Source, db: &Db, id: &str) -> Option<Review> {
    let row = {
        let conn = lock(db);
        db::review_by_id(&conn, id).ok().flatten()?
    };
    full_review(source, &row).await
}

/// One stored review as the full review screen draws it: the prose, plus the film
/// it is about.
///
/// `None` when the film can't be resolved, which is the one case where this differs
/// from the clamped card — that card is a line of prose in a list and stands up
/// without the film, whereas this whole page is *about* a film. Its backdrop, its
/// poster, its genres and its director are the page around the text, and rendering
/// them as blanks would be worse than a 404.
async fn full_review(source: &Source, row: &db::UserReviewRow) -> Option<Review> {
    let detail = movie_detail_by_id(source, &row.movie_id).await?;

    Some(Review {
        id: db::review_id(&row.person_id, &row.movie_id),
        movie: Movie {
            id: detail.id,
            title: detail.title,
            year: detail.year,
            poster: detail.poster,
        },
        backdrop: Some(detail.backdrop),
        // Both out of the film's own credits grid, which is where the detail page
        // draws them from too — so the two screens can't disagree about who
        // directed it.
        director: detail
            .details
            .iter()
            .find(|fact| fact.label == "Director")
            .map(|fact| fact.value.clone()),
        genres: detail.genres,
        author_id: row.person_id.clone(),
        author_name: row.name.clone(),
        author_handle: row.handle.clone(),
        author_avatar: row.avatar.clone(),
        author_followed: row.followed,
        watched_on: match map::long_date(&row.created_at) {
            Some(date) => format!("Reviewed on {date}"),
            None => "Reviewed recently".into(),
        },
        rating_half_stars: row.half_stars,
        paragraphs: map::paragraphs(&row.body),
        // No stored count: nobody but the visitor can like anything yet, so the
        // button reads nothing until they do and 1 after — see `hydrate::like_count`.
        like_count: None,
        comments: Vec::new(),
        liked: false,
    })
}

/// `GET /api/movies` — the detail pages of the trending films.
pub async fn movie_details(source: &Source) -> Vec<MovieDetail> {
    let Some(client) = source.client() else {
        return data::movie_details();
    };

    let images = client.images().await;
    let Ok(summaries) = client.trending().await else {
        return data::movie_details();
    };

    let mut details = Vec::new();
    for summary in summaries.iter().take(TRENDING_NEEDED) {
        // Sequential rather than a `JoinSet`: after the first request these are
        // all cache hits, and the endpoint is unused by the frontend.
        if let Ok(film) = client.movie(summary.id).await {
            details.push(map::movie_detail(&film, &images));
        }
    }
    details
}

/// One detail page.
///
/// `None` is a real 404 in TMDB mode — the demo's "every id resolves" behaviour
/// existed only because a single film was ever designed.
pub async fn movie_detail_by_id(source: &Source, id: &str) -> Option<MovieDetail> {
    let Some(client) = source.client() else {
        return Some(data::movie_detail_by_id(id));
    };

    // `157336-interstellar` and a bare `157336` both work; the slug is decoration.
    let tmdb_id = map::tmdb_id(id)?;
    let images = client.images().await;
    match client.movie(tmdb_id).await {
        Ok(film) => Some(map::movie_detail(&film, &images)),
        Err(error) => {
            tracing::warn!(%error, id, "no detail page");
            None
        }
    }
}

/// The search screen.
///
/// Two upstream routes, one local candidate window — see `SEARCH_PAGES`.
pub async fn search(source: &Source, query: &SearchQuery) -> SearchResponse {
    let Some(client) = source.client() else {
        return data::search(query);
    };

    match search_tmdb(client, query).await {
        Ok(response) => response,
        // A 404 means the query named something that doesn't exist — a person id
        // typed by hand. Demo films under that heading would be worse than nothing,
        // so the honest answer is an empty grid.
        Err(error) if error.is_missing() => {
            tracing::info!(%error, "no such record; serving an empty result set");
            empty_search(query)
        }
        Err(error) => {
            tracing::warn!(%error, "falling back to demo search results");
            data::search(query)
        }
    }
}

/// A well-formed search response with nothing in it.
///
/// The screen has to render *something*: its own copy already says "No films match
/// your filters", and every chip reading 0 is the truthful count for a set of no
/// films.
fn empty_search(query: &SearchQuery) -> SearchResponse {
    SearchResponse {
        query: query.q.clone().unwrap_or_default(),
        total_results: 0,
        results: Vec::new(),
        filters: window_facets(
            &[],
            "",
            query.genre.as_deref().filter(|g| !g.is_empty()),
            query.year.as_deref().filter(|y| !y.is_empty()),
            query.min_rating.unwrap_or(0),
        ),
        page: 1,
        page_count: 1,
        // Deliberately `None` even though `person=` was set: the id resolved to
        // nobody, and naming it would put a made-up label over an empty grid.
        person: None,
    }
}

async fn search_tmdb(client: &Arc<Tmdb>, query: &SearchQuery) -> tmdb::Result<SearchResponse> {
    let text = query.q.as_deref().unwrap_or("").trim();
    let genre = query.genre.as_deref().filter(|g| !g.is_empty());
    let year = query.year.as_deref().filter(|y| !y.is_empty());
    let min_rating = query.min_rating.unwrap_or(0);
    let requested_page = query.page.unwrap_or(1).max(1);
    // A person filter takes over the whole search, because their filmography *is*
    // the candidate set — narrower than anything `/search/movie` or `/discover`
    // would return, and complete in one request. The text box, the genre chips, the
    // decade and the rating all still apply, but locally over that set.
    if let Some(person) = query.person.as_deref().filter(|p| !p.is_empty()) {
        // A `person=` that isn't a number names nobody, so it filters to nothing
        // rather than being dropped — silently ignoring it would show all of TMDB
        // under a link the visitor thinks narrowed it.
        let Ok(id) = person.parse::<u32>() else {
            return Ok(empty_search(query));
        };
        return filmography_search(client, query, id, text, genre, year, min_rating, requested_page)
            .await;
    }

    if text.is_empty() {
        browse(client, query, genre, year, min_rating, requested_page).await
    } else {
        text_search(client, query, text, genre, year, min_rating, requested_page).await
    }
}

/// One person's films: what a cast portrait or a credits-grid name leads to.
///
/// `/person/{id}?append_to_response=movie_credits` returns the entire filmography in
/// one cached response, so unlike the text route there is no window here — the
/// counts and the results are over everything they were credited on. Which also
/// makes text *and* person work together, something no upstream endpoint offers:
/// `/search/movie` has no person parameter and `/discover` has no text one.
///
/// An unresolvable id is not an error: the filter names nobody, `person` comes back
/// `None`, and the screen shows an empty grid rather than a failed page.
#[allow(clippy::too_many_arguments)]
async fn filmography_search(
    client: &Arc<Tmdb>,
    query: &SearchQuery,
    person_id: u32,
    text: &str,
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
    requested_page: u32,
) -> tmdb::Result<SearchResponse> {
    let images = client.images().await;
    let person = client.person(person_id).await?;
    let candidates = map::filmography(&person, &images);

    let lowercased = text.to_lowercase();
    let matched: Vec<&data::CatalogueEntry> = candidates
        .iter()
        // Applied locally here, unlike the text route, where TMDB already matched
        // it upstream. A substring over the one title we hold is all this can do —
        // narrowing a filmography, which is the job here, rather than discovering
        // films by name.
        .filter(|entry| entry.matches_text(&lowercased))
        .filter(|entry| genre.is_none_or(|g| entry.has_genre(g)))
        .filter(|entry| year.is_none_or(|y| entry.in_decade(y)))
        .filter(|entry| entry.meets_minimum(min_rating))
        .collect();

    let page_count = matched.len().div_ceil(PAGE_SIZE).max(1) as u32;
    let page = requested_page.min(page_count);
    let results = matched
        .iter()
        .skip((page as usize - 1) * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|entry| entry.to_search_result())
        .collect();

    Ok(SearchResponse {
        query: query.q.clone().unwrap_or_default(),
        total_results: matched.len() as u32,
        results,
        // Counted over the filmography, so a chip reading "9" yields nine films of
        // theirs — the same set-agreement invariant the other two routes keep.
        filters: window_facets(&candidates, &lowercased, genre, year, min_rating),
        page,
        page_count,
        person: Some(CreditedPerson { name: person.name.clone(), id: person_id.to_string() }),
    })
}

/// No text: TMDB applies the filters and counts the matches, so both the results
/// and the chip counts are exact across the whole catalogue rather than a window.
///
/// The screen shows 8 cards to TMDB's 20, so one upstream page covers 2.5 of ours
/// and page 3 straddles two of theirs. Rather than track that, this fetches the one
/// upstream page containing the requested slice, plus the next when the slice
/// crosses the boundary.
async fn browse(
    client: &Arc<Tmdb>,
    query: &SearchQuery,
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
    requested_page: u32,
) -> tmdb::Result<SearchResponse> {
    let filters = |page: u32| DiscoverFilters {
        genre_id: genre.and_then(map::genre_id),
        released_between: year.and_then(decade_bounds),
        // The sidebar's stars are out of 5; TMDB votes are out of 10.
        min_vote_average: (min_rating > 0).then(|| f32::from(min_rating) * 2.0),
        page,
    };

    let images = client.images().await;

    // Which of our 8-card pages this is, translated into TMDB's 20-item pages.
    let first_item = (requested_page as usize - 1) * PAGE_SIZE;
    let upstream_page = (first_item / tmdb::UPSTREAM_PAGE_SIZE as usize) as u32 + 1;
    let offset = first_item % tmdb::UPSTREAM_PAGE_SIZE as usize;

    let head = client.discover(&filters(upstream_page)).await?;
    let mut summaries = head.results.clone();
    if offset + PAGE_SIZE > summaries.len() && upstream_page < head.total_pages {
        // The slice runs off the end of this upstream page; pull the next one.
        let tail = client.discover(&filters(upstream_page + 1)).await?;
        summaries.extend(tail.results);
    }

    let results: Vec<SearchResult> = summaries
        .iter()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|s| map::search_result(s, &images).to_search_result())
        .collect();

    // TMDB reports the true total, and won't serve past page 500 — so the paginator
    // must not offer a page the next click would 400 on.
    let total_results = head.total_results;
    let reachable = (tmdb::MAX_DISCOVER_PAGE * tmdb::UPSTREAM_PAGE_SIZE) as usize;
    let page_count =
        (total_results as usize).min(reachable).div_ceil(PAGE_SIZE).max(1) as u32;

    Ok(SearchResponse {
        query: query.q.clone().unwrap_or_default(),
        total_results,
        results,
        filters: discover_facets(client, genre, year, min_rating).await,
        page: requested_page.min(page_count),
        page_count,
        // No `person=` reached this route — `search_tmdb` sends those to
        // `filmography_search` before either of the other two are considered.
        person: None,
    })
}

/// Free text: `/search/movie` ignores every filter, so a window of candidates is
/// pulled and genre, decade and rating are applied here.
///
/// Both the results and the chip counts come from that one window, which is what
/// keeps a chip from reading "4" and then yielding nothing. `total_results` is
/// therefore matches *within the window* — stated in the README rather than hidden.
async fn text_search(
    client: &Arc<Tmdb>,
    query: &SearchQuery,
    text: &str,
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
    requested_page: u32,
) -> tmdb::Result<SearchResponse> {
    let images = client.images().await;
    let summaries = client.search(text, SEARCH_PAGES).await?;
    let candidates: Vec<data::CatalogueEntry> =
        summaries.iter().map(|s| map::search_result(s, &images)).collect();

    // The text has already been applied upstream, so it isn't re-tested here: TMDB
    // matches alternative titles and translations that a substring check over the
    // one title we hold would reject, and dropping those would contradict the
    // result count the user just saw.
    let matched: Vec<&data::CatalogueEntry> = candidates
        .iter()
        .filter(|entry| genre.is_none_or(|g| entry.has_genre(g)))
        .filter(|entry| year.is_none_or(|y| entry.in_decade(y)))
        .filter(|entry| entry.meets_minimum(min_rating))
        .collect();

    let page_count = matched.len().div_ceil(PAGE_SIZE).max(1) as u32;
    let page = requested_page.min(page_count);
    let results = matched
        .iter()
        .skip((page as usize - 1) * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|entry| entry.to_search_result())
        .collect();

    Ok(SearchResponse {
        query: query.q.clone().unwrap_or_default(),
        total_results: matched.len() as u32,
        results,
        // Empty text, because TMDB already applied it — re-testing it locally would
        // drop the alternative titles and translations it matched on.
        filters: window_facets(&candidates, "", genre, year, min_rating),
        page,
        page_count,
        person: None,
    })
}

/// The sidebar's chips in browse mode, each an exact count from TMDB.
///
/// Leave-one-out, as in `data::facets`: a genre chip's count ignores the current
/// genre selection but honours the decade and the rating floor, so picking one chip
/// doesn't zero all the others. Eight counts, so all eight `total_results` lookups
/// run concurrently — sequentially this would be eight round trips deep.
async fn discover_facets(
    client: &Arc<Tmdb>,
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
) -> SearchFilters {
    let min_vote = (min_rating > 0).then(|| f32::from(min_rating) * 2.0);

    let mut tasks = tokio::task::JoinSet::new();
    for (index, label) in data::GENRE_FACETS.iter().enumerate() {
        let client = Arc::clone(client);
        let filters = DiscoverFilters {
            genre_id: map::genre_id(label),
            released_between: year.and_then(decade_bounds),
            min_vote_average: min_vote,
            page: 1,
        };
        tasks.spawn(async move {
            (true, index, client.discover(&filters).await.map(|p| p.total_results).unwrap_or(0))
        });
    }
    for (index, label) in data::YEAR_FACETS.iter().enumerate() {
        let client = Arc::clone(client);
        let filters = DiscoverFilters {
            genre_id: genre.and_then(map::genre_id),
            released_between: decade_bounds(label),
            min_vote_average: min_vote,
            page: 1,
        };
        tasks.spawn(async move {
            (false, index, client.discover(&filters).await.map(|p| p.total_results).unwrap_or(0))
        });
    }

    let mut genre_counts = [0u32; data::GENRE_FACETS.len()];
    let mut year_counts = [0u32; data::YEAR_FACETS.len()];
    while let Some(result) = tasks.join_next().await {
        // A panicked task means a bug, not a bad count; skipping it leaves that
        // chip at zero rather than failing the whole screen.
        if let Ok((is_genre, index, count)) = result {
            if is_genre {
                genre_counts[index] = count;
            } else {
                year_counts[index] = count;
            }
        }
    }

    SearchFilters {
        genres: data::GENRE_FACETS
            .iter()
            .zip(genre_counts)
            .map(|(label, count)| GenreFacet {
                label: (*label).into(),
                selected: genre == Some(label),
                count,
            })
            .collect(),
        years: data::YEAR_FACETS
            .iter()
            .zip(year_counts)
            .map(|(label, count)| YearFacet {
                label: (*label).into(),
                selected: year == Some(label),
                count,
            })
            .collect(),
        minimum_rating_stars: min_rating,
    }
}

/// The sidebar's chips counted over a local candidate set rather than upstream:
/// the text route's window, or a person's filmography.
///
/// `lowercased` is the text still to apply — empty for the text route, where TMDB
/// already matched it and re-testing it here would drop the alternative titles and
/// translations it matched on. See `data::facets`, which does the same for the demo
/// catalogue.
fn window_facets(
    candidates: &[data::CatalogueEntry],
    lowercased: &str,
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
) -> SearchFilters {
    let pool: Vec<&data::CatalogueEntry> = candidates
        .iter()
        .filter(|entry| entry.matches_text(lowercased))
        .filter(|entry| entry.meets_minimum(min_rating))
        .collect();

    let genres = data::GENRE_FACETS
        .iter()
        .map(|label| GenreFacet {
            label: (*label).into(),
            selected: genre == Some(label),
            count: pool
                .iter()
                .filter(|entry| entry.has_genre(label))
                .filter(|entry| year.is_none_or(|y| entry.in_decade(y)))
                .count() as u32,
        })
        .collect();

    let years = data::YEAR_FACETS
        .iter()
        .map(|label| YearFacet {
            label: (*label).into(),
            selected: year == Some(label),
            count: pool
                .iter()
                .filter(|entry| entry.in_decade(label))
                .filter(|entry| genre.is_none_or(|g| entry.has_genre(g)))
                .count() as u32,
        })
        .collect();

    SearchFilters { genres, years, minimum_rating_stars: min_rating }
}

/// "2010s" -> (2010, 2019). Inclusive, and `None` for a label that isn't a decade.
fn decade_bounds(label: &str) -> Option<(u16, u16)> {
    let start: u16 = label.trim_end_matches('s').parse().ok()?;
    Some((start, start + 9))
}

// --- Shared plumbing ----------------------------------------------------------

// `trending(client)` was here, returning the week's films paired with their votes.
// Its only callers were the feed's rails, which paired trending film *i* with rail
// row *i* — the mechanism that let invented "Live Now" rooms and activity lines point
// at real posters. Both rails are gone, and the two places that still want the
// trending list (`harvest_graph` and the search screen's empty state) call
// `client.trending()` directly for their own reasons.

/// Whether a review id names a review we hold.
///
/// Used by the mutation handlers to reject a bogus id before writing a like or a
/// comment nothing will ever render. Deliberately a plain SQLite lookup rather than
/// `review_by_id`, which resolves the film: liking a review must not depend on TMDB
/// being reachable.
pub fn review_exists(db: &Db, id: &str) -> bool {
    let conn = lock(db);
    db::review_by_id(&conn, id).ok().flatten().is_some()
}

/// Whether this comment exists. Guards replies and likes against ids nothing
/// renders.
///
/// Every comment there is was posted by the visitor — reviews arrive with none —
/// so SQLite is the whole answer.
pub fn comment_exists(db: &Db, review_id: &str, comment_id: &str) -> bool {
    let conn = lock(db);
    db::comment_exists(&conn, review_id, comment_id).unwrap_or(false)
}

/// A review with the visitor's likes, comments and replies folded in.
pub async fn hydrated_review(source: &Source, db: &Db, id: &str) -> Option<Review> {
    let review = review_by_id(source, db, id).await?;
    Some(hydrate::review(review, &store(db)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo path must not touch the network, so `Source::Demo` has to be
    /// buildable and usable without a client.
    #[tokio::test]
    async fn demo_mode_serves_the_catalogue_and_invents_no_social_life() {
        let source = Source::Demo { reason: "testing".into() };
        let conn = db::open(":memory:").unwrap();
        let db: Db = Arc::new(Mutex::new(conn));

        // Both feeds are empty for a visitor who follows nobody and has logged
        // nothing, in either mode. This used to be the export's two "Live Now" rooms,
        // four "Recent Entries" and three activity lines — every one of them invented,
        // which is the point: an empty feed is the true answer here.
        let page = feed_page(&source, &db, None).await;
        assert!(page.items.is_empty());
        // And it says so rather than offering a cursor: a client handed one would ask
        // for a second empty page, and go on asking.
        assert!(page.next_cursor.is_none());

        let mobile = mobile_feed(&source, &db).await;
        assert!(mobile.stories.is_empty());
        assert!(mobile.items.is_empty());

        // Reviews are the graph's, not the export's, so an unseeded database has
        // none — which is a different claim from "the export has none".
        assert!(reviews(&source, &db).await.is_empty());

        // The demo's "every id resolves" behaviour, which TMDB mode replaces with
        // a real 404.
        assert!(movie_detail_by_id(&source, "anything-at-all").await.is_some());
    }

    /// With a graph and a journal, both feeds fill from them — and only from them.
    ///
    /// The recommendation rail stays empty without a token, since it is the one
    /// section that needs TMDB and has nothing honest to fall back to.
    #[tokio::test]
    async fn the_feeds_are_built_from_follows_and_the_journal() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));
        let followed: Vec<String> = {
            let conn = lock(&db);
            db::seed_graph(&conn, &db::demo_graph()).unwrap();
            db::set_rating(&conn, "neon-reverie", 8).unwrap();
            db::following(&conn).unwrap().into_iter().map(|row| row.id).collect()
        };

        let page = feed_page(&source, &db, None).await;
        assert!(!page.items.is_empty());

        let reviews: Vec<&UserReview> = page
            .items
            .iter()
            .filter_map(|item| match item {
                FeedItem::Review(review) => Some(review),
                _ => None,
            })
            .collect();
        assert!(!reviews.is_empty());
        // Every review card is by somebody the visitor follows. A feed of strangers is
        // not a feed.
        for review in &reviews {
            assert!(followed.contains(&review.author_id), "{} is not followed", review.author_id);
            assert!(review.author_followed);
        }

        // The entry cards are the visitor's journal: the film they just rated, once.
        let entries: Vec<&FeedEntry> = page
            .items
            .iter()
            .filter_map(|item| match item {
                FeedItem::Entry(entry) => Some(entry),
                _ => None,
            })
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].movie.id, "neon-reverie");
        assert_eq!(entries[0].rating_half_stars, 8);

        assert!(
            !page.items.iter().any(|item| matches!(item, FeedItem::Recommendation(_))),
            "no token, so nothing to recommend from"
        );

        // The mobile rail is the same graph, one circle per followed person, each
        // opening a review that really exists.
        let mobile = mobile_feed(&source, &db).await;
        assert_eq!(mobile.stories.len(), followed.len().min(STORIES_SHOWN));
        for story in &mobile.stories {
            if let Some(id) = &story.review_id {
                assert!(story.unseen, "a circle with a review to open draws its ring");
                assert!(review_exists(&db, id));
            } else {
                assert!(!story.unseen);
            }
        }

        // And the cards are those reviews, subtitled with their author's first name.
        // Not the same count as the page above: this screen draws one fixed grid, while
        // the desktop feed pages through the whole window.
        assert!(!mobile.items.is_empty());
        assert!(mobile.items.iter().all(|item| item.review_id.is_some()));
        // "Elena rated it" — one word for the author, since the card has room for one.
        let subtitle = &mobile.items[0].subtitle;
        assert!(subtitle.ends_with(" rated it"), "{subtitle}");
        assert_eq!(subtitle.split_whitespace().count(), 3, "{subtitle}");
    }

    /// The cursor survives a round trip and refuses everything else.
    ///
    /// It goes out to a client and comes back as a URL parameter, so anything at all can
    /// arrive here. A rejection is not an error: `feed_page` starts over, which is what a
    /// stale cursor in a tab left open across a deploy should do.
    #[test]
    fn a_cursor_round_trips_and_rejects_junk() {
        let cursor = FeedCursor { reviews: 8, entries: 4, recommendations: 2 };
        assert_eq!(cursor.to_string(), "8.4.2");
        assert_eq!(FeedCursor::parse("8.4.2"), Some(cursor));
        assert_eq!(cursor.consumed(), 14);

        for junk in ["", "8", "8.4", "8.4.2.1", "a.b.c", "-1.0.0", "8.4.2 ", "8..2"] {
            assert_eq!(FeedCursor::parse(junk), None, "{junk:?} must not parse");
        }

        // And the public wrapper normalises rather than trusting the string: what comes
        // back is what this module would have written, or nothing.
        assert_eq!(feed_cursor(Some("8.4.2")).as_deref(), Some("8.4.2"));
        assert_eq!(feed_cursor(Some("8.4.2.1")), None);
        assert_eq!(feed_cursor(None), None);
    }

    /// The feed pages through the graph and then stops.
    ///
    /// The two things a client depends on: pages don't repeat cards, and the last one says
    /// it's the last. A feed that kept handing out cursors would be fetched forever.
    #[tokio::test]
    async fn the_feed_pages_without_repeating_and_then_ends() {
        let (source, db) = graph().await;
        {
            let conn = lock(&db);
            db::set_rating(&conn, "neon-reverie", 8).unwrap();
        }

        let mut seen: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0;

        loop {
            let page = feed_page(&source, &db, cursor.as_deref()).await;
            pages += 1;
            assert!(page.items.len() <= FEED_PAGE_SIZE);
            assert!(!page.from_cache, "content never claims to have come from the cache");
            for item in &page.items {
                let id = match item {
                    FeedItem::Review(review) => format!("review-{}", review.id),
                    FeedItem::Entry(entry) => format!("entry-{}", entry.movie.id),
                    FeedItem::Recommendation(rec) => format!("rec-{}", rec.movie.id),
                };
                assert!(!seen.contains(&id), "{id} appeared twice");
                seen.push(id);
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
            assert!(pages < 40, "the feed should have ended by now");
        }

        assert!(pages > 1, "a seeded graph fills more than one page");
        assert!(seen.len() <= FEED_MAX_ITEMS);
        // The visitor's one rated film is in there, and only once — the journal is a
        // source like the others, not a rail bolted on the side.
        assert_eq!(seen.iter().filter(|id| *id == "entry-neon-reverie").count(), 1);
    }

    /// A collection is the tile's page: the same films, uncapped.
    #[tokio::test]
    async fn a_collection_is_the_uncapped_version_of_its_tile() {
        let (source, db) = graph().await;
        {
            let conn = lock(&db);
            for id in ["neon-reverie", "le-souffle", "the-drop"] {
                db::set_favorite(&conn, id, Some(true)).unwrap();
            }
            db::set_rating(&conn, "red-shift", 7).unwrap();
        }

        let favorites = collection(&source, &db, "favorites", None).await.expect("the visitor's");
        assert_eq!(favorites.slug, "favorites");
        assert_eq!(favorites.title, "Favorite Films");
        assert!(favorites.owner.is_none(), "the visitor's own has no owner header");
        // Three, where the profile tile shows at most `FAVORITES_SHOWN` — that gap is the
        // whole reason this page exists.
        assert_eq!(favorites.movies.len(), 3);
        assert!(favorites.movies.iter().all(|m| m.rating_half_stars.is_none()));

        // The journal is the one collection with stars behind it.
        let journal = collection(&source, &db, "journal", None).await.expect("the visitor's");
        assert_eq!(journal.movies.len(), 1);
        assert_eq!(journal.movies[0].movie.id, "red-shift");
        assert_eq!(journal.movies[0].rating_half_stars, Some(7));

        // Somebody else's carries a header saying whose it is.
        let theirs = collection(&source, &db, "favorites", Some("elenarostova"))
            .await
            .expect("a seeded user");
        let owner = theirs.owner.expect("somebody else's names them");
        assert_eq!(owner.handle, "@elenarostova");
        assert!(theirs.description.contains("Elena"), "{}", theirs.description);
    }

    /// Every way of naming nothing is a 404, not an empty grid.
    ///
    /// An empty grid would say "this collection exists and has no films in it", which is a
    /// different and false claim about a URL that names nothing.
    #[tokio::test]
    async fn an_unknown_collection_is_a_404() {
        let (source, db) = graph().await;

        assert!(collection(&source, &db, "everything", None).await.is_none());
        assert!(collection(&source, &db, "favorites", Some("nobody-at-all")).await.is_none());
        // Somebody else has no journal: their reviews are already their page.
        assert!(collection(&source, &db, "journal", Some("elenarostova")).await.is_none());

        assert!(is_collection("watchlist"));
        assert!(!is_collection("Watchlist"), "slugs are the URL's, and the URL is lowercase");
    }

    /// A suggestion may not claim the visitor liked a film they only bookmarked.
    ///
    /// The rail asks TMDB about favourites first and then watchlist entries, and both
    /// kinds reach the same card — so the wording has to follow the seed rather than be
    /// fixed. It was fixed at "Because you liked X", which said that about a watchlisted
    /// film the visitor may not have seen. Pure, so no network and no token.
    #[test]
    fn a_suggestion_says_which_kind_of_seed_it_came_from() {
        let favorites = vec!["157336-interstellar".to_string()];
        let watchlist = vec!["1339713-obsession".to_string(), "9003-hellraiser".to_string()];

        let mixed = seeds(&favorites, &watchlist);
        assert_eq!(mixed.len(), RECOMMEND_SEEDS);
        // Favourite first, and it keeps its flag; the watchlist follows behind.
        assert_eq!(mixed[0].id, "157336-interstellar");
        assert!(mixed[0].favorite);
        assert!(mixed[1..].iter().all(|seed| !seed.favorite));

        // More seeds than the budget: the watchlist is what gets cut, never a favourite.
        let many = vec!["a".to_string(), "b".into(), "c".into(), "d".into()];
        let all_favorites = seeds(&many, &watchlist);
        assert_eq!(all_favorites.len(), RECOMMEND_SEEDS);
        assert!(all_favorites.iter().all(|seed| seed.favorite));

        let rec = |favorite| Recommendation {
            movie: Movie {
                id: "9003-hellraiser".into(),
                title: "Hellraiser".into(),
                year: None,
                poster: missing_poster(),
            },
            star_rating: None,
            because: "Obsession".into(),
            because_movie_id: "1339713-obsession".into(),
            because_favorite: favorite,
            on_watchlist: false,
        };
        assert_eq!(because_line(&rec(true)), "Because you liked Obsession");
        assert_eq!(because_line(&rec(false)), "Because Obsession is on your watchlist");
    }

    /// The review screen and the film page draw the same review, expanded and
    /// clamped — so one id has to reach it from either direction, and the two must
    /// agree about who wrote it.
    #[tokio::test]
    async fn one_id_reaches_a_review_from_the_card_and_the_page() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));
        {
            let conn = lock(&db);
            db::seed_graph(&conn, &db::demo_graph()).unwrap();
        }

        // The clamped card, as a film page lists it.
        let cards = reviews_of_movie(&source, &db, "dune-part-two").await;
        assert!(!cards.is_empty(), "the demo graph has reviews of this film");
        let card = &cards[0];

        // The same id, expanded.
        let full = review_by_id(&source, &db, &card.id).await.expect("the card's id resolves");
        assert_eq!(full.id, card.id);
        assert_eq!(full.author_name, card.author_name);
        assert_eq!(full.author_handle, card.author_handle);
        assert_eq!(full.author_id, card.author_id);
        assert_eq!(full.rating_half_stars, card.rating_half_stars);
        assert_eq!(full.movie.id, "dune-part-two");
        // The prose, split into paragraphs rather than clamped to four lines.
        assert!(!full.paragraphs.is_empty());
        assert_eq!(full.paragraphs.join(" "), card.body);

        // And the guards the mutation handlers use agree with the lookup.
        assert!(review_exists(&db, &card.id));
        assert!(!review_exists(&db, "user-nobody-dune-part-two"));
        assert!(review_by_id(&source, &db, "user-nobody-dune-part-two").await.is_none());
    }

    /// `GET /api/reviews` opens on people the visitor follows.
    #[tokio::test]
    async fn the_review_list_puts_followed_people_first() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));
        {
            let conn = lock(&db);
            db::seed_graph(&conn, &db::demo_graph()).unwrap();
        }

        let listed = reviews(&source, &db).await;
        assert!(!listed.is_empty());
        assert!(listed.len() <= RECENT_REVIEWS as usize);
        assert!(listed[0].author_followed, "the list opens on a friend");

        // Followed authors come as one block, not interleaved.
        let followed: Vec<bool> = listed.iter().map(|r| r.author_followed).collect();
        let first_stranger = followed.iter().position(|f| !f).unwrap_or(followed.len());
        assert!(followed[first_stranger..].iter().all(|f| !f));
    }

    /// The profile has no `data::` fallback because it needs none — every strip
    /// below the header is the visitor's own rows, which exist in both modes.
    #[tokio::test]
    async fn the_profile_reflects_the_visitors_own_rows() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));

        // An untouched visitor: a header and four empty strips. Empty rather than
        // borrowed posters — see `Profile::favorites`. "Following" is empty too,
        // because on a database with no graph in it the visitor follows nobody.
        let empty = profile(&source, &db).await;
        assert_eq!(empty.name, "Alex Mercer");
        assert_eq!(empty.handle, "@alexm_cinema");
        assert_eq!(empty.bio, VISITOR_BIO, "an unedited bio is the export's line");
        assert!(empty.favorites.is_empty());
        assert!(empty.watchlist.is_empty());
        assert!(empty.recent_reviews.is_empty());
        assert!(empty.following.is_empty());
        assert_eq!(empty.following_count, 0);

        // With the graph seeded, the count and the list agree — and both agree with
        // the friend directory, which is the point of taking the count from the
        // graph rather than from the list's length.
        {
            let conn = lock(&db);
            db::seed_graph(&conn, &db::demo_graph()).unwrap();
        }
        let seeded = profile(&source, &db).await;
        assert_eq!(seeded.following_count as usize, seeded.following.len());
        assert_eq!(seeded.following_count as usize, people(&db, "").following.len());
        assert!(!seeded.following.is_empty());
        assert!(seeded.following.iter().all(|f| !f.subtitle.is_empty()));
        assert!(seeded.following.iter().all(|f| f.handle.is_some()));

        {
            let conn = lock(&db);
            db::set_watchlist(&conn, "le-souffle", Some(true)).unwrap();
            db::set_watchlist(&conn, "red-shift", Some(true)).unwrap();
            db::set_rating(&conn, "neon-reverie", 9).unwrap();
            db::set_rating(&conn, "the-drop", 5).unwrap();
            db::set_favorite(&conn, "the-drop", Some(true)).unwrap();
        }

        let filled = profile(&source, &db).await;
        // Newest first, and both resolved to a real title and poster.
        assert_eq!(
            filled.watchlist.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["red-shift", "le-souffle"]
        );
        assert_eq!(filled.watchlist[0].title, "Red Shift");

        // Favourites are what the visitor pressed the heart on — not their
        // highest rating, which is `neon-reverie` at 9 half-stars.
        assert_eq!(
            filled.favorites.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["the-drop"]
        );
        assert_eq!(filled.recent_reviews.len(), 2);
        assert!(filled.recent_reviews.iter().any(|r| r.rating_half_stars == Some(9)));
        assert!(filled.recent_reviews[0].blurb.is_some());
    }

    /// Prose the visitor wrote reaches their profile, and it replaces the synopsis
    /// blurb — a Recent Reviews tile printing the film's own marketing copy under
    /// the heading "your review" was the thing that made it not one.
    #[tokio::test]
    async fn what_the_visitor_writes_lands_on_their_profile() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));

        {
            let conn = lock(&db);
            db::set_rating(&conn, "neon-reverie", 9).unwrap();
            db::set_visitor_review(&conn, "le-souffle", "Two hours of held breath.").unwrap();
        }

        let filled = profile(&source, &db).await;
        assert_eq!(filled.recent_reviews.len(), 2, "a rating and a review are both entries");

        let written = filled.recent_reviews.iter().find(|r| r.id == "le-souffle").unwrap();
        assert_eq!(written.title, "Le Souffle");
        assert_eq!(written.body.as_deref(), Some("Two hours of held breath."));
        assert_eq!(written.rating_half_stars, None, "written about, never scored");
        assert!(written.blurb.is_none(), "the synopsis stood in for the visitor's own words");

        // A rating with no prose still shows the film's first sentence, which is what
        // that tile has always drawn.
        let rated = filled.recent_reviews.iter().find(|r| r.id == "neon-reverie").unwrap();
        assert_eq!(rated.rating_half_stars, Some(9));
        assert!(rated.body.is_none());
        assert!(rated.blurb.is_some());
    }

    /// The bio is the one field of the visitor's identity they own. Clearing it puts
    /// the export's line back rather than leaving the header blank.
    #[tokio::test]
    async fn the_bio_can_be_edited_and_reset() {
        let source = Source::Demo { reason: "testing".into() };
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));

        assert_eq!(set_bio(&db, "  Only watches sequels. ").unwrap(), "Only watches sequels.");
        assert_eq!(profile(&source, &db).await.bio, "Only watches sequels.");

        assert_eq!(set_bio(&db, "").unwrap(), VISITOR_BIO);
        assert_eq!(profile(&source, &db).await.bio, VISITOR_BIO);
    }

    #[test]
    fn a_blurb_stops_at_the_first_sentence() {
        assert_eq!(first_sentence("One thing. Then another."), Some("One thing.".into()));
        // No sentence break to cut at: the whole line, rather than a truncation.
        assert_eq!(first_sentence("A single clause"), Some("A single clause".into()));
        assert_eq!(first_sentence("   "), None);
        // A decimal or an initial isn't a sentence end — `". "` needs the space.
        assert_eq!(first_sentence("Rated 7.8 by critics"), Some("Rated 7.8 by critics".into()));
    }

    #[test]
    fn the_status_message_only_appears_in_demo_mode() {
        let demo = status(&Source::Demo { reason: "No TMDB_TOKEN is set.".into() });
        assert_eq!(demo.data_source, DataSource::Demo);
        let message = demo.message.expect("demo mode must explain itself");
        assert!(message.contains("No TMDB_TOKEN"));
        assert!(message.contains("TMDB_TOKEN"), "the message must name the variable to set");
    }

    #[test]
    fn decades_become_inclusive_year_bounds() {
        assert_eq!(decade_bounds("2010s"), Some((2010, 2019)));
        assert_eq!(decade_bounds("2000s"), Some((2000, 2009)));
        // The sidebar only offers decades, but a hand-typed label must not panic.
        assert_eq!(decade_bounds("nineties"), None);
        assert_eq!(decade_bounds(""), None);
    }

    /// A poisoned lock is recovered rather than propagated — see `lock`.
    #[test]
    fn a_poisoned_lock_still_serves_requests() {
        let db: Db = Arc::new(Mutex::new(db::open(":memory:").unwrap()));

        let poisoner = Arc::clone(&db);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("poison the mutex");
        })
        .join();

        assert!(db.is_poisoned());
        // The visitor's state is still readable, which is the whole point.
        assert!(store(&db).watchlist.is_empty());
    }

    // --- The social graph -----------------------------------------------------

    /// Demo mode with the graph seeded — what the friend screens boot with when
    /// there is no token.
    async fn graph() -> (Source, Db) {
        let source = Source::Demo { reason: "testing".into() };
        let conn = db::open(":memory:").unwrap();
        db::seed_graph(&conn, &harvest_graph(&source).await).unwrap();
        (source, Arc::new(Mutex::new(conn)))
    }

    /// Without a token the harvest makes no request and falls back to the demo
    /// graph, so the friend screens work in both modes.
    #[tokio::test]
    async fn the_demo_harvest_needs_no_network() {
        let users = harvest_graph(&Source::Demo { reason: "testing".into() }).await;
        assert!(!users.is_empty());
        assert!(users.iter().any(|u| u.followed_by_visitor), "the app must open with friends");
        assert!(users.iter().any(|u| u.follows_visitor), "and with someone following back");
        assert!(users.iter().all(|u| !u.handle.starts_with('@')));
    }

    /// An empty query draws the visitor's two lists and no results.
    ///
    /// It used to list every account, which is what the removed "Everyone" panel showed.
    /// A friends screen that opens on a directory of strangers buries the people you
    /// actually know, so search is now something you ask for.
    #[tokio::test]
    async fn an_empty_query_searches_for_nobody() {
        let (_, db) = graph().await;
        let idle = people(&db, "");

        assert_eq!(idle.query, "");
        assert!(idle.results.is_empty(), "no search, no results");
        // Both sides of the graph are still there — they are the screen, not the search.
        assert!(!idle.following.is_empty() && !idle.followers.is_empty());
        // Whitespace is the same as nothing: a stray space in the box isn't a query.
        assert!(people(&db, "   ").results.is_empty());
    }

    #[tokio::test]
    async fn the_directory_carries_both_sides_of_the_graph() {
        let (_, db) = graph().await;
        let idle = people(&db, "");

        let elena = people(&db, "elenarostova");
        assert_eq!(elena.query, "elenarostova");
        // A card knows both relationship bits, so the button and the badge on one
        // row can't disagree.
        let elena = elena.results.iter().find(|p| p.handle == "@elenarostova").unwrap();
        assert!(elena.following && elena.follows_you);
        assert_eq!(elena.review_count, 5);

        let found = people(&db, "kline");
        assert_eq!(found.query, "kline");
        assert_eq!(found.results.len(), 1);
        // The visitor's own lists don't shrink to the search term — they're beside
        // the results, not inside them.
        assert_eq!(found.following.len(), idle.following.len());
    }

    #[tokio::test]
    async fn a_persons_page_resolves_films_and_counts() {
        let (source, db) = graph().await;

        let elena = person(&source, &db, "elenarostova").await.expect("a seeded user");
        assert_eq!(elena.name, "Elena Rostova");
        assert_eq!(elena.handle, "@elenarostova");
        assert!(elena.following && elena.follows_you);
        assert_eq!(elena.reviews.len(), 5);

        // Each review resolved to a real film, not a slug-derived guess.
        let dune = elena.reviews.iter().find(|r| r.movie_id == "dune-part-two").unwrap();
        assert_eq!(dune.movie_title, "Dune: Part Two");
        assert!(dune.poster.is_some());
        assert_eq!(dune.rating_half_stars, 9);
        assert_eq!(dune.written_on, "March 15, 2024", "the date is pre-formatted");
        assert!(dune.author_followed);

        // The `@` is optional, and an unknown nickname is a real miss so the route
        // can 404 rather than draw an empty page.
        assert!(person(&source, &db, "@elenarostova").await.is_some());
        assert!(person(&source, &db, "nobody").await.is_none());
        // The export's decorative cast has no page.
        assert!(person(&source, &db, "elena").await.is_none());
    }

    /// "Other people's profiles should look exactly the same as your profile."
    /// Their page carries both strips, resolved to real films, not reviews alone.
    #[tokio::test]
    async fn a_persons_page_shows_their_favourites_and_watchlist() {
        let (source, db) = graph().await;
        let elena = person(&source, &db, "elenarostova").await.expect("a seeded user");

        assert!(!elena.favorites.is_empty(), "a seeded person's favourites are empty");
        assert!(!elena.watchlist.is_empty(), "a seeded person's watchlist is empty");
        // Resolved through the same `movies_for` the visitor's own strips use, so
        // every poster is a real film rather than a slug rendered as a title.
        for film in elena.favorites.iter().chain(&elena.watchlist) {
            assert!(!film.title.is_empty() && film.title != "Untitled");
            assert!(!film.poster.src.is_empty(), "{} has no poster", film.title);
        }
        // The two strips never name the same film, so the page doesn't say she both
        // loves a film and hasn't seen it.
        assert!(elena.favorites.iter().all(|f| !elena.watchlist.iter().any(|w| w.id == f.id)));

        // Clamped by the same two constants the visitor's own strips are, so one
        // component can draw either screen without a grid that overflows on one.
        assert!(elena.favorites.len() <= FAVORITES_SHOWN);
        assert!(elena.watchlist.len() <= WATCHLIST_SHOWN);
    }

    /// The film page's section, end to end: friends first, then the best-rated
    /// stranger — and the ids are the real film's, not a rail's pairing trick.
    #[tokio::test]
    async fn a_films_reviews_are_friends_first_then_best_rated() {
        let (source, db) = graph().await;

        let reviews = reviews_of_movie(&source, &db, "dune-part-two").await;
        assert_eq!(reviews.len(), 3);
        assert!(reviews.iter().all(|r| r.movie_id == "dune-part-two"));
        assert!(reviews.iter().all(|r| r.movie_title == "Dune: Part Two"));

        let followed: Vec<bool> = reviews.iter().map(|r| r.author_followed).collect();
        assert_eq!(followed, [true, true, false]);
        // Priya rated it higher than Marcus, and still sorts below him: friendship
        // outranks the score, and the score only breaks ties within a group.
        assert_eq!(reviews[1].rating_half_stars, 7);
        assert_eq!(reviews[2].rating_half_stars, 8);

        // Every id in the payload is a real one the frontend can link to.
        assert!(reviews.iter().all(|r| r.author_handle.starts_with('@')));
        assert!(reviews.iter().all(|r| !r.id.is_empty() && !r.author_id.is_empty()));

        // A film nobody reviewed is an empty list, not an error — the section
        // hides itself.
        assert!(reviews_of_movie(&source, &db, "project-kepler").await.len() <= 1);
        assert!(reviews_of_movie(&source, &db, "no-such-film").await.is_empty());
    }

    #[tokio::test]
    async fn following_reports_the_new_count_and_404s_on_a_stranger() {
        let (_, db) = graph().await;
        let before = people(&db, "").following.len();

        let followed =
            set_follow(&db, "user-priyanaidu", Some(true)).unwrap().expect("a real user");
        assert!(followed.following);
        assert_eq!(followed.person_id, "user-priyanaidu");
        assert_eq!(followed.following_count as usize, before + 1);
        // The directory agrees immediately, so the screen can trust one response.
        assert_eq!(people(&db, "").following.len(), before + 1);

        let dropped = set_follow(&db, "user-priyanaidu", Some(false)).unwrap().unwrap();
        assert!(!dropped.following);
        assert_eq!(dropped.following_count as usize, before);

        // `Ok(None)` is a 404, distinct from an `Err`, which is a failed write.
        assert!(set_follow(&db, "elena", Some(true)).unwrap().is_none());
        assert!(set_follow(&db, "nobody", None).unwrap().is_none());
    }

    /// Every profile "Following" row links to a page that opens, because only real
    /// users can be followed. `handle` stays optional on the type because the
    /// *stories and activity rails* still carry the export's unlinkable cast; this
    /// list no longer does.
    #[tokio::test]
    async fn every_followed_row_opens_a_page() {
        let (source, db) = graph().await;
        let profile = profile(&source, &db).await;

        assert!(!profile.following.is_empty(), "the seeded friends must be there");
        for row in &profile.following {
            let handle = row.handle.as_deref().expect("a followed person has a page");
            assert!(handle.starts_with('@'));
            assert!(person(&source, &db, handle).await.is_some(), "{handle} has no page");
            // Their bio stands in for the rail sentence they don't have.
            assert!(row.subtitle.contains("reviewed"), "{}", row.subtitle);
        }

        // The rails themselves are untouched — the export's cast still draws them.
        let stories = mobile_feed(&source, &db).await.stories;
        assert!(!stories.is_empty(), "the stories rail is still the export's cast");
    }

    /// A bio says something true about the person rather than inventing a
    /// personality — there is no bio field upstream to borrow one from.
    #[test]
    fn a_bio_describes_how_they_rate() {
        let review = |stars: u8| ("f".to_string(), stars, "b".to_string(), "d".to_string());
        assert_eq!(review_bio(&[review(9)]), "1 film reviewed · generous ratings");
        assert_eq!(review_bio(&[review(7), review(7)]), "2 films reviewed · middling ratings");
        assert_eq!(review_bio(&[review(2), review(4)]), "2 films reviewed · hard to please");
    }
}
