//! Mutable state for the interactive parts of the demo.
//!
//! Everything lives in memory behind one `RwLock` and is lost on restart — there
//! is no database and no per-user identity, so "the visitor" is whoever is
//! talking to this process. That is enough for a design demo and keeps the
//! static content in `data` authoritative: the store only ever holds the deltas
//! the visitor creates (watchlist adds, ratings, likes, posted comments).
//!
//! Locks are held for the length of a single field access. Nothing awaits while
//! holding one, so `std::sync::RwLock` is fine here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

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

#[derive(Debug, Default)]
pub struct Store {
    /// Movie ids on the watchlist.
    pub watchlist: BTreeSet<String>,
    /// Movie id -> the visitor's own rating, in half-stars (1..=10).
    pub ratings: BTreeMap<String, u8>,
    /// Review ids the visitor liked.
    pub liked_reviews: BTreeSet<String>,
    /// Comment ids the visitor liked.
    pub liked_comments: BTreeSet<String>,
    /// Review id -> comments the visitor posted, oldest first.
    pub posted_comments: BTreeMap<String, Vec<PostedComment>>,
    /// (review id, comment id) -> replies the visitor posted, oldest first.
    pub posted_replies: BTreeMap<(String, String), Vec<PostedReply>>,
    counter: u64,
}

impl Store {
    /// Monotonic id for freshly posted content, e.g. "comment-3".
    pub fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}-{}", self.counter)
    }
}

#[derive(Clone, Default)]
pub struct AppState {
    pub store: Arc<RwLock<Store>>,
}

impl AppState {
    /// Read the store. Panics only if a writer panicked while holding the lock,
    /// which would mean the process is already in an unknown state.
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, Store> {
        self.store.read().expect("store lock poisoned")
    }

    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, Store> {
        self.store.write().expect("store lock poisoned")
    }
}
