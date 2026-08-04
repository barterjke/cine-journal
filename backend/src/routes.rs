//! HTTP handlers.
//!
//! Reads rebuild their slice of `data` per request and pass it through `hydrate`
//! to pick up whatever the visitor has changed. Writes touch only `state` — the
//! transcribed content is never mutated.
//!
//! Mutations return the piece of state they changed rather than the whole screen,
//! so the frontend can patch a button without refetching the page around it.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Serialize;

use crate::models::*;
use crate::state::{AppState, PostedComment, PostedReply};
use crate::{data, hydrate};

/// Ratings are half-stars out of five, so ten is the ceiling.
const MAX_HALF_STARS: u8 = 10;

/// Longest accepted comment or reply. The composer is a 3-row textarea; this is
/// generous for that and stops the in-memory store growing without bound.
const MAX_BODY_LEN: usize = 2_000;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
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

// --- Reads --------------------------------------------------------------------

async fn feed(State(state): State<AppState>) -> Json<Feed> {
    Json(hydrate::feed(data::feed(), &state.read()))
}

async fn mobile_feed(State(state): State<AppState>) -> Json<MobileFeed> {
    Json(hydrate::mobile_feed(data::mobile_feed(), &state.read()))
}

async fn reviews(State(state): State<AppState>) -> Json<Vec<Review>> {
    let store = state.read();
    Json(data::reviews().into_iter().map(|r| hydrate::review(r, &store)).collect())
}

async fn review(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match data::review_by_id(&id) {
        Some(review) => Json(hydrate::review(review, &state.read())).into_response(),
        None => not_found(format!("no review with id '{id}'")),
    }
}

async fn movies(State(state): State<AppState>) -> Json<Vec<MovieDetail>> {
    let store = state.read();
    Json(data::movie_details().into_iter().map(|m| hydrate::movie_detail(m, &store)).collect())
}

/// Every id resolves — see `data::movie_detail_by_id`. Links from the feed and
/// search can't 404.
async fn movie(State(state): State<AppState>, Path(id): Path<String>) -> Json<MovieDetail> {
    Json(hydrate::movie_detail(data::movie_detail_by_id(&id), &state.read()))
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    Json(hydrate::search(data::search(&query), &state.read()))
}

/// The visitor's watchlist, most recently added last.
async fn watchlist_ids(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.read().watchlist.iter().cloned().collect())
}

// --- Writes -------------------------------------------------------------------

/// Add, remove, or toggle. Idempotent when the body states the target value, so
/// a double-click can't desync the button from the store.
async fn watchlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<WatchlistRequest>>,
) -> Json<WatchlistState> {
    let requested = body.and_then(|Json(body)| body.on_watchlist);
    let mut store = state.write();

    let on_watchlist = match requested {
        Some(true) => {
            store.watchlist.insert(id.clone());
            true
        }
        Some(false) => {
            store.watchlist.remove(&id);
            false
        }
        None => {
            if store.watchlist.remove(&id) {
                false
            } else {
                store.watchlist.insert(id.clone());
                true
            }
        }
    };

    Json(WatchlistState { movie_id: id, on_watchlist })
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

    let mut store = state.write();
    let your_rating_half_stars = if body.rating_half_stars == 0 {
        store.ratings.remove(&id);
        None
    } else {
        store.ratings.insert(id.clone(), body.rating_half_stars);
        Some(body.rating_half_stars)
    };

    Json(RatingState { movie_id: id, your_rating_half_stars }).into_response()
}

async fn like_review(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(review) = data::review_by_id(&id) else {
        return not_found(format!("no review with id '{id}'"));
    };

    let mut store = state.write();
    let liked = if store.liked_reviews.remove(&id) {
        false
    } else {
        store.liked_reviews.insert(id.clone());
        true
    };

    let like_count = hydrate::like_count(review.like_count, liked);
    Json(LikeState { id, liked, like_count }).into_response()
}

async fn like_comment(
    State(state): State<AppState>,
    Path((review_id, comment_id)): Path<(String, String)>,
) -> Response {
    let Some(review) = data::review_by_id(&review_id) else {
        return not_found(format!("no review with id '{review_id}'"));
    };

    // A comment the visitor posted this session isn't in `data`, so fall back to
    // the store before deciding the id is bogus.
    let base_count = match review.comments.iter().find(|c| c.id == comment_id) {
        Some(comment) => comment.like_count,
        None => {
            let posted = state
                .read()
                .posted_comments
                .get(&review_id)
                .is_some_and(|comments| comments.iter().any(|c| c.id == comment_id));
            if !posted {
                return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
            }
            None
        }
    };

    let mut store = state.write();
    let liked = if store.liked_comments.remove(&comment_id) {
        false
    } else {
        store.liked_comments.insert(comment_id.clone());
        true
    };

    let like_count = hydrate::like_count(base_count, liked);
    Json(LikeState { id: comment_id, liked, like_count }).into_response()
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
    let Some(review) = data::review_by_id(&id) else {
        return not_found(format!("no review with id '{id}'"));
    };
    let body = match validate_body(&body.body) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };

    let mut store = state.write();
    let comment_id = store.next_id("comment");
    store
        .posted_comments
        .entry(id)
        .or_default()
        .push(PostedComment { id: comment_id, body });

    Json(hydrate::review(review, &store)).into_response()
}

async fn post_reply(
    State(state): State<AppState>,
    Path((review_id, comment_id)): Path<(String, String)>,
    Json(body): Json<PostBodyRequest>,
) -> Response {
    let Some(review) = data::review_by_id(&review_id) else {
        return not_found(format!("no review with id '{review_id}'"));
    };
    let body = match validate_body(&body.body) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };

    // Only replies to comments that exist — otherwise the reply would be stored
    // under a key nothing renders and vanish silently.
    let known = review.comments.iter().any(|c| c.id == comment_id)
        || state
            .read()
            .posted_comments
            .get(&review_id)
            .is_some_and(|comments| comments.iter().any(|c| c.id == comment_id));
    if !known {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }

    let mut store = state.write();
    let reply_id = store.next_id("reply");
    store
        .posted_replies
        .entry((review_id, comment_id))
        .or_default()
        .push(PostedReply { id: reply_id, body });

    Json(hydrate::review(review, &store)).into_response()
}
