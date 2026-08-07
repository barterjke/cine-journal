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

/// Longest accepted comment, reply or review. The composer is a 3-row textarea;
/// this is generous for that and stops one client filling the database.
const MAX_BODY_LEN: usize = 2_000;

/// Longest accepted bio. Much shorter than a review because the profile header
/// clamps it to one or two lines — anything past this would be stored and never
/// read.
const MAX_BIO_LEN: usize = 280;

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
        .route("/api/movies/{id}/reviews", get(movie_reviews))
        .route("/api/movies/{id}/watchlist", post(watchlist))
        .route("/api/movies/{id}/favorite", post(favorite))
        .route("/api/movies/{id}/rating", put(rate))
        .route("/api/movies/{id}/review", put(write_review))
        .route("/api/watchlist", get(watchlist_ids))
        .route("/api/profile", get(profile).put(edit_bio))
        .route("/api/people", get(people))
        .route("/api/people/{handle}", get(person))
        .route("/api/people/{id}/follow", post(follow))
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
    let reviews = content::reviews(&state.source, &state.db).await;
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

/// The reviews of one film, the people the visitor follows first.
///
/// A separate request from the detail payload rather than a field on it: the film's
/// facts are cached upstream for a day, and the reviews change the moment you
/// follow someone. Folding them together would mean either caching a follow state
/// or not caching the film.
///
/// No 404 for an unknown film — an empty list is the honest answer for "which of
/// your friends reviewed this", and every id in demo mode resolves anyway.
async fn movie_reviews(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Vec<UserReview>> {
    Json(content::reviews_of_movie(&state.source, &state.db, &id).await)
}

/// The friend directory: nickname search, plus who the visitor follows and who
/// follows them. All three in one response so the screen's lists can't disagree
/// about whether a follow landed.
async fn people(
    State(state): State<AppState>,
    Query(query): Query<PeopleQuery>,
) -> Json<PeopleResponse> {
    Json(content::people(&state.db, query.q.as_deref().unwrap_or_default()))
}

/// One person's page, by nickname. The `@` is optional in the path, so both
/// `/api/people/elenarostova` and `/api/people/@elenarostova` resolve.
async fn person(State(state): State<AppState>, Path(handle): Path<String>) -> Response {
    match content::person(&state.source, &state.db, &handle).await {
        Some(profile) => Json(profile).into_response(),
        None => not_found(format!("no person with nickname '{handle}'")),
    }
}

/// Follow or unfollow, by id. Idempotent when the body states the target, so a
/// double-click can't leave the button disagreeing with the graph.
///
/// Ids rather than nicknames here, unlike the read above: this writes a row keyed
/// on `people.id`, and resolving a nickname first would let a rename orphan it.
async fn follow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<FollowRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.following);
    match content::set_follow(&state.db, &id, requested) {
        Ok(Some(state)) => Json(state).into_response(),
        Ok(None) => not_found(format!("no person with id '{id}'")),
        Err(error) => write_failed(error),
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

/// The profile screen. No `hydrate` pass: the visitor's own rows are what this
/// payload *is*, rather than a delta folded over borrowed content.
async fn profile(State(state): State<AppState>) -> Json<Profile> {
    Json(content::profile(&state.source, &state.db).await)
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

/// Favourite, un-favourite, or toggle. Same shape as the watchlist above, and
/// deliberately a sibling of it rather than something derived from the ratings:
/// a favourite is a thing you say, not the top of a sorted list.
async fn favorite(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<FavoriteRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.is_favorite);

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_favorite(&conn, &id, requested)
    };

    match result {
        Ok(is_favorite) => Json(FavoriteState { movie_id: id, is_favorite }).into_response(),
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
    if !content::review_exists(&state.db, &id) {
        return not_found(format!("no review with id '{id}'"));
    }
    // No stored count to add to: reviews arrive with none, so the button reads
    // nothing until this click and 1 after.
    let base_count = None;

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
    if !content::review_exists(&state.db, &review_id) {
        return not_found(format!("no review with id '{review_id}'"));
    }
    if !content::comment_exists(&state.db, &review_id, &comment_id) {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }
    // As in `like_review`: every comment is one the visitor posted, so there is no
    // count behind it.
    let base_count = None;

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

/// Trim and cap a field, allowing blank. Counts characters, not bytes, so an
/// accented or non-Latin review isn't rejected earlier than an ASCII one.
///
/// Returns the error *message* rather than a built `Response` — a `Response` in
/// an `Err` variant makes the whole `Result` as large as the success path.
fn validate_capped(field: &str, text: &str, max: usize) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.chars().count() > max {
        return Err(format!("{field} must be at most {max} characters"));
    }
    Ok(trimmed.to_string())
}

/// The same, for the fields where blank is a mistake rather than an instruction.
///
/// A comment or reply has nowhere to go if it's empty; a review or a bio uses the
/// empty string to mean "delete this", so those two call `validate_capped` direct.
fn validate_body(body: &str) -> Result<String, String> {
    let trimmed = validate_capped("body", body, MAX_BODY_LEN)?;
    if trimmed.is_empty() {
        return Err("body must not be empty".into());
    }
    Ok(trimmed)
}

/// Write, rewrite, or clear the visitor's own review of a film.
///
/// A PUT rather than a POST because there is only ever one: the composer is
/// prefilled with `your_review` and saving replaces it. Blank deletes, which is
/// how the composer's "Remove" gets back to no review at all.
async fn write_review(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReviewRequest>,
) -> Response {
    let body = match validate_capped("body", &body.body, MAX_BODY_LEN) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_visitor_review(&conn, &id, &body)
    };

    match result {
        Ok(your_review) => Json(ReviewState { movie_id: id, your_review }).into_response(),
        Err(error) => write_failed(error),
    }
}

/// Edit the profile bio. Blank restores the export's line rather than leaving the
/// header empty, and the response carries whichever of the two is now stored so
/// the field can't sit showing something the profile doesn't.
async fn edit_bio(State(state): State<AppState>, Json(body): Json<BioRequest>) -> Response {
    let bio = match validate_capped("bio", &body.bio, MAX_BIO_LEN) {
        Ok(bio) => bio,
        Err(message) => return bad_request(message),
    };

    match content::set_bio(&state.db, &bio) {
        Ok(bio) => Json(BioState { bio }).into_response(),
        Err(error) => write_failed(error),
    }
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
    if !content::review_exists(&state.db, &id) {
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
    if !content::review_exists(&state.db, &review_id) {
        return not_found(format!("no review with id '{review_id}'"));
    }
    // Only replies to comments that exist — otherwise the reply would be stored
    // under a key nothing renders and vanish silently.
    if !content::comment_exists(&state.db, &review_id, &comment_id) {
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
