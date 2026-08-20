//! CinéJournal API — real films from TMDB, a social layer of its own, plus
//! whatever the visitor changes on top of both.
//!
//! Three layers, and the split is the point:
//!
//! - **Content** is immutable. In TMDB mode it comes from `tmdb` (mapped to the
//!   wire types by `tmdb::map`); with no token it comes from `data`, the dataset
//!   transcribed verbatim from the static export. `content` is the seam, so
//!   nothing downstream knows which one answered.
//! - **The social layer** — friends, stories, live rooms — lives in SQLite (`db`),
//!   because TMDB has no such thing.
//! - **Each user's deltas** — watchlist, ratings, likes, posted comments — also
//!   live in SQLite, are read as a snapshot into `state::Store`, and are folded
//!   into responses by `hydrate`.
//!
//! **Identity is Google sign-in, and it lives in `auth`.** An account is a row in the
//! same `people` table the seeded folk are in, so it has a page, a nickname and an
//! avatar like anyone else and can be followed. Reads are public and answer for an
//! anonymous browser against an empty `Store`; writes need a session and 401 without
//! one. Sign-in is optional: with no Google credentials the server boots and says so.
//!
//! The export's poster and avatar images live in `reference/cine-journal/img/` and
//! are served from `/img`; the social layer's avatars still come from there, while
//! TMDB posters and Google profile pictures are absolute CDN URLs.

mod auth;
mod cache;
mod content;
mod data;
mod db;
mod hydrate;
mod models;
mod routes;
mod state;
mod tmdb;

use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

/// Where the export's images live, relative to the workspace root.
const IMG_DIR: &str = "../reference/cine-journal/img";

/// Not 3000 — that port is commonly taken by a Next.js dev server. Override
/// with `PORT=... cargo run`; keep `frontend/vite.config.ts` in sync.
const DEFAULT_PORT: u16 = 3001;

/// Which interface to listen on. Loopback by default, so a development server is not
/// exposed on every network the machine is on. Override with `BIND_ADDR`.
///
/// A container has to override it — loopback there is reachable only from inside
/// the container. `docker-compose.yml` sets `BIND_ADDR=0.0.0.0`, and that is safe
/// because the container publishes no ports.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1";

#[tokio::main]
async fn main() {
    // `cargo run` starts in `backend/`, but the file people edit is at the repo
    // root, so try there first and fall back to the usual upward search.
    if dotenvy::from_path("../.env").is_err() {
        let _ = dotenvy::dotenv();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cine_journal_api=info,tower_http=info".into()),
        )
        .init();

    // Permissive, and safe to be, because it does not allow credentials. Both
    // environments are single-origin anyway — Vite proxies `/api` in development
    // (see `frontend/vite.config.ts`) and Caddy does the same in production — so
    // nothing the app itself does is cross-origin.
    //
    // Refusing credentials is what makes the wildcard harmless now that writes need
    // a session: a browser will not attach the session cookie to a cross-origin
    // request under this policy, so another site cannot spend a visitor's session
    // from their browser. Adding `allow_credentials(true)` here would open exactly
    // that hole, and would require naming an explicit origin instead of `Any`.
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let img_dir = PathBuf::from(IMG_DIR);
    if !img_dir.is_dir() {
        tracing::warn!(
            path = %img_dir.display(),
            "image directory not found — /img will 404; run from the backend/ directory"
        );
    }

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| db::DEFAULT_PATH.to_string());
    let conn = db::open(&db_path).unwrap_or_else(|e| panic!("could not open {db_path}: {e}"));
    tracing::info!(path = %db_path, "database ready");

    // Reads the token from the environment and verifies it upstream, so a bad one
    // becomes a banner rather than six broken screens. Never logs the token.
    let source = content::Source::from_env().await;

    seed_graph(&conn, &source).await;

    // Optional by design. With no `REDIS_URL`, or with a Redis that is down, every
    // cache operation is a miss and the feed is built from source on each request —
    // slower, never broken. Connecting is lazy, so a dead server can't delay startup.
    let cache = cache::Cache::from_env().await;

    // Also optional. With no credentials sign-in is off, every read still works and
    // every write is a 401 — the same degrade-don't-die posture as Redis above.
    // Reads the credentials from the environment and never logs them.
    let google = auth::Google::from_env();

    let state = state::AppState::new(source, Arc::new(Mutex::new(conn)), cache, google);
    let app = routes::router(state)
        .nest_service("/img", ServeDir::new(img_dir))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    // Parsed, so `::` and a specific interface both work. A value that isn't an IP
    // panics rather than falling back to loopback: falling back would bind
    // somewhere unreachable while still logging "listening".
    let ip: IpAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
        .parse()
        .unwrap_or_else(|e| panic!("BIND_ADDR is not an IP address: {e}"));

    let addr = SocketAddr::from((ip, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("listening on http://{addr}");

    axum::serve(listener, app).await.expect("server error");
}

/// Fill the social graph on first run: some users, some followers, some reviews,
/// so the friend screens have something to show before anyone has used the app.
///
/// Never fatal. A harvest needs the network, and refusing to boot because TMDB was
/// slow would trade a partly-populated friend list for no application at all — the
/// screens already say when a list is empty. Runs before the server binds rather
/// than in a background task, so the graph is never half-visible mid-request.
async fn seed_graph(conn: &rusqlite::Connection, source: &content::Source) {
    match db::needs_graph_seed(conn) {
        // Already seeded, or the visitor has since followed people — either way the
        // graph is theirs now and re-seeding would talk over it.
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            tracing::warn!(%error, "could not check the social graph; skipping the seed");
            return;
        }
    }

    let users = content::harvest_graph(source).await;
    if users.is_empty() {
        tracing::warn!("social graph left empty — no people to seed");
        return;
    }

    let reviews: usize = users.iter().map(|u| u.reviews.len()).sum();
    match db::seed_graph(conn, &users) {
        Ok(count) => tracing::info!(people = count, reviews, "social graph seeded"),
        Err(error) => tracing::warn!(%error, "could not seed the social graph"),
    }
}
