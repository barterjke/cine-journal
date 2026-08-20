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
//!
//! "The visitor" below means **whoever is reading this response** — the signed-in
//! account, or nobody. Every field documented as theirs comes from their own
//! `state::Store`, which is empty for a reader with no session, so the flags read as
//! untouched rather than as somebody else's. See `auth` and `routes`.

use serde::{Deserialize, Serialize};

/// An image as the export used it: a local path plus the alt text that was
/// transcribed from Stitch's `data-alt` generation prompt.
///
/// `Deserialize` because a built feed page is cached as JSON and read back (see
/// `feed`), so every type reachable from `FeedItem` has to round-trip. Nothing
/// accepts one of these from a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub src: String,
    pub alt: String,
}

impl Image {
    /// The stand-in for a film with no poster.
    ///
    /// One place, because the alternative was three. The demo dataset used to fall
    /// back to a real film's poster *and* its description, so a film with no artwork
    /// was shown another film's — which misattributes the artwork rather than
    /// admitting there is none.
    pub fn missing_poster() -> Self {
        Self::new("img/poster-missing.svg", "No poster available for this film.")
    }

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Movie {
    pub id: String,
    pub title: String,
    pub year: Option<u16>,
    pub poster: Image,
}

/// A poster tile in the desktop "Recent Entries" grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedEntry {
    pub id: String,
    pub movie: Movie,
    pub rating_half_stars: u8,
    /// Drives the hover "+" button's state. From `state`, not `data`.
    pub on_watchlist: bool,
}

/// A circle in the mobile feed's stories rail: one person the visitor follows,
/// whose tap opens their newest review.
///
/// `review_id` is what makes the circle a link rather than decoration — the export's
/// rail went nowhere, and a story with nothing behind it is exactly the fake
/// functionality this replaced. `None` for someone the visitor follows who hasn't
/// written anything; the UI draws those dimmed and unlinked rather than dropping
/// them, since who you follow is a fact whether or not they've posted.
#[derive(Debug, Clone, Serialize)]
pub struct Story {
    pub id: String,
    pub name: String,
    pub avatar: Image,
    /// Their newest review, or `None` if they have none.
    pub review_id: Option<String>,
    /// Their page, so a long-press or the name can go there.
    pub handle: String,
    /// Whether they have something to show — the ring is drawn for these and the
    /// circle is dimmed for the rest. Replaces the export's invented `unseen`,
    /// which claimed a read/unread state nothing recorded.
    pub unseen: bool,
}

/// A poster card in the mobile feed grid.
#[derive(Debug, Clone, Serialize)]
pub struct MobileFeedItem {
    pub id: String,
    pub movie: Movie,
    /// Pre-formatted subtitle, either "Elena rated it" for a followed person's
    /// review or the reason a recommendation is here.
    pub subtitle: String,
    /// The author's rating where this card is somebody's review; `None` for a
    /// recommendation, which nobody the visitor follows has scored.
    pub rating_half_stars: Option<u8>,
    /// The review this card opens, when it is one. `None` for a recommendation,
    /// whose poster goes to the film.
    pub review_id: Option<String>,
    /// Drives the overlay "+" button's state. From `state`, not `data`.
    pub on_watchlist: bool,
}

/// A reply nested under a top-level comment.
///
/// Carries its author like a comment does, because a thread is shared now: a reply
/// under somebody else's comment can be by a third person again.
#[derive(Debug, Clone, Serialize)]
pub struct Reply {
    pub id: String,
    /// Who wrote it. `author_handle` links to their page; `author_id` is what the
    /// follow button posts to.
    pub author_id: String,
    /// Their real name, always — never the literal "You". See `Comment::is_you`.
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    /// Whether the viewer wrote it.
    pub is_you: bool,
    /// "August 20, 2026", pre-formatted as everywhere else in this file.
    pub timestamp: String,
    pub body: String,
}

/// One comment on a review, with its replies.
///
/// **Everybody's comments, not just the viewer's.** A thread used to be assembled
/// from the viewer's own `state::Store` and every row was labelled "You", which was
/// only ever true because there was one visitor. Comments are content now: they come
/// out of SQLite with their author joined in, and the same thread is served to
/// whoever asks — including a reader with no account, who can read it but not post.
#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub id: String,
    /// Who wrote it, as on `Reply`.
    pub author_id: String,
    /// Their real name, always. The client renders "You" when `is_you` rather than
    /// the server substituting the word, so the avatar, the handle and the link to
    /// their page stay usable on the viewer's own rows too.
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    /// Whether the viewer wrote it. The one thing a client cannot work out for
    /// itself: the session is an `HttpOnly` cookie, so the browser never learns its
    /// own account id unless it asks `/api/auth/me` and compares.
    pub is_you: bool,
    /// "August 20, 2026". Was the constant "Just now", because a posted comment had
    /// no stored time worth printing; every comment has a real one now.
    pub timestamp: String,
    pub body: String,
    /// How many people have liked it, or `null` for none.
    ///
    /// A real total out of `liked_comments`, the viewer's own like included. It used
    /// to be per-viewer and read 1 to everybody.
    pub like_count: Option<u32>,
    /// Oldest first, as the thread renders them.
    pub replies: Vec<Reply>,
    /// Whether *the viewer* liked it. From `state`.
    pub liked: bool,
}

/// One review in full, plus its conversation — what the review screen draws.
///
/// The expanded form of `UserReview`, which is the same review clamped to four
/// lines in a list. Both are built from the same row, so the author fields are the
/// same fields: a card and the page it opens can't credit different people.
///
/// The author is whoever wrote it — a seeded person or a real account, drawn the same
/// way either way. An account's reviews used to be invisible to everybody but their
/// owner; they reach these screens now, which is what makes following somebody worth
/// doing. See `db::REVIEW_SOURCE`.
#[derive(Debug, Clone, Serialize)]
pub struct Review {
    /// `<person_id>-<movie_id>`, the same id `UserReview` carries.
    pub id: String,
    pub movie: Movie,
    /// Backdrop behind the header. Only the desktop screen uses it.
    pub backdrop: Option<Image>,
    pub director: Option<String>,
    pub genres: Vec<String>,
    /// Who wrote it. `author_handle` is what links to their page — the id is for
    /// the follow button, which posts to `/api/people/{id}/follow`.
    pub author_id: String,
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    /// Whether the visitor follows them, so the header can offer the same follow
    /// button their page does rather than making you go there to press it.
    pub author_followed: bool,
    /// Verbatim ("Reviewed on March 15, 2024").
    pub watched_on: String,
    /// `null` for prose written without a score — see `UserReview`.
    pub rating_half_stars: Option<u8>,
    /// One string per rendered `<p>`.
    pub paragraphs: Vec<String>,
    /// How many people have liked this review, or `null` for none.
    ///
    /// A real total out of `liked_reviews`, including the viewer's own like, so two
    /// people liking it reads 2 to both of them. It used to be per-viewer — 1 if you
    /// had liked it and nothing otherwise — which was true only while there was one
    /// visitor.
    pub like_count: Option<u32>,
    /// The whole thread, oldest first. Everybody's comments, not just the viewer's.
    pub comments: Vec<Comment>,
    /// Whether *the viewer* liked the review. From `state`.
    pub liked: bool,
}

/// A cast member on the movie detail screen.
#[derive(Debug, Clone, Serialize)]
pub struct CastMember {
    /// TMDB's person id as a string, which is also what `/search?person=` takes —
    /// so the rail's portraits link to this actor's other films.
    pub id: String,
    pub name: String,
    pub role: String,
    pub portrait: Image,
    /// Whether this id means anything to `/search?person=`.
    ///
    /// `false` for the demo dataset's invented cast: they have real names and
    /// portraits but no filmography, so a link would land on an empty grid. The
    /// frontend draws those names unlinked — the same distinction
    /// `FollowedPerson.handle` already makes for the export's decorative people.
    pub searchable: bool,
}

/// One label/value row in the detail screen's credits grid.
#[derive(Debug, Clone, Serialize)]
pub struct DetailFact {
    pub label: String,
    pub value: String,
    /// The people named in `value`, in the order they appear there, so each name
    /// can link to their other films.
    ///
    /// Parallel to the text rather than replacing it because a row's value is
    /// sometimes not a person at all — "Production" names a studio, and "Writers"
    /// is a comma-joined list of up to three. Empty means "nothing here is a
    /// person to link", which is what the studio row wants.
    pub people: Vec<CreditedPerson>,
}

/// One name inside a `DetailFact`, and where clicking it goes.
#[derive(Debug, Clone, Serialize)]
pub struct CreditedPerson {
    /// Exactly as it appears in the row's `value`, so the frontend can match it
    /// there rather than re-deriving the joined string.
    pub name: String,
    /// TMDB's person id, for `/search?person=`.
    pub id: String,
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
    /// What kind it is: "Trailer", "Teaser", "Clip", "Featurette", "Behind the
    /// Scenes". Labelled on the slide, because a carousel of five videos all
    /// captioned by their own titles doesn't say which is the actual trailer.
    pub kind: String,
    /// The still behind the play button.
    ///
    /// YouTube's own frame for the video, not the film's backdrop: with several
    /// videos in one rail, one shared image would make every slide look identical.
    /// A film with no video at all still has stills, and those carry their own.
    pub thumbnail: Image,
}

/// One frame from the film, as the Media carousel shows it.
///
/// A separate type from `Trailer` rather than one "media item" union: a still is
/// opened, a video is played, and the two have nothing in common but a thumbnail.
/// Collapsing them would mean a `kind` tag the frontend has to branch on anyway.
#[derive(Debug, Clone, Serialize)]
pub struct Still {
    /// The carousel thumbnail. Sized for the rail, not for full-screen.
    pub image: Image,
    /// The same frame at the largest size the source has, for the lightbox.
    /// Separate because a rail of eight originals is several megabytes.
    pub full: Image,
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
    /// `None` for an announced film with no release date yet, which the title line
    /// then omits rather than printing "(0)". Reachable now that a filmography can
    /// be browsed — unreleased credits are normal there, unlike in a search result.
    pub year: Option<u16>,
    /// Age rating for one country ("PG-13", "R"). `None` where TMDB has none, and
    /// the metadata line then omits the segment rather than printing "NR".
    pub certification: Option<String>,
    /// Verbatim runtime string ("1h 58m") — never parsed, only displayed.
    pub runtime: String,
    pub genres: Vec<String>,
    pub poster: Image,
    /// A still from the film. Not a hero image any more, and no longer the Media
    /// block's thumbnail either — the videos there carry their own. It backs the
    /// review screen's faded header, which is the one place a single wide frame is
    /// still what's wanted.
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
    /// Every video the Media carousel offers, best first — the newest official
    /// trailer, then the rest. Empty for a film TMDB has nothing embeddable for.
    ///
    /// A list rather than one video because films carry 45–170 of them: one tile
    /// showed a fraction of a percent of what exists, and which fraction was
    /// decided entirely by a sort order the visitor couldn't see.
    pub trailers: Vec<Trailer>,
    /// Frames from the film, for the same carousel. Empty only for a film with no
    /// images at all.
    pub stills: Vec<Still>,
    /// "Where to Watch" rows. Empty is normal — most films aren't streaming
    /// anywhere in a given country, and the section hides itself.
    pub watch_options: Vec<WatchOption>,
    /// Where the "Where to Watch" rows link. One URL for all of them; see
    /// `WatchOption`.
    pub watch_link: Option<String>,
    pub cast: Vec<CastMember>,
    /// Whether this film is on the visitor's watchlist. From `state`.
    pub on_watchlist: bool,
    /// Whether the visitor called this one a favourite. From `state`.
    ///
    /// Separate from a high `your_rating_half_stars`: the heart says the film is
    /// theirs, the stars say it is good, and the profile's Favorite Films strip
    /// draws this one.
    pub is_favorite: bool,
    /// The visitor's own rating in half-stars, or `None` if they haven't rated
    /// it. Distinct from `score`, which is the crowd's. From `state`.
    pub your_rating_half_stars: Option<u8>,
    /// What the visitor wrote about this film, if anything. Fills the composer on
    /// the film's page, so writing again edits rather than duplicates. From `state`.
    pub your_review: Option<String>,
}

/// A result card on the search screen.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    /// `None` for an unreleased film — see `MovieDetail::year`. A filmography lists
    /// announced films alongside released ones, so the card has to draw both.
    pub year: Option<u16>,
    /// A 0.0–5.0 average, shown as a number next to one star glyph. This is a
    /// crowd average, not one person's score, so unlike `rating_half_stars` it
    /// is genuinely fractional and never drawn as discrete glyphs.
    ///
    /// `None` for a film nobody has voted on — "★ 0.0" would state an average that
    /// doesn't exist, which is the same claim the detail screen already declines to
    /// make by gating its score block on `vote_count`. The demo dataset keeps its
    /// 0.0 (the export drew it), so this is `Some(0.0)` there rather than `None`.
    pub star_rating: Option<f32>,
    /// `None` renders the export's "Poster Missing" placeholder tile.
    pub poster: Option<Image>,
    // There was a `grayscale` flag here, desaturating one card's poster. It was a
    // per-item art direction choice about one invented film, `false` for every real
    // one, so in TMDB mode it was a field that could only ever say "no".
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
    /// Who the `person=` filter names, resolved upstream so the screen can say
    /// "Films with Christopher Nolan" rather than "Films with 525".
    ///
    /// `None` when no person is filtered on, and also when the id doesn't resolve —
    /// an unknown id filters to nothing anyway, and inventing a name for it would
    /// be worse than saying none.
    pub person: Option<CreditedPerson>,
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
    /// A TMDB person id: everything they were credited on, cast or crew.
    ///
    /// Where a cast portrait and a credits-grid name lead. It only reaches TMDB
    /// through `discover?with_people=`, which `/search/movie` has no equivalent
    /// for — so with text *and* a person set, the person is applied locally over
    /// the text's candidates. See `content::search`.
    pub person: Option<String>,
}

/// `POST /api/movies/{id}/watchlist`. Omitting the body toggles.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WatchlistRequest {
    /// `Some(true)` adds, `Some(false)` removes, `None` toggles.
    pub on_watchlist: Option<bool>,
}

/// `POST /api/movies/{id}/favorite`. Omitting the body toggles.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FavoriteRequest {
    /// `Some(true)` favourites, `Some(false)` un-favourites, `None` toggles.
    pub is_favorite: Option<bool>,
}

/// `PUT /api/movies/{id}/rating`.
#[derive(Debug, Clone, Deserialize)]
pub struct RatingRequest {
    /// 0..=10 half-stars. 0 clears the rating.
    pub rating_half_stars: u8,
}

/// `PUT /api/movies/{id}/review` — the visitor's own prose about a film.
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewRequest {
    /// Empty or whitespace deletes the review, which is how the composer clears one.
    /// A separate DELETE would be tidier REST and one more thing for the client to
    /// get wrong; the composer only ever knows the text it is holding.
    pub body: String,
}

/// `PUT /api/profile` — the one part of the visitor's identity they own.
#[derive(Debug, Clone, Deserialize)]
pub struct BioRequest {
    /// Empty or whitespace restores the default rather than leaving the field in an
    /// in-between state; the response says which of the two is now stored.
    pub bio: String,
}

/// `POST /api/reviews/{id}/comments` and `.../comments/{cid}/replies`.
#[derive(Debug, Clone, Deserialize)]
pub struct PostBodyRequest {
    pub body: String,
}

/// `GET /api/people?q=`. One optional term, matched against nickname and name.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PeopleQuery {
    /// Absent or empty means "don't search" — the screen then draws only Following and
    /// Followers. It used to mean "list everyone", which is what the removed "Everyone"
    /// panel showed; a directory of every account is not a friends screen.
    pub q: Option<String>,
}

/// `GET /api/feed?cursor=`. Absent means the first page.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FeedQuery {
    /// The opaque cursor from the previous page's `next_cursor`. An unparseable one
    /// is treated as absent rather than rejected: the shape can change between
    /// deploys, and a stale cursor in a scrolled-back tab should restart the feed,
    /// not break it.
    pub cursor: Option<String>,
    /// Skip the cache and build this page fresh — what the client sends on its
    /// revalidation request, and what fills the cache for the next visitor.
    #[serde(default)]
    pub refresh: bool,
}

/// `GET /api/collections/{slug}?person=`. Absent `person` means the visitor's own.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CollectionQuery {
    /// A nickname, with or without the `@`. Whose collection to show.
    pub person: Option<String>,
}

/// `POST /api/people/{id}/follow`. Omitting the body toggles, as with the watchlist.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FollowRequest {
    /// `Some(true)` follows, `Some(false)` unfollows, `None` toggles.
    pub following: Option<bool>,
}

/// Result of a watchlist mutation.
#[derive(Debug, Clone, Serialize)]
pub struct WatchlistState {
    pub movie_id: String,
    pub on_watchlist: bool,
}

/// Result of a favourite mutation.
#[derive(Debug, Clone, Serialize)]
pub struct FavoriteState {
    pub movie_id: String,
    pub is_favorite: bool,
}

/// Result of a rating mutation.
#[derive(Debug, Clone, Serialize)]
pub struct RatingState {
    pub movie_id: String,
    pub your_rating_half_stars: Option<u8>,
}

/// Result of writing, editing or clearing the visitor's review of a film.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewState {
    pub movie_id: String,
    /// The stored text, or `None` if the review was cleared. Echoed back rather
    /// than assumed, because the server trims it.
    pub your_review: Option<String>,
}

/// Result of editing the visitor's bio: the line now on their profile.
///
/// Always a string, never `None`. Clearing it gives back the default, which is empty
/// for a real account and the export's sentence for the legacy visitor — see
/// `content::default_bio`.
#[derive(Debug, Clone, Serialize)]
pub struct BioState {
    pub bio: String,
}

/// Result of a like mutation on a review or a comment.
#[derive(Debug, Clone, Serialize)]
pub struct LikeState {
    pub id: String,
    pub liked: bool,
    /// The count including the visitor's own like, so the UI can just render it.
    pub like_count: Option<u32>,
}

/// One suggested film, with the film of the visitor's own that prompted it.
///
/// `because` is the whole difference between a recommendation and a shelf of
/// posters: the card says "because you liked Interstellar", which is a claim the
/// data can back, and it comes from the seed that actually produced this film
/// rather than from whichever favourite happens to be first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub movie: Movie,
    /// 0.0–5.0 crowd average, or `None` for a film nobody has voted on — the same
    /// distinction `SearchResult::star_rating` makes, for the same reason.
    pub star_rating: Option<f32>,
    /// The title of the visitor's film this was recommended from.
    pub because: String,
    /// That film's id, so the attribution can link to the film it names.
    pub because_movie_id: String,
    /// Whether the seed was a favourite rather than merely on the watchlist.
    ///
    /// The two are different claims and the card has to make the right one: "because
    /// you liked Insidious" is false about a film the visitor has only bookmarked and
    /// may not have seen. Seeds are taken from favourites first (see
    /// `content::RECOMMEND_SEEDS`), so both kinds reach this rail.
    pub because_favorite: bool,
    /// Whether the visitor already has it on their watchlist, so the "+" button
    /// draws the same state it does on every other poster in the app.
    pub on_watchlist: bool,
}

/// One card in the infinite feed: somebody's review, a suggestion, or one of the
/// visitor's own journal entries.
///
/// A tagged union rather than three parallel arrays, because the whole point of a
/// scrolling feed is that the three kinds are *interleaved* — the client renders the
/// list in the order it arrives and never has to decide how to merge rails. `kind` is
/// the discriminant serde writes, so the TypeScript side is a discriminated union.
///
/// Every variant already carries an id of its own, and the client keys its cards on
/// `kind` plus that — deliberately not on the film alone, because the same film
/// legitimately appears twice in one feed: a friend reviewed it *and* the visitor logged
/// it. Those are two cards saying different things.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeedItem {
    /// A review by somebody the visitor follows.
    Review(UserReview),
    /// A film suggested from the visitor's own favourites or watchlist.
    Recommendation(Recommendation),
    /// A film the visitor logged themselves.
    Entry(FeedEntry),
}

/// `GET /api/feed?cursor=` — one page of the infinite feed.
///
/// Paginated rather than the three fixed rails the screen used to draw, because a
/// feed that ends after six reviews is a summary. The cursor is opaque on purpose:
/// it encodes how far into each of the three underlying sources the page reached,
/// and a client that parsed it would break the moment a fourth source is added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    /// What to ask for next. `None` means the feed is exhausted — the client stops
    /// observing and says so, rather than spinning forever on an empty page.
    pub next_cursor: Option<String>,
    /// Whether this page came out of Redis rather than being built for this request.
    ///
    /// Not decoration: the first page is served from the cache and revalidated in the
    /// background (see `cache::feed`), so the client shows a quiet "refreshing" note
    /// and reloads once the fresh copy lands. Without this the screen couldn't tell
    /// the two apart and would either never refresh or always flash.
    pub from_cache: bool,
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
    /// `Some("@msbreviews")` for a real user with a page of their own; `None` for
    /// the export's decorative cast, whom the UI must not link.
    pub handle: Option<String>,
}

/// One person in a list — search results, followers, following.
///
/// Deliberately not `FollowedPerson`: that one carries a pre-formatted activity
/// line for the profile's rail, while this carries the two *relationship* bits a
/// list needs to draw its follow button. Merging them would give every caller a
/// field it has no answer for.
#[derive(Debug, Clone, Serialize)]
pub struct PersonCard {
    pub id: String,
    pub name: String,
    /// "@msbreviews", with the sigil. Every person in a list is a real user.
    pub handle: String,
    pub avatar: Image,
    pub bio: Option<String>,
    /// Whether the visitor follows them. Drives the button's state.
    pub following: bool,
    /// Whether they follow the visitor — the "Follows you" chip.
    pub follows_you: bool,
    /// How many reviews they've written, so a row can say why they're worth following.
    pub review_count: u32,
}

/// One user's review of one film, as their page and a film's page both draw it.
///
/// Carries the film *and* the author because both screens need one of them and
/// neither can cheaply look it up: a person's page lists films, a film's page
/// lists people, and one shape serves both.
///
/// The author can be a seeded person or a real account, and nothing here says which:
/// both are somebody with a page, so the card is drawn the same way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReview {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    /// Whether the visitor follows the author — the "Following" chip on a film's page.
    pub author_followed: bool,
    pub movie_id: String,
    pub movie_title: String,
    pub poster: Option<Image>,
    /// `null` for prose written without a score.
    ///
    /// A seeded person's review always carries one — the harvest read both off TMDB
    /// together. An account can write about a film without rating it, because the two
    /// are separate acts here, and `0` would draw five empty stars and read as a
    /// one-star-out-of-five verdict. Same shape `RatedFilm` already uses.
    pub rating_half_stars: Option<u8>,
    pub body: String,
    /// "12 November 2014", pre-formatted as everywhere else in this file.
    pub written_on: String,
}

/// `GET /api/people/{handle}` — one person's page.
///
/// Deliberately the same shape as `Profile` from `favorites` down: a person's page
/// and your own page are the same kind of page, so they draw the same sections from
/// the same fields and one client component renders both. What differs is only the
/// header — you get an editable bio, they get a follow button and the two
/// relationship flags — which is the difference that is actually real.
#[derive(Debug, Clone, Serialize)]
pub struct PersonProfile {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Image,
    pub bio: Option<String>,
    pub following: bool,
    pub follows_you: bool,
    /// Films they call favourites.
    pub favorites: Vec<Movie>,
    /// Films they mean to watch.
    pub watchlist: Vec<Movie>,
    /// Their reviews, newest first.
    pub reviews: Vec<UserReview>,
    /// How many they've written, so a client that clamps the list still prints the
    /// true number.
    pub review_count: u32,
    // Deliberately no follower/following counts. A seeded person has no edges of
    // their own — the harvest gives them a static "follows you" flag and nothing
    // else — so their count would be a number invented for the page, and a page
    // reading "1 followers" under someone's name is worse than a page that doesn't
    // claim to know. Real accounts do follow each other, so the two relationships
    // that are always true, `following` and `follows_you`, are above.
}

/// `GET /api/people` — the friend-search screen.
#[derive(Debug, Clone, Serialize)]
pub struct PeopleResponse {
    /// Echoed back, as `SearchResponse` does, so the client can tell whose results
    /// these are when two requests race.
    pub query: String,
    pub results: Vec<PersonCard>,
    /// Everyone the visitor follows, and everyone who follows them. Sent alongside
    /// the results rather than behind two more endpoints: the screen draws all
    /// three lists at once, and one request means they can't disagree.
    pub following: Vec<PersonCard>,
    pub followers: Vec<PersonCard>,
}

/// `POST /api/people/{id}/follow` — the new state of that one edge.
#[derive(Debug, Clone, Serialize)]
pub struct FollowState {
    pub person_id: String,
    pub following: bool,
    /// The visitor's total after the change, so the profile's count can update
    /// without a second request.
    pub following_count: u32,
}

/// One line in the profile's "Recent Reviews" tile: a film the visitor rated, or
/// wrote about, or both.
///
/// Still not a `Review` — that type is a whole screen, with a backdrop, a cast line
/// and a comment thread. This is one row.
///
/// `rating_half_stars` is optional because the two acts are independent: the
/// composer on a film's page accepts prose without a score, and the star picker a
/// score without prose.
#[derive(Debug, Clone, Serialize)]
pub struct RatedFilm {
    pub id: String,
    pub title: String,
    pub rating_half_stars: Option<u8>,
    /// What the visitor wrote about the film, when they wrote something. Their own
    /// words take the line; `blurb` fills it otherwise.
    pub body: Option<String>,
    /// One sentence of the synopsis — what the row says about a film they rated but
    /// haven't written about. `None` when the source has no synopsis either.
    pub blurb: Option<String>,
    /// The film's artwork, for the row's thumbnail.
    ///
    /// `None` twice over: for a film with no poster at all, and for one the active
    /// source can no longer resolve. The row is still returned either way — the prose
    /// is what it is for, and dropping somebody's review because TMDB was unreachable
    /// would make the tile shrink for reasons of its own.
    ///
    /// `Option` rather than the required `Image` every `Movie` carries, because a
    /// cramped row can leave a gap where a full-size tile has to draw a placeholder.
    pub poster: Option<Image>,
    /// "Oct 12" — when they wrote the review, or scored the film if they wrote
    /// nothing. Pre-formatted, as every other date in this file is, and short because
    /// it shares a line with the stars and the like count.
    ///
    /// `None` for a rating stored before `ratings.rated_at` existed. There is no date
    /// to print for those and no honest one to invent.
    pub written_on: Option<String>,
    /// How many people have liked their review of this film, or `null` for none.
    ///
    /// A real total, as on `Review` and `Comment`. `null` at zero rather than `0`, so
    /// the row draws no number until somebody presses the button — see
    /// `hydrate::like_count`. Always `null` for a score with no prose, since there is
    /// no review there to like.
    pub like_count: Option<u32>,
}

/// `GET /api/profile` — the signed-in user's whole profile screen in one request.
///
/// **401 when nobody is signed in.** This is the account's own page rather than
/// content, so there is nothing to answer with for a reader who has no account —
/// see the note at the top of `routes`.
///
/// The header is the account's own `people` row: an account lives in the same table
/// the other people do, so its name, nickname and avatar are Google's and its page is
/// reachable at `/api/people/{handle}` like anybody else's.
///
/// The **bio** is the one part of it the user writes — `db::set_user_bio`, falling
/// back to a default when they never have.
///
/// Everything below the header is theirs out of SQLite: favourites, watchlist,
/// ratings, written reviews and whoever they follow.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    /// "@sam", with the sigil, since it's never used as a lookup key.
    pub handle: String,
    pub avatar: Image,
    /// "Cinephile since 2026" — the export's phrasing, with the year they joined.
    pub member_since: String,
    pub bio: String,
    /// The films they marked as favourites, most recent first.
    ///
    /// Was "their highest-rated films", which meant the strip rearranged itself
    /// whenever they rated anything and could never be *chosen*. The heart on a
    /// film's page writes this now. Empty until they press one, which is honest
    /// rather than a row of borrowed posters.
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

/// `GET /api/collections/{slug}` — one named set of films, in full.
///
/// The profile's tiles are summaries capped at four and six posters
/// (`content::FAVORITES_SHOWN`, `WATCHLIST_SHOWN`); this is the page behind them,
/// which is what makes a tile worth clicking. Uncapped deliberately: a collection
/// page that silently stopped at some number would be the same summary again.
///
/// `owner` and `title` are resolved here rather than composed by the client, so the
/// heading can say "Elena Rostova's Watchlist" without the screen knowing whose page
/// it came from.
#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    /// The slug that addresses it: "favorites", "watchlist", "journal".
    pub slug: String,
    /// "Favorite Films", "Watchlist", "Your Journal" — already in the right person's
    /// voice, so the client prints it as-is.
    pub title: String,
    /// One line under the heading saying what the set *is*, since a grid of posters
    /// can't say it. Empty collections lean on this entirely.
    pub description: String,
    /// Whose collection it is: `None` for the visitor's own, the person's name
    /// otherwise. Drives the back link as well as the heading.
    pub owner: Option<CollectionOwner>,
    pub movies: Vec<CollectionMovie>,
}

/// Whose collection is on screen, when it isn't the visitor's.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionOwner {
    pub name: String,
    /// "@elenarostova", so the page can link back to theirs.
    pub handle: String,
    pub avatar: Image,
}

/// One film in a collection grid: a poster, plus whatever the collection knows about
/// it that a bare `Movie` doesn't.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionMovie {
    pub movie: Movie,
    /// The owner's rating, where the collection is one that has ratings behind it —
    /// the visitor's journal does, their favourites don't. `None` draws no stars
    /// rather than zero.
    pub rating_half_stars: Option<u8>,
    /// Whether the *visitor* has this on their watchlist, so the grid's "+" behaves as
    /// it does everywhere else — including on somebody else's collection, where the
    /// button is about the visitor and the poster is about them.
    pub on_watchlist: bool,
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

/// How somebody can sign in, when they can.
///
/// An enum rather than a bare `bool`, so a second provider is a variant rather than a
/// second flag and a client that only knows `"google"` keeps working. Serialized as the
/// bare string `"google"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignIn {
    Google,
}

/// `GET /api/status`. The two facts about the server a client needs before it draws
/// any chrome: whether the films are real, and whether anybody can sign in.
///
/// A separate endpoint rather than a field on all six payload types: these are facts
/// about the server, not about a screen, and the banner and the sign-in button are
/// rendered once by shared chrome. One extra request per page load, cheap and cacheable.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub data_source: DataSource,
    /// Why the data is fake, and what to do about it. `None` in TMDB mode, which
    /// is what tells the frontend to render nothing.
    pub message: Option<String>,
    /// Where to get a token. Always sent, so the copy lives in one place.
    pub docs_url: &'static str,
    /// `"google"` when sign-in is configured, `null` when it is not.
    ///
    /// Whether, never what: the client id and the secret stay in the process. A client
    /// used to learn this by asking `/api/auth/google` with `redirect: 'manual'` and
    /// watching for a 302 or a 503, which spent a single-use CSRF row per press and
    /// could not answer until the button was pressed.
    pub sign_in: Option<SignIn>,
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
