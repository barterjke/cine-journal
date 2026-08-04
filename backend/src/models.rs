//! Wire types for the CinéJournal API.
//!
//! Every field maps to something the reference screens actually render, so there
//! is nothing here the frontend can't use. Ratings are carried as halves
//! (`rating_half_stars`, 0..=10) rather than floats — the screens draw discrete
//! full/half/empty star glyphs, and integers keep that exact.
//!
//! The static content is transcribed in `data`; anything the visitor changes
//! lives in `state` and is folded into these types by `hydrate` on the way out.
//! Request bodies are at the bottom of the file.

use serde::{Deserialize, Serialize};

/// An image as the export used it: a local path plus the alt text that was
/// transcribed from Stitch's `data-alt` generation prompt.
#[derive(Debug, Clone, Serialize)]
pub struct Image {
    pub src: String,
    pub alt: String,
}

impl Image {
    /// Callers pass the export's literal `img/…` string; the `src` that goes out
    /// on the wire is made root-relative.
    ///
    /// The export was a flat directory of HTML files, so `img/poster.jpg`
    /// resolved correctly from every page. In the SPA it does not: on
    /// `/movie/red-shift` the browser asks for `/movie/img/poster.jpg`, which the
    /// dev server answers with `index.html` and the tile renders as broken alt
    /// text. Normalizing here keeps the fix in one place instead of in ~60 call
    /// sites — and keeps them verbatim transcriptions.
    ///
    /// Anything already absolute is passed through untouched: a TMDB CDN URL, and
    /// the `data:` URI behind `tmdb::map::initials_avatar` — which has no `//`, so
    /// the test for one is a scheme rather than an authority.
    pub fn new(src: &str, alt: &str) -> Self {
        let src = if src.starts_with('/') || has_scheme(src) {
            src.to_string()
        } else {
            format!("/{src}")
        };
        Self { src, alt: alt.to_string() }
    }
}

/// Whether `src` starts with a URI scheme (`https:`, `data:`) rather than a path.
///
/// A bare `img/poster.jpg` has no colon; a Windows-style path can't reach here. The
/// scheme grammar is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`, so a leading
/// digit or a colon in a filename doesn't qualify.
fn has_scheme(src: &str) -> bool {
    let Some((scheme, _)) = src.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// A film. `year` is optional only because the mobile feed omits it.
#[derive(Debug, Clone, Serialize)]
pub struct Movie {
    pub id: String,
    pub title: String,
    pub year: Option<u16>,
    pub poster: Image,
}

/// A "Live Now" discussion card on the desktop feed.
#[derive(Debug, Clone, Serialize)]
pub struct LiveDiscussion {
    pub id: String,
    pub movie: Movie,
    pub rating_half_stars: u8,
    pub blurb: String,
    /// Avatars shown in the stacked row, left to right.
    pub participants: Vec<Image>,
    /// Rendered as "+14" after the avatars. `None` hides the chip.
    pub overflow_count: Option<u32>,
}

/// A poster tile in the desktop "Recent Entries" grid.
#[derive(Debug, Clone, Serialize)]
pub struct FeedEntry {
    pub id: String,
    pub movie: Movie,
    pub rating_half_stars: u8,
    /// Drives the hover "+" button's state. From `state`, not `data`.
    pub on_watchlist: bool,
}

/// What a friend did, for the desktop sidebar rail.
///
/// Serializes to a bare string ("watched" / "added_to_watchlist") — these are
/// unit variants, so no `tag`/`content` attributes: those would nest the value
/// in an object and the frontend compares against a plain string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// "watched <movie>"
    Watched,
    /// "added <movie> to Watchlist"
    AddedToWatchlist,
}

/// A row in the desktop "Friends Activity" sidebar.
#[derive(Debug, Clone, Serialize)]
pub struct FriendActivity {
    pub id: String,
    pub author_name: String,
    pub author_avatar: Image,
    /// Pre-formatted relative time, verbatim from the demo ("2h ago", "Yesterday").
    pub timestamp: String,
    pub kind: ActivityKind,
    /// Which film the row is about, so the title can link to its detail page.
    pub movie_id: String,
    pub movie_title: String,
    /// Absent for watchlist adds, which show no stars.
    pub rating_half_stars: Option<u8>,
    /// The pull-quote under the entry. Absent for watchlist adds.
    pub quote: Option<String>,
}

/// A circle in the mobile feed's stories rail.
#[derive(Debug, Clone, Serialize)]
pub struct Story {
    pub id: String,
    pub name: String,
    pub avatar: Image,
    /// Unseen stories get the blue gradient ring; seen ones are dimmed.
    pub unseen: bool,
}

/// A poster card in the mobile feed grid.
#[derive(Debug, Clone, Serialize)]
pub struct MobileFeedItem {
    pub id: String,
    pub movie: Movie,
    /// Pre-formatted subtitle, verbatim ("Elena watched • 4h ago").
    pub subtitle: String,
    /// Absent for "Red Shift" — the demo shows no rating there.
    pub rating_half_stars: Option<u8>,
    /// Drives the overlay "+" button's state. From `state`, not `data`.
    pub on_watchlist: bool,
}

/// A reply nested under a top-level comment.
#[derive(Debug, Clone, Serialize)]
pub struct Reply {
    pub id: String,
    pub author_name: String,
    pub author_avatar: Image,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub id: String,
    pub author_name: String,
    pub author_avatar: Image,
    pub timestamp: String,
    pub body: String,
    /// Absent where the demo renders no like button.
    pub like_count: Option<u32>,
    pub replies: Vec<Reply>,
    /// Whether the visitor liked this comment. From `state`.
    pub liked: bool,
}

/// The desktop review screen: one long-form review plus its conversation.
#[derive(Debug, Clone, Serialize)]
pub struct Review {
    pub id: String,
    pub movie: Movie,
    /// Backdrop behind the header. Only the desktop screen uses it.
    pub backdrop: Option<Image>,
    pub director: Option<String>,
    pub genres: Vec<String>,
    pub author_name: String,
    pub author_avatar: Image,
    /// Verbatim ("Watched on March 15, 2024", "Reviewed yesterday").
    pub watched_on: String,
    pub rating_half_stars: u8,
    /// One string per rendered `<p>`.
    pub paragraphs: Vec<String>,
    pub like_count: Option<u32>,
    pub comments: Vec<Comment>,
    /// The "#Cinematography #MustWatch" line on the mobile screen.
    pub hashtags: Vec<String>,
    /// Whether the visitor liked the review itself. From `state`.
    pub liked: bool,
}

/// A cast member on the movie detail screen.
#[derive(Debug, Clone, Serialize)]
pub struct CastMember {
    pub id: String,
    pub name: String,
    pub role: String,
    pub portrait: Image,
}

/// One label/value row in the detail screen's credits grid.
#[derive(Debug, Clone, Serialize)]
pub struct DetailFact {
    pub label: String,
    pub value: String,
}

/// A trailer or clip, as the detail screen's Media block plays it.
///
/// Carries the video's `key` and `site` rather than a finished embed URL: the
/// frontend builds both the thumbnail and the `<iframe>` src from them, and which
/// of those it needs is a rendering decision. `site` is checked there too, since
/// only YouTube is embeddable.
#[derive(Debug, Clone, Serialize)]
pub struct Trailer {
    /// The video's own title ("Official Trailer", "Trailer 4").
    pub name: String,
    /// Site-scoped id — on YouTube, what follows `watch?v=`.
    pub key: String,
    /// "YouTube" or "Vimeo".
    pub site: String,
    /// The still the play button sits over. TMDB serves no per-video thumbnail,
    /// so this is the film's own backdrop.
    pub thumbnail: Image,
}

/// One row in "Where to Watch".
///
/// A provider, not a link: TMDB's attribution terms allow linking only to their
/// own watch page, and they publish no per-provider deep link. `MovieDetail`
/// carries that one URL for all of these.
#[derive(Debug, Clone, Serialize)]
pub struct WatchOption {
    pub provider: String,
    /// How the film is available: "Stream", "Rent", "Buy", "Free".
    pub kind: String,
    /// The service's logo. `None` for a provider TMDB has no artwork for, which
    /// the frontend draws as a generic glyph.
    pub logo: Option<Image>,
}

/// The movie detail screen. Distinct from `Review` — this is the film's own
/// page (poster, credits, score, media, cast) rather than someone's write-up.
#[derive(Debug, Clone, Serialize)]
pub struct MovieDetail {
    pub id: String,
    pub title: String,
    pub year: u16,
    /// Age rating for one country ("PG-13", "R"). `None` where TMDB has none, and
    /// the metadata line then omits the segment rather than printing "NR".
    pub certification: Option<String>,
    /// Verbatim runtime string ("1h 58m") — never parsed, only displayed.
    pub runtime: String,
    pub genres: Vec<String>,
    pub poster: Image,
    /// A still from the film. Not a hero image any more — it backs the Media
    /// block's play tile, since TMDB serves no per-video thumbnail.
    pub backdrop: Image,
    pub synopsis: String,
    /// The crowd average on a 0–10 scale, shown as the big number. Deliberately
    /// *not* half-stars: the design prints "7.8 / 10", and rounding it to the
    /// nearest half-star first would make it read 8.0.
    pub score: f32,
    /// How many votes that average is over. 0 hides the line — "based on 0
    /// ratings" beside a score is worse than no attribution at all.
    pub vote_count: u32,
    /// The credits grid: Director, Writers, Cinematography, Music, Production.
    /// A fact the source doesn't have is omitted, so the grid draws fewer rows
    /// rather than "Unknown".
    pub details: Vec<DetailFact>,
    /// The trailer the Media block plays, if there is one.
    pub trailer: Option<Trailer>,
    /// "Where to Watch" rows. Empty is normal — most films aren't streaming
    /// anywhere in a given country, and the section hides itself.
    pub watch_options: Vec<WatchOption>,
    /// Where the "Where to Watch" rows link. One URL for all of them; see
    /// `WatchOption`.
    pub watch_link: Option<String>,
    pub cast: Vec<CastMember>,
    /// Whether this film is on the visitor's watchlist. From `state`.
    pub on_watchlist: bool,
    /// The visitor's own rating in half-stars, or `None` if they haven't rated
    /// it. Distinct from `score`, which is the crowd's. From `state`.
    pub your_rating_half_stars: Option<u8>,
}

/// A result card on the search screen.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub year: u16,
    /// A 0.0–5.0 average, shown as a number next to one star glyph. This is a
    /// crowd average, not one person's score, so unlike `rating_half_stars` it
    /// is genuinely fractional and never drawn as discrete glyphs.
    pub star_rating: f32,
    /// `None` renders the export's "Poster Missing" placeholder tile.
    pub poster: Option<Image>,
    /// The export renders card 3's poster desaturated. Carried as data because
    /// it is a per-item art direction choice, not a rule the grid can infer.
    pub grayscale: bool,
    /// Genres this film matches, for the sidebar's genre filter.
    pub genres: Vec<String>,
    /// Drives the hover "Log" button's state. From `state`.
    pub on_watchlist: bool,
}

/// A genre chip in the search sidebar.
///
/// `count` is how many films in the whole catalogue carry this genre, ignoring
/// the current genre selection but respecting the query and the other filters —
/// so a chip never reads "12" and then yields nothing when you click it.
#[derive(Debug, Clone, Serialize)]
pub struct GenreFacet {
    pub label: String,
    pub selected: bool,
    pub count: u32,
}

/// A decade radio option in the search sidebar. `count` follows the same
/// leave-one-out rule as `GenreFacet`.
#[derive(Debug, Clone, Serialize)]
pub struct YearFacet {
    pub label: String,
    pub selected: bool,
    pub count: u32,
}

/// The filter state this response was computed under, echoed back so the
/// sidebar's controls always agree with the results beside them.
#[derive(Debug, Clone, Serialize)]
pub struct SearchFilters {
    pub genres: Vec<GenreFacet>,
    pub years: Vec<YearFacet>,
    /// Minimum-rating row, in whole filled stars out of 5. 0 means "any".
    pub minimum_rating_stars: u8,
}

/// Everything the search screen needs, in one request.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    /// The query echoed back — the export shows it in quotes in the subtitle.
    pub query: String,
    /// How many films matched in total, across all pages.
    pub total_results: u32,
    /// Just this page's slice of the matches.
    pub results: Vec<SearchResult>,
    pub filters: SearchFilters,
    pub page: u32,
    /// At least 1, so the paginator always renders one page button even when
    /// nothing matched.
    pub page_count: u32,
}

// --- Request bodies -----------------------------------------------------------

/// Query string for `GET /api/search`. Every field is optional; omitting all of
/// them yields the export's own default view.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchQuery {
    /// Free text matched case-insensitively against title and genre.
    pub q: Option<String>,
    /// Exact genre label ("Sci-Fi"). Absent means "any genre".
    pub genre: Option<String>,
    /// Decade label ("2010s"). Absent means "any decade".
    pub year: Option<String>,
    /// Whole stars out of 5; results below this are dropped. Absent means 0.
    pub min_rating: Option<u8>,
    /// 1-based. Out-of-range pages clamp to the last real page.
    pub page: Option<u32>,
}

/// `POST /api/movies/{id}/watchlist`. Omitting the body toggles.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WatchlistRequest {
    /// `Some(true)` adds, `Some(false)` removes, `None` toggles.
    pub on_watchlist: Option<bool>,
}

/// `PUT /api/movies/{id}/rating`.
#[derive(Debug, Clone, Deserialize)]
pub struct RatingRequest {
    /// 0..=10 half-stars. 0 clears the rating.
    pub rating_half_stars: u8,
}

/// `POST /api/reviews/{id}/comments` and `.../comments/{cid}/replies`.
#[derive(Debug, Clone, Deserialize)]
pub struct PostBodyRequest {
    pub body: String,
}

/// Result of a watchlist mutation.
#[derive(Debug, Clone, Serialize)]
pub struct WatchlistState {
    pub movie_id: String,
    pub on_watchlist: bool,
}

/// Result of a rating mutation.
#[derive(Debug, Clone, Serialize)]
pub struct RatingState {
    pub movie_id: String,
    pub your_rating_half_stars: Option<u8>,
}

/// Result of a like mutation on a review or a comment.
#[derive(Debug, Clone, Serialize)]
pub struct LikeState {
    pub id: String,
    pub liked: bool,
    /// The count including the visitor's own like, so the UI can just render it.
    pub like_count: Option<u32>,
}

/// Everything the desktop feed needs, in one request.
#[derive(Debug, Clone, Serialize)]
pub struct Feed {
    pub live: Vec<LiveDiscussion>,
    pub recent: Vec<FeedEntry>,
    pub friend_activity: Vec<FriendActivity>,
}

/// Everything the mobile feed needs, in one request.
#[derive(Debug, Clone, Serialize)]
pub struct MobileFeed {
    pub stories: Vec<Story>,
    pub items: Vec<MobileFeedItem>,
}

/// One person the visitor follows, as the profile's list draws them.
///
/// `subtitle` is a pre-formatted line ("Watched Interstellar • 2h ago"), not
/// structured data: the mock prints one truncated sentence per row, and the three
/// verbs it can be built from already live in the activity rail.
#[derive(Debug, Clone, Serialize)]
pub struct FollowedPerson {
    pub id: String,
    pub name: String,
    pub avatar: Image,
    pub subtitle: String,
}

/// One line in the profile's "Recent Reviews" tile.
///
/// Not a `Review`: these are the visitor's *own* rated films, and the blurb is
/// the film's synopsis rather than prose they wrote — the app has no place to
/// write a review yet, only to rate one.
#[derive(Debug, Clone, Serialize)]
pub struct RatedFilm {
    pub id: String,
    pub title: String,
    pub rating_half_stars: u8,
    /// One sentence of the synopsis, or `None` when the source has none.
    pub blurb: Option<String>,
}

/// `GET /api/profile` — the whole profile screen in one request.
///
/// The identity fields are transcribed from the Stitch export, not invented here
/// and not stored: there is still exactly one visitor and no notion of signing in
/// (see `state`), so a `people` row for them would encode an account system that
/// doesn't exist. What *is* real is everything below the header — the watchlist,
/// the ratings and the seeded friends all come from SQLite.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    /// "@alexm_cinema", with the sigil, since it's never used as a lookup key.
    pub handle: String,
    pub avatar: Image,
    /// "Cinephile since 2018" — the export's phrasing, kept whole.
    pub member_since: String,
    pub bio: String,
    /// The visitor's highest-rated films, best first. The mock's "Favorite Films"
    /// strip: empty until they rate something, which is honest rather than a row
    /// of borrowed posters.
    pub favorites: Vec<Movie>,
    /// Their watchlist, most recently added first.
    pub watchlist: Vec<Movie>,
    /// Their most recent ratings, newest first.
    pub recent_reviews: Vec<RatedFilm>,
    pub following: Vec<FollowedPerson>,
    /// How many people they follow. Equals `following.len()` today — the mock
    /// showed 124 beside a list of 3, and a count the list contradicts is the kind
    /// of decoration an SPA can't afford.
    pub following_count: u32,
}

/// Where the films in every other response came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    /// Live from TMDB.
    Tmdb,
    /// The invented dataset transcribed from the Stitch export — no token, or
    /// TMDB is unreachable. The frontend renders a banner saying so.
    Demo,
}

/// `GET /api/status`. What the frontend needs to decide whether to warn the user
/// that the films on screen are made up.
///
/// A separate endpoint rather than a field on all six payload types: it's one
/// fact about the server, not about a screen, and the banner is rendered once by
/// shared chrome. One extra request per page load, cheap and cacheable.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub data_source: DataSource,
    /// Why the data is fake, and what to do about it. `None` in TMDB mode, which
    /// is what tells the frontend to render nothing.
    pub message: Option<String>,
    /// Where to get a token. Always sent, so the copy lives in one place.
    pub docs_url: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A relative `src` breaks on any nested SPA route — see `Image::new`.
    #[test]
    fn image_paths_are_root_relative() {
        assert_eq!(Image::new("img/poster-red-shift.jpg", "").src, "/img/poster-red-shift.jpg");
        assert_eq!(Image::new("/img/already-absolute.jpg", "").src, "/img/already-absolute.jpg");
        assert_eq!(Image::new("https://cdn/x.jpg", "").src, "https://cdn/x.jpg");
        // TMDB's real CDN form, which must survive untouched.
        assert_eq!(
            Image::new("https://image.tmdb.org/t/p/w500/abc.jpg", "").src,
            "https://image.tmdb.org/t/p/w500/abc.jpg"
        );
    }

    /// A `data:` URI has a scheme but no `//`, so the passthrough test can't look
    /// for an authority — see `has_scheme`.
    #[test]
    fn data_uris_are_not_treated_as_paths() {
        let svg = "data:image/svg+xml;charset=utf-8,<svg/>";
        assert_eq!(Image::new(svg, "").src, svg);

        assert!(has_scheme("https://cdn/x.jpg"));
        assert!(has_scheme("data:image/png;base64,AAAA"));
        assert!(!has_scheme("img/poster.jpg"));
        // A colon in a filename is not a scheme.
        assert!(!has_scheme("img/2001: a space odyssey.jpg"));
        assert!(!has_scheme(":leading-colon"));
    }
}
