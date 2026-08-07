//! Folds the visitor's state (`state`) into the static content (`data`).
//!
//! Keeping this separate means `data` stays a pure transcription of the export
//! and never has to know a store exists. Every handler builds its payload from
//! `data`, passes it through here, and serializes the result.

use crate::models::*;
use crate::state::Store;

/// What a comment row calls the visitor. Their own posts are labelled by relation
/// rather than by name, as the export drew them — the name belongs on the profile,
/// which is where `VISITOR_NAME` goes.
const BYLINE: &str = "You";

/// The one visitor, transcribed from the export rather than stored.
///
/// There is still no per-user identity here (see `state`): every client is this
/// person. A `people` row for them would imply an account system that doesn't
/// exist, so these live in code beside the byline they belong with.
///
/// The name and the avatar are the export's own — `review-mobile.html` drew Alex
/// Mercer with this photo, and `data::architecture_review` still credits them —
/// which is why the profile header has a real face behind it rather than a
/// placeholder. The handle, the joined line and the bio are `reference/profile/`'s
/// copy, verbatim.
pub const VISITOR_NAME: &str = "Alex Mercer";
pub const VISITOR_HANDLE: &str = "@alexm_cinema";
pub const VISITOR_SINCE: &str = "Cinephile since 2018";
pub const VISITOR_BIO: &str =
    "Amateur critic, full-time dreamer. Obsessed with French New Wave and neon-lit neo-noirs.";

const VISITOR_AVATAR_ALT: &str =
    "A portrait of a young person in a brightly lit, modern setting wearing stylish minimalist clothing.";

/// Posted content has no real timestamp — the export's are pre-formatted strings
/// like "2 hours ago", and inventing a clock-based one would drift out of that
/// vocabulary the moment a minute passed.
const JUST_NOW: &str = "Just now";

/// One face for the visitor everywhere.
///
/// This was Elena's small avatar until the profile screen existed, which made the
/// mismatch load-bearing: Elena is a *friend* on the stories rail, so the visitor
/// wearing her photo on their own comments and their own name on their profile
/// read as two different people.
pub fn visitor_avatar() -> Image {
    Image::new("img/avatar-alex-mercer.jpg", VISITOR_AVATAR_ALT)
}

/// The like count to show beside a button, given the transcribed count and
/// whether the visitor has liked it.
///
/// Shared by the hydrate passes and by the two like handlers, which return the
/// new count directly — three copies of this drifted apart once already.
/// Something the export drew without a count reads 1 once liked and goes back to
/// showing none once unliked, rather than a visible zero.
pub fn like_count(base: Option<u32>, liked: bool) -> Option<u32> {
    match (base, liked) {
        (Some(n), true) => Some(n + 1),
        (Some(n), false) => Some(n),
        (None, true) => Some(1),
        (None, false) => None,
    }
}

pub fn feed(mut feed: Feed, store: &Store) -> Feed {
    for entry in &mut feed.recent {
        entry.on_watchlist = store.watchlist.contains(&entry.movie.id);
    }
    // The recommendation rail's posters carry the same "+" as every other poster in
    // the app, so they need the same flag. `content::recommended` leaves it false
    // and already drops anything on the watchlist, but it can only drop what was on
    // the list when the rail was built — this is the pass that keeps the button
    // honest for a film watchlisted since.
    for rec in &mut feed.recommended {
        rec.on_watchlist = store.watchlist.contains(&rec.movie.id);
    }
    feed
}

pub fn mobile_feed(mut feed: MobileFeed, store: &Store) -> MobileFeed {
    for item in &mut feed.items {
        item.on_watchlist = store.watchlist.contains(&item.movie.id);
    }
    feed
}

pub fn movie_detail(mut movie: MovieDetail, store: &Store) -> MovieDetail {
    movie.on_watchlist = store.watchlist.contains(&movie.id);
    movie.is_favorite = store.favorites.contains(&movie.id);
    movie.your_rating_half_stars = store.ratings.get(&movie.id).copied();
    movie.your_review = store.written_reviews.get(&movie.id).cloned();
    movie
}

pub fn search(mut response: SearchResponse, store: &Store) -> SearchResponse {
    for result in &mut response.results {
        result.on_watchlist = store.watchlist.contains(&result.id);
    }
    response
}

/// Applies the visitor's likes and appends anything they posted.
///
/// A review arrives with an empty thread and no like count — nobody but the visitor
/// can like or comment on anything yet — so this pass is where the whole
/// conversation comes from. Counts are bumped by one when liked so the number beside
/// the button matches what was just clicked.
pub fn review(mut review: Review, store: &Store) -> Review {
    review.liked = store.liked_reviews.contains(&review.id);
    review.like_count = like_count(review.like_count, review.liked);

    // Append the visitor's own comments *before* the pass below, so a reply to
    // one of them is rendered too — the reply loop has to see every comment,
    // not just the transcribed ones.
    if let Some(posted) = store.posted_comments.get(&review.id) {
        review.comments.extend(posted.iter().map(|comment| Comment {
            id: comment.id.clone(),
            author_name: BYLINE.into(),
            author_avatar: visitor_avatar(),
            timestamp: JUST_NOW.into(),
            body: comment.body.clone(),
            // Starts with no count at all rather than a visible zero, matching
            // the export's second comment, which renders no like button.
            like_count: None,
            replies: Vec::new(),
            liked: false,
        }));
    }

    for comment in &mut review.comments {
        comment.liked = store.liked_comments.contains(&comment.id);
        comment.like_count = like_count(comment.like_count, comment.liked);

        let key = (review.id.clone(), comment.id.clone());
        if let Some(replies) = store.posted_replies.get(&key) {
            comment.replies.extend(replies.iter().map(|posted| Reply {
                id: posted.id.clone(),
                author_name: BYLINE.into(),
                author_avatar: visitor_avatar(),
                body: posted.body.clone(),
            }));
        }
    }

    review
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;
    use crate::state::{PostedComment, PostedReply};

    fn store() -> Store {
        Store::default()
    }

    /// A bare film, for the two feed passes below. Both only ever read
    /// `movie.id`, so the rest is filler.
    fn movie(id: &str) -> Movie {
        Movie {
            id: id.into(),
            title: data::title_from_slug(id),
            year: None,
            poster: Image::new("img/poster-missing.svg", "No poster."),
        }
    }

    /// Both feeds are built by `content` from SQLite now rather than transcribed in
    /// `data`, so these construct the payload directly. That is the whole contract
    /// this module has: it takes a built payload and stamps the visitor's state on it.
    #[test]
    fn watchlist_flows_into_every_screen() {
        let mut store = store();
        store.watchlist.insert("le-souffle".into());

        let built = Feed {
            friend_reviews: Vec::new(),
            recent: ["le-souffle", "the-drop"]
                .map(|id| FeedEntry {
                    id: format!("entry-{id}"),
                    movie: movie(id),
                    rating_half_stars: 8,
                    on_watchlist: false,
                })
                .into(),
            recommended: vec![Recommendation {
                movie: movie("le-souffle"),
                star_rating: Some(4.5),
                because: "Neon Reverie".into(),
                because_movie_id: "neon-reverie".into(),
                because_favorite: true,
                on_watchlist: false,
            }],
        };

        let hydrated = feed(built, &store);
        let entry = hydrated.recent.iter().find(|e| e.movie.id == "le-souffle").unwrap();
        assert!(entry.on_watchlist);
        assert!(
            hydrated.recent.iter().filter(|e| e.movie.id != "le-souffle").all(|e| !e.on_watchlist)
        );
        // The recommendation rail draws the same "+" button, so it needs the same flag
        // — a film already on the watchlist showing an empty "+" was the bug here.
        assert!(hydrated.recommended[0].on_watchlist);

        assert!(movie_detail(data::movie_detail_by_id("le-souffle"), &store).on_watchlist);
        assert!(!movie_detail(data::movie_detail_by_id("the-drop"), &store).on_watchlist);
    }

    #[test]
    fn watchlist_flows_into_the_mobile_feed() {
        let mut store = store();
        store.watchlist.insert("red-shift".into());

        let built = MobileFeed {
            stories: Vec::new(),
            items: ["the-horizon", "red-shift"]
                .map(|id| MobileFeedItem {
                    id: format!("card-{id}"),
                    movie: movie(id),
                    subtitle: "Elena rated it".into(),
                    rating_half_stars: Some(8),
                    review_id: Some(format!("user-elenarostova-{id}")),
                    on_watchlist: false,
                })
                .into(),
        };

        let hydrated = mobile_feed(built, &store);
        let flagged: Vec<&str> =
            hydrated.items.iter().filter(|i| i.on_watchlist).map(|i| i.movie.id.as_str()).collect();
        assert_eq!(flagged, ["red-shift"]);
    }

    #[test]
    fn rating_flows_into_the_detail_page() {
        let mut store = store();
        store.ratings.insert("neon-reverie".into(), 7);

        let rated = movie_detail(data::movie_detail_by_id("neon-reverie"), &store);
        assert_eq!(rated.your_rating_half_stars, Some(7));

        let unrated = movie_detail(data::movie_detail_by_id("the-drop"), &store);
        assert_eq!(unrated.your_rating_half_stars, None);
    }

    /// A review as `content::full_review` hands one over: a `user_reviews` row
    /// expanded into a page, with an empty thread and no like count. Everything
    /// under it is the visitor's, which is what this module puts there.
    fn seeded_review() -> Review {
        Review {
            id: "user-elenarostova-dune-part-two".into(),
            movie: Movie {
                id: "dune-part-two".into(),
                title: "Dune: Part Two".into(),
                year: Some(2024),
                poster: Image::new("img/poster-dune-part-two.jpg", "Poster for Dune: Part Two."),
            },
            backdrop: None,
            director: Some("Denis Villeneuve".into()),
            genres: vec!["Sci-Fi".into()],
            author_id: "user-elenarostova".into(),
            author_name: "Elena Rostova".into(),
            author_handle: "@elenarostova".into(),
            author_avatar: Image::new("img/avatar-elena-rostova.jpg", "Elena Rostova."),
            author_followed: true,
            watched_on: "Reviewed on March 15, 2024".into(),
            rating_half_stars: 9,
            paragraphs: vec!["Villeneuve builds a world you can feel the grit of.".into()],
            like_count: None,
            comments: Vec::new(),
            liked: false,
        }
    }

    /// The id every test below keys its store entries on.
    const ID: &str = "user-elenarostova-dune-part-two";

    /// A review carries no stored count, so the first like reads 1 rather than
    /// inventing a number to add to.
    #[test]
    fn liking_a_review_starts_its_count_at_one() {
        let base = seeded_review();
        assert_eq!(base.like_count, None);

        let mut store = store();
        store.liked_reviews.insert(ID.into());

        let liked = review(base, &store);
        assert!(liked.liked);
        assert_eq!(liked.like_count, Some(1));
    }

    /// And unliking takes the number away entirely rather than showing a zero.
    #[test]
    fn unliking_removes_the_count_rather_than_zeroing_it() {
        let hydrated = review(seeded_review(), &store());
        assert!(!hydrated.liked);
        assert_eq!(hydrated.like_count, None);
    }

    #[test]
    fn posted_comments_become_the_thread() {
        let mut store = store();
        store.posted_comments.insert(
            ID.into(),
            vec![
                PostedComment { id: "comment-1".into(), body: "First".into() },
                PostedComment { id: "comment-2".into(), body: "Second".into() },
            ],
        );

        let hydrated = review(seeded_review(), &store);
        assert_eq!(hydrated.comments.len(), 2);
        assert_eq!(hydrated.comments[0].body, "First");
        let last = hydrated.comments.last().unwrap();
        assert_eq!(last.body, "Second");
        assert_eq!(last.author_name, BYLINE);
        assert_eq!(last.timestamp, JUST_NOW);
        // No count until liked — the export's second comment drew no like button.
        assert_eq!(last.like_count, None);
    }

    /// Liking a comment must not invent a count out of thin air — it starts from 0,
    /// so the button reads 1.
    #[test]
    fn liking_a_comment_starts_from_zero() {
        let mut store = store();
        store
            .posted_comments
            .insert(ID.into(), vec![PostedComment { id: "comment-1".into(), body: "Mine".into() }]);
        store.liked_comments.insert("comment-1".into());

        let hydrated = review(seeded_review(), &store);
        let comment = &hydrated.comments[0];
        assert!(comment.liked);
        assert_eq!(comment.like_count, Some(1));
    }

    /// A reply to a comment posted this session has to render too — which only
    /// works if posted comments are appended before the reply pass.
    #[test]
    fn posted_replies_attach_to_their_comment() {
        let mut store = store();
        store.posted_comments.insert(
            ID.into(),
            vec![PostedComment { id: "comment-1".into(), body: "Mine".into() }],
        );
        store.posted_replies.insert(
            (ID.into(), "comment-1".into()),
            vec![PostedReply { id: "reply-2".into(), body: "And a follow-up".into() }],
        );

        let hydrated = review(seeded_review(), &store);
        let mine = &hydrated.comments[0];
        assert_eq!(mine.replies.len(), 1);
        assert_eq!(mine.replies[0].body, "And a follow-up");
        assert_eq!(mine.replies[0].author_name, BYLINE);
    }

    /// Replies keyed to one review don't leak into another.
    #[test]
    fn replies_are_scoped_to_their_review() {
        let mut store = store();
        store
            .posted_comments
            .insert(ID.into(), vec![PostedComment { id: "comment-1".into(), body: "Mine".into() }]);
        store.posted_replies.insert(
            ("some-other-review".into(), "comment-1".into()),
            vec![PostedReply { id: "reply-2".into(), body: "Wrong review".into() }],
        );

        let hydrated = review(seeded_review(), &store);
        assert!(hydrated.comments[0].replies.is_empty());
    }

    /// And a like keyed to another review doesn't either — both screens read the
    /// same tables, so the id is the only thing keeping them apart.
    #[test]
    fn likes_are_scoped_to_their_review() {
        let mut store = store();
        store.liked_reviews.insert("user-marcusdrey-dune-part-two".into());

        let hydrated = review(seeded_review(), &store);
        assert!(!hydrated.liked, "another person's review of the same film");
        assert_eq!(hydrated.like_count, None);
    }

    /// The like handlers return this number directly rather than re-hydrating, so
    /// it has to agree with what the hydrate passes produce.
    #[test]
    fn like_counts_round_trip() {
        assert_eq!(like_count(Some(24), true), Some(25));
        assert_eq!(like_count(Some(24), false), Some(24));
        assert_eq!(like_count(None, true), Some(1));
        assert_eq!(like_count(None, false), None);
    }

    #[test]
    fn an_empty_store_changes_nothing() {
        let base = seeded_review();
        let hydrated = review(base.clone(), &store());
        assert!(!hydrated.liked);
        assert_eq!(hydrated.like_count, base.like_count);
        assert_eq!(hydrated.paragraphs, base.paragraphs);
        assert!(hydrated.comments.is_empty());
    }
}
