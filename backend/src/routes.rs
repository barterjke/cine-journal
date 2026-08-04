//! HTTP handlers.
//!
//! Reads ask `content` for their slice — real films from TMDB, or the transcribed
//! export when there's no token — and pass it through `hydrate` to pick up whatever
//! the visitor has changed. Writes touch only the visitor's own SQLite tables; the
//! film content is never mutated, whichever source it came from.
//!
//! Mutations return the piece of state they changed rather than the whole screen,
//! so the frontend can patch a button without refetching the page around it. The
//! two composers are the exception: posting returns the whole review, because the
//! thread, the "Conversation (n)" heading and the new row all move at once.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Serialize;

use crate::models::*;
use crate::state::AppState;
use crate::{content, db, hydrate};

/// Ratings are half-stars out of five, so ten is the ceiling.
const MAX_HALF_STARS: u8 = 10;

/// Longest accepted comment or reply. The composer is a 3-row textarea; this is
/// generous for that and stops one client filling the database.
const MAX_BODY_LEN: usize = 2_000;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/feed", get(feed))
        .route("/api/feed/mobile", get(mobile_feed))
        .route("/api/reviews", get(reviews))
        .route("/api/reviews/{id}", get(review))
        .route("/api/reviews/{id}/like", post(like_review))
        .route("/api/reviews/{id}/comments", post(post_comment))
        .route("/api/reviews/{id}/comments/{comment_id}/like", post(like_comment))
        .route("/api/reviews/{id}/comments/{comment_id}/replies", post(post_reply))
        .route("/api/movies", get(movies))
        .route("/api/movies/{id}", get(movie))
        .route("/api/movies/{id}/watchlist", post(watchlist))
        .route("/api/movies/{id}/rating", put(rate))
        .route("/api/watchlist", get(watchlist_ids))
        .route("/api/search", get(search))
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn bad_request(message: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, Json(ApiError { error: message.into() })).into_response()
}

fn not_found(message: impl Into<String>) -> Response {
    (StatusCode::NOT_FOUND, Json(ApiError { error: message.into() })).into_response()
}

/// A write failed at the database. 500 rather than a cheerful 200 with a lie in
/// it — the button must not flip if the row didn't land.
fn write_failed(error: impl std::fmt::Display) -> Response {
    tracing::error!(%error, "a write failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError { error: "could not save that — see the server log".into() }),
    )
        .into_response()
}

// --- Reads --------------------------------------------------------------------

/// Whether the films on screen are real. The frontend's demo banner reads this.
async fn status(State(state): State<AppState>) -> Json<Status> {
    Json(content::status(&state.source))
}

async fn feed(State(state): State<AppState>) -> Json<Feed> {
    let feed = content::feed(&state.source, &state.db).await;
    Json(hydrate::feed(feed, &state.store()))
}

async fn mobile_feed(State(state): State<AppState>) -> Json<MobileFeed> {
    let feed = content::mobile_feed(&state.source, &state.db).await;
    Json(hydrate::mobile_feed(feed, &state.store()))
}

async fn reviews(State(state): State<AppState>) -> Json<Vec<Review>> {
    let reviews = content::reviews(&state.source).await;
    let store = state.store();
    Json(reviews.into_iter().map(|r| hydrate::review(r, &store)).collect())
}

async fn review(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match content::hydrated_review(&state.source, &state.db, &id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{id}'")),
    }
}

async fn movies(State(state): State<AppState>) -> Json<Vec<MovieDetail>> {
    let details = content::movie_details(&state.source).await;
    let store = state.store();
    Json(details.into_iter().map(|m| hydrate::movie_detail(m, &store)).collect())
}

/// In demo mode every id resolves, because only one film was ever designed. With
/// TMDB behind it an unknown id is a real 404.
async fn movie(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match content::movie_detail_by_id(&state.source, &id).await {
        Some(detail) => Json(hydrate::movie_detail(detail, &state.store())).into_response(),
        None => not_found(format!("no movie with id '{id}'")),
    }
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let response = content::search(&state.source, &query).await;
    Json(hydrate::search(response, &state.store()))
}

/// The visitor's watchlist, most recently added last.
async fn watchlist_ids(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.store().watchlist.into_iter().collect())
}

// --- Writes -------------------------------------------------------------------

/// Add, remove, or toggle. Idempotent when the body states the target value, so
/// a double-click can't desync the button from the store.
async fn watchlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<WatchlistRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.on_watchlist);

    // Scoped so the guard is dropped before this function's next await point.
    // Holding a `std::sync::Mutex` across an await would block the executor.
    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_watchlist(&conn, &id, requested)
    };

    match result {
        Ok(on_watchlist) => Json(WatchlistState { movie_id: id, on_watchlist }).into_response(),
        Err(error) => write_failed(error),
    }
}

/// Set the visitor's rating. `0` clears it, which is how the UI un-rates a film
/// by clicking the star it is already on.
async fn rate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RatingRequest>,
) -> Response {
    if body.rating_half_stars > MAX_HALF_STARS {
        return bad_request(format!(
            "rating_half_stars must be 0..={MAX_HALF_STARS}, got {}",
            body.rating_half_stars
        ));
    }

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_rating(&conn, &id, body.rating_half_stars)
    };

    match result {
        Ok(your_rating_half_stars) => {
            Json(RatingState { movie_id: id, your_rating_half_stars }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

async fn like_review(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !content::review_exists(&state.source, &id).await {
        return not_found(format!("no review with id '{id}'"));
    }
    let base_count = content::review_like_base(&state.source, &id).await;

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::toggle_review_like(&conn, &id)
    };

    match result {
        Ok(liked) => {
            let like_count = hydrate::like_count(base_count, liked);
            Json(LikeState { id, liked, like_count }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

async fn like_comment(
    State(state): State<AppState>,
    Path((review_id, comment_id)): Path<(String, String)>,
) -> Response {
    if !content::review_exists(&state.source, &review_id).await {
        return not_found(format!("no review with id '{review_id}'"));
    }
    // A comment the visitor posted isn't in the content, so check both.
    if !content::comment_exists(&state.source, &state.db, &review_id, &comment_id).await {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }
    let base_count = content::comment_like_base(&state.source, &review_id, &comment_id).await;

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::toggle_comment_like(&conn, &comment_id)
    };

    match result {
        Ok(liked) => {
            let like_count = hydrate::like_count(base_count, liked);
            Json(LikeState { id: comment_id, liked, like_count }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

/// Reject blank and oversized bodies, and hand back the trimmed text.
///
/// Returns the error *message* rather than a built `Response` — a `Response` in
/// an `Err` variant makes the whole `Result` as large as the success path.
fn validate_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("body must not be empty".into());
    }
    if trimmed.chars().count() > MAX_BODY_LEN {
        return Err(format!("body must be at most {MAX_BODY_LEN} characters"));
    }
    Ok(trimmed.to_string())
}

/// Post a top-level comment. Returns the whole review so the thread, the
/// "Conversation (n)" heading and the new row all update from one response.
async fn post_comment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PostBodyRequest>,
) -> Response {
    let body = match validate_body(&body.body) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };
    if !content::review_exists(&state.source, &id).await {
        return not_found(format!("no review with id '{id}'"));
    }

    {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(error) = db::add_comment(&conn, &id, &body) {
            return write_failed(error);
        }
    }

    // Re-read rather than patch the copy in hand: the stored row is what the next
    // request will see, so returning anything else could disagree with it.
    match content::hydrated_review(&state.source, &state.db, &id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{id}'")),
    }
}

async fn post_reply(
    State(state): State<AppState>,
    Path((review_id, comment_id)): Path<(String, String)>,
    Json(body): Json<PostBodyRequest>,
) -> Response {
    let body = match validate_body(&body.body) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };
    if !content::review_exists(&state.source, &review_id).await {
        return not_found(format!("no review with id '{review_id}'"));
    }
    // Only replies to comments that exist — otherwise the reply would be stored
    // under a key nothing renders and vanish silently.
    if !content::comment_exists(&state.source, &state.db, &review_id, &comment_id).await {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }

    {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(error) = db::add_reply(&conn, &review_id, &comment_id, &body) {
            return write_failed(error);
        }
    }

    match content::hydrated_review(&state.source, &state.db, &review_id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{review_id}'")),
    }
}
