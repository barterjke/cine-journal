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
//! - **The visitor's deltas** — watchlist, ratings, likes, posted comments — also
//!   live in SQLite, are read as a snapshot into `state::Store`, and are folded
//!   into responses by `hydrate`.
//!
//! There is no per-user identity, so all clients share one visitor.
//!
//! The export's poster and avatar images live in `reference/cine-journal/img/` and
//! are served from `/img`; the social layer's avatars still come from there, while
//! TMDB posters are absolute CDN URLs.

mod content;
mod data;
mod db;
mod hydrate;
mod models;
mod routes;
mod state;
mod tmdb;

use std::{
    net::SocketAddr,
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

    // The frontend is served by Vite on a different port in dev, so the browser
    // treats API calls as cross-origin. No auth and no credentials on any
    // endpoint, so permissive is fine for local development. The writes are
    // unauthenticated by design (one shared visitor) and now land in a real file
    // on disk, so this must not be exposed publicly.
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

    let state = state::AppState::new(source, Arc::new(Mutex::new(conn)));
    let app = routes::router(state)
        .nest_service("/img", ServeDir::new(img_dir))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("listening on http://{addr}");

    axum::serve(listener, app).await.expect("server error");
}
