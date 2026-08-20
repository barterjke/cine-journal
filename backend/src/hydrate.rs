//! Folds one user's state (`state`) into the content (`data` or TMDB).
//!
//! Keeping this separate means `data` stays a pure transcription of the export
//! and never has to know a store exists. Every handler builds its payload from
//! `data`, passes it through here, and serializes the result.
//!
//! Nothing here knows about accounts. It takes a `&Store` and stamps it onto a
//! payload; whose store it is was decided before it was loaded. That is why an
//! anonymous request works without a branch anywhere below: it passes an empty one.
//!
//! What is *not* here any more is the comment thread. This module used to build one
//! out of the store, which meant the thread was whatever the viewer had written
//! themselves. Threads are shared content now and come from `db::thread`; the only
//! per-viewer thing left about them is which likes are the reader's own.

use crate::models::*;
use crate::state::Store;

/// The export's own visitor, now the identity of one account.
///
/// These used to *be* the identity, because there was one visitor and no accounts.
/// A signed-in user has their own `people` row, so these are down to one job: they
/// dress the legacy account that `db::migrate` hands the pre-accounts rows to — see
/// `db::LEGACY_USER_ID`.
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

/// The legacy account's face.
///
/// This was Elena's small avatar until the profile screen existed, which made the
/// mismatch load-bearing: Elena is a *friend* on the stories rail, so the visitor
/// wearing her photo on their own comments and their own name on their profile
/// read as two different people.
pub fn visitor_avatar() -> Image {
    Image::new("img/avatar-alex-mercer.jpg", VISITOR_AVATAR_ALT)
}

/// How a like total renders beside a button: nothing at all when nobody has pressed
/// it, the number otherwise.
///
/// One place for that rule, because `content` applies it when building a review and
/// the two like handlers apply it again when answering a press. Three copies of it
/// drifted apart once already.
///
/// It used to take a transcribed base count and the viewer's own flag and add them,
/// because every like there was belonged to the one visitor. Totals are real now —
/// counted out of `liked_reviews` and `liked_comments`, the viewer's own included — so
/// there is nothing left to add and adding would double-count them.
pub fn like_count(total: u32) -> Option<u32> {
    (total > 0).then_some(total)
}

/// One page of the infinite feed.
///
/// Applied **after** the cache, not before it: a cached page carries whatever the
/// watchlist looked like when it was built, and the visitor may have added something
/// since. Running this on the way out of the cache is what keeps every "+" on a stale
/// page honest — the alternative is a cached feed whose buttons lie until it expires.
///
/// The recommendation cards need this as much as the entries do. `content::recommended`
/// leaves the flag false and already drops anything on the watchlist, but it can only
/// drop what was on the list when the page was built.
///
/// Review cards carry no watchlist flag: a review is somebody's prose about a film,
/// and the card's action is "read it", not "add it".
pub fn feed_page(mut page: FeedPage, store: &Store) -> FeedPage {
    for item in &mut page.items {
        match item {
            FeedItem::Entry(entry) => {
                entry.on_watchlist = store.watchlist.contains(&entry.movie.id);
            }
            FeedItem::Recommendation(rec) => {
                rec.on_watchlist = store.watchlist.contains(&rec.movie.id);
            }
            FeedItem::Review(_) => {}
        }
    }
    page
}

/// The collection grid's "+" buttons.
///
/// The flag is the *visitor's*, even on somebody else's collection: the poster is about
/// them, the button is about you. A person's favourites page showing their watchlist
/// state would offer you a button that changed nothing you could see.
pub fn collection(mut collection: Collection, store: &Store) -> Collection {
    for item in &mut collection.movies {
        item.on_watchlist = store.watchlist.contains(&item.movie.id);
    }
    collection
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

/// Marks which of a review's likes are the reader's own.
///
/// Just the flags. `content` builds the review and its whole thread out of SQLite,
/// counts included, because both are content that everybody sees; what only this
/// reader can be told is which hearts are filled in for *them*. Two people looking at
/// the same review get the same numbers and different hearts.
///
/// An anonymous reader passes an empty store, so nothing is flagged — and no branch
/// anywhere had to learn about that.
pub fn review(mut review: Review, store: &Store) -> Review {
    review.liked = store.liked_reviews.contains(&review.id);
    for comment in &mut review.comments {
        comment.liked = store.liked_comments.contains(&comment.id);
    }
    review
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

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
            poster: Image::missing_poster(),
        }
    }

    /// Both feeds are built by `content` from SQLite now rather than transcribed in
    /// `data`, so these construct the payload directly. That is the whole contract
    /// this module has: it takes a built payload and stamps the visitor's state on it.
    #[test]
    fn watchlist_flows_into_every_screen() {
        let mut store = store();
        store.watchlist.insert("le-souffle".into());

        let built = FeedPage {
            items: vec![
                FeedItem::Entry(FeedEntry {
                    id: "entry-le-souffle".into(),
                    movie: movie("le-souffle"),
                    rating_half_stars: 8,
                    on_watchlist: false,
                }),
                FeedItem::Entry(FeedEntry {
                    id: "entry-the-drop".into(),
                    movie: movie("the-drop"),
                    rating_half_stars: 8,
                    on_watchlist: false,
                }),
                FeedItem::Recommendation(Recommendation {
                    movie: movie("le-souffle"),
                    star_rating: Some(4.5),
                    because: "Neon Reverie".into(),
                    because_movie_id: "neon-reverie".into(),
                    because_favorite: true,
                    on_watchlist: false,
                }),
            ],
            next_cursor: None,
            from_cache: false,
        };

        let hydrated = feed_page(built, &store);
        let flagged: Vec<&str> = hydrated
            .items
            .iter()
            .filter_map(|item| match item {
                FeedItem::Entry(entry) if entry.on_watchlist => Some(entry.movie.id.as_str()),
                FeedItem::Recommendation(rec) if rec.on_watchlist => Some(rec.movie.id.as_str()),
                _ => None,
            })
            .collect();
        // Both cards about the watchlisted film, and nothing else. The recommendation
        // draws the same "+" as the entry does, so a film already on the watchlist
        // showing an empty one was the bug here.
        assert_eq!(flagged, ["le-souffle", "le-souffle"]);

        assert!(movie_detail(data::movie_detail_by_id("le-souffle"), &store).on_watchlist);
        assert!(!movie_detail(data::movie_detail_by_id("the-drop"), &store).on_watchlist);
    }

    /// The flag is stamped on the way out of the cache, so a page built before the
    /// visitor watchlisted something still draws the right button. Same call as above —
    /// what this pins is that `from_cache` doesn't route around it.
    #[test]
    fn a_cached_page_gets_current_watchlist_state() {
        let mut store = store();
        store.watchlist.insert("le-souffle".into());

        let stale = FeedPage {
            items: vec![FeedItem::Entry(FeedEntry {
                id: "entry-le-souffle".into(),
                movie: movie("le-souffle"),
                rating_half_stars: 8,
                on_watchlist: false,
            })],
            next_cursor: Some("1.0.0".into()),
            from_cache: true,
        };

        let hydrated = feed_page(stale, &store);
        assert!(hydrated.from_cache, "the flag survives the pass");
        assert_eq!(hydrated.next_cursor.as_deref(), Some("1.0.0"));
        match &hydrated.items[0] {
            FeedItem::Entry(entry) => assert!(entry.on_watchlist),
            other => panic!("expected an entry, got {other:?}"),
        }
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

    /// A review as `content::full_review` hands one over: the page, its real like
    /// total and its whole thread, with the per-viewer flags still false. Setting
    /// those is the only job this module has left on a review.
    fn seeded_review() -> Review {
        Review {
            id: ID.into(),
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
            rating_half_stars: Some(9),
            paragraphs: vec!["Villeneuve builds a world you can feel the grit of.".into()],
            like_count: None,
            comments: Vec::new(),
            liked: false,
        }
    }

    /// One comment on it, by somebody else, with two people already liking it.
    fn comment(id: &str) -> Comment {
        Comment {
            id: id.into(),
            author_id: "account-2002".into(),
            author_name: "Ada Lovelace".into(),
            author_handle: "@ada".into(),
            author_avatar: Image::new("img/avatar-ada.jpg", "Ada."),
            is_you: false,
            timestamp: "August 20, 2026".into(),
            body: "Agreed about the sound.".into(),
            like_count: Some(2),
            replies: Vec::new(),
            liked: false,
        }
    }

    /// The id every test below keys its store entries on.
    const ID: &str = "user-elenarostova-dune-part-two";

    /// The reader's own like fills the heart. The *number* is not this module's to
    /// touch: it is everybody's total, counted in `db`, and adding the viewer's like
    /// to it here would count them twice.
    #[test]
    fn liking_a_review_fills_the_heart_and_leaves_the_total_alone() {
        let mut base = seeded_review();
        base.like_count = Some(3);

        let mut store = store();
        store.liked_reviews.insert(ID.into());

        let liked = review(base, &store);
        assert!(liked.liked);
        assert_eq!(liked.like_count, Some(3), "the viewer's own like was double-counted");
    }

    /// A review nobody has liked shows no number at all rather than a visible zero.
    #[test]
    fn an_unliked_review_shows_no_count() {
        let hydrated = review(seeded_review(), &store());
        assert!(!hydrated.liked);
        assert_eq!(hydrated.like_count, None);
    }

    /// The same for a comment: the heart is the reader's, the number is everybody's.
    #[test]
    fn a_comment_carries_the_readers_own_like_and_the_shared_total() {
        let mut base = seeded_review();
        base.comments = vec![comment("comment-1"), comment("comment-2")];

        let mut store = store();
        store.liked_comments.insert("comment-2".into());

        let hydrated = review(base, &store);
        assert!(!hydrated.comments[0].liked);
        assert!(hydrated.comments[1].liked);
        // Both still read 2, because two people liked each of them.
        assert_eq!(hydrated.comments[0].like_count, Some(2));
        assert_eq!(hydrated.comments[1].like_count, Some(2));
    }

    /// A like keyed to another review doesn't leak in — both screens read the same
    /// tables, so the id is the only thing keeping them apart.
    #[test]
    fn likes_are_scoped_to_their_review() {
        let mut store = store();
        store.liked_reviews.insert("user-marcusdrey-dune-part-two".into());
        store.liked_comments.insert("comment-99".into());

        let mut base = seeded_review();
        base.comments = vec![comment("comment-1")];

        let hydrated = review(base, &store);
        assert!(!hydrated.liked, "another person's review of the same film");
        assert!(!hydrated.comments[0].liked);
    }

    /// The rule the like handlers and `content` both apply: nothing at all for nobody,
    /// the real total otherwise. It used to add the viewer's own like to a transcribed
    /// base, which was only right while every like belonged to one visitor.
    #[test]
    fn a_like_total_hides_itself_at_zero() {
        assert_eq!(like_count(0), None);
        assert_eq!(like_count(1), Some(1));
        assert_eq!(like_count(24), Some(24));
    }

    /// An anonymous reader passes an empty store, and gets the thread untouched:
    /// every comment still there, every author still credited, nothing liked.
    #[test]
    fn an_empty_store_leaves_the_thread_readable() {
        let mut base = seeded_review();
        base.comments = vec![comment("comment-1")];
        base.like_count = Some(4);

        let hydrated = review(base.clone(), &store());
        assert!(!hydrated.liked);
        assert_eq!(hydrated.like_count, Some(4));
        assert_eq!(hydrated.paragraphs, base.paragraphs);
        assert_eq!(hydrated.comments.len(), 1, "a signed-out reader lost the thread");
        assert_eq!(hydrated.comments[0].author_name, "Ada Lovelace");
        assert!(!hydrated.comments[0].is_you);
        assert_eq!(hydrated.comments[0].like_count, Some(2));
    }
}
