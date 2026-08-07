//! Wire structs for the TMDB v3 payloads we consume.
//!
//! Only the fields the app actually renders are declared — TMDB sends far more
//! (`budget`, `imdb_id`, `spoken_languages`, `popularity`, …) and serde ignores
//! what isn't listed. Recorded samples of every one of these live in
//! `backend/tests/fixtures/`, and `map`'s tests deserialize them, so a field
//! renamed upstream fails a test rather than a request.
//!
//! Almost everything is `Option`, and that is not defensive padding: TMDB
//! genuinely omits or nulls `poster_path`, `backdrop_path`, `release_date`,
//! `runtime` and `character` on real records. A required field here would turn
//! one incomplete film into a failed screen.

use serde::Deserialize;

/// `GET /3/configuration` — where the image CDN lives and what sizes it serves.
#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub images: ImageConfig,
}

#[derive(Debug, Deserialize)]
pub struct ImageConfig {
    pub secure_base_url: String,
    pub poster_sizes: Vec<String>,
    pub backdrop_sizes: Vec<String>,
    pub profile_sizes: Vec<String>,
    /// Streaming-service logos, for the "Where to Watch" rows.
    #[serde(default)]
    pub logo_sizes: Vec<String>,
}

/// A page of `/3/trending/movie/week`, `/3/search/movie` or `/3/discover/movie`.
///
/// All three return the same envelope around the same summary shape, which is
/// what lets one struct back both the feeds and the search screen.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub page: u32,
    pub results: Vec<T>,
    pub total_pages: u32,
    pub total_results: u32,
}

/// One film as the list endpoints describe it: genres by id, no runtime, no crew.
///
/// No `overview` or `backdrop_path`: the screens these back — the two feeds and the
/// search grid — render neither. The detail page does, and gets them from
/// `MovieDetail`.
#[derive(Debug, Clone, Deserialize)]
pub struct MovieSummary {
    pub id: u32,
    pub title: String,
    pub release_date: Option<String>,
    pub poster_path: Option<String>,
    /// 0.0–10.0. Zero for a film nobody has voted on — `vote_count` is what tells
    /// the two apart, and `map::star_rating` reads both.
    #[serde(default)]
    pub vote_average: f32,
    /// How many votes that average is over. Zero exactly when `vote_average` is
    /// (verified across 111 credits in `person-525.json`), so it's what makes
    /// "unrated" distinguishable from "rated zero" on a search card.
    #[serde(default)]
    pub vote_count: u32,
    #[serde(default)]
    pub genre_ids: Vec<u32>,
}

/// `GET /3/movie/{id}?append_to_response=credits,images,videos,release_dates,watch/providers`.
///
/// One request rather than six: every appended block backs a section of the detail
/// screen, and splitting them would multiply the latency of the page for no gain.
/// TMDB imposes no cost for appending — the response is one document.
#[derive(Debug, Deserialize)]
pub struct MovieDetail {
    pub id: u32,
    pub title: String,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub release_date: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    /// Minutes. `None` or `0` for unreleased films and most shorts.
    pub runtime: Option<u32>,
    #[serde(default)]
    pub vote_average: f32,
    /// How many people voted. Printed as "Based on N ratings" beside the score,
    /// so an average of 8.5 from three votes reads as what it is.
    #[serde(default)]
    pub vote_count: u32,
    #[serde(default)]
    pub genres: Vec<Genre>,
    #[serde(default)]
    pub production_companies: Vec<ProductionCompany>,
    #[serde(default)]
    pub credits: Credits,
    #[serde(default)]
    pub images: Images,
    #[serde(default)]
    pub videos: Videos,
    #[serde(default)]
    pub release_dates: ReleaseDates,
    /// TMDB names this key with a slash, which is not a valid Rust identifier.
    #[serde(default, rename = "watch/providers")]
    pub watch_providers: WatchProviders,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Genre {
    /// Only the tests read this — `map::GENRES` is a transcribed table, and
    /// `genre_table_matches_tmdb` checks it against the real list by id. The
    /// request path goes the other way, from a label to an id.
    #[cfg_attr(not(test), allow(dead_code))]
    pub id: u32,
    pub name: String,
}

/// `GET /3/genre/movie/list`.
///
/// Not fetched at runtime: `map::GENRES` transcribes the list, because the chip
/// labels differ from TMDB's names and the mapping has to exist in code anyway. The
/// recorded response is a test fixture, which is what keeps the table honest.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
pub struct GenreList {
    pub genres: Vec<Genre>,
}

#[derive(Debug, Deserialize)]
pub struct ProductionCompany {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Credits {
    #[serde(default)]
    pub cast: Vec<CastCredit>,
    #[serde(default)]
    pub crew: Vec<CrewCredit>,
}

#[derive(Debug, Deserialize)]
pub struct CastCredit {
    pub id: u32,
    pub name: String,
    /// Empty string on real records ("Self", uncredited roles), not just absent.
    pub character: Option<String>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CrewCredit {
    /// The person, not the credit. Carried so the credits grid can link a name to
    /// `discover?with_people=`, which is how "everything this director made" is
    /// asked for — a name alone would only reach `/search/movie`, which matches
    /// film titles and would find nothing.
    pub id: u32,
    pub name: String,
    /// "Director", "Director of Photography", "Original Music Composer" — the
    /// three the detail screen names. Matched exactly; see `map::crew_named`.
    pub job: String,
}

/// `GET /3/person/{id}?append_to_response=movie_credits`, behind `/search?person=`.
///
/// The search URL carries only the id, so the name has to be resolved somewhere;
/// doing it here keeps `/search?person=525` short, shareable and impossible to
/// mislabel by hand.
#[derive(Debug, Deserialize)]
pub struct Person {
    pub name: String,
    #[serde(default)]
    pub movie_credits: PersonCredits,
}

/// Everything one person was credited on, split by how.
///
/// Both halves are read and merged: the detail screen links actors and directors
/// with the same affordance, and a director who also acted belongs under their own
/// name for both. The two lists overlap — Nolan appears in `cast` and `crew` for the
/// films he cameos in — so `map::filmography` dedupes by film id.
#[derive(Debug, Default, Deserialize)]
pub struct PersonCredits {
    #[serde(default)]
    pub cast: Vec<MovieSummary>,
    #[serde(default)]
    pub crew: Vec<MovieSummary>,
}

/// The appended `images` block.
///
/// Only `backdrops` is declared: they fill the Media carousel and back up a film
/// with no `backdrop_path`. Alternative posters aren't rendered anywhere — the
/// screens show one poster per film, and `poster_path` is already the chosen one.
#[derive(Debug, Default, Deserialize)]
pub struct Images {
    #[serde(default)]
    pub backdrops: Vec<ImageRecord>,
}

/// One still. Films carry 72–192 of these, far more than a carousel wants, so the
/// fields beyond the path are all there to choose *which* — see `map::stills`.
#[derive(Debug, Deserialize)]
pub struct ImageRecord {
    pub file_path: String,
    /// The language of any text burned into the image. `None` — the majority —
    /// means a plain frame from the film, which is what a stills rail wants; a
    /// value means a title card or a localized poster crop.
    #[serde(default)]
    pub iso_639_1: Option<String>,
    /// TMDB's own crowd score for the image. Their list arrives sorted by this
    /// descending, so it is the ranking rather than a filter.
    #[serde(default)]
    pub vote_average: f32,
}

/// The appended `videos` block — trailers, teasers, clips and featurettes.
#[derive(Debug, Default, Deserialize)]
pub struct Videos {
    #[serde(default)]
    pub results: Vec<VideoRecord>,
}

/// One video. `key` is a site-scoped id, not a URL: on YouTube it's what goes
/// after `watch?v=`.
#[derive(Debug, Deserialize)]
pub struct VideoRecord {
    pub name: String,
    pub key: String,
    /// "YouTube" or "Vimeo". Only the former is embeddable here.
    pub site: String,
    /// "Trailer", "Teaser", "Clip", "Featurette", "Behind the Scenes", "Bloopers".
    #[serde(rename = "type")]
    pub kind: String,
    /// Whether the studio published it, as opposed to a fan upload.
    #[serde(default)]
    pub official: bool,
    /// RFC 3339. Newest official trailer wins, so this is the sort key.
    #[serde(default)]
    pub published_at: Option<String>,
}

/// The appended `release_dates` block: one entry per country, each with its own
/// certification. There is no single global rating, which is why a country has to
/// be chosen — see `map::certification`.
#[derive(Debug, Default, Deserialize)]
pub struct ReleaseDates {
    #[serde(default)]
    pub results: Vec<CountryReleases>,
}

#[derive(Debug, Deserialize)]
pub struct CountryReleases {
    pub iso_3166_1: String,
    #[serde(default)]
    pub release_dates: Vec<ReleaseDate>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseDate {
    /// "PG-13", "R", "NR" — and frequently an empty string, which means "not
    /// rated here" rather than "rated NR".
    #[serde(default)]
    pub certification: String,
}

/// The appended `watch/providers` block, keyed by country code.
///
/// A map rather than a struct with a field per country: the keys are the ~130
/// country codes TMDB has data for, and only the one we ask about is read.
#[derive(Debug, Default, Deserialize)]
pub struct WatchProviders {
    #[serde(default)]
    pub results: std::collections::HashMap<String, CountryProviders>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CountryProviders {
    /// TMDB's own "watch" page for the film. Their attribution terms require
    /// linking here rather than deep-linking a provider, and we have no
    /// per-provider URL anyway.
    pub link: Option<String>,
    /// Included in a subscription.
    #[serde(default)]
    pub flatrate: Vec<Provider>,
    #[serde(default)]
    pub rent: Vec<Provider>,
    #[serde(default)]
    pub buy: Vec<Provider>,
    /// Free with ads.
    #[serde(default)]
    pub ads: Vec<Provider>,
    #[serde(default)]
    pub free: Vec<Provider>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    pub provider_name: String,
    pub logo_path: Option<String>,
    /// TMDB's own ordering hint — lower is more prominent.
    #[serde(default)]
    pub display_priority: u32,
}

/// A page of `GET /3/movie/{id}/reviews`.
///
/// Read once, at startup, by `content::harvest_graph` — the prose becomes a review
/// row belonging to one of our own users, and nothing re-reads it afterwards. TMDB's
/// own review id is deliberately not kept: our reviews are keyed on
/// `(person, film)`, so carrying the upstream id would imply a link back that
/// doesn't exist.
#[derive(Debug, Deserialize)]
pub struct ReviewRecord {
    pub author: String,
    pub author_details: AuthorDetails,
    pub content: String,
    /// RFC 3339, e.g. "2014-11-12T16:06:04Z". Formatted by `map::watched_on`.
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorDetails {
    /// The author's TMDB nickname ("msbreviews", "Spartan117"), distinct from the
    /// display `author` name beside it. This is what `content::harvest_graph` seeds
    /// the app's own handles from — a real chosen nickname reads as one, which is
    /// exactly what a search-by-nickname screen needs to be testable against.
    pub username: Option<String>,
    /// 0.0–10.0, and genuinely absent on more than half of real reviews.
    pub rating: Option<f32>,
    /// A TMDB path like "/abc.jpg", but documented to also arrive as
    /// "/https://secure.gravatar.com/…" — a full URL behind a stray slash.
    /// `map::avatar` handles both.
    pub avatar_path: Option<String>,
}
