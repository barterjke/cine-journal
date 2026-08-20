//! What one user changed, and the handles the handlers need to reach it.
//!
//! `Store` is a **snapshot**, rebuilt from SQLite once per request by
//! `db::load_store` and thrown away after. It holds only deltas — watchlist adds,
//! ratings, likes, posted comments — which `hydrate` folds into the content on the
//! way out, so the film data stays authoritative wherever it came from.
//!
//! Every row behind it is keyed on a user id. A request with a valid session gets
//! that account's rows; an anonymous request gets `Store::default()` and never
//! touches the tables at all, which is what keeps reads public without leaking
//! anybody's watchlist. `db::ANONYMOUS` is the id that owns nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::content::{Db, Source};

/// One request's view of one user's state.
///
/// Only what is genuinely **per viewer**. Comments and replies used to live here, and
/// that was the bug behind shared threads: a comment is content everybody can see, not
/// a delta belonging to whoever wrote it, so `db::thread` reads the whole conversation
/// and this keeps only which likes the viewer pressed.
///
/// Ordered sets and maps rather than hash ones, so a payload built from this is
/// byte-identical between two requests over the same data. Writes go straight to
/// SQLite (see `db`), never through here, so there is nothing to flush.
#[derive(Debug, Default)]
pub struct Store {
    /// Movie ids on the watchlist.
    pub watchlist: BTreeSet<String>,
    /// Movie ids the user marked as favourites. Stored rather than derived from
    /// `ratings`: a favourite is a separate statement from a high score, and the
    /// heart on a film's page is what writes it.
    pub favorites: BTreeSet<String>,
    /// Movie id -> the user's own rating, in half-stars (1..=10).
    pub ratings: BTreeMap<String, u8>,
    /// Movie id -> what the user wrote about it. Independent of `ratings`, so
    /// un-rating a film leaves the prose alone and vice versa.
    pub written_reviews: BTreeMap<String, String>,
    /// Review ids the user liked. Drives the filled heart, not the number beside it:
    /// the count is everybody's and comes from `db`.
    pub liked_reviews: BTreeSet<String>,
    /// Comment ids the user liked, for the same reason.
    pub liked_comments: BTreeSet<String>,
}

/// Everything a handler needs: where films come from, where the users' own rows
/// live, where built feed pages are parked, and whether sign-in is configured.
///
/// `Source` is behind the same `Arc` as the rest because axum clones the state for
/// every request and `Tmdb` owns a connection pool and a cache — cloning that per
/// request would throw the cache away each time. `Cache` needs no `Arc` of its own:
/// `redis::aio::ConnectionManager` is internally reference-counted and multiplexes,
/// so cloning it shares one connection rather than opening another.
#[derive(Clone)]
pub struct AppState {
    pub source: std::sync::Arc<Source>,
    pub db: Db,
    /// The feed cache, or a no-op stand-in when no Redis is configured or reachable.
    /// Every operation on it degrades to a miss, so nothing downstream branches on
    /// whether it's real — see `cache`.
    pub cache: crate::cache::Cache,
    /// The Google credentials, or `None` when none are configured. `None` means
    /// nobody can sign in, so every write 401s and every read still works — see
    /// `auth::Google::from_env`.
    pub google: Option<crate::auth::Google>,
}

impl AppState {
    pub fn new(
        source: Source,
        db: Db,
        cache: crate::cache::Cache,
        google: Option<crate::auth::Google>,
    ) -> Self {
        Self { source: std::sync::Arc::new(source), db, cache, google }
    }

    /// A fresh snapshot of one user's state, or an empty one for an anonymous
    /// reader.
    pub fn store(&self, user: Option<&str>) -> Store {
        crate::content::store(&self.db, user)
    }
}
