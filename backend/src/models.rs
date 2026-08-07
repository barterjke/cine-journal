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

/// A poster tile in the desktop "Recent Entries" grid.
#[derive(Debug, Clone, Serialize)]
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

/// One review in full, plus its conversation — what the review screen draws.
///
/// The expanded form of `UserReview`, which is the same review clamped to four
/// lines in a list. Both come from one `user_reviews` row, so the author fields are
/// the same fields: a card and the page it opens can't credit different people.
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
    pub rating_half_stars: u8,
    /// One string per rendered `<p>`.
    pub paragraphs: Vec<String>,
    pub like_count: Option<u32>,
    pub comments: Vec<Comment>,
    /// Whether the visitor liked the review itself. From `state`.
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
    /// Empty or whitespace restores the export's line rather than blanking the
    /// profile, so there is no way to end up with a header that looks broken.
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
    /// Absent or empty lists everyone, which is what the screen shows on arrival.
    pub q: Option<String>,
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
/// Always a string, never `None` — clearing it restores the export's line, and the
/// header has to have something to draw.
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

/// Everything the desktop feed needs, in one request.
///
/// The three sections are what the visitor's own graph and taste produce. The
/// export's two — a "Live Now" rail of invented discussion rooms and a "Friends
/// Activity" sidebar of invented verbs — are gone: neither had anything behind it,
/// and no upstream or local source can supply either (there are no rooms, and
/// "watched" is an event nothing records). What is real is that people the visitor
/// follows write reviews and give ratings, and that their favourites and watchlist
/// imply films they haven't seen — so those are the two rails.
#[derive(Debug, Clone, Serialize)]
pub struct Feed {
    /// Reviews and ratings by the people the visitor follows, newest first.
    pub friend_reviews: Vec<UserReview>,
    /// Films the visitor has logged, as the export's "Recent Entries" grid.
    pub recent: Vec<FeedEntry>,
    /// Films suggested from their favourites and watchlist. Empty until they have
    /// one of either — a recommendation with no seed behind it would be back to
    /// showing whatever trends and calling it personal.
    pub recommended: Vec<Recommendation>,
}

/// One suggested film, with the film of the visitor's own that prompted it.
///
/// `because` is the whole difference between a recommendation and a shelf of
/// posters: the card says "because you liked Interstellar", which is a claim the
/// data can back, and it comes from the seed that actually produced this film
/// rather than from whichever favourite happens to be first.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
    pub rating_half_stars: u8,
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
    // Deliberately no follower/following counts. The graph stores the visitor's own
    // edges and nothing else — there is no person-to-person following — so any such
    // count would be 0 or 1, and a page reading "1 followers" under someone's name
    // is worse than a page that doesn't claim to know. The two relationships that
    // *are* real, `following` and `follows_you`, are above.
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
}

/// `GET /api/profile` — the whole profile screen in one request.
///
/// The visitor still has no `people` row: there is exactly one of them and no notion
/// of signing in (see `state`), so a row in the table the *other* people share would
/// encode an account system that doesn't exist. Their name, handle, avatar and joined
/// line are the export's, held as constants in `hydrate`.
///
/// Their **bio** is the exception, and is stored — `db::visitor_bio`, falling back to
/// the export's line when they've never edited it. Editing one line of text is not an
/// account system, and a profile you cannot change any part of is not a profile.
///
/// Everything below the header is theirs out of SQLite: favourites, watchlist,
/// ratings, written reviews and the seeded friends.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    /// "@alexm_cinema", with the sigil, since it's never used as a lookup key.
    pub handle: String,
    pub avatar: Image,
    /// "Cinephile since 2018" — the export's phrasing, kept whole.
    pub member_since: String,
    pub bio: String,
    /// The films the visitor marked as favourites, most recent first.
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
