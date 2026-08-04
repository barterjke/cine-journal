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

/// How many trending films the curated rails draw from. Four live cards, four
/// recent entries, three activity rows and four mobile cards, all disjoint.
const TRENDING_NEEDED: usize = 8;

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

/// The desktop feed: two live rooms, four recent entries, three activity rows.
pub async fn feed(source: &Source, db: &Db) -> Feed {
    let Some(client) = source.client() else {
        return data::feed();
    };

    let films = match trending(client).await {
        Ok(films) => films,
        Err(error) => {
            tracing::warn!(%error, "falling back to the demo feed");
            return data::feed();
        }
    };

    // One lock, one scope, no `.await` inside it.
    let (discussions, activity) = {
        let conn = lock(db);
        (db::discussions(&conn).unwrap_or_default(), db::activity(&conn).unwrap_or_default())
    };

    // Each rail takes a disjoint slice, so no film appears twice on the screen.
    let live = discussions
        .iter()
        .zip(&films)
        .map(|(room, (movie, vote))| LiveDiscussion {
            id: room.id.clone(),
            movie: movie.clone(),
            rating_half_stars: map::half_stars(*vote),
            blurb: room.blurb.clone(),
            participants: room.participants.clone(),
            overflow_count: room.overflow_count,
        })
        .collect();

    let recent = films
        .iter()
        .skip(discussions.len())
        .take(4)
        .map(|(movie, vote)| FeedEntry {
            id: format!("entry-{}", movie.id),
            movie: movie.clone(),
            rating_half_stars: map::half_stars(*vote),
            on_watchlist: false,
        })
        .collect();

    let friend_activity = activity
        .iter()
        .zip(films.iter().skip(discussions.len()))
        .map(|(row, (movie, _))| FriendActivity {
            id: row.id.clone(),
            author_name: row.person.name.clone(),
            author_avatar: row.person.avatar.clone(),
            timestamp: row.timestamp.clone(),
            kind: row.kind,
            movie_id: movie.id.clone(),
            movie_title: movie.title.clone(),
            rating_half_stars: row.rating_half_stars,
            quote: row.quote.clone(),
        })
        .collect();

    Feed { live, recent, friend_activity }
}

/// The mobile feed: the stories rail plus four poster cards.
pub async fn mobile_feed(source: &Source, db: &Db) -> MobileFeed {
    let Some(client) = source.client() else {
        return data::mobile_feed();
    };

    let films = match trending(client).await {
        Ok(films) => films,
        Err(error) => {
            tracing::warn!(%error, "falling back to the demo mobile feed");
            return data::mobile_feed();
        }
    };

    let (people, captions) = {
        let conn = lock(db);
        (db::stories(&conn).unwrap_or_default(), db::captions(&conn).unwrap_or_default())
    };

    let stories = people
        .into_iter()
        .map(|person| Story {
            id: format!("story-{}", person.id),
            name: person.name,
            avatar: person.avatar,
            unseen: person.unseen,
        })
        .collect();

    let items = captions
        .iter()
        .zip(&films)
        .map(|(caption, (movie, vote))| MobileFeedItem {
            id: caption.id.clone(),
            // The export's mobile cards print no year under the poster.
            movie: Movie { year: None, ..movie.clone() },
            subtitle: caption.caption.clone(),
            rating_half_stars: caption.show_rating.then(|| map::half_stars(*vote)),
            on_watchlist: false,
        })
        .collect();

    MobileFeed { stories, items }
}

/// The reviews both review screens read, newest film first.
///
/// TMDB reviews are sparse — plenty of trending films have none — so this walks
/// the trending list until it finds a film with at least two, which is what the
/// two screens need (desktop takes `[0]`, mobile `[1]`).
pub async fn reviews(source: &Source) -> Vec<Review> {
    let Some(client) = source.client() else {
        return data::reviews();
    };

    match reviews_from_trending(client).await {
        Ok(reviews) if !reviews.is_empty() => reviews,
        Ok(_) => {
            tracing::warn!("no trending film has reviews — falling back to the demo reviews");
            data::reviews()
        }
        Err(error) => {
            tracing::warn!(%error, "falling back to the demo reviews");
            data::reviews()
        }
    }
}

async fn reviews_from_trending(client: &Tmdb) -> tmdb::Result<Vec<Review>> {
    let images = client.images().await;
    let summaries = client.trending().await?;

    // Five is enough to find a reviewed film in practice while bounding the worst
    // case at ten round trips, all of them cached after the first request.
    for summary in summaries.iter().take(5) {
        let records = client.reviews(summary.id).await?;
        if records.len() < 2 {
            continue;
        }
        let film = client.movie(summary.id).await?;
        return Ok(records.iter().map(|record| map::review(record, &film, &images)).collect());
    }

    Ok(Vec::new())
}

/// One review by its `<film>-<review>` id.
pub async fn review_by_id(source: &Source, id: &str) -> Option<Review> {
    let Some(client) = source.client() else {
        return data::review_by_id(id);
    };

    let Some((movie_id, review_id)) = map::split_review_id(id) else {
        // A demo id in TMDB mode: the frontend may still hold one from a stale
        // tab, and answering it costs nothing.
        return data::review_by_id(id);
    };

    let images = client.images().await;
    let records = client.reviews(movie_id).await.ok()?;
    let record = records.iter().find(|r| r.id == review_id)?;
    let film = client.movie(movie_id).await.ok()?;
    Some(map::review(record, &film, &images))
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
        Err(error) => {
            tracing::warn!(%error, "falling back to demo search results");
            data::search(query)
        }
    }
}

async fn search_tmdb(client: &Arc<Tmdb>, query: &SearchQuery) -> tmdb::Result<SearchResponse> {
    let text = query.q.as_deref().unwrap_or("").trim();
    let genre = query.genre.as_deref().filter(|g| !g.is_empty());
    let year = query.year.as_deref().filter(|y| !y.is_empty());
    let min_rating = query.min_rating.unwrap_or(0);
    let requested_page = query.page.unwrap_or(1).max(1);

    if text.is_empty() {
        browse(client, query, genre, year, min_rating, requested_page).await
    } else {
        text_search(client, query, text, genre, year, min_rating, requested_page).await
    }
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
        filters: window_facets(&candidates, genre, year, min_rating),
        page,
        page_count,
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

/// The sidebar's chips in text mode, counted over the same candidate window the
/// results came from — `discover` can't help here, since it has no text parameter.
fn window_facets(
    candidates: &[data::CatalogueEntry],
    genre: Option<&str>,
    year: Option<&str>,
    min_rating: u8,
) -> SearchFilters {
    let pool: Vec<&data::CatalogueEntry> =
        candidates.iter().filter(|entry| entry.meets_minimum(min_rating)).collect();

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

/// This week's trending films, paired with their crowd votes.
///
/// Posterless films are dropped (`map::movie` returns `None`) because every rail
/// here renders a required poster. One upstream call, cached for ten minutes.
async fn trending(client: &Tmdb) -> tmdb::Result<Vec<(Movie, f32)>> {
    let images = client.images().await;
    let summaries = client.trending().await?;
    Ok(summaries
        .iter()
        .filter_map(|summary| map::movie(summary, &images).map(|movie| (movie, summary.vote_average)))
        .take(TRENDING_NEEDED)
        .collect())
}

/// Whether a review id could name a review this source serves.
///
/// Used by the mutation handlers to reject a bogus id before writing a row nothing
/// will ever render.
pub async fn review_exists(source: &Source, id: &str) -> bool {
    review_by_id(source, id).await.is_some()
}

/// The base like count a review or comment shows before the visitor's own like.
///
/// `None` in TMDB mode for both: there is no upstream count. `hydrate::like_count`
/// then renders nothing until the visitor likes it, and 1 after — never a zero.
pub async fn review_like_base(source: &Source, id: &str) -> Option<u32> {
    match source {
        Source::Tmdb(_) => None,
        Source::Demo { .. } => data::review_by_id(id).and_then(|review| review.like_count),
    }
}

pub async fn comment_like_base(source: &Source, review_id: &str, comment_id: &str) -> Option<u32> {
    match source {
        Source::Tmdb(_) => None,
        Source::Demo { .. } => data::review_by_id(review_id)
            .and_then(|review| review.comments.into_iter().find(|c| c.id == comment_id))
            .and_then(|comment| comment.like_count),
    }
}

/// Whether this comment exists — either in the content or as one the visitor
/// posted. Guards replies and likes against ids nothing renders.
pub async fn comment_exists(source: &Source, db: &Db, review_id: &str, comment_id: &str) -> bool {
    let posted = {
        let conn = lock(db);
        db::comment_exists(&conn, review_id, comment_id).unwrap_or(false)
    };
    if posted {
        return true;
    }

    match source {
        // Upstream reviews carry no comments at all, so anything not in SQLite is
        // unknown.
        Source::Tmdb(_) => false,
        Source::Demo { .. } => data::review_by_id(review_id)
            .is_some_and(|review| review.comments.iter().any(|c| c.id == comment_id)),
    }
}

/// A review with the visitor's likes, comments and replies folded in.
pub async fn hydrated_review(source: &Source, db: &Db, id: &str) -> Option<Review> {
    let review = review_by_id(source, id).await?;
    Some(hydrate::review(review, &store(db)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo path must not touch the network, so `Source::Demo` has to be
    /// buildable and usable without a client.
    #[tokio::test]
    async fn demo_mode_serves_the_export_verbatim() {
        let source = Source::Demo { reason: "testing".into() };
        let conn = db::open(":memory:").unwrap();
        let db: Db = Arc::new(Mutex::new(conn));

        let feed = feed(&source, &db).await;
        assert_eq!(feed.live.len(), 2);
        assert_eq!(feed.live[0].movie.title, "The Silence of Space");
        assert_eq!(feed.recent.len(), 4);
        assert_eq!(feed.friend_activity.len(), 3);

        let mobile = mobile_feed(&source, &db).await;
        assert_eq!(mobile.stories.len(), 5);
        assert_eq!(mobile.items[0].movie.title, "The Horizon");

        let reviews = reviews(&source).await;
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[0].id, "dune-part-two");

        // The demo's "every id resolves" behaviour, which TMDB mode replaces with
        // a real 404.
        assert!(movie_detail_by_id(&source, "anything-at-all").await.is_some());
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
}
