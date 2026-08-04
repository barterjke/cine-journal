//! Pure `dto` → `models` conversion. No I/O, no clock, no network.
//!
//! Everything here is a total function of its arguments, which is what lets the
//! tests at the bottom run against the recorded payloads in
//! `backend/tests/fixtures/` instead of against TMDB. A field renamed upstream
//! then fails a test rather than a request.

use crate::data::CatalogueEntry;
use crate::models::*;

use super::dto;

/// TMDB's genre ids, which are a stable documented enumeration rather than
/// something to be discovered at runtime.
///
/// Kept in code so genre filtering works on the first request and can't be
/// broken by one failed call, and `genre_table_matches_tmdb` asserts it against
/// the recorded `/3/genre/movie/list` payload so upstream drift fails a test.
///
/// The labels are the *export's* vocabulary, not TMDB's: the search sidebar's
/// chips say "Sci-Fi" (`data::GENRE_FACETS`) and TMDB says "Science Fiction". If
/// those two disagreed, the detail page's genre chip would link to
/// `/search?genre=Science Fiction`, which no facet matches — the chip and the
/// film would name the same genre differently.
const GENRES: [(u32, &str, &str); 19] = [
    (28, "Action", "Action"),
    (12, "Adventure", "Adventure"),
    (16, "Animation", "Animation"),
    (35, "Comedy", "Comedy"),
    (80, "Crime", "Crime"),
    (99, "Documentary", "Documentary"),
    (18, "Drama", "Drama"),
    (10751, "Family", "Family"),
    (14, "Fantasy", "Fantasy"),
    (36, "History", "History"),
    (27, "Horror", "Horror"),
    (10402, "Music", "Music"),
    (9648, "Mystery", "Mystery"),
    (10749, "Romance", "Romance"),
    (878, "Science Fiction", "Sci-Fi"),
    (10770, "TV Movie", "TV Movie"),
    (53, "Thriller", "Thriller"),
    (10752, "War", "War"),
    (37, "Western", "Western"),
];

/// The export's label for a TMDB genre name, unchanged if there's no alias.
pub fn genre_label(tmdb_name: &str) -> String {
    GENRES
        .iter()
        .find(|(_, tmdb, _)| *tmdb == tmdb_name)
        .map_or_else(|| tmdb_name.to_string(), |(_, _, label)| (*label).to_string())
}

/// The TMDB id behind a genre id, by either vocabulary.
///
/// Accepts the display label ("Sci-Fi") and TMDB's own name ("Science Fiction")
/// so a hand-typed `?genre=` in the URL works either way.
pub fn genre_id(label: &str) -> Option<u32> {
    GENRES
        .iter()
        .find(|(_, tmdb, display)| label.eq_ignore_ascii_case(tmdb) || label.eq_ignore_ascii_case(display))
        .map(|(id, _, _)| *id)
}

fn genre_labels_for_ids(ids: &[u32]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| GENRES.iter().find(|(gid, _, _)| gid == id))
        .map(|(_, _, label)| (*label).to_string())
        .collect()
}

// --- App ids ------------------------------------------------------------------

/// The app id for a TMDB film: `157336-interstellar`.
///
/// `Movie.id` is a `String` throughout, and the demo data fills it with slugs, so
/// a bare number would work but read as an opaque token in a URL. The numeric
/// prefix is what's authoritative — `tmdb_id` ignores everything after it, so
/// `/movie/157336` resolves too and a stale link with an outdated slug still
/// lands on the right film.
///
/// A demo slug can never collide with one of these: none of them start with a
/// digit.
pub fn app_id(tmdb_id: u32, title: &str) -> String {
    let slug = slugify(title);
    if slug.is_empty() {
        tmdb_id.to_string()
    } else {
        format!("{tmdb_id}-{slug}")
    }
}

/// The TMDB id an app id refers to, or `None` for a demo slug.
pub fn tmdb_id(app_id: &str) -> Option<u32> {
    let digits: String = app_id.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    // A slug that merely *starts* with digits ("2001-a-space-odyssey" as a demo
    // entry) would parse here, which is why demo ids are checked first by the
    // caller. Within TMDB mode the prefix is always the id.
    digits.parse().ok()
}

/// "Dune: Part Two" -> "dune-part-two". Capped so one absurd title can't produce
/// an unwieldy URL.
fn slugify(title: &str) -> String {
    const MAX: usize = 60;
    let mut slug = String::new();
    let mut pending_dash = false;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else if ch.is_alphanumeric() {
            // Non-ASCII letters ("Amélie", "千と千尋") are dropped rather than
            // transliterated: the numeric prefix already identifies the film, so
            // the slug only has to be readable, and a percent-encoded URL is
            // less readable than a shorter one. No dash is inserted for them —
            // "amlie" reads better than "am-lie", and a title with no ASCII at
            // all falls back to the bare id.
        } else {
            pending_dash = true;
        }
    }

    // Truncated once, at the end. Doing it inside the loop is fiddly — a
    // separator contributes a dash *and* a letter in one pass — and every
    // character pushed above is single-byte ASCII, so this can't split a `char`.
    slug.truncate(MAX);
    slug.trim_matches('-').to_string()
}

// --- Images -------------------------------------------------------------------

/// Which CDN and which rendition to ask for.
///
/// Sizes are chosen against how the screens actually draw them: posters at most
/// ~300px wide (`w500` covers a 2× display), the Media tile ~640px (`w1280`),
/// cast circles ~80px (`w185`), provider logos 32px (`w92`). Asking for
/// `original` would ship multi-megabyte JPEGs into a row of thumbnails.
#[derive(Debug, Clone)]
pub struct ImageBase {
    base: String,
    poster: String,
    backdrop: String,
    profile: String,
    logo: String,
}

impl Default for ImageBase {
    /// The values `/3/configuration` returns today, so a failed config call
    /// degrades to correct URLs rather than to no images.
    fn default() -> Self {
        Self {
            base: "https://image.tmdb.org/t/p/".into(),
            poster: "w500".into(),
            backdrop: "w1280".into(),
            profile: "w185".into(),
            // Provider logos are drawn at 32px; `w92` covers a 2× display.
            logo: "w92".into(),
        }
    }
}

impl ImageBase {
    pub fn from_config(config: &dto::ImageConfig) -> Self {
        let fallback = Self::default();
        Self {
            base: config.secure_base_url.clone(),
            poster: pick(&config.poster_sizes, &fallback.poster),
            backdrop: pick(&config.backdrop_sizes, &fallback.backdrop),
            profile: pick(&config.profile_sizes, &fallback.profile),
            logo: pick(&config.logo_sizes, &fallback.logo),
        }
    }

    fn url(&self, size: &str, path: &str) -> String {
        // TMDB paths are already leading-slashed; the base already trailing-.
        format!("{}{}{}", self.base, size, path)
    }

    pub fn poster(&self, path: &str, title: &str) -> Image {
        Image::new(&self.url(&self.poster, path), &format!("Poster for {title}."))
    }

    pub fn backdrop(&self, path: &str, title: &str) -> Image {
        Image::new(&self.url(&self.backdrop, path), &format!("A still frame from {title}."))
    }

    pub fn profile(&self, path: &str, name: &str) -> Image {
        Image::new(&self.url(&self.profile, path), &format!("A portrait of {name}."))
    }

    /// A streaming service's wordmark, for a "Where to Watch" row.
    pub fn logo(&self, path: &str, provider: &str) -> Image {
        Image::new(&self.url(&self.logo, path), &format!("The {provider} logo."))
    }

    /// A review author's avatar.
    ///
    /// `avatar_path` is absent on more than half of real reviews (verified: 4 of 7
    /// on one film, 23 of 41 across four), and TMDB documents a second form where a
    /// full Gravatar URL arrives behind a stray leading slash
    /// (`/https://secure.gravatar.com/…`). Both are handled here so no caller has
    /// to know.
    pub fn avatar(&self, path: Option<&str>, name: &str) -> Image {
        let alt = format!("{name}'s profile picture.");
        match path {
            Some(path) if path.starts_with("/http") => Image::new(&path[1..], &alt),
            Some(path) if !path.is_empty() => Image::new(&self.url(&self.profile, path), &alt),
            // Deliberately *not* one of the export's photographs: putting a picture
            // of a specific person on a majority of real reviews attributes them to
            // someone who didn't write them. Initials are honest about having no
            // photo, and the frontend draws them as a monogram.
            _ => Image::new(&initials_avatar(name), &alt),
        }
    }
}

/// A `data:` URI drawing someone's initials — the stand-in when there is no photo.
///
/// Inline SVG rather than a static file so it needs no asset and no new route, and
/// rather than a canvas or a font glyph so it renders identically everywhere. Palette
/// and radius follow the export's avatar treatment: a flat slate circle, white text.
fn initials_avatar(name: &str) -> String {
    let initials: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(|c| c.to_uppercase())
        .collect();
    // A name with no letters at all ("...") would otherwise draw an empty circle.
    let initials = if initials.is_empty() { "?".to_string() } else { initials };

    // No user text reaches this unescaped: only the alphanumerics kept above, so
    // there is nothing that could close the `<text>` element.
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'>\
         <rect width='64' height='64' fill='%23475569'/>\
         <text x='32' y='33' fill='%23ffffff' font-family='Inter, system-ui, sans-serif' \
         font-size='26' font-weight='600' text-anchor='middle' dominant-baseline='central'>\
         {initials}</text></svg>"
    );
    format!("data:image/svg+xml;charset=utf-8,{svg}")
}

/// The preferred size if the CDN offers it, else whatever it offers largest.
fn pick(offered: &[String], preferred: &str) -> String {
    if offered.iter().any(|s| s == preferred) {
        return preferred.to_string();
    }
    // `original` is last in TMDB's lists and is full-resolution — a 4K backdrop
    // behind a cast circle — so the largest *named* size is the one to fall back to.
    offered.iter().rfind(|s| *s != "original").cloned().unwrap_or_else(|| preferred.to_string())
}

// --- Scalars ------------------------------------------------------------------

/// `vote_average` (0.0–10.0) as half-stars out of five (0–10).
///
/// The scales coincide, so this is a round rather than a rescale: 8.0 → 8 halves
/// → four filled stars. Clamped because TMDB has been seen to return slightly
/// over 10.0.
pub fn half_stars(vote_average: f32) -> u8 {
    vote_average.round().clamp(0.0, 10.0) as u8
}

/// The fractional 0.0–5.0 average the search cards print as a number.
pub fn star_rating(vote_average: f32) -> f32 {
    (vote_average / 2.0).clamp(0.0, 5.0)
}

/// The year out of "2014-11-05".
pub fn year(release_date: Option<&str>) -> Option<u16> {
    release_date?.get(..4)?.parse().ok()
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "2014-11-12T16:06:04Z" -> "November 12, 2014".
///
/// Parsed off the fixed prefix rather than with a date crate: the only date
/// arithmetic in the app is none, and the format is pinned by TMDB.
pub fn long_date(timestamp: &str) -> Option<String> {
    let date = timestamp.get(..10)?;
    let mut parts = date.split('-');
    let y: u16 = parts.next()?.parse().ok()?;
    let m: usize = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    let month = MONTHS.get(m.checked_sub(1)?)?;
    Some(format!("{month} {d}, {y}"))
}

/// 169 -> "2h 49m". `None` and 0 give an em dash — the metadata row reads fine
/// with one, and "0m" would look like a bug.
pub fn runtime(minutes: Option<u32>) -> String {
    match minutes {
        Some(m) if m >= 60 => {
            let (h, r) = (m / 60, m % 60);
            if r == 0 {
                format!("{h}h")
            } else {
                format!("{h}h {r}m")
            }
        }
        Some(m) if m > 0 => format!("{m}m"),
        _ => "—".into(),
    }
}

/// Review prose split into the paragraphs the screens render.
///
/// Real payloads separate them with `\r\n\r\n`. A body with no blank line at all
/// becomes one paragraph rather than none.
pub fn paragraphs(content: &str) -> Vec<String> {
    let split: Vec<String> = content
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(|p| p.replace('\n', " ").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    if split.is_empty() {
        vec![content.trim().to_string()]
    } else {
        split
    }
}

fn crew_named(credits: &dto::Credits, job: &str) -> Option<String> {
    credits.crew.iter().find(|c| c.job == job).map(|c| c.name.clone())
}

/// Everyone credited with the first of `jobs` that anyone holds, comma-joined.
///
/// Two things `crew_named` can't do, both forced by what the payloads actually
/// contain (checked across five films): a film credits several writers, and the
/// job title varies — three modern films expose only `Writer`, while *Absolute
/// Power* credits `Screenplay` + `Novel` and *Your Fault* `Novel` +
/// `Screenplay`. Hence a priority chain.
///
/// Only the first matching title's group is taken, not the union: a row reading
/// "Writers" should name the people who wrote the screenplay, and appending the
/// novelist would credit them for work they didn't do.
fn crew_group(credits: &dto::Credits, jobs: &[&str]) -> Option<String> {
    for job in jobs {
        let mut names: Vec<&str> = Vec::new();
        for credit in credits.crew.iter().filter(|c| c.job == *job) {
            // A person can hold the same job twice on one film (co-credits are
            // sometimes duplicated per department), and the row would repeat them.
            if !names.contains(&credit.name.as_str()) {
                names.push(&credit.name);
            }
        }
        // Three names is what the grid's second column fits on one line; beyond
        // that the row wraps and the label stops lining up with its value.
        if !names.is_empty() {
            names.truncate(3);
            return Some(names.join(", "));
        }
    }
    None
}

/// Which country's age rating and streaming availability to report.
///
/// TMDB publishes both per country with no global value, so one has to be
/// chosen. A constant rather than a request parameter: the app has one visitor
/// and no locale, and inventing a `?country=` nobody sets would be a knob
/// pretending to be a feature.
const COUNTRY: &str = "US";

/// The age rating for `COUNTRY`, or `None`.
///
/// Empty-string certifications are common and mean "released here, not rated" —
/// distinct from a rating *of* "NR". Skipping them lets the metadata line drop
/// the segment rather than print an empty gap between two bullets.
pub fn certification(dates: &dto::ReleaseDates) -> Option<String> {
    dates
        .results
        .iter()
        .find(|entry| entry.iso_3166_1 == COUNTRY)?
        .release_dates
        .iter()
        .map(|release| release.certification.trim())
        .find(|cert| !cert.is_empty())
        .map(str::to_string)
}

/// The one video the Media block plays.
///
/// Ranked: official beats a fan upload, a `Trailer` beats a `Teaser` beats
/// anything else, and newer beats older. Films carry two to five official
/// trailers each, so without the recency tiebreak the tile would play whichever
/// TMDB happened to list first — frequently a teaser from a year before release.
///
/// Non-YouTube videos are dropped. Vimeo appears occasionally and the frontend
/// only builds YouTube embeds, so a Vimeo `key` would render a dead player.
fn pick_video(videos: &dto::Videos) -> Option<&dto::VideoRecord> {
    fn rank(kind: &str) -> u8 {
        match kind {
            "Trailer" => 2,
            "Teaser" => 1,
            _ => 0,
        }
    }

    videos
        .results
        .iter()
        .filter(|v| v.site == "YouTube")
        .max_by(|a, b| {
            a.official
                .cmp(&b.official)
                .then_with(|| rank(&a.kind).cmp(&rank(&b.kind)))
                // RFC 3339 with a fixed offset sorts correctly as text, so no
                // date parsing is needed to find the newest.
                .then_with(|| a.published_at.cmp(&b.published_at))
        })
}

/// The "Where to Watch" rows for `COUNTRY`, and the one link they all share.
///
/// TMDB's five availability lists collapse to the four labels the design prints:
/// `flatrate` is "Stream", `free` and `ads` are both "Free", and rent and buy
/// keep their names. They're read in that order — the order a visitor cares
/// about — and a provider appearing in several (Apple TV rents *and* sells
/// almost everything) is listed once, under the first.
///
/// The shapes vary widely: of five films checked, one had only `flatrate` and
/// another no `flatrate` at all. Any subset is normal, and nothing here is
/// required — an empty result hides the section.
pub fn watch_options(
    providers: &dto::WatchProviders,
    images: &ImageBase,
) -> (Vec<WatchOption>, Option<String>) {
    let Some(country) = providers.results.get(COUNTRY) else {
        return (Vec::new(), None);
    };

    // A popular film can be on twenty services. The column has room for a
    // handful, and the link at the bottom is where "all of them" lives.
    const MAX_ROWS: usize = 4;

    let groups: [(&str, &Vec<dto::Provider>); 5] = [
        ("Stream", &country.flatrate),
        ("Free", &country.free),
        ("Free", &country.ads),
        ("Rent", &country.rent),
        ("Buy", &country.buy),
    ];

    let mut rows: Vec<WatchOption> = Vec::new();
    for (kind, group) in groups {
        let mut group: Vec<&dto::Provider> = group.iter().collect();
        // TMDB's own prominence hint, so the row order matches what its site shows.
        group.sort_by_key(|p| p.display_priority);
        for provider in group {
            if rows.len() == MAX_ROWS {
                break;
            }
            if rows.iter().any(|row| row.provider == provider.provider_name) {
                continue;
            }
            rows.push(WatchOption {
                provider: provider.provider_name.clone(),
                kind: kind.to_string(),
                logo: provider
                    .logo_path
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .map(|p| images.logo(p, &provider.provider_name)),
            });
        }
    }

    // The link is dropped along with the rows: on its own it would be a section
    // heading above a single "see all" that lists nothing.
    let link = if rows.is_empty() { None } else { country.link.clone() };
    (rows, link)
}

// --- Screens ------------------------------------------------------------------

/// The small `Movie` the feeds and reviews embed.
///
/// Returns `None` for a film with no poster: `Movie.poster` is a required
/// `Image`, and a card whose poster 404s is worse than one film fewer in a
/// curated rail. `SearchResult`, whose poster *is* optional, keeps them — that
/// screen has a "Poster Missing" placeholder.
pub fn movie(summary: &dto::MovieSummary, images: &ImageBase) -> Option<Movie> {
    let poster = summary.poster_path.as_deref()?;
    Some(Movie {
        id: app_id(summary.id, &summary.title),
        title: summary.title.clone(),
        year: year(summary.release_date.as_deref()),
        poster: images.poster(poster, &summary.title),
    })
}

/// A search-result card. Posterless films are kept — see `movie`.
pub fn search_result(summary: &dto::MovieSummary, images: &ImageBase) -> CatalogueEntry {
    CatalogueEntry {
        id: app_id(summary.id, &summary.title),
        title: summary.title.clone(),
        // The decade filter needs a number. A film with no release date is
        // unreleased; 0 puts it outside every decade facet, so it shows only
        // when no decade is selected — the same treatment the demo gives
        // Le Souffle (1960) against its three 2000s-and-later chips.
        year: year(summary.release_date.as_deref()).unwrap_or(0),
        star_rating: star_rating(summary.vote_average),
        poster: summary.poster_path.as_deref().map(|p| images.poster(p, &summary.title)),
        // Desaturating one poster was per-item art direction in the export, not
        // a rule a grid can infer, and there is no upstream equivalent.
        grayscale: false,
        genres: genre_labels_for_ids(&summary.genre_ids),
    }
}

/// The detail screen.
///
/// `on_watchlist` and `your_rating_half_stars` are left at their defaults — as in
/// `data`, `hydrate` fills them from the store.
pub fn movie_detail(detail: &dto::MovieDetail, images: &ImageBase) -> MovieDetail {
    let title = detail.title.clone();

    let poster = detail
        .poster_path
        .as_deref()
        .map(|p| images.poster(p, &title))
        .unwrap_or_else(|| Image::new("img/poster-neon-reverie.jpg", &format!("Poster for {title}.")));

    // The still behind the Media block's play button. Prefers the film's own
    // backdrop, then any from the images block, then the poster — TMDB serves no
    // per-video thumbnail, and an empty src would leave a 16:9 hole with a play
    // glyph floating in it.
    let backdrop = detail
        .backdrop_path
        .as_deref()
        .or_else(|| detail.images.backdrops.first().map(|b| b.file_path.as_str()))
        .map(|p| images.backdrop(p, &title))
        .unwrap_or_else(|| poster.clone());

    let cast = detail
        .credits
        .cast
        .iter()
        // Ten, because the design scrolls the cast horizontally rather than
        // gridding it: the rail shows six or seven at desktop width and the rest
        // are what there is to scroll to. "Full cast & crew" covers the rest.
        .take(10)
        .map(|member| CastMember {
            id: member.id.to_string(),
            name: member.name.clone(),
            role: member
                .character
                .as_deref()
                .filter(|c| !c.is_empty())
                .unwrap_or("—")
                .to_string(),
            portrait: match member.profile_path.as_deref() {
                Some(path) => images.profile(path, &member.name),
                // An initials monogram rather than a stock face, for the same
                // reason as the review avatars — see `ImageBase::avatar`.
                None => Image::new(&initials_avatar(&member.name), &format!("A portrait of {}.", member.name)),
            },
        })
        .collect();

    // The five rows of the credits grid, in the reference's order. A row whose
    // value TMDB doesn't have is omitted rather than shown as "Unknown" — the
    // frontend maps over this list, so a sparse film just draws fewer rows.
    let mut details = Vec::new();
    if let Some(director) = crew_named(&detail.credits, "Director") {
        details.push(DetailFact { label: "Director".into(), value: director });
    }
    // The job title varies by film, so this is a chain rather than one match —
    // see `crew_group`.
    if let Some(writers) = crew_group(&detail.credits, &["Writer", "Screenplay", "Story", "Novel", "Author", "Book"]) {
        details.push(DetailFact { label: "Writers".into(), value: writers });
    }
    if let Some(dop) = crew_named(&detail.credits, "Director of Photography") {
        details.push(DetailFact { label: "Cinematography".into(), value: dop });
    }
    if let Some(composer) = crew_named(&detail.credits, "Original Music Composer") {
        details.push(DetailFact { label: "Music".into(), value: composer });
    }
    if let Some(studio) = detail.production_companies.first() {
        details.push(DetailFact { label: "Production".into(), value: studio.name.clone() });
    }

    let synopsis = detail
        .overview
        .as_deref()
        .filter(|o| !o.trim().is_empty())
        .or(detail.tagline.as_deref())
        .unwrap_or("No synopsis on file.")
        .to_string();

    let trailer = pick_video(&detail.videos).map(|video| Trailer {
        name: video.name.clone(),
        key: video.key.clone(),
        site: video.site.clone(),
        thumbnail: backdrop.clone(),
    });

    let (watch_options, watch_link) = watch_options(&detail.watch_providers, images);

    MovieDetail {
        id: app_id(detail.id, &title),
        year: year(detail.release_date.as_deref()).unwrap_or(0),
        certification: certification(&detail.release_dates),
        runtime: runtime(detail.runtime),
        genres: detail.genres.iter().map(|g| genre_label(&g.name)).collect(),
        poster,
        backdrop,
        synopsis,
        // Carried at TMDB's own 0–10 scale rather than rounded to half-stars:
        // the design prints "7.8 / 10", and half-stars would render it 8.0.
        score: detail.vote_average.clamp(0.0, 10.0),
        vote_count: detail.vote_count,
        details,
        trailer,
        watch_options,
        watch_link,
        cast,
        title,
        on_watchlist: false,
        your_rating_half_stars: None,
    }
}

/// One TMDB review as the review screens expect it.
///
/// `comments` is left empty: TMDB has no reply threads, so the conversation comes
/// from SQLite and `hydrate::review` appends it. `like_count` is `None` for the
/// same reason — there is no upstream count, and `hydrate::like_count` renders
/// nothing until the visitor likes it, then 1, rather than a visible zero.
pub fn review(
    record: &dto::ReviewRecord,
    film: &dto::MovieDetail,
    images: &ImageBase,
) -> Review {
    let movie = Movie {
        id: app_id(film.id, &film.title),
        title: film.title.clone(),
        year: year(film.release_date.as_deref()),
        poster: film
            .poster_path
            .as_deref()
            .map(|p| images.poster(p, &film.title))
            .unwrap_or_else(|| Image::new("img/poster-neon-reverie.jpg", &format!("Poster for {}.", film.title))),
    };

    Review {
        id: review_id(film.id, &record.id),
        movie,
        backdrop: film.backdrop_path.as_deref().map(|p| images.backdrop(p, &film.title)),
        director: crew_named(&film.credits, "Director"),
        genres: film.genres.iter().map(|g| genre_label(&g.name)).collect(),
        author_name: record.author.clone(),
        author_avatar: images.avatar(record.author_details.avatar_path.as_deref(), &record.author),
        watched_on: match long_date(&record.created_at) {
            Some(date) => format!("Reviewed on {date}"),
            None => "Reviewed recently".into(),
        },
        // An unrated review still draws a star row, so fall back to the crowd
        // average rather than to zero stars, which would misreport the author.
        rating_half_stars: half_stars(record.author_details.rating.unwrap_or(film.vote_average)),
        paragraphs: paragraphs(&record.content),
        like_count: None,
        comments: Vec::new(),
        hashtags: Vec::new(),
        liked: false,
    }
}

/// `157336-5463856c0e0a267815002598` — the film scopes the review, which is what
/// lets one id identify a review without a second lookup to find its film.
pub fn review_id(tmdb_movie_id: u32, tmdb_review_id: &str) -> String {
    format!("{tmdb_movie_id}-{tmdb_review_id}")
}

/// The film id and review id back out of one. Inverse of `review_id`.
pub fn split_review_id(id: &str) -> Option<(u32, &str)> {
    let (movie, review) = id.split_once('-')?;
    Some((movie.parse().ok()?, review))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
    }

    fn trending() -> dto::Page<dto::MovieSummary> {
        serde_json::from_str(&fixture("trending.json")).expect("trending.json")
    }

    fn interstellar() -> dto::MovieDetail {
        serde_json::from_str(&fixture("movie-157336.json")).expect("movie-157336.json")
    }

    fn reviews() -> dto::Page<dto::ReviewRecord> {
        serde_json::from_str(&fixture("reviews-157336.json")).expect("reviews-157336.json")
    }

    /// The genre table is transcribed rather than fetched, so it has to be
    /// checked against the real list — otherwise a renamed or added genre would
    /// silently stop filtering.
    #[test]
    fn genre_table_matches_tmdb() {
        let list: dto::GenreList = serde_json::from_str(&fixture("genres.json")).unwrap();
        assert_eq!(list.genres.len(), GENRES.len());
        for genre in &list.genres {
            let found = GENRES.iter().find(|(id, _, _)| *id == genre.id);
            let (_, tmdb_name, _) = found.unwrap_or_else(|| panic!("unknown genre id {}", genre.id));
            assert_eq!(*tmdb_name, genre.name);
        }
    }

    /// The one alias that matters: the sidebar chip says "Sci-Fi".
    #[test]
    fn science_fiction_is_relabelled_to_the_chip_vocabulary() {
        assert_eq!(genre_label("Science Fiction"), "Sci-Fi");
        assert_eq!(genre_label("Drama"), "Drama");
        assert_eq!(genre_label("Something New"), "Something New");

        // And both spellings resolve back to the id `discover` wants.
        assert_eq!(genre_id("Sci-Fi"), Some(878));
        assert_eq!(genre_id("Science Fiction"), Some(878));
        assert_eq!(genre_id("sci-fi"), Some(878));
        assert_eq!(genre_id("Noir"), None);
    }

    #[test]
    fn app_ids_round_trip() {
        assert_eq!(app_id(157336, "Interstellar"), "157336-interstellar");
        assert_eq!(app_id(693134, "Dune: Part Two"), "693134-dune-part-two");
        assert_eq!(tmdb_id("157336-interstellar"), Some(157336));
        // A bare numeric id resolves, so a link that lost its slug still works.
        assert_eq!(tmdb_id("157336"), Some(157336));
        // Demo slugs never start with a digit, so they can't be mistaken for one.
        assert_eq!(tmdb_id("neon-reverie"), None);
        assert_eq!(tmdb_id(""), None);
    }

    #[test]
    fn slugs_drop_punctuation_without_collapsing_words() {
        assert_eq!(slugify("Spider-Man: Brand New Day"), "spider-man-brand-new-day");
        assert_eq!(slugify("  ...  "), "");
        assert_eq!(slugify("2001: A Space Odyssey"), "2001-a-space-odyssey");
        // Non-ASCII letters are dropped, not transliterated — the numeric prefix
        // is what identifies the film.
        assert_eq!(slugify("Amélie"), "amlie");
        // A title with no ASCII at all yields no slug, and `app_id` falls back to
        // the bare number rather than emitting a trailing dash.
        assert_eq!(slugify("千と千尋の神隠し"), "");
        assert_eq!(app_id(129, "千と千尋の神隠し"), "129");
        // The cap is a hard bound: a separator adds a dash and a letter in one
        // pass, so an end-of-loop check would overshoot.
        assert!(slugify(&"long ".repeat(40)).len() <= 60);
    }

    #[test]
    fn ratings_map_between_the_two_scales() {
        assert_eq!(half_stars(8.0), 8); // four filled stars
        assert_eq!(half_stars(6.7), 7); // three and a half
        assert_eq!(half_stars(0.0), 0);
        assert_eq!(half_stars(11.0), 10); // clamped
        assert_eq!(star_rating(8.0), 4.0);
        assert_eq!(star_rating(0.0), 0.0);
    }

    #[test]
    fn runtimes_read_like_the_export() {
        assert_eq!(runtime(Some(169)), "2h 49m");
        assert_eq!(runtime(Some(118)), "1h 58m");
        assert_eq!(runtime(Some(120)), "2h");
        assert_eq!(runtime(Some(42)), "42m");
        assert_eq!(runtime(Some(0)), "—");
        assert_eq!(runtime(None), "—");
    }

    #[test]
    fn dates_are_parsed_off_the_fixed_prefix() {
        assert_eq!(long_date("2014-11-12T16:06:04Z").as_deref(), Some("November 12, 2014"));
        assert_eq!(long_date("2014-11-05").as_deref(), Some("November 5, 2014"));
        assert_eq!(long_date("not a date"), None);
        assert_eq!(long_date("2014-13-01"), None); // month 13 has no name
        assert_eq!(year(Some("2014-11-05")), Some(2014));
        assert_eq!(year(None), None);
    }

    #[test]
    fn review_prose_splits_on_blank_lines() {
        let body = "First para.\r\n\r\nSecond para.\r\n\r\n\r\nThird.";
        assert_eq!(paragraphs(body), ["First para.", "Second para.", "Third."]);
        // No blank line at all is one paragraph, not none.
        assert_eq!(paragraphs("Just one line."), ["Just one line."]);
        assert_eq!(paragraphs("   ").len(), 1);
    }

    #[test]
    fn image_urls_are_absolute_and_survive_image_new() {
        let images = ImageBase::default();
        let poster = images.poster("/abc.jpg", "Interstellar");
        assert_eq!(poster.src, "https://image.tmdb.org/t/p/w500/abc.jpg");
        assert_eq!(poster.alt, "Poster for Interstellar.");

        // `Image::new` prepends `/` to relative paths; an absolute URL must pass
        // through untouched or every remote image would 404.
        assert!(!poster.src.starts_with("/http"));
        assert_eq!(images.backdrop("/b.jpg", "X").src, "https://image.tmdb.org/t/p/w1280/b.jpg");
    }

    #[test]
    fn configured_sizes_are_used_when_offered() {
        let config: dto::Configuration = serde_json::from_str(&fixture("configuration.json")).unwrap();
        let images = ImageBase::from_config(&config.images);
        assert_eq!(images.poster("/a.jpg", "T").src, "https://image.tmdb.org/t/p/w500/a.jpg");

        // A CDN that stops offering w500 falls back to its largest sized
        // rendition rather than to a URL that 404s.
        let narrow = dto::ImageConfig {
            secure_base_url: "https://cdn/".into(),
            poster_sizes: vec!["w92".into(), "w154".into(), "original".into()],
            backdrop_sizes: vec![],
            profile_sizes: vec![],
            logo_sizes: vec![],
        };
        assert_eq!(ImageBase::from_config(&narrow).poster("/a.jpg", "T").src, "https://cdn/w154/a.jpg");
        // An empty list can't even fall back, so the preferred size stands.
        assert_eq!(ImageBase::from_config(&narrow).logo("/l.jpg", "Hulu").src, "https://cdn/w92/l.jpg");
    }

    /// More than half of real reviews have no avatar, and TMDB documents a
    /// second form where a full Gravatar URL arrives behind a stray slash.
    #[test]
    fn avatars_handle_missing_and_gravatar_forms() {
        let images = ImageBase::default();
        assert_eq!(
            images.avatar(Some("/k5J.jpg"), "Matthew").src,
            "https://image.tmdb.org/t/p/w185/k5J.jpg"
        );
        assert_eq!(
            images.avatar(Some("/https://secure.gravatar.com/avatar/x.jpg"), "A").src,
            "https://secure.gravatar.com/avatar/x.jpg"
        );
        // No photo -> initials, not someone else's face. Most real reviews take
        // this path, so it has to be honest about who wrote them.
        let anon = images.avatar(None, "Manuel São Bento");
        assert!(anon.src.starts_with("data:image/svg+xml"), "got {}", anon.src);
        assert!(anon.src.contains(">MS<"), "got {}", anon.src);
        assert_eq!(anon.alt, "Manuel São Bento's profile picture.");
        // An empty string is as absent as a null.
        assert!(images.avatar(Some(""), "nkuk").src.contains(">N<"));
    }

    #[test]
    fn initials_come_from_the_first_two_words() {
        assert!(initials_avatar("Elena Rostova").contains(">ER<"));
        assert!(initials_avatar("Manuel São Bento").contains(">MS<"), "at most two");
        assert!(initials_avatar("nkuk").contains(">N<"));
        assert!(initials_avatar("  spaced   out  ").contains(">SO<"));
        // Leading punctuation is skipped rather than drawn.
        assert!(initials_avatar("\"Quoted\" Name").contains(">QN<"));
        // A name with no letters still draws something.
        assert!(initials_avatar("...").contains(">?<"));
        assert!(initials_avatar("").contains(">?<"));
        // Non-ASCII names keep their own letters.
        assert!(initials_avatar("Ольга Ким").contains(">ОК<"));
    }

    /// The initials are interpolated into markup, so a hostile name must not be
    /// able to contribute a single character of structure.
    #[test]
    fn initials_cannot_inject_markup() {
        let benign = initials_avatar("Ada Byron");
        let hostile = initials_avatar("<script>alert(1)</script> x");

        // Same shape, same length: only the two initials differ, so nothing from
        // the name reached the markup.
        assert_eq!(hostile.len(), benign.len(), "got {hostile}");
        assert_eq!(hostile.matches('<').count(), benign.matches('<').count());
        assert_eq!(hostile.matches('>').count(), benign.matches('>').count());
        // 'S' from <script>, 'X' from the second word — the only survivors.
        assert!(hostile.contains(">SX<"), "got {hostile}");
    }

    #[test]
    fn the_trending_page_maps_to_feed_movies() {
        let page = trending();
        let images = ImageBase::default();
        assert!(page.results.len() >= 10);

        let first = movie(&page.results[0], &images).expect("trending films have posters");
        assert!(first.id.starts_with(&page.results[0].id.to_string()));
        assert_eq!(first.title, page.results[0].title);
        assert!(first.poster.src.starts_with("https://image.tmdb.org/"));
        assert!(first.year.is_some());
    }

    #[test]
    fn a_posterless_film_is_dropped_from_the_feeds_but_kept_in_search() {
        let page: dto::Page<dto::MovieSummary> =
            serde_json::from_str(&fixture("search-untitled.json")).unwrap();
        let images = ImageBase::default();

        let posterless: Vec<_> = page.results.iter().filter(|r| r.poster_path.is_none()).collect();
        assert!(!posterless.is_empty(), "fixture should contain posterless films");

        // `Movie.poster` is required, so a feed card can't be built.
        assert!(movie(posterless[0], &images).is_none());
        // `SearchResult.poster` is optional — the grid has a placeholder.
        assert!(search_result(posterless[0], &images).poster.is_none());
    }

    #[test]
    fn interstellar_maps_to_a_complete_detail_page() {
        let images = ImageBase::default();
        let detail = movie_detail(&interstellar(), &images);

        assert_eq!(detail.id, "157336-interstellar");
        assert_eq!(detail.title, "Interstellar");
        assert_eq!(detail.year, 2014);
        assert_eq!(detail.certification.as_deref(), Some("PG-13"));
        assert_eq!(detail.runtime, "2h 49m");
        assert!(detail.genres.contains(&"Sci-Fi".to_string()), "got {:?}", detail.genres);
        assert!(detail.synopsis.len() > 80);

        // The score is the raw 0–10 average, not rounded to half-stars: the
        // design prints it to one decimal beside "/ 10".
        assert!((detail.score - interstellar().vote_average).abs() < f32::EPSILON);
        assert!(detail.vote_count > 1000, "got {}", detail.vote_count);

        // Ten fills the horizontal rail with something left to scroll to.
        assert_eq!(detail.cast.len(), 10);
        assert_eq!(detail.cast[0].name, "Matthew McConaughey");
        assert_eq!(detail.cast[0].role, "Cooper");

        // The five credit rows, in the order the grid draws them.
        let labels: Vec<&str> = detail.details.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["Director", "Writers", "Cinematography", "Music", "Production"]);
        let by = |label: &str| {
            detail.details.iter().find(|f| f.label == label).map(|f| f.value.clone()).unwrap()
        };
        assert_eq!(by("Director"), "Christopher Nolan");
        // Both writers, in credit order — one row, not one per person.
        assert_eq!(by("Writers"), "Jonathan Nolan, Christopher Nolan");
        assert_eq!(by("Cinematography"), "Hoyte van Hoytema");
        assert_eq!(by("Music"), "Hans Zimmer");
        assert_eq!(by("Production"), "Legendary Pictures");

        // Filled by `hydrate`, never by the mapper.
        assert!(!detail.on_watchlist);
        assert_eq!(detail.your_rating_half_stars, None);
    }

    /// The Media block plays the newest official *trailer*, which is not the
    /// newest video: the fixture's two most recent entries are clips from after
    /// the film's 10th anniversary, so a pure recency sort would play one of them.
    #[test]
    fn the_media_block_prefers_the_newest_official_trailer() {
        let images = ImageBase::default();
        let detail = movie_detail(&interstellar(), &images);

        let trailer = detail.trailer.expect("Interstellar has official trailers");
        assert_eq!(trailer.name, "Trailer 4");
        assert_eq!(trailer.key, "LY19rHKAaAg");
        assert_eq!(trailer.site, "YouTube");
        // No per-video thumbnail upstream, so the tile shows the film's backdrop.
        assert_eq!(trailer.thumbnail.src, detail.backdrop.src);
    }

    /// A film with no video at all has no Media block, and a Vimeo-only one is
    /// treated the same — the frontend builds YouTube embeds only, so a Vimeo key
    /// would render a dead player.
    #[test]
    fn a_film_with_no_embeddable_video_has_no_trailer() {
        let videos: dto::Videos = serde_json::from_str(r#"{"results":[]}"#).unwrap();
        assert!(pick_video(&videos).is_none());

        let vimeo: dto::Videos = serde_json::from_str(
            r#"{"results":[{"name":"T","key":"123","site":"Vimeo","type":"Trailer","official":true}]}"#,
        )
        .unwrap();
        assert!(pick_video(&vimeo).is_none());

        // A fan upload is still better than nothing when it's all there is.
        let unofficial: dto::Videos = serde_json::from_str(
            r#"{"results":[{"name":"T","key":"abc","site":"YouTube","type":"Trailer","official":false}]}"#,
        )
        .unwrap();
        assert_eq!(pick_video(&unofficial).map(|v| v.key.as_str()), Some("abc"));
    }

    /// An empty certification means "released here, not rated" and has to be
    /// skipped rather than printed — real payloads are full of them, and France's
    /// entry for this very film leads with two.
    #[test]
    fn certifications_skip_the_blank_entries() {
        let dates = interstellar().release_dates;
        assert_eq!(certification(&dates).as_deref(), Some("PG-13"));

        let blank: dto::ReleaseDates = serde_json::from_str(
            r#"{"results":[{"iso_3166_1":"US","release_dates":[{"certification":""},{"certification":"R"}]}]}"#,
        )
        .unwrap();
        assert_eq!(certification(&blank).as_deref(), Some("R"));

        // Nothing but blanks, and a country we don't report on, both yield None —
        // the metadata line then omits the segment instead of printing a gap.
        let all_blank: dto::ReleaseDates = serde_json::from_str(
            r#"{"results":[{"iso_3166_1":"US","release_dates":[{"certification":" "}]}]}"#,
        )
        .unwrap();
        assert_eq!(certification(&all_blank), None);
        let elsewhere: dto::ReleaseDates = serde_json::from_str(
            r#"{"results":[{"iso_3166_1":"FR","release_dates":[{"certification":"TP"}]}]}"#,
        )
        .unwrap();
        assert_eq!(certification(&elsewhere), None);
        assert_eq!(certification(&serde_json::from_str(r#"{"results":[]}"#).unwrap()), None);
    }

    #[test]
    fn watch_rows_dedupe_across_the_availability_lists() {
        let images = ImageBase::default();
        let detail = movie_detail(&interstellar(), &images);

        assert_eq!(detail.watch_options.len(), 4, "got {:?}", detail.watch_options);
        // `flatrate` is read first and sorted by TMDB's own prominence hint, so
        // Prime Video — priority 3 — heads the column.
        assert_eq!(detail.watch_options[0].provider, "Amazon Prime Video");
        assert_eq!(detail.watch_options[0].kind, "Stream");
        assert!(detail.watch_options[0].logo.is_some());
        assert!(
            detail.watch_link.as_deref().is_some_and(|l| l.contains("themoviedb.org")),
            "got {:?}",
            detail.watch_link
        );

        // Amazon Video both rents and sells this film; the column lists it once.
        let names: Vec<&str> = detail.watch_options.iter().map(|o| o.provider.as_str()).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "got {names:?}");
    }

    /// The provider shapes vary widely — of five films checked live, one had only
    /// `flatrate` and another none at all. Any subset has to map, and nothing is
    /// a section-hiding empty except a genuinely absent country.
    #[test]
    fn missing_provider_lists_are_normal_rather_than_an_error() {
        let images = ImageBase::default();

        let rent_only: dto::WatchProviders = serde_json::from_str(
            r#"{"results":{"US":{"link":"https://x","rent":[{"provider_name":"Apple TV","logo_path":null}]}}}"#,
        )
        .unwrap();
        let (rows, link) = watch_options(&rent_only, &images);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "Rent");
        // No artwork upstream — the frontend draws a generic glyph instead.
        assert!(rows[0].logo.is_none());
        assert_eq!(link.as_deref(), Some("https://x"));

        // Free and ad-supported both read as "Free": the distinction is upstream
        // bookkeeping, not something the row can usefully say.
        let ads: dto::WatchProviders = serde_json::from_str(
            r#"{"results":{"US":{"link":"https://x","ads":[{"provider_name":"Tubi","logo_path":"/t.jpg"}]}}}"#,
        )
        .unwrap();
        assert_eq!(watch_options(&ads, &images).0[0].kind, "Free");

        // A country TMDB has no data for hides the section — and takes the link
        // with it, since a heading over one "see all" lists nothing.
        let elsewhere: dto::WatchProviders =
            serde_json::from_str(r#"{"results":{"GB":{"link":"https://x","rent":[]}}}"#).unwrap();
        assert_eq!(watch_options(&elsewhere, &images).0.len(), 0);
        assert_eq!(watch_options(&elsewhere, &images).1, None);

        // Present but empty is the same as absent.
        let empty: dto::WatchProviders =
            serde_json::from_str(r#"{"results":{"US":{"link":"https://x"}}}"#).unwrap();
        assert_eq!(watch_options(&empty, &images).1, None);
    }

    /// The writers row needs a chain rather than one exact job: three modern
    /// films expose only `Writer`, while *Absolute Power* credits `Screenplay` +
    /// `Novel` and *Your Fault* `Novel` + `Screenplay`.
    #[test]
    fn the_writers_row_falls_through_the_job_titles() {
        let credits = |json: &str| -> dto::Credits { serde_json::from_str(json).unwrap() };
        let jobs = ["Writer", "Screenplay", "Story", "Novel", "Author", "Book"];

        let writer = credits(r#"{"crew":[{"name":"A","job":"Writer"},{"name":"B","job":"Writer"}]}"#);
        assert_eq!(crew_group(&writer, &jobs).as_deref(), Some("A, B"));

        // No `Writer` credit at all: the next title in the chain answers.
        let screenplay = credits(r#"{"crew":[{"name":"C","job":"Novel"},{"name":"D","job":"Screenplay"}]}"#);
        // Screenplay outranks Novel, and only the winning group is named — a row
        // saying "Writers" shouldn't credit the novelist for the script.
        assert_eq!(crew_group(&screenplay, &jobs).as_deref(), Some("D"));

        // Duplicated co-credits collapse; a fourth name is dropped so the row
        // stays on one line beside its label.
        let dupes = credits(
            r#"{"crew":[{"name":"A","job":"Writer"},{"name":"A","job":"Writer"},
                        {"name":"B","job":"Writer"},{"name":"C","job":"Writer"},
                        {"name":"D","job":"Writer"}]}"#,
        );
        assert_eq!(crew_group(&dupes, &jobs).as_deref(), Some("A, B, C"));

        assert_eq!(crew_group(&credits(r#"{"crew":[]}"#), &jobs), None);
    }

    /// A cast member with no photograph gets a monogram, not a stock face — the
    /// same reasoning as the review avatars.
    #[test]
    fn a_photoless_cast_member_gets_a_monogram() {
        let images = ImageBase::default();
        let mut film = interstellar();
        film.credits.cast[0].profile_path = None;

        let portrait = &movie_detail(&film, &images).cast[0].portrait;
        assert!(portrait.src.starts_with("data:image/svg+xml"), "got {}", portrait.src);
        assert!(portrait.src.contains(">MM<"), "got {}", portrait.src);
    }

    #[test]
    fn a_tmdb_review_maps_onto_the_review_screen() {
        let film = interstellar();
        let images = ImageBase::default();
        let page = reviews();
        let record = &page.results[0];

        let review = review(record, &film, &images);
        assert_eq!(review.id, format!("157336-{}", record.id));
        assert_eq!(split_review_id(&review.id), Some((157336, record.id.as_str())));
        assert_eq!(review.movie.title, "Interstellar");
        assert_eq!(review.director.as_deref(), Some("Christopher Nolan"));
        assert!(review.watched_on.starts_with("Reviewed on "));
        assert!(!review.paragraphs.is_empty());
        assert!(review.author_avatar.src.contains("://") || review.author_avatar.src.starts_with('/'));

        // No upstream count and no upstream thread — both come from elsewhere.
        assert_eq!(review.like_count, None);
        assert!(review.comments.is_empty());
        assert!(!review.liked);
    }

    /// An unrated review still draws a star row, so it borrows the crowd average
    /// rather than reporting the author gave it zero.
    #[test]
    fn an_unrated_review_falls_back_to_the_crowd_average() {
        let film = interstellar();
        let images = ImageBase::default();
        let page = reviews();

        let unrated = page
            .results
            .iter()
            .find(|r| r.author_details.rating.is_none())
            .expect("fixture should contain an unrated review");
        assert_eq!(review(unrated, &film, &images).rating_half_stars, half_stars(film.vote_average));

        let rated = page
            .results
            .iter()
            .find(|r| r.author_details.rating.is_some())
            .expect("fixture should contain a rated review");
        let expected = half_stars(rated.author_details.rating.unwrap());
        assert_eq!(review(rated, &film, &images).rating_half_stars, expected);
    }

    /// Every recorded payload deserializes — the point of keeping fixtures.
    #[test]
    fn every_fixture_deserializes() {
        let _: dto::Configuration = serde_json::from_str(&fixture("configuration.json")).unwrap();
        let _: dto::GenreList = serde_json::from_str(&fixture("genres.json")).unwrap();
        let _: dto::Page<dto::MovieSummary> = serde_json::from_str(&fixture("trending.json")).unwrap();
        let _: dto::Page<dto::MovieSummary> = serde_json::from_str(&fixture("search-dune.json")).unwrap();
        let _: dto::Page<dto::MovieSummary> =
            serde_json::from_str(&fixture("discover-878.json")).unwrap();
        let _: dto::MovieDetail = serde_json::from_str(&fixture("movie-157336.json")).unwrap();
        let _: dto::Page<dto::ReviewRecord> = serde_json::from_str(&fixture("reviews-157336.json")).unwrap();
    }
}
