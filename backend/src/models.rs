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
    pub fn new(src: &str, alt: &str) -> Self {
        let src = if src.starts_with('/') || src.contains("://") {
            src.to_string()
        } else {
            format!("/{src}")
        };
        Self { src, alt: alt.to_string() }
    }
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
#[derive(Debug, Clone, Serialize)]
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

/// Where a still sits in the export's asymmetric bento grid.
///
/// Deliberately semantic rather than a Tailwind class string: Tailwind's JIT only
/// emits CSS for classes it finds literally in the source it scans, so class
/// names arriving over the wire generate nothing and every tile collapses to the
/// default 1×1. The frontend owns the class vocabulary and maps these variants
/// onto it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StillShape {
    /// Full-bleed 16:9 across two columns at every breakpoint.
    Hero,
    /// Sits beside the hero: full width on mobile, one column and free height on
    /// desktop.
    Companion,
    /// One column; square on mobile, 16:9 on desktop.
    Compact,
    /// One column on mobile, widening to two on desktop.
    Panorama,
}

/// A gallery still. The export's grid is deliberately asymmetric, so each still
/// carries its own shape rather than the grid deriving one by index.
#[derive(Debug, Clone, Serialize)]
pub struct GalleryStill {
    pub id: String,
    pub image: Image,
    pub shape: StillShape,
}

/// One label/value row in the detail screen's Details sidebar.
#[derive(Debug, Clone, Serialize)]
pub struct DetailFact {
    pub label: String,
    pub value: String,
}

/// The movie detail screen. Distinct from `Review` — this is the film's own
/// page (hero, synopsis, cast, gallery) rather than someone's write-up of it.
#[derive(Debug, Clone, Serialize)]
pub struct MovieDetail {
    pub id: String,
    pub title: String,
    pub year: u16,
    pub director: String,
    /// Verbatim runtime string ("1h 58m") — never parsed, only displayed.
    pub runtime: String,
    pub genres: Vec<String>,
    pub poster: Image,
    pub backdrop: Image,
    pub synopsis: String,
    pub cast: Vec<CastMember>,
    /// Rendered as "12 Stills" beside the Gallery heading; may exceed
    /// `gallery.len()`, as it does in the export (12 claimed, 4 shown).
    pub still_count: u32,
    pub gallery: Vec<GalleryStill>,
    pub details: Vec<DetailFact>,
    /// 0..=100. The export shows 0% / "Not Started".
    pub watch_progress_percent: u8,
    /// Verbatim progress label ("Not Started").
    pub watch_progress_label: String,
    /// Whether this film is on the visitor's watchlist. From `state`.
    pub on_watchlist: bool,
    /// The visitor's own rating in half-stars, or `None` if they haven't rated
    /// it. Distinct from any crowd average. From `state`.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A relative `src` breaks on any nested SPA route — see `Image::new`.
    #[test]
    fn image_paths_are_root_relative() {
        assert_eq!(Image::new("img/poster-red-shift.jpg", "").src, "/img/poster-red-shift.jpg");
        assert_eq!(Image::new("/img/already-absolute.jpg", "").src, "/img/already-absolute.jpg");
        assert_eq!(Image::new("https://cdn/x.jpg", "").src, "https://cdn/x.jpg");
    }
}
