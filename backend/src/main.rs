//! CinéJournal API — serves the demo dataset the static export rendered inline,
//! plus whatever the visitor changes on top of it.
//!
//! The transcribed content in `data` is immutable; watchlist adds, ratings, likes
//! and posted comments live in `state` (in memory, lost on restart) and are
//! folded into responses by `hydrate`. There is no per-user identity, so all
//! clients share one visitor.
//!
//! The poster/avatar images live in `reference/cine-journal/img/` and are served
//! from `/img` so the frontend can use the same relative `src` paths the export
//! did.

mod data;
mod hydrate;
mod models;
mod routes;
mod state;

use std::{net::SocketAddr, path::PathBuf};

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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cine_journal_api=info,tower_http=info".into()),
        )
        .init();

    // The frontend is served by Vite on a different port in dev, so the browser
    // treats API calls as cross-origin. Public demo data with no auth and no
    // credentials — permissive is fine. The writes are unauthenticated by design
    // (one shared visitor, in-memory only), so this must not be exposed publicly.
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let img_dir = PathBuf::from(IMG_DIR);
    if !img_dir.is_dir() {
        tracing::warn!(
            path = %img_dir.display(),
            "image directory not found — /img will 404; run from the backend/ directory"
        );
    }

    let app = routes::router(state::AppState::default())
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
