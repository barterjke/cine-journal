//! What the visitor changed, and the handles the handlers need to reach it.
//!
//! `Store` is a **snapshot**, rebuilt from SQLite once per request by
//! `db::load_store` and thrown away after. It holds only deltas — watchlist adds,
//! ratings, likes, posted comments — which `hydrate` folds into the content on the
//! way out, so the film data stays authoritative wherever it came from.
//!
//! There is still no per-user identity: "the visitor" is whoever is talking to
//! this process, and everyone shares one row set. What changed with SQLite is that
//! their changes now outlive the process.

use std::collections::{BTreeMap, BTreeSet};

use crate::content::{Db, Source};

/// A comment the visitor posted, before it is dressed up as a `models::Comment`.
#[derive(Debug, Clone)]
pub struct PostedComment {
    pub id: String,
    pub body: String,
}

/// A reply the visitor posted under some comment.
#[derive(Debug, Clone)]
pub struct PostedReply {
    pub id: String,
    pub body: String,
}

/// One request's view of the visitor's state.
///
/// Ordered maps rather than hash maps because `hydrate` renders posted content in
/// insertion order and the ids sort the way they were created — `comment-1` before
/// `comment-2`. Writes go straight to SQLite (see `db`), never through here, so
/// there is nothing to flush.
#[derive(Debug, Default)]
pub struct Store {
    /// Movie ids on the watchlist.
    pub watchlist: BTreeSet<String>,
    /// Movie ids the visitor marked as favourites. Stored rather than derived from
    /// `ratings`: a favourite is a separate statement from a high score, and the
    /// heart on a film's page is what writes it.
    pub favorites: BTreeSet<String>,
    /// Movie id -> the visitor's own rating, in half-stars (1..=10).
    pub ratings: BTreeMap<String, u8>,
    /// Movie id -> what the visitor wrote about it. Independent of `ratings`, so
    /// un-rating a film leaves the prose alone and vice versa.
    pub written_reviews: BTreeMap<String, String>,
    /// Review ids the visitor liked.
    pub liked_reviews: BTreeSet<String>,
    /// Comment ids the visitor liked.
    pub liked_comments: BTreeSet<String>,
    /// Review id -> comments the visitor posted, oldest first.
    pub posted_comments: BTreeMap<String, Vec<PostedComment>>,
    /// (review id, comment id) -> replies the visitor posted, oldest first.
    pub posted_replies: BTreeMap<(String, String), Vec<PostedReply>>,
}

/// Everything a handler needs: where films come from, and where the visitor's own
/// rows live.
///
/// `Source` is behind the same `Arc` as the rest because axum clones the state for
/// every request and `Tmdb` owns a connection pool and a cache — cloning that per
/// request would throw the cache away each time.
#[derive(Clone)]
pub struct AppState {
    pub source: std::sync::Arc<Source>,
    pub db: Db,
}

impl AppState {
    pub fn new(source: Source, db: Db) -> Self {
        Self { source: std::sync::Arc::new(source), db }
    }

    /// A fresh snapshot of the visitor's state.
    pub fn store(&self) -> Store {
        crate::content::store(&self.db)
    }
}
