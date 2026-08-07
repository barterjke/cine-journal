//! The TMDB v3 client.
//!
//! Auth is the v4 read access token in an `Authorization: Bearer` header, which
//! works against the v3 endpoints. The alternative — `?api_key=` — puts the
//! credential in the URL, where the tracing layer would log it.
//!
//! Every response is cached in memory with a TTL, so a page reload costs nothing
//! upstream and the six screens between them make at most a handful of calls.
//! Nothing here maps payloads to wire types; that's `map`, which is pure and
//! tested against recorded fixtures.

pub mod dto;
pub mod map;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use tokio::sync::RwLock;

const API_BASE: &str = "https://api.themoviedb.org/3";

/// Cache lifetimes, by how often the thing behind them actually changes.
const TTL_CONFIGURATION: Duration = Duration::from_secs(24 * 60 * 60);
const TTL_DETAIL: Duration = Duration::from_secs(24 * 60 * 60);
const TTL_TRENDING: Duration = Duration::from_secs(10 * 60);
const TTL_SEARCH: Duration = Duration::from_secs(5 * 60);

/// Upstream wouldn't answer. The message is safe to show a user and never
/// contains the token — see `Tmdb::fetch`.
#[derive(Debug, Clone)]
pub struct Error(pub String, pub Kind);

/// Whether the request was wrong or upstream was.
///
/// The distinction matters because the fallbacks differ: an unreachable TMDB should
/// serve the demo dataset, whereas a 404 for a hand-typed id has no fallback worth
/// offering — the honest answer is that there's no such thing, and pretending
/// otherwise would show invented films under someone's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// TMDB says no such record: a 404, and nothing a retry or a cache would fix.
    Missing,
    /// Anything else — unreachable, rejected, unparseable, rate-limited.
    Upstream,
}

impl Error {
    /// Whether this is TMDB's "no such record", rather than a failure of ours or
    /// theirs.
    pub fn is_missing(&self) -> bool {
        self.1 == Kind::Missing
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One cached response body, kept as raw JSON.
///
/// Storing text rather than the deserialized struct keeps the cache a single
/// `HashMap<String, _>` instead of one per payload type, and re-parsing a cached
/// body is far cheaper than the round trip it replaces.
struct Entry {
    stored: Instant,
    ttl: Duration,
    body: String,
}

impl Entry {
    fn is_fresh(&self) -> bool {
        self.stored.elapsed() < self.ttl
    }
}

pub struct Tmdb {
    http: reqwest::Client,
    token: String,
    cache: RwLock<HashMap<String, Entry>>,
    /// From `/3/configuration`, resolved on first use. `map::ImageBase::default`
    /// is what the endpoint returns today, so a failed config call costs nothing
    /// but a stale size choice.
    images: RwLock<Option<map::ImageBase>>,
}

impl Tmdb {
    /// A client for a non-empty token.
    ///
    /// The caller decides what a missing token means — here it's simply an
    /// absent client, and `content` serves the demo dataset instead.
    pub fn new(token: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("cine-journal/0.1 (+https://github.com/barterjke/cine-journal)")
            .build()
            .map_err(|e| Error(format!("could not build an HTTP client: {e}"), Kind::Upstream))?;

        Ok(Self {
            http,
            token,
            cache: RwLock::new(HashMap::new()),
            images: RwLock::new(None),
        })
    }

    /// GET `path` (with a leading slash), deserialize, and cache the body.
    ///
    /// `path` is also the cache key, so callers must build query strings
    /// deterministically — the same request twice has to produce the same string
    /// or the cache never hits.
    async fn fetch<T: DeserializeOwned>(&self, path: &str, ttl: Duration) -> Result<T> {
        if let Some(entry) = self.cache.read().await.get(path) {
            if entry.is_fresh() {
                return parse(path, &entry.body);
            }
        }

        let url = format!("{API_BASE}{path}");
        let response = self
            .http
            .get(&url)
            // The one place the token is used. It goes in a header, never in the
            // URL, so nothing that logs a request path can leak it.
            .bearer_auth(&self.token)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                // `reqwest`'s Display includes the URL but never the headers, so
                // this is safe to log and to show a user.
                Error(format!("TMDB request to {path} failed: {e}"), Kind::Upstream)
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| Error(format!("could not read the TMDB response for {path}: {e}"), Kind::Upstream))?;

        if !status.is_success() {
            // TMDB explains failures in a `status_message`, which is worth
            // surfacing — a 401 says the token is invalid, and that is exactly
            // what the banner should tell the user.
            let detail = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("status_message")?.as_str().map(str::to_string))
                .unwrap_or_else(|| status.to_string());
            let kind = if status == reqwest::StatusCode::NOT_FOUND {
                Kind::Missing
            } else {
                Kind::Upstream
            };
            return Err(Error(
                format!("TMDB returned {} for {path}: {detail}", status.as_u16()),
                kind,
            ));
        }

        let parsed = parse(path, &body)?;
        self.cache
            .write()
            .await
            .insert(path.to_string(), Entry { stored: Instant::now(), ttl, body });
        Ok(parsed)
    }

    /// Whether the token works. Called once at startup so a bad token surfaces in
    /// the banner rather than as six broken screens.
    pub async fn authenticate(&self) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Authentication {
            success: bool,
        }

        let auth: Authentication = self.fetch("/authentication", TTL_CONFIGURATION).await?;
        if auth.success {
            Ok(())
        } else {
            Err(Error("TMDB rejected the token".into(), Kind::Upstream))
        }
    }

    /// The image CDN's base URL and the renditions to request.
    ///
    /// Falls back to today's published values rather than failing: they change
    /// about never, and no screen renders without them.
    pub async fn images(&self) -> map::ImageBase {
        if let Some(images) = self.images.read().await.clone() {
            return images;
        }

        let resolved = match self.fetch::<dto::Configuration>("/configuration", TTL_CONFIGURATION).await
        {
            Ok(config) => map::ImageBase::from_config(&config.images),
            Err(error) => {
                tracing::warn!(%error, "using the default TMDB image base");
                map::ImageBase::default()
            }
        };

        *self.images.write().await = Some(resolved.clone());
        resolved
    }

    /// This week's trending films — the pool every curated rail draws from.
    pub async fn trending(&self) -> Result<Vec<dto::MovieSummary>> {
        let page: dto::Page<dto::MovieSummary> =
            self.fetch("/trending/movie/week", TTL_TRENDING).await?;
        Ok(page.results)
    }

    /// One film and everything the detail screen draws, in a single round trip.
    ///
    /// Five appended blocks, one request: `credits` for the cast rail and the
    /// credits grid, `images` to back up a film with no `backdrop_path`, `videos`
    /// for the Media block, `release_dates` for the age rating (TMDB publishes no
    /// global one) and `watch/providers` for "Where to Watch". Appending costs
    /// nothing upstream — the response is one document — whereas five separate
    /// calls would multiply the page's latency for the same bytes.
    ///
    /// The path doubles as this request's cache key, so it has to stay
    /// byte-stable: no reordering, no conditional segments.
    pub async fn movie(&self, id: u32) -> Result<dto::MovieDetail> {
        let path =
            format!("/movie/{id}?append_to_response=credits,images,videos,release_dates,watch/providers");
        self.fetch(&path, TTL_DETAIL).await
    }

    /// One person and everything they were credited on.
    ///
    /// `movie_credits` appended rather than `discover?with_people=`, because the
    /// filmography arrives *complete* in one response — 63 films for Nolan, 342 for
    /// Spielberg, each carrying the same `title`/`release_date`/`poster_path`/
    /// `vote_average`/`genre_ids` a search card needs (verified across four people).
    /// A paginated `discover` would be one request per page of results and could
    /// only ever be filtered by what `discover` accepts; with the whole list in
    /// hand, the decade and rating chips get exact counts and a text query can be
    /// applied over it — which `discover` has no parameter for at all.
    ///
    /// A day's TTL, like the film details: a filmography changes about as often as
    /// a runtime, and the search screen would otherwise re-fetch it per page.
    pub async fn person(&self, id: u32) -> Result<dto::Person> {
        self.fetch(&format!("/person/{id}?append_to_response=movie_credits"), TTL_DETAIL).await
    }

    /// Films TMDB recommends alongside this one — one seed, twenty suggestions.
    ///
    /// The feed's recommendation rail is built from several of these, one per film
    /// the visitor favourited or watchlisted, because a single seed's twenty are all
    /// neighbours of one film: seeding from three gave 54 distinct titles with six
    /// recommended by more than one, and that overlap is what ranks them.
    ///
    /// `/recommendations` rather than `/similar`: the former is TMDB's own
    /// collaborative signal (what people who liked this also liked), the latter is
    /// keyword-and-genre overlap, which for a favourite already picked *by* genre
    /// returns more of the same genre and says nothing new.
    ///
    /// Returns the same `MovieSummary` a search result is, so no new mapping. A
    /// day's TTL, as with the details: recommendations move about as slowly.
    pub async fn recommendations(&self, id: u32) -> Result<Vec<dto::MovieSummary>> {
        let page: dto::Page<dto::MovieSummary> =
            self.fetch(&format!("/movie/{id}/recommendations"), TTL_DETAIL).await?;
        Ok(page.results)
    }

    /// A film's reviews. Empty rather than an error for a film with none.
    pub async fn reviews(&self, id: u32) -> Result<Vec<dto::ReviewRecord>> {
        let page: dto::Page<dto::ReviewRecord> =
            self.fetch(&format!("/movie/{id}/reviews"), TTL_DETAIL).await?;
        Ok(page.results)
    }

    /// Free-text search.
    ///
    /// Note that `/search/movie` **ignores** `with_genres` — verified: "dune" and
    /// "dune"+Sci-Fi both report 1095 results. Genre, decade and rating filtering
    /// therefore happen locally in `content::search`, over these candidates.
    pub async fn search(&self, query: &str, pages: u32) -> Result<Vec<dto::MovieSummary>> {
        let encoded = urlencode(query);
        let mut all = Vec::new();

        for page in 1..=pages.max(1) {
            let path = format!("/search/movie?query={encoded}&page={page}&include_adult=false");
            let batch: dto::Page<dto::MovieSummary> = self.fetch(&path, TTL_SEARCH).await?;
            let last = batch.page >= batch.total_pages || batch.results.is_empty();
            all.extend(batch.results);
            if last {
                break;
            }
        }

        Ok(all)
    }

    /// Filtered browse, for when there's no text to search by.
    ///
    /// Unlike `/search/movie`, `discover` does apply these — so the filters are
    /// pushed upstream here and the whole catalogue is reachable rather than only
    /// what one text query surfaced.
    ///
    /// Returns the whole envelope, not just the results: `total_results` is an
    /// exact count of the matches across all of TMDB, which is what lets the
    /// search screen paginate honestly and count its sidebar chips without
    /// guessing from a window.
    pub async fn discover(&self, filters: &DiscoverFilters) -> Result<dto::Page<dto::MovieSummary>> {
        let mut path = String::from("/discover/movie?include_adult=false&sort_by=popularity.desc");
        if let Some(genre) = filters.genre_id {
            path.push_str(&format!("&with_genres={genre}"));
        }
        if let Some((from, to)) = filters.released_between {
            path.push_str(&format!(
                "&primary_release_date.gte={from}-01-01&primary_release_date.lte={to}-12-31"
            ));
        }
        if let Some(min) = filters.min_vote_average {
            path.push_str(&format!("&vote_average.gte={min}"));
        }
        // TMDB refuses pages past 500 with a 400, so a hand-typed `?page=9999`
        // must not become an upstream error.
        path.push_str(&format!("&page={}", filters.page.clamp(1, MAX_DISCOVER_PAGE)));

        self.fetch(&path, TTL_SEARCH).await
    }
}

/// TMDB's hard ceiling on `/discover/movie?page=`.
pub const MAX_DISCOVER_PAGE: u32 = 500;

/// How many results one upstream page holds. Fixed by TMDB, and the search screen
/// needs it to map its own 8-per-page grid onto these.
pub const UPSTREAM_PAGE_SIZE: u32 = 20;

/// What `/discover/movie` can filter by, of the things the sidebar offers.
#[derive(Debug, Default, Clone)]
pub struct DiscoverFilters {
    pub genre_id: Option<u32>,
    /// Inclusive year bounds, from a decade chip like "2010s".
    pub released_between: Option<(u16, u16)>,
    /// On TMDB's 0–10 scale.
    pub min_vote_average: Option<f32>,
    pub page: u32,
}

fn parse<T: DeserializeOwned>(path: &str, body: &str) -> Result<T> {
    serde_json::from_str(body)
        .map_err(|e| Error(format!("could not parse the TMDB response for {path}: {e}"), Kind::Upstream))
}

/// Percent-encode a query value.
///
/// Hand-rolled rather than pulling in a crate: the only untrusted string that
/// ever reaches a URL here is the search box's contents, and the rule for a
/// query value is short enough to state completely.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The search box's contents end up in a URL, so anything that could break
    /// out of the query value has to be encoded.
    #[test]
    fn query_values_are_encoded() {
        assert_eq!(urlencode("dune"), "dune");
        assert_eq!(urlencode("dune part two"), "dune%20part%20two");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("100%"), "100%25");
        assert_eq!(urlencode("?#/"), "%3F%23%2F");
        // Multi-byte UTF-8 is encoded per byte.
        assert_eq!(urlencode("é"), "%C3%A9");
        assert_eq!(urlencode("2001: A Space Odyssey"), "2001%3A%20A%20Space%20Odyssey");
    }

    /// A fresh entry is served from cache; a stale one isn't.
    #[test]
    fn cache_entries_expire() {
        let fresh =
            Entry { stored: Instant::now(), ttl: Duration::from_secs(60), body: "{}".into() };
        assert!(fresh.is_fresh());

        let stale = Entry {
            stored: Instant::now() - Duration::from_secs(120),
            ttl: Duration::from_secs(60),
            body: "{}".into(),
        };
        assert!(!stale.is_fresh());
    }

    /// A parse failure names the path but never echoes a credential, because the
    /// path it names never carries one.
    #[test]
    fn parse_errors_name_the_path() {
        let err = parse::<dto::Configuration>("/configuration", "not json").unwrap_err();
        assert!(err.0.contains("/configuration"));
    }
}
