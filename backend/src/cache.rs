//! The feed's Redis cache, and the reason it can't be relied on.
//!
//! The feed is generated per request from SQLite and TMDB — three sources, one
//! upstream call per film — so building the first page costs a second or two on a
//! cold TMDB cache. Redis turns that into the *second* visitor's problem: page one is
//! served from the cache immediately and rebuilt in the background, and the fresh
//! copy is what the next request reads. Stale-while-revalidate, the same shape every
//! social feed uses.
//!
//! **Every operation here degrades to `None`.** There may be no Redis server at all —
//! that is the normal case for a fresh checkout, since nothing in the README asks you
//! to install one — and a feed that 500s because a cache is missing would be worse
//! than no cache. So a missing `REDIS_URL`, a refused connection and a corrupt value
//! all produce the same answer: "nothing cached", and the caller builds the page. The
//! only visible difference is latency.
//!
//! Nothing here is a source of truth. The cache holds *rendered pages*, which are
//! derived from SQLite and TMDB and are rebuilt from them on every miss, so losing
//! Redis entirely loses nothing but the head start.

use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::AsyncCommands;

/// How long a cached feed page stays servable.
///
/// Ten minutes rather than seconds: the page is revalidated in the background on
/// every visit anyway, so this is only the ceiling on how stale a page can be for
/// somebody arriving after a long quiet period — and the underlying data (a friend's
/// review, a film the visitor logged) changes on that order at most.
const TTL: Duration = Duration::from_secs(600);

/// How long to wait for Redis before giving up and building the page.
///
/// Short on purpose. The cache exists to make the feed faster, so a slow cache has to
/// lose to the thing it was optimizing — waiting three seconds for Redis to answer
/// "no" is strictly worse than never asking.
const TIMEOUT: Duration = Duration::from_millis(300);

/// The env var naming the server. Absent disables the cache silently.
const URL_VAR: &str = "REDIS_URL";

/// A handle on the feed cache, or a no-op stand-in for one.
///
/// Cloneable and cheap to clone — `ConnectionManager` is an `Arc` internally and
/// multiplexes commands over one connection, so `AppState` can hold this directly and
/// every request gets the same connection rather than opening its own.
#[derive(Clone)]
pub struct Cache {
    /// `None` when no `REDIS_URL` is set, or when the URL didn't parse. A connection
    /// that is merely *down* is still `Some`: the manager reconnects on its own, so
    /// throwing the handle away on the first failure would mean a Redis restart
    /// disabled the cache until the API was restarted too.
    connection: Option<ConnectionManager>,
}

impl Cache {
    /// Connect, or decide to do without.
    ///
    /// Lazy: `new_lazy_with_config` returns without touching the network, so a Redis
    /// that is down cannot delay startup — the first `get` finds out instead, times
    /// out, and the feed is built from source. That also means "connected" is never
    /// logged as a fact here, because at this point it isn't one.
    pub async fn from_env() -> Self {
        let url = std::env::var(URL_VAR).unwrap_or_default().trim().to_string();
        if url.is_empty() {
            tracing::info!(
                "redis: disabled (no {URL_VAR}) — the feed is built from source on every request"
            );
            return Self { connection: None };
        }

        // The URL can carry a password, so nothing here logs it — only whether it
        // parsed, matching how `content::Source` treats the TMDB token.
        let client = match redis::Client::open(url) {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, "redis: disabled — could not parse {URL_VAR}");
                return Self { connection: None };
            }
        };

        let config = redis::aio::ConnectionManagerConfig::new()
            .set_connection_timeout(Some(TIMEOUT))
            .set_response_timeout(Some(TIMEOUT));

        match ConnectionManager::new_lazy_with_config(client, config) {
            Ok(connection) => {
                tracing::info!("redis: enabled — feed pages will be cached");
                Self { connection: Some(connection) }
            }
            Err(error) => {
                tracing::warn!(%error, "redis: disabled — could not build a connection manager");
                Self { connection: None }
            }
        }
    }

    /// A cache that is never there, for tests.
    ///
    /// Not `from_env`: that reads the environment, and a `REDIS_URL` exported in
    /// somebody's shell would make the test suite talk to a real server.
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self { connection: None }
    }

    /// Read a cached value, or `None` for every kind of failure.
    ///
    /// Deserialization failure is a miss rather than an error, because the shape of a
    /// cached page changes whenever `FeedItem` does: after a deploy the old JSON is
    /// simply unreadable, and treating that as a fault would break the feed for
    /// exactly as long as the TTL. Being unreadable, it is also overwritten by the
    /// next `set` on the same key.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let mut connection = self.connection.clone()?;

        // Belt and braces over the manager's own response timeout: a connection stuck
        // mid-handshake is not a response, and this request is on a user's critical
        // path.
        let json: Option<String> = match tokio::time::timeout(TIMEOUT, connection.get(key)).await {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                tracing::debug!(%error, key, "redis: read failed — building from source");
                return None;
            }
            Err(_) => {
                tracing::debug!(key, "redis: read timed out — building from source");
                return None;
            }
        };

        match serde_json::from_str(&json?) {
            Ok(value) => Some(value),
            Err(error) => {
                // Expected after a deploy that changed the payload shape, hence debug.
                tracing::debug!(%error, key, "redis: cached value no longer parses — ignoring it");
                None
            }
        }
    }

    /// Store a value under `key` with the module's TTL. Failures are logged and
    /// dropped: the response this was computed for has already been sent.
    pub async fn set<T: serde::Serialize>(&self, key: &str, value: &T) {
        let Some(mut connection) = self.connection.clone() else {
            return;
        };
        let json = match serde_json::to_string(value) {
            Ok(json) => json,
            Err(error) => {
                // A payload that won't serialize is a bug in the wire types, not a
                // cache problem — worth a real warning.
                tracing::warn!(%error, key, "could not serialize a value for the cache");
                return;
            }
        };

        let write = connection.set_ex::<_, _, ()>(key, json, TTL.as_secs());
        match tokio::time::timeout(TIMEOUT, write).await {
            Ok(Ok(())) => tracing::debug!(key, "redis: cached"),
            Ok(Err(error)) => tracing::debug!(%error, key, "redis: write failed"),
            Err(_) => tracing::debug!(key, "redis: write timed out"),
        }
    }

    /// Drop a key, so the next read rebuilds it.
    ///
    /// Called after a write that changes what the feed would contain — following
    /// someone, logging a film — because the alternative is a feed that keeps showing
    /// the pre-click version for the rest of the TTL. Serving a stale page to somebody
    /// who just changed the thing it is about is the one case where "eventually
    /// consistent" reads as broken.
    pub async fn forget(&self, key: &str) {
        let Some(mut connection) = self.connection.clone() else {
            return;
        };
        let delete = connection.del::<_, ()>(key);
        match tokio::time::timeout(TIMEOUT, delete).await {
            Ok(Ok(())) => tracing::debug!(key, "redis: invalidated"),
            Ok(Err(error)) => tracing::debug!(%error, key, "redis: delete failed"),
            Err(_) => tracing::debug!(key, "redis: delete timed out"),
        }
    }
}

/// What the key calls an anonymous reader.
///
/// Their feed is built from public content only, so it is cacheable and shareable —
/// but it is a *different* feed from any signed-in one, and it needs a name of its
/// own to say so.
const ANONYMOUS: &str = "anon";

/// The key one feed page lives under, **for one user**.
///
/// Namespaced and versioned. The prefix keeps this out of the way of anything else
/// sharing the server (a local Redis is usually shared), and the version is bumped by
/// hand when the payload shape changes — belt to `get`'s braces, since it retires old
/// entries immediately rather than letting them expire unread. It went to v2 when the
/// user id joined the key, and v3 when a review's `body` became nullable — a rating with
/// nothing written is a review now, so a cached page from before that cannot be read as
/// one of these.
///
/// **The user id is the load-bearing part.** A feed is built from whom you follow and
/// what you have logged, so two accounts asking for the same cursor want two
/// different pages. Leaving the id out would make the cache serve whichever page was
/// built last to whoever asks next — one person's feed handed to another, and a hit
/// rather than an error, so nothing would look wrong.
pub fn feed_key(user: Option<&str>, cursor: Option<&str>) -> String {
    let user = user.unwrap_or(ANONYMOUS);
    match cursor {
        None => format!("cinejournal:v3:feed:{user}:head"),
        Some(cursor) => format!("cinejournal:v3:feed:{user}:{cursor}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cross-user leak this key exists to prevent. Two accounts on the same
    /// cursor must not collide, and neither may collide with an anonymous reader.
    #[test]
    fn every_reader_has_their_own_feed_key() {
        let anon = feed_key(None, None);
        let sam = feed_key(Some("account-1"), None);
        let ada = feed_key(Some("account-2"), None);

        assert_ne!(sam, ada, "two accounts share a cached feed");
        assert_ne!(sam, anon, "a signed-in feed is cached where an anonymous one is read");
        assert!(sam.contains("account-1"));
        assert!(anon.contains(ANONYMOUS));

        // And the same holds for a deeper page: the cursor alone is not the key.
        assert_ne!(
            feed_key(Some("account-1"), Some("8.4.2")),
            feed_key(Some("account-2"), Some("8.4.2"))
        );
        // The head and a cursored page are still distinct for one account.
        assert_ne!(feed_key(Some("account-1"), Some("8.4.2")), sam);
    }
}
