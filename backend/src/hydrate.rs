//! Folds the visitor's state (`state`) into the static content (`data`).
//!
//! Keeping this separate means `data` stays a pure transcription of the export
//! and never has to know a store exists. Every handler builds its payload from
//! `data`, passes it through here, and serializes the result.

use crate::models::*;
use crate::state::Store;

/// The avatar the composer uses for the visitor's own posts. The export drew a
/// plain "ME" monogram rather than a photo, so there is no image to reuse — this
/// stands in for one on the posted comment rows.
const VISITOR_NAME: &str = "You";
const VISITOR_AVATAR_ALT: &str = "The signed-in visitor's placeholder avatar.";

/// Posted content has no real timestamp — the export's are pre-formatted strings
/// like "2 hours ago", and inventing a clock-based one would drift out of that
/// vocabulary the moment a minute passed.
const JUST_NOW: &str = "Just now";

fn visitor_avatar() -> Image {
    // Elena's small avatar doubles as the visitor's; the export shipped no
    // dedicated one, and every other image here is a real file on disk.
    Image::new("img/avatar-elena-rostova-sm.jpg", VISITOR_AVATAR_ALT)
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
    movie.your_rating_half_stars = store.ratings.get(&movie.id).copied();
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
/// Like counts are bumped by one when liked so the number beside the button
/// matches what was just clicked; the underlying static count is never mutated.
pub fn review(mut review: Review, store: &Store) -> Review {
    review.liked = store.liked_reviews.contains(&review.id);
    review.like_count = like_count(review.like_count, review.liked);

    // Append the visitor's own comments *before* the pass below, so a reply to
    // one of them is rendered too — the reply loop has to see every comment,
    // not just the transcribed ones.
    if let Some(posted) = store.posted_comments.get(&review.id) {
        review.comments.extend(posted.iter().map(|comment| Comment {
            id: comment.id.clone(),
            author_name: VISITOR_NAME.into(),
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
                author_name: VISITOR_NAME.into(),
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

    #[test]
    fn watchlist_flows_into_every_screen() {
        let mut store = store();
        store.watchlist.insert("le-souffle".into());

        let feed = feed(data::feed(), &store);
        let entry = feed.recent.iter().find(|e| e.movie.id == "le-souffle").unwrap();
        assert!(entry.on_watchlist);
        assert!(feed.recent.iter().filter(|e| e.movie.id != "le-souffle").all(|e| !e.on_watchlist));

        assert!(movie_detail(data::movie_detail_by_id("le-souffle"), &store).on_watchlist);
        assert!(!movie_detail(data::movie_detail_by_id("the-drop"), &store).on_watchlist);
    }

    #[test]
    fn watchlist_flows_into_the_mobile_feed() {
        let mut store = store();
        store.watchlist.insert("red-shift".into());

        let feed = mobile_feed(data::mobile_feed(), &store);
        let flagged: Vec<&str> =
            feed.items.iter().filter(|i| i.on_watchlist).map(|i| i.movie.id.as_str()).collect();
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

    #[test]
    fn liking_a_review_bumps_its_count_by_one() {
        let base = data::review_by_id("dune-part-two").unwrap();
        assert_eq!(base.like_count, Some(24));

        let mut store = store();
        store.liked_reviews.insert("dune-part-two".into());

        let liked = review(base, &store);
        assert!(liked.liked);
        assert_eq!(liked.like_count, Some(25));
    }

    /// Liking a comment that renders no count must not invent one out of thin
    /// air — it starts from 0, so the button reads 1.
    #[test]
    fn liking_a_countless_comment_starts_from_zero() {
        let mut store = store();
        store.liked_comments.insert("comment-sarah-j".into());

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        let comment = hydrated.comments.iter().find(|c| c.id == "comment-sarah-j").unwrap();
        assert!(comment.liked);
        assert_eq!(comment.like_count, Some(1));
    }

    #[test]
    fn posted_comments_are_appended_last() {
        let mut store = store();
        store
            .posted_comments
            .insert("dune-part-two".into(), vec![PostedComment { id: "c-1".into(), body: "Mine".into() }]);

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        assert_eq!(hydrated.comments.len(), 3);
        let last = hydrated.comments.last().unwrap();
        assert_eq!(last.body, "Mine");
        assert_eq!(last.author_name, VISITOR_NAME);
        assert_eq!(last.timestamp, JUST_NOW);
    }

    #[test]
    fn posted_replies_attach_to_their_comment() {
        let mut store = store();
        store.posted_replies.insert(
            ("dune-part-two".into(), "comment-marcus".into()),
            vec![PostedReply { id: "r-1".into(), body: "Agreed".into() }],
        );

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        let marcus = hydrated.comments.iter().find(|c| c.id == "comment-marcus").unwrap();
        assert_eq!(marcus.replies.len(), 1);
        assert_eq!(marcus.replies[0].body, "Agreed");

        // Sarah's own nested reply is untouched.
        let sarah = hydrated.comments.iter().find(|c| c.id == "comment-sarah-j").unwrap();
        assert_eq!(sarah.replies.len(), 1);
        assert_eq!(sarah.replies[0].author_name, "Elena (Author)");
    }

    /// A reply to a comment the visitor posted this session has to render too —
    /// which only works if posted comments are appended before the reply pass.
    #[test]
    fn replies_attach_to_the_visitors_own_comments() {
        let mut store = store();
        store.posted_comments.insert(
            "dune-part-two".into(),
            vec![PostedComment { id: "comment-1".into(), body: "Mine".into() }],
        );
        store.posted_replies.insert(
            ("dune-part-two".into(), "comment-1".into()),
            vec![PostedReply { id: "reply-2".into(), body: "And a follow-up".into() }],
        );

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        let mine = hydrated.comments.iter().find(|c| c.id == "comment-1").unwrap();
        assert_eq!(mine.replies.len(), 1);
        assert_eq!(mine.replies[0].body, "And a follow-up");
    }

    /// Liking one's own posted comment gives it a count of 1, not none.
    #[test]
    fn the_visitors_own_comment_can_be_liked() {
        let mut store = store();
        store.posted_comments.insert(
            "dune-part-two".into(),
            vec![PostedComment { id: "comment-1".into(), body: "Mine".into() }],
        );
        store.liked_comments.insert("comment-1".into());

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        let mine = hydrated.comments.iter().find(|c| c.id == "comment-1").unwrap();
        assert!(mine.liked);
        assert_eq!(mine.like_count, Some(1));
    }

    /// Replies keyed to one review don't leak into another.
    #[test]
    fn replies_are_scoped_to_their_review() {
        let mut store = store();
        store.posted_replies.insert(
            ("some-other-review".into(), "comment-marcus".into()),
            vec![PostedReply { id: "r-1".into(), body: "Wrong review".into() }],
        );

        let hydrated = review(data::review_by_id("dune-part-two").unwrap(), &store);
        let marcus = hydrated.comments.iter().find(|c| c.id == "comment-marcus").unwrap();
        assert!(marcus.replies.is_empty());
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
        let base = data::review_by_id("dune-part-two").unwrap();
        let hydrated = review(base.clone(), &store());
        assert!(!hydrated.liked);
        assert_eq!(hydrated.like_count, base.like_count);
        assert_eq!(hydrated.comments.len(), base.comments.len());
    }
}
