//! HTTP handlers.
//!
//! Reads ask `content` for their slice — real films from TMDB, or the transcribed
//! export when there's no token — and pass it through `hydrate` to pick up whatever
//! the reader has changed. Writes touch only that user's own SQLite tables; the film
//! content is never mutated, whichever source it came from.
//!
//! **Reads are public, writes need a session.** The split is enforced by which
//! extractor a handler takes, not by a check inside it:
//!
//! - `Viewer` is `Option<User>`. A read takes it and hydrates against that user's
//!   `Store`, or an empty one when there is nobody. Signing out changes what a page
//!   says, never whether it answers.
//! - `CurrentUser` rejects with 401 before the handler body runs. Every write takes
//!   it, so forgetting the check is not something a handler can do — it would have to
//!   ask for a type it then never uses.
//!
//! The two exceptions are `/api/profile` and `/api/watchlist`, which are reads that
//! take `CurrentUser`. Neither is content: they are the account's own pages, and for
//! a reader with no account there is nothing to return but an invented identity.
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

use crate::auth::{self, CurrentUser, Viewer};
use crate::models::*;
use crate::state::AppState;
use crate::{cache, content, db, hydrate};

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
        .route("/api/auth/google", get(auth::start))
        .route("/api/auth/google/callback", get(auth::callback))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
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
        .route("/api/collections/{slug}", get(collection))
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

/// Whether the films on screen are real, and whether anybody can sign in. The demo
/// banner and the sign-in button both read this.
///
/// The presence of the credentials, never the credentials: `Google` is mapped to a
/// variant here and its fields are never reachable from outside `auth`.
async fn status(State(state): State<AppState>) -> Json<Status> {
    let sign_in = state.google.as_ref().map(|_| SignIn::Google);
    Json(content::status(&state.source, sign_in))
}

/// One page of the infinite feed, from Redis when it's there.
///
/// Stale-while-revalidate, and the client drives both halves: it asks once without
/// `refresh` and gets whatever the cache has (`from_cache: true`), which paints
/// immediately; seeing that flag it asks again with `refresh=true`, which skips the
/// cache, builds the page and stores it. So the visitor reads last request's feed while
/// this request's is being made, and the *next* visitor reads what this one built.
///
/// The revalidation lives on the client rather than in a spawned task here because a
/// background rebuild would have nowhere to deliver to — there is no push channel, so
/// the fresh page would sit in Redis until somebody reloaded anyway. This way the
/// screen swaps it in the moment it lands.
///
/// `hydrate::feed_page` runs on both paths, after the cache: a page built ten minutes
/// ago knows nothing about a film watchlisted since, and the "+" buttons have to be
/// current even when the cards aren't.
///
/// The cache key carries the user id. It has to: a feed is built from whom *you*
/// follow and what *you* logged, so a key without the id would hand one account's
/// page to the next reader as a cache hit — see `cache::feed_key`.
async fn feed(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(query): Query<FeedQuery>,
) -> Json<FeedPage> {
    // Normalised through `content`, so a cursor the server can't parse can't become a
    // cache key of its own — otherwise a client sending junk would fill Redis with
    // entries nothing will ever read again.
    let cursor = content::feed_cursor(query.cursor.as_deref());
    let key = cache::feed_key(viewer.id(), cursor.as_deref());

    if !query.refresh {
        if let Some(cached) = state.cache.get::<FeedPage>(&key).await {
            let page = FeedPage { from_cache: true, ..cached };
            return Json(hydrate::feed_page(page, &state.store(viewer.id())));
        }
    }

    let page = content::feed_page(&state.source, &state.db, viewer.id(), cursor.as_deref()).await;
    // Stored before hydration, so what's in Redis is the content and not one reader's
    // watchlist stamped onto it — belt to the key's braces.
    state.cache.set(&key, &page).await;
    Json(hydrate::feed_page(page, &state.store(viewer.id())))
}

/// One collection in full — the page behind a profile tile.
///
/// A single endpoint for the visitor's and anybody else's, selected by `?person=`,
/// because they are the same page: same grid, same posters, same "+" buttons. Only the
/// heading and the presence of ratings differ, and `content` resolves both.
async fn collection(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(slug): Path<String>,
    Query(query): Query<CollectionQuery>,
) -> Response {
    let collection =
        content::collection(&state.source, &state.db, viewer.id(), &slug, query.person.as_deref())
            .await;
    match collection {
        Some(collection) => {
            Json(hydrate::collection(collection, &state.store(viewer.id()))).into_response()
        }
        None => not_found(format!("no collection '{slug}'")),
    }
}

async fn mobile_feed(State(state): State<AppState>, viewer: Viewer) -> Json<MobileFeed> {
    let feed = content::mobile_feed(&state.source, &state.db, viewer.id()).await;
    Json(hydrate::mobile_feed(feed, &state.store(viewer.id())))
}

async fn reviews(State(state): State<AppState>, viewer: Viewer) -> Json<Vec<Review>> {
    let reviews = content::reviews(&state.source, &state.db, viewer.id()).await;
    let store = state.store(viewer.id());
    Json(reviews.into_iter().map(|r| hydrate::review(r, &store)).collect())
}

async fn review(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<String>,
) -> Response {
    match content::hydrated_review(&state.source, &state.db, viewer.id(), &id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{id}'")),
    }
}

async fn movies(State(state): State<AppState>, viewer: Viewer) -> Json<Vec<MovieDetail>> {
    let details = content::movie_details(&state.source).await;
    let store = state.store(viewer.id());
    Json(details.into_iter().map(|m| hydrate::movie_detail(m, &store)).collect())
}

/// In demo mode every id resolves, because only one film was ever designed. With
/// TMDB behind it an unknown id is a real 404.
async fn movie(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<String>,
) -> Response {
    match content::movie_detail_by_id(&state.source, &id).await {
        Some(detail) => {
            Json(hydrate::movie_detail(detail, &state.store(viewer.id()))).into_response()
        }
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
    viewer: Viewer,
    Path(id): Path<String>,
) -> Json<Vec<UserReview>> {
    Json(content::reviews_of_movie(&state.source, &state.db, viewer.id(), &id).await)
}

/// The friend directory: nickname search, plus who the reader follows and who
/// follows them. All three in one response so the screen's lists can't disagree
/// about whether a follow landed.
///
/// A read, so it answers for an anonymous reader — with results and two empty lists,
/// since a reader with no account follows nobody.
async fn people(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(query): Query<PeopleQuery>,
) -> Json<PeopleResponse> {
    Json(content::people(&state.db, viewer.id(), query.q.as_deref().unwrap_or_default()))
}

/// One person's page, by nickname. The `@` is optional in the path, so both
/// `/api/people/elenarostova` and `/api/people/@elenarostova` resolve.
async fn person(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(handle): Path<String>,
) -> Response {
    match content::person(&state.source, &state.db, viewer.id(), &handle).await {
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
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    body: Option<Json<FollowRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.following);
    match content::set_follow(&state.db, &me.id, &id, requested) {
        // Following someone changes whose reviews the feed draws, so the cached page is
        // wrong the moment this lands.
        Ok(Some(follow)) => {
            invalidate_feed(&state, &me.id).await;
            Json(follow).into_response()
        }
        Ok(None) => not_found(format!("no person with id '{id}'")),
        Err(error) => write_failed(error),
    }
}

async fn search(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let response = content::search(&state.source, &query).await;
    Json(hydrate::search(response, &state.store(viewer.id())))
}

/// The signed-in user's watchlist, most recently added last.
///
/// One of the two reads behind a session: an anonymous reader has no watchlist, and
/// answering with `[]` would say they have an empty one.
async fn watchlist_ids(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> Json<Vec<String>> {
    Json(state.store(Some(&me.id)).watchlist.into_iter().collect())
}

/// The profile screen. No `hydrate` pass: the account's own rows are what this
/// payload *is*, rather than a delta folded over borrowed content.
///
/// The other read behind a session, for the same reason: without an account there is
/// no profile, only an invented one.
async fn profile(State(state): State<AppState>, CurrentUser(me): CurrentUser) -> Response {
    match content::profile(&state.source, &state.db, &me.id).await {
        Some(profile) => Json(profile).into_response(),
        // The session named a row that has since gone. Treated as not being signed in,
        // because that is what it now is.
        None => auth::unauthorized(),
    }
}

// --- Writes -------------------------------------------------------------------

/// Drop one user's cached first page, after a write the feed would draw differently.
///
/// Called by every write the feed reads from: following someone changes whose reviews
/// appear, rating or reviewing a film adds an entry, and favouriting or watchlisting one
/// moves the recommendations. Without this the user would click and then scroll
/// through the pre-click feed for the rest of the TTL, which reads as the click not
/// having worked.
///
/// Only their own key, because that is the only page the write changed — the id in the
/// key is what makes a targeted invalidation possible at all.
///
/// Only the head, too. Deeper pages are addressed by cursor and ride out their TTL
/// rather than being walked, which would mean a `SCAN` across the namespace on every
/// click; the client de-duplicates by `FeedItem::id`, so a stale deep page overlapping
/// a rebuilt head shows its cards once rather than twice.
async fn invalidate_feed(state: &AppState, user: &str) {
    state.cache.forget(&cache::feed_key(Some(user), None)).await;
}

/// Add, remove, or toggle. Idempotent when the body states the target value, so
/// a double-click can't desync the button from the store.
async fn watchlist(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    body: Option<Json<WatchlistRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.on_watchlist);

    // Scoped so the guard is dropped before this function's next await point.
    // Holding a `std::sync::Mutex` across an await would block the executor.
    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_watchlist(&conn, &me.id, &id, requested)
    };

    match result {
        Ok(on_watchlist) => {
            invalidate_feed(&state, &me.id).await;
            Json(WatchlistState { movie_id: id, on_watchlist }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

/// Favourite, un-favourite, or toggle. Same shape as the watchlist above, and
/// deliberately a sibling of it rather than something derived from the ratings:
/// a favourite is a thing you say, not the top of a sorted list.
async fn favorite(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    body: Option<Json<FavoriteRequest>>,
) -> Response {
    let requested = body.and_then(|Json(body)| body.is_favorite);

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_favorite(&conn, &me.id, &id, requested)
    };

    match result {
        Ok(is_favorite) => {
            invalidate_feed(&state, &me.id).await;
            Json(FavoriteState { movie_id: id, is_favorite }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

/// Set the user's rating. `0` clears it, which is how the UI un-rates a film
/// by clicking the star it is already on.
async fn rate(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
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
        db::set_rating(&conn, &me.id, &id, body.rating_half_stars)
    };

    match result {
        Ok(your_rating_half_stars) => {
            invalidate_feed(&state, &me.id).await;
            Json(RatingState { movie_id: id, your_rating_half_stars }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

async fn like_review(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> Response {
    if !content::review_exists(&state.db, &id) {
        return not_found(format!("no review with id '{id}'"));
    }

    // The toggle returns the new total as well as this user's own state, so the
    // number the button shows is the one the table now holds rather than a guess.
    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::toggle_review_like(&conn, &me.id, &id)
    };

    match result {
        Ok((liked, total)) => {
            let like_count = hydrate::like_count(total);
            Json(LikeState { id, liked, like_count }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

/// Like or unlike **anybody's** comment. A thread is shared, so the only thing that
/// has to be true is that the comment is on this review.
async fn like_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((review_id, comment_id)): Path<(String, String)>,
) -> Response {
    if !content::review_exists(&state.db, &review_id) {
        return not_found(format!("no review with id '{review_id}'"));
    }
    if !content::comment_exists(&state.db, &review_id, &comment_id) {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::toggle_comment_like(&conn, &me.id, &comment_id)
    };

    match result {
        Ok((liked, total)) => {
            let like_count = hydrate::like_count(total);
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

/// Write, rewrite, or clear the user's own review of a film.
///
/// A PUT rather than a POST because there is only ever one: the composer is
/// prefilled with `your_review` and saving replaces it. Blank deletes, which is
/// how the composer's "Remove" gets back to no review at all.
async fn write_review(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<ReviewRequest>,
) -> Response {
    let body = match validate_capped("body", &body.body, MAX_BODY_LEN) {
        Ok(body) => body,
        Err(message) => return bad_request(message),
    };

    let result = {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        db::set_user_review(&conn, &me.id, &id, &body)
    };

    match result {
        Ok(your_review) => {
            invalidate_feed(&state, &me.id).await;
            Json(ReviewState { movie_id: id, your_review }).into_response()
        }
        Err(error) => write_failed(error),
    }
}

/// Edit the profile bio. Blank restores the default rather than leaving the header
/// empty, and the response carries whichever of the two is now stored so the field
/// can't sit showing something the profile doesn't.
async fn edit_bio(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Json(body): Json<BioRequest>,
) -> Response {
    let bio = match validate_capped("bio", &body.bio, MAX_BIO_LEN) {
        Ok(bio) => bio,
        Err(message) => return bad_request(message),
    };

    match content::set_bio(&state.db, &me.id, &bio) {
        Ok(bio) => Json(BioState { bio }).into_response(),
        Err(error) => write_failed(error),
    }
}

/// Post a top-level comment. Returns the whole review so the thread, the
/// "Conversation (n)" heading and the new row all update from one response.
async fn post_comment(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
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
        if let Err(error) = db::add_comment(&conn, &me.id, &id, &body) {
            return write_failed(error);
        }
    }

    // Re-read rather than patch the copy in hand: the stored row is what the next
    // request will see, so returning anything else could disagree with it.
    match content::hydrated_review(&state.source, &state.db, Some(&me.id), &id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{id}'")),
    }
}

async fn post_reply(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
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
    // Only replies to comments that exist — otherwise the reply would be stored under
    // a key nothing renders and vanish silently. Anybody's comment will do: replying to
    // somebody else is what makes the thread a conversation.
    if !content::comment_exists(&state.db, &review_id, &comment_id) {
        return not_found(format!("no comment with id '{comment_id}' on '{review_id}'"));
    }

    {
        let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(error) = db::add_reply(&conn, &me.id, &review_id, &comment_id, &body) {
            return write_failed(error);
        }
    }

    match content::hydrated_review(&state.source, &state.db, Some(&me.id), &review_id).await {
        Some(review) => Json(review).into_response(),
        None => not_found(format!("no review with id '{review_id}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::http::{header, Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// The route table, in front of an in-memory database with the demo graph in it.
    ///
    /// Demo mode, so nothing here touches TMDB; no cache, so nothing touches Redis;
    /// Google configured but pointing nowhere, so the auth routes answer without a
    /// network call — every test below stops before the token exchange.
    fn app() -> (Router, AppState) {
        app_with(Some(auth::Google::testing()))
    }

    /// The same, with the credentials decided by the caller — `None` for a server
    /// where sign-in was never configured.
    fn app_with(google: Option<auth::Google>) -> (Router, AppState) {
        let conn = db::open(":memory:").expect("in-memory database");
        db::seed_graph(&conn, &db::demo_graph()).expect("a seeded graph");
        let state = AppState::new(
            content::Source::Demo { reason: "testing".into() },
            std::sync::Arc::new(std::sync::Mutex::new(conn)),
            cache::Cache::disabled(),
            google,
        );
        (router(state.clone()), state)
    }

    /// A stand-in for Google's two endpoints, on a port the OS picks.
    ///
    /// Enough to finish a sign-in: a token to present, and a profile to turn into an
    /// account. It exists so the success path — the exchange, the account, the session,
    /// the cookie, the redirect — is covered by something other than reading the code.
    async fn fake_google() -> String {
        let google = Router::new()
            .route(
                "/token",
                post(|| async {
                    Json(serde_json::json!({ "access_token": "an-access-token" }))
                }),
            )
            .route(
                "/userinfo",
                get(|| async {
                    Json(serde_json::json!({
                        "sub": "1001",
                        "email": "sam.okonkwo@example.com",
                        "name": "Sam Okonkwo",
                        "picture": "https://cdn.test/sam.jpg",
                    }))
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let address = listener.local_addr().expect("an address");
        tokio::spawn(async move {
            let _ = axum::serve(listener, google).await;
        });
        format!("http://{address}")
    }

    /// Walk a whole sign-in: start it, then hand the callback the state it issued.
    ///
    /// Returns the callback's response, so a test can read the redirect and the cookie
    /// off it.
    async fn complete_sign_in(app: &Router) -> axum::http::Response<Body> {
        let start = Request::builder().uri("/api/auth/google").body(Body::empty()).unwrap();
        let consent = app.clone().oneshot(start).await.expect("a redirect");
        let issued = consent
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|url| url.split("state=").nth(1))
            .and_then(|rest| rest.split('&').next())
            .expect("a state parameter")
            .to_string();

        let callback = Request::builder()
            .uri(format!("/api/auth/google/callback?code=a-code&state={issued}"))
            .body(Body::empty())
            .unwrap();
        app.clone().oneshot(callback).await.expect("a response")
    }

    /// Sign somebody in and hand back the cookie a browser would then send.
    fn sign_in(state: &AppState, sub: &str, handle: &str) -> String {
        let conn = state.db.lock().unwrap();
        let account = db::upsert_google_account(
            &conn,
            &db::GoogleAccount {
                sub: sub.into(),
                email: Some(format!("{handle}@example.com")),
                name: format!("{handle} the viewer"),
                avatar: Image::new("img/avatar-test.jpg", "A test avatar."),
                handle: handle.into(),
            },
        )
        .expect("an account");
        let token = format!("token-for-{sub}");
        db::create_session(&conn, &token, &account.id).expect("a session");
        format!("cj_session={token}")
    }

    /// One request, with an optional cookie and an optional JSON body.
    async fn call(
        app: &Router,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<&str>,
    ) -> (StatusCode, String) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let request = match body {
            Some(json) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        let response = app.clone().oneshot(request).await.expect("a response");
        let status = response.status();
        let bytes = response.into_body().collect().await.expect("a body").to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    /// Every write, with the smallest body each accepts. One list, so a route added
    /// without a session check is caught by the loop below rather than by a reviewer.
    const WRITES: [(Method, &str, Option<&str>); 10] = [
        (Method::POST, "/api/movies/le-souffle/watchlist", None),
        (Method::POST, "/api/movies/le-souffle/favorite", None),
        (Method::PUT, "/api/movies/le-souffle/rating", Some(r#"{"rating_half_stars":8}"#)),
        (Method::PUT, "/api/movies/le-souffle/review", Some(r#"{"body":"Words."}"#)),
        (Method::PUT, "/api/profile", Some(r#"{"bio":"Words."}"#)),
        (Method::POST, "/api/people/user-priyanaidu/follow", None),
        (Method::POST, "/api/reviews/user-elenarostova-le-souffle/like", None),
        (
            Method::POST,
            "/api/reviews/user-elenarostova-le-souffle/comments",
            Some(r#"{"body":"Words."}"#),
        ),
        (
            Method::POST,
            "/api/reviews/user-elenarostova-le-souffle/comments/comment-1/like",
            None,
        ),
        (
            Method::POST,
            "/api/reviews/user-elenarostova-le-souffle/comments/comment-1/replies",
            Some(r#"{"body":"Words."}"#),
        ),
    ];

    /// The hole this closes: before sign-in existed, every one of these accepted
    /// anybody. All of them must now refuse a request with no session.
    #[tokio::test]
    async fn every_write_without_a_session_is_a_401() {
        let (app, _) = app();

        for (method, uri, body) in WRITES {
            let (status, response) = call(&app, method.clone(), uri, None, body).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri} answered {status}");
            // The same `{ "error": … }` shape every other failure uses, so one client
            // path reads them all.
            assert!(response.contains("\"error\""), "{method} {uri} sent {response}");
        }

        // A cookie naming a session that does not exist is no session at all.
        let (status, _) =
            call(&app, Method::POST, WRITES[0].1, Some("cj_session=made-up"), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// And with a session all of them work, so the 401 above is the check rather than
    /// the route being broken.
    #[tokio::test]
    async fn every_write_with_a_session_is_accepted() {
        let (app, state) = app();
        let cookie = sign_in(&state, "1001", "testviewer");

        // Posted first, so the two `comment-1` routes have a comment to act on.
        let (status, _) = call(
            &app,
            Method::POST,
            "/api/reviews/user-elenarostova-le-souffle/comments",
            Some(&cookie),
            Some(r#"{"body":"Mine."}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        for (method, uri, body) in WRITES {
            let (status, response) = call(&app, method.clone(), uri, Some(&cookie), body).await;
            assert_eq!(status, StatusCode::OK, "{method} {uri} answered {status}: {response}");
        }
    }

    /// Reads stay public. The site is browsable with no account, against an empty
    /// `Store` — so the buttons read as untouched rather than as somebody else's.
    #[tokio::test]
    async fn an_anonymous_reader_can_still_browse() {
        let (app, _) = app();

        for uri in [
            "/api/status",
            "/api/feed",
            "/api/feed/mobile",
            "/api/reviews",
            "/api/people?q=elena",
            "/api/people/elenarostova",
            "/api/movies/le-souffle",
            "/api/movies/le-souffle/reviews",
            "/api/collections/watchlist",
            "/api/reviews/user-elenarostova-le-souffle",
        ] {
            let (status, _) = call(&app, Method::GET, uri, None, None).await;
            assert_eq!(status, StatusCode::OK, "{uri} answered {status}");
        }

        // The feed has content — seeded reviews are public — and no personal deltas on
        // it. `on_watchlist` false everywhere is the empty store showing through.
        let (_, feed) = call(&app, Method::GET, "/api/feed", None, None).await;
        let page: FeedPage = serde_json::from_str(&feed).expect("a feed page");
        assert!(!page.items.is_empty(), "a signed-out home page with nothing on it");
        assert!(page.items.iter().all(|item| match item {
            FeedItem::Entry(entry) => !entry.on_watchlist,
            FeedItem::Recommendation(rec) => !rec.on_watchlist,
            FeedItem::Review(_) => true,
        }));

        // The two reads that are somebody's own pages rather than content.
        for uri in ["/api/profile", "/api/watchlist"] {
            let (status, _) = call(&app, Method::GET, uri, None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} answered {status}");
        }
    }

    /// The isolation the whole feature rests on, end to end over HTTP: what one
    /// browser writes must not appear in another's.
    #[tokio::test]
    async fn two_users_watchlists_do_not_bleed_into_each_other() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let ada = sign_in(&state, "2002", "ada");

        let (status, _) =
            call(&app, Method::POST, "/api/movies/le-souffle/watchlist", Some(&sam), None).await;
        assert_eq!(status, StatusCode::OK);

        let watchlist = |cookie: &str| {
            let app = app.clone();
            let cookie = cookie.to_string();
            async move { call(&app, Method::GET, "/api/watchlist", Some(&cookie), None).await.1 }
        };
        assert_eq!(watchlist(&sam).await, r#"["le-souffle"]"#);
        assert_eq!(watchlist(&ada).await, "[]", "one account read another's watchlist");

        // The same film, from the other account: both may have it, and each sees only
        // their own row.
        let (status, _) =
            call(&app, Method::POST, "/api/movies/le-souffle/watchlist", Some(&ada), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(watchlist(&ada).await, r#"["le-souffle"]"#);

        // And the flag on a shared page follows the reader, not the last writer.
        let flagged = |cookie: Option<&str>| {
            let app = app.clone();
            let cookie = cookie.map(str::to_string);
            async move {
                let (_, body) =
                    call(&app, Method::GET, "/api/movies/red-shift", cookie.as_deref(), None).await;
                // As JSON rather than as `MovieDetail`, which is serialize-only.
                serde_json::from_str::<serde_json::Value>(&body).expect("a film")["on_watchlist"]
                    .as_bool()
                    .expect("a flag")
            }
        };
        call(&app, Method::POST, "/api/movies/red-shift/watchlist", Some(&sam), None).await;
        assert!(flagged(Some(&sam)).await);
        assert!(!flagged(Some(&ada)).await);
        assert!(!flagged(None).await);
    }

    /// The profile is the signed-in account's own, not a shared one.
    #[tokio::test]
    async fn each_account_gets_its_own_profile() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let ada = sign_in(&state, "2002", "ada");

        let handle = |cookie: &str| {
            let app = app.clone();
            let cookie = cookie.to_string();
            async move {
                let (status, body) =
                    call(&app, Method::GET, "/api/profile", Some(&cookie), None).await;
                assert_eq!(status, StatusCode::OK);
                serde_json::from_str::<serde_json::Value>(&body).expect("a profile")["handle"]
                    .as_str()
                    .expect("a handle")
                    .to_string()
            }
        };

        assert_eq!(handle(&sam).await, "@sam");
        assert_eq!(handle(&ada).await, "@ada");

        // `/api/auth/me` agrees with it, and is a 401 for a reader with no session.
        let (status, me) = call(&app, Method::GET, "/api/auth/me", Some(&sam), None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(me.contains("@sam"));
        let (status, body) = call(&app, Method::GET, "/api/auth/me", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("\"error\""));
    }

    /// Logging out revokes: the session row goes, so the same cookie stops working
    /// even though the browser might still be holding it.
    #[tokio::test]
    async fn logging_out_revokes_the_session_and_clears_the_cookie() {
        let (app, state) = app();
        let cookie = sign_in(&state, "1001", "sam");

        let (status, _) = call(&app, Method::GET, "/api/profile", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::OK);

        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/auth/logout")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // The browser is told to drop it.
        let cleared = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a Set-Cookie")
            .to_str()
            .unwrap()
            .to_string();
        assert!(cleared.contains("cj_session="));
        assert!(cleared.contains("Max-Age=0"));
        assert!(cleared.contains("HttpOnly"));

        // And the token itself is dead, which is the part a signed cookie could not do.
        let (status, _) = call(&app, Method::GET, "/api/profile", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) =
            call(&app, Method::POST, "/api/movies/le-souffle/watchlist", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Logging out again is fine: the caller wanted to be signed out and they are.
        let (status, _) = call(&app, Method::POST, "/api/auth/logout", None, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    /// Where a failed callback sends the browser.
    ///
    /// Every outcome of the callback is a 302, so a test reads the `Location` rather
    /// than a status and a body.
    async fn callback_location(app: &Router, query: &str) -> String {
        let request = Request::builder()
            .uri(format!("/api/auth/google/callback?{query}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.expect("a response");
        assert_eq!(response.status(), StatusCode::FOUND, "the callback answered with a body");
        // A failed sign-in must not touch a session the browser already holds.
        assert!(
            response.headers().get(header::SET_COOKIE).is_none(),
            "a failed callback set a cookie"
        );
        response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string()
    }

    /// The CSRF check, over HTTP. A callback carrying a `state` this server never
    /// issued must be refused before anything is exchanged or written.
    #[tokio::test]
    async fn the_callback_refuses_a_state_it_did_not_issue() {
        let (app, state) = app();

        let landed = callback_location(&app, "code=a-code&state=not-ours").await;
        assert_eq!(landed, "http://localhost:5173/?auth_error=expired");
        // No session was created on the way past.
        let sessions: i64 = {
            let conn = state.db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap()
        };
        assert_eq!(sessions, 0);

        // A state that *was* issued but is presented twice is refused the second time,
        // which is the replay case.
        {
            let conn = state.db.lock().unwrap();
            db::remember_auth_state(&conn, "ours").unwrap();
            assert!(db::consume_auth_state(&conn, "ours").unwrap());
        }
        let landed = callback_location(&app, "code=a-code&state=ours").await;
        assert!(landed.ends_with("?auth_error=expired"), "{landed}");
    }

    /// The four slugs, each from the thing that produces it. They are a contract with
    /// the frontend, which renders a message per slug, so the spelling is the test.
    #[tokio::test]
    async fn every_callback_failure_names_itself_in_one_of_four_slugs() {
        let (app, state) = app();

        for (query, slug) in [
            // The user pressed cancel at Google's consent screen.
            ("error=access_denied", "cancelled"),
            // Google refused for some reason of its own.
            ("error=invalid_scope", "denied"),
            ("error=admin_policy_enforced&error_description=nope", "denied"),
            // A stale or hand-edited callback: no state to check, or one nobody issued.
            ("code=a-code", "expired"),
            ("state=only-a-state", "expired"),
            ("code=a-code&state=never-issued", "expired"),
        ] {
            let landed = callback_location(&app, query).await;
            assert_eq!(
                landed,
                format!("http://localhost:5173/?auth_error={slug}"),
                "{query} landed on {landed}"
            );
        }

        // `failed` is the exchange going wrong, which needs a state that really was
        // issued. `Google::testing` points the token endpoint at a closed port, so the
        // request is refused rather than reaching Google.
        {
            let conn = state.db.lock().unwrap();
            db::remember_auth_state(&conn, "issued").unwrap();
        }
        let landed = callback_location(&app, "code=a-code&state=issued").await;
        assert_eq!(landed, "http://localhost:5173/?auth_error=failed");
        // And that state was spent on the way through, so a retry of the same URL is
        // `expired` rather than another attempt at the exchange.
        let landed = callback_location(&app, "code=a-code&state=issued").await;
        assert!(landed.ends_with("?auth_error=expired"), "{landed}");

        // Nothing was written by any of it.
        let conn = state.db.lock().unwrap();
        for table in ["sessions", "auth_states"] {
            let rows: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(rows, 0, "{table} was left with something in it");
        }
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM people WHERE is_account = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(accounts, 0, "a failed sign-in created an account");
    }

    /// A failed callback carries the slug and nothing else. A query parameter is
    /// visible, shareable and logged by every proxy in between, so Google's own error
    /// text and any internal detail stay out of it.
    #[tokio::test]
    async fn a_failed_callback_leaks_nothing_into_the_url() {
        let (app, _) = app();

        let landed = callback_location(
            &app,
            "error=server_error&error_description=Something%20internal%20broke\
             &error_uri=https%3A%2F%2Fgoogle.test%2Fwhy\
             &state=secret-state&code=secret-code",
        )
        .await;

        assert_eq!(landed, "http://localhost:5173/?auth_error=denied");
        for leaked in
            ["Something", "internal", "google.test", "secret-state", "secret-code", "server_error"]
        {
            assert!(!landed.contains(leaked), "{leaked:?} reached the URL: {landed}");
        }
    }

    /// On a server with no credentials a hand-typed callback still lands in the app
    /// rather than on a JSON page. Relative, because there is no `PUBLIC_URL` to build
    /// an absolute one from.
    #[tokio::test]
    async fn a_callback_without_credentials_still_lands_in_the_app() {
        let (app, _) = app_with(None);
        assert_eq!(callback_location(&app, "code=a-code&state=x").await, "/?auth_error=failed");
    }

    /// Starting a sign-in redirects to Google with a state it has stored, and that
    /// state is what the callback above checks against.
    #[tokio::test]
    async fn starting_a_sign_in_stores_the_state_it_sends() {
        let (app, state) = app();

        let request =
            Request::builder().uri("/api/auth/google").body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);

        let location =
            response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
        assert!(location.starts_with("https://accounts.google.com/"), "{location}");
        assert!(location.contains("response_type=code"));
        assert!(location.contains("client_id=test-client-id"));
        // Percent-encoded, or Google reads a truncated parameter.
        assert!(location.contains("redirect_uri=http%3A%2F%2Flocalhost%3A5173%2Fapi%2Fauth%2Fgoogle%2Fcallback"));

        // The state on the URL is the one in the table, so the callback can spend it.
        let sent = location
            .split("state=")
            .nth(1)
            .and_then(|rest| rest.split('&').next())
            .expect("a state parameter")
            .to_string();
        assert!(!sent.is_empty());
        let conn = state.db.lock().unwrap();
        assert!(db::consume_auth_state(&conn, &sent).unwrap(), "the state was not stored");

        // Nothing about the credentials is in the redirect but the client id, which is
        // public by design — the secret must never leave the process.
        assert!(!location.contains("test-client-secret"));
    }

    /// A review written by an account has to reach the people who follow them. That
    /// is the product: it used to be written, stored, and seen by nobody.
    #[tokio::test]
    async fn an_accounts_review_reaches_a_followers_feed_and_their_page() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let ada = sign_in(&state, "2002", "ada");

        // Ada follows Sam, then Sam writes about a film and scores it.
        let (status, _) =
            call(&app, Method::POST, "/api/people/account-1001/follow", Some(&ada), None).await;
        assert_eq!(status, StatusCode::OK);
        for (uri, body) in [
            ("/api/movies/le-souffle/rating", r#"{"rating_half_stars":9}"#),
            ("/api/movies/le-souffle/review", r#"{"body":"The café scene, forever."}"#),
        ] {
            let (status, _) = call(&app, Method::PUT, uri, Some(&sam), Some(body)).await;
            assert_eq!(status, StatusCode::OK);
        }

        // Sam's public page, as Ada sees it.
        let (status, page) =
            call(&app, Method::GET, "/api/people/sam", Some(&ada), None).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["review_count"], 1, "their page prints a count of nothing");
        let listed = page["reviews"].as_array().expect("reviews");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["body"], "The café scene, forever.");
        assert_eq!(listed[0]["rating_half_stars"], 9);
        // Credited to them by name, nickname and face — not to "the visitor".
        assert_eq!(listed[0]["author_handle"], "@sam");
        assert_eq!(listed[0]["author_id"], "account-1001");
        assert!(listed[0]["author_avatar"]["src"].as_str().is_some());

        // Ada's feed, which is why she followed them.
        let (_, feed) = call(&app, Method::GET, "/api/feed", Some(&ada), None).await;
        let feed: serde_json::Value = serde_json::from_str(&feed).unwrap();
        let by_sam: Vec<&serde_json::Value> = feed["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["kind"] == "review" && item["author_id"] == "account-1001")
            .collect();
        assert_eq!(by_sam.len(), 1, "a follower's feed misses the review");
        assert_eq!(by_sam[0]["author_followed"], true);

        // The film's own page lists it beside the seeded people's.
        let (_, on_film) =
            call(&app, Method::GET, "/api/movies/le-souffle/reviews", Some(&ada), None).await;
        let on_film: serde_json::Value = serde_json::from_str(&on_film).unwrap();
        let authors: Vec<&str> =
            on_film.as_array().unwrap().iter().filter_map(|r| r["author_id"].as_str()).collect();
        assert!(authors.contains(&"account-1001"));
        assert!(authors.len() > 1, "the seeded reviews of this film went missing");

        // And the review page opens from the id the card carries, for anybody —
        // including a reader with no account.
        let id = listed[0]["id"].as_str().unwrap().to_string();
        for cookie in [Some(ada.as_str()), None] {
            let (status, _) =
                call(&app, Method::GET, &format!("/api/reviews/{id}"), cookie, None).await;
            assert_eq!(status, StatusCode::OK);
        }
    }

    /// Prose with no score publishes as `null` rather than as zero stars, which would
    /// read as a one-out-of-five verdict.
    #[tokio::test]
    async fn a_review_written_without_a_score_publishes_a_null_rating() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");

        let (status, _) = call(
            &app,
            Method::PUT,
            "/api/movies/endless/review",
            Some(&sam),
            Some(r#"{"body":"No stars from me."}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (_, page) = call(&app, Method::GET, "/api/people/sam", None, None).await;
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["reviews"][0]["rating_half_stars"], serde_json::Value::Null);
    }

    /// The other half of the product: a conversation. Two accounts on one review,
    /// each seeing the other, each able to reply to and like what the other wrote.
    #[tokio::test]
    async fn two_users_share_one_comment_thread() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let ada = sign_in(&state, "2002", "ada");
        let review = "/api/reviews/user-elenarostova-le-souffle";

        let post = |cookie: &str, uri: String, body: String| {
            let app = app.clone();
            let cookie = cookie.to_string();
            async move {
                let (status, body) =
                    call(&app, Method::POST, &uri, Some(&cookie), Some(&body)).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                serde_json::from_str::<serde_json::Value>(&body).expect("a review")
            }
        };

        // Sam comments; the response is the whole review, thread included.
        let after = post(&sam, format!("{review}/comments"), r#"{"body":"Loved it."}"#.into()).await;
        let comments = after["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 1);
        let sams_comment = comments[0]["id"].as_str().unwrap().to_string();
        assert_eq!(comments[0]["author_handle"], "@sam");
        assert_eq!(comments[0]["is_you"], true, "the author must be told it is theirs");
        assert_eq!(comments[0]["author_name"], "sam the viewer", "the real name, not 'You'");
        // A real date, where every comment used to read "Just now".
        assert!(comments[0]["timestamp"].as_str().unwrap().contains(','));

        // Ada sees it, and it is not hers.
        let (_, seen) = call(&app, Method::GET, review, Some(&ada), None).await;
        let seen: serde_json::Value = serde_json::from_str(&seen).unwrap();
        assert_eq!(seen["comments"].as_array().unwrap().len(), 1, "one user cannot see another's");
        assert_eq!(seen["comments"][0]["is_you"], false);
        assert_eq!(seen["comments"][0]["author_handle"], "@sam");

        // Ada replies to Sam's comment, which used to 404.
        let after = post(
            &ada,
            format!("{review}/comments/{sams_comment}/replies"),
            r#"{"body":"Overrated."}"#.into(),
        )
        .await;
        let replies = after["comments"][0]["replies"].as_array().unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["author_handle"], "@ada");
        assert_eq!(replies[0]["is_you"], true, "the reply is Ada's, seen by Ada");
        // The comment above it is still Sam's, on the same page.
        assert_eq!(after["comments"][0]["is_you"], false);

        // Ada also comments in her own right, and the thread stays oldest-first.
        let after = post(&ada, format!("{review}/comments"), r#"{"body":"Second thoughts."}"#.into())
            .await;
        let bodies: Vec<&str> = after["comments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["body"].as_str().unwrap())
            .collect();
        assert_eq!(bodies, ["Loved it.", "Second thoughts."]);
    }

    /// Like counts are everybody's. Two people liking one comment must read 2 to both
    /// of them, not 1 each — which is what a per-viewer count did.
    #[tokio::test]
    async fn a_like_count_is_shared_and_the_heart_is_not() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let ada = sign_in(&state, "2002", "ada");
        let review = "/api/reviews/user-elenarostova-le-souffle";

        let (_, posted) = call(
            &app,
            Method::POST,
            &format!("{review}/comments"),
            Some(&sam),
            Some(r#"{"body":"Worth it."}"#),
        )
        .await;
        let posted: serde_json::Value = serde_json::from_str(&posted).unwrap();
        let comment = posted["comments"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(posted["comments"][0]["like_count"], serde_json::Value::Null);

        // Each press reports the new total, the presser's own state alongside it.
        let like = |cookie: &str, uri: String| {
            let app = app.clone();
            let cookie = cookie.to_string();
            async move {
                let (status, body) = call(&app, Method::POST, &uri, Some(&cookie), None).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                serde_json::from_str::<serde_json::Value>(&body).expect("a like state")
            }
        };

        let first = like(&sam, format!("{review}/comments/{comment}/like")).await;
        assert_eq!((first["liked"].as_bool(), first["like_count"].as_u64()), (Some(true), Some(1)));
        let second = like(&ada, format!("{review}/comments/{comment}/like")).await;
        assert_eq!((second["liked"].as_bool(), second["like_count"].as_u64()), (Some(true), Some(2)));

        // Both of them read 2, and both see their own heart filled.
        for cookie in [&sam, &ada] {
            let (_, page) = call(&app, Method::GET, review, Some(cookie), None).await;
            let page: serde_json::Value = serde_json::from_str(&page).unwrap();
            assert_eq!(page["comments"][0]["like_count"], 2, "a shared count read as 1");
            assert_eq!(page["comments"][0]["liked"], true);
        }
        // A third reader sees the same 2 and an empty heart.
        let (_, page) = call(&app, Method::GET, review, None, None).await;
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["comments"][0]["like_count"], 2);
        assert_eq!(page["comments"][0]["liked"], false);

        // The review's own likes work the same way.
        let first = like(&sam, format!("{review}/like")).await;
        assert_eq!(first["like_count"], 1);
        let second = like(&ada, format!("{review}/like")).await;
        assert_eq!(second["like_count"], 2);
        let (_, page) = call(&app, Method::GET, review, Some(&sam), None).await;
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["like_count"], 2);
        assert_eq!(page["liked"], true);

        // Unliking takes one away rather than clearing the row for everybody.
        let undone = like(&sam, format!("{review}/comments/{comment}/like")).await;
        assert_eq!((undone["liked"].as_bool(), undone["like_count"].as_u64()), (Some(false), Some(1)));
    }

    /// A signed-out reader can follow a conversation. Joining it is what needs an
    /// account.
    #[tokio::test]
    async fn anonymous_can_read_a_thread_but_not_post_to_it() {
        let (app, state) = app();
        let sam = sign_in(&state, "1001", "sam");
        let review = "/api/reviews/user-elenarostova-le-souffle";

        let (_, posted) = call(
            &app,
            Method::POST,
            &format!("{review}/comments"),
            Some(&sam),
            Some(r#"{"body":"Read this, stranger."}"#),
        )
        .await;
        let comment = serde_json::from_str::<serde_json::Value>(&posted).unwrap()["comments"][0]
            ["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Readable, in full, with its author credited and nothing claimed as the
        // reader's own.
        let (status, page) = call(&app, Method::GET, review, None, None).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["comments"].as_array().unwrap().len(), 1);
        assert_eq!(page["comments"][0]["body"], "Read this, stranger.");
        assert_eq!(page["comments"][0]["author_handle"], "@sam");
        assert_eq!(page["comments"][0]["is_you"], false);
        assert_eq!(page["comments"][0]["liked"], false);

        // But not writable, at any of the three ways in.
        for (uri, body) in [
            (format!("{review}/comments"), Some(r#"{"body":"Me too."}"#)),
            (format!("{review}/comments/{comment}/replies"), Some(r#"{"body":"Me too."}"#)),
            (format!("{review}/comments/{comment}/like"), None),
        ] {
            let (status, answer) = call(&app, Method::POST, &uri, None, body).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} answered {status}");
            assert!(answer.contains("\"error\""));
        }

        // And nothing was written on the way past.
        let (_, page) = call(&app, Method::GET, review, None, None).await;
        let page: serde_json::Value = serde_json::from_str(&page).unwrap();
        assert_eq!(page["comments"].as_array().unwrap().len(), 1);
        assert!(page["comments"][0]["replies"].as_array().unwrap().is_empty());
        assert_eq!(page["comments"][0]["like_count"], serde_json::Value::Null);
    }

    /// The flag a client reads instead of probing `/api/auth/google` on every press.
    ///
    /// Whether, never what: the client id appears in the consent URL but has no
    /// business in a status payload, and the secret has no business anywhere.
    #[tokio::test]
    async fn the_status_says_whether_sign_in_is_available() {
        let (app, _) = app();

        let (status, body) = call(&app, Method::GET, "/api/status", None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.contains("test-client-id") && !body.contains("test-client-secret"));

        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["sign_in"], "google");
        // Additive: the fields the demo banner already reads are untouched.
        assert_eq!(body["data_source"], "demo");
        assert!(body["message"].is_string());
        assert!(body["docs_url"].is_string());
    }

    /// The whole flow, end to end against a stub Google: consent, exchange, profile,
    /// account, session, cookie, and home.
    ///
    /// The one outcome that carries a cookie, and the one that lands on `/` with no
    /// slug on it.
    #[tokio::test]
    async fn a_finished_sign_in_lands_home_with_the_cookie() {
        let base = fake_google().await;
        let (app, state) = app_with(Some(auth::Google::testing_at(&base)));

        let response = complete_sign_in(&app).await;
        assert_eq!(response.status(), StatusCode::FOUND);
        let landed = response.headers().get(header::LOCATION).unwrap().to_str().unwrap();
        assert_eq!(landed, "http://localhost:5173/");
        assert!(!landed.contains("auth_error"), "a finished sign-in reported an error");

        // The cookie, with every attribute that keeps it from being read or replayed.
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a session cookie")
            .to_str()
            .unwrap()
            .to_string();
        for flag in ["cj_session=", "HttpOnly", "SameSite=Lax", "Path=/", "Max-Age="] {
            assert!(cookie.contains(flag), "{flag} is missing from {cookie}");
        }
        // Plain-HTTP localhost, so no `Secure` — see `auth::Google::secure_cookie`.
        assert!(!cookie.contains("Secure"));

        // And it is a session that works, on the account Google described.
        let token = cookie.split(';').next().unwrap().to_string();
        let (status, me) = call(&app, Method::GET, "/api/auth/me", Some(&token), None).await;
        assert_eq!(status, StatusCode::OK);
        let me: serde_json::Value = serde_json::from_str(&me).unwrap();
        assert_eq!(me["name"], "Sam Okonkwo");
        // The nickname comes from the email's local part, cleaned.
        assert_eq!(me["handle"], "@samokonkwo");
        assert_eq!(me["id"], "account-1001");
        assert_eq!(me["avatar"]["src"], "https://cdn.test/sam.jpg");
        // No email on the wire, even though Google sent one.
        assert!(!me.as_object().unwrap().contains_key("email"));

        // A write works now, which is the point of the whole exercise.
        let (status, _) =
            call(&app, Method::POST, "/api/movies/le-souffle/watchlist", Some(&token), None).await;
        assert_eq!(status, StatusCode::OK);

        // The state was spent, so the same callback URL cannot be replayed into a
        // second session.
        let sessions: i64 = {
            let conn = state.db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap()
        };
        assert_eq!(sessions, 1);

        // Signing in again finds the same account rather than making a second one.
        let response = complete_sign_in(&app).await;
        assert_eq!(response.status(), StatusCode::FOUND);
        let conn = state.db.lock().unwrap();
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM people WHERE is_account = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(accounts, 1);
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .unwrap(),
            2,
            "a second browser gets a second session, not a replaced one"
        );
    }

    /// With no credentials the server still runs: reads work, writes 401, and the
    /// sign-in button gets a straight answer rather than a broken redirect.
    #[tokio::test]
    async fn without_google_credentials_sign_in_says_so() {
        let (app, _) = app_with(None);

        let (status, _) = call(&app, Method::GET, "/api/feed", None, None).await;
        assert_eq!(status, StatusCode::OK, "reads must survive having no sign-in");

        // `/api/status` says so, which is how a client knows not to draw the button.
        let (status, body) = call(&app, Method::GET, "/api/status", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["sign_in"], serde_json::Value::Null);

        // Pressing it anyway is still a straight answer rather than a broken redirect.
        // Deliberately a 503 and not a 302: a client that still probes this endpoint
        // reads a 302 as "sign-in works".
        let (status, body) = call(&app, Method::GET, "/api/auth/google", None, None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("not configured"));

        let (status, _) =
            call(&app, Method::POST, "/api/movies/le-souffle/watchlist", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Logout still clears the cookie, so a session from before the credentials were
        // removed can still be ended.
        let (status, _) = call(&app, Method::POST, "/api/auth/logout", None, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
}
