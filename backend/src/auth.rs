//! Google sign-in, sessions, and the two extractors every handler goes through.
//!
//! The flow is OAuth 2.0's authorization code flow, run entirely on the server:
//!
//! 1. `GET /api/auth/google` mints a random `state`, stores it, and redirects to
//!    Google's consent screen.
//! 2. Google sends the browser back to `GET /api/auth/google/callback` with a code
//!    and that `state`.
//! 3. The `state` is spent — see `db::consume_auth_state`. A value that was never
//!    issued, was already used, or is stale is refused here and nothing else runs.
//!    This is the CSRF check, and it is not optional: without it anyone could hand a
//!    victim's browser a callback URL carrying *their* code and log the victim into
//!    the attacker's account.
//! 4. The code is exchanged for an access token, the token buys the user's profile,
//!    and the profile becomes a `people` row.
//! 5. A session row is written and its token goes out as a cookie.
//!
//! **The session is a row, not a signed cookie.** The cookie holds nothing but an
//! opaque random token; everything about the user is looked up. That is what makes
//! logout a revocation: deleting the row ends the session for every browser holding
//! the token, which a self-contained cookie cannot do without a blocklist that is
//! this table under a worse name.
//!
//! **Nothing here logs a secret.** Not the client id, not the client secret, not a
//! session token, not an authorization code — the same rule `content::Source` follows
//! for the TMDB token and `cache::Cache` for the Redis URL. Log lines say what
//! happened, never with what.
//!
//! Sign-in is optional. With no `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` the
//! server boots, says so once, serves every read, and refuses every write with a 401
//! — the same degrade-don't-die posture as a missing Redis.

use axum::{
    extract::{Query, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::models::Image;
use crate::state::AppState;

/// The cookie the session token rides in.
const COOKIE: &str = "cj_session";

/// How long the cookie is offered for, in seconds. Matches the row's own lifetime in
/// `db`; the row is what actually decides, so a cookie outliving it would only mean
/// one wasted request.
const COOKIE_MAX_AGE: u32 = 30 * 24 * 60 * 60;

/// Where the sign-in starts.
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

/// Where a code is exchanged for a token.
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Where the token buys a profile.
///
/// Google also returns an `id_token`, a signed JWT carrying the same fields. This
/// endpoint is used instead because verifying that JWT properly means fetching and
/// caching Google's rotating public keys, and the fields are identical — over a
/// channel we opened ourselves to a host we pinned, with a token we just received.
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

/// What is asked of Google: who you are and your email address. Nothing else — no
/// contacts, no calendar, no offline access.
const SCOPE: &str = "openid email profile";

/// How long to wait on Google before giving up.
///
/// The user is watching a blank tab while this runs, so a slow upstream has to
/// become an error rather than an indefinite wait.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The three environment variables. Named here so the log lines and this file's
/// documentation cannot drift.
const CLIENT_ID_VAR: &str = "GOOGLE_CLIENT_ID";
const CLIENT_SECRET_VAR: &str = "GOOGLE_CLIENT_SECRET";
const PUBLIC_URL_VAR: &str = "PUBLIC_URL";

/// Where the app is reachable, when `PUBLIC_URL` doesn't say.
///
/// The Vite dev server, not the API's own port: `vite.config.ts` proxies `/api`, so
/// the browser only ever sees this origin in development.
const DEFAULT_PUBLIC_URL: &str = "http://localhost:5173";

/// The path Google is told to come back to. Also the value to register in the Google
/// Cloud console, appended to `PUBLIC_URL`.
const CALLBACK_PATH: &str = "/api/auth/google/callback";

/// How many random bytes a session token and a `state` value each carry.
const TOKEN_BYTES: usize = 32;

/// A random hex string of `bytes` bytes, from the operating system.
///
/// `getrandom` rather than a seeded PRNG: a session token is a bearer credential, so
/// guessing one has to be as hard as guessing a password. A failure here is fatal on
/// purpose — the alternative is falling back to something predictable, which would
/// turn "the OS has no entropy source" into "anyone can forge a session".
pub fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the operating system has no random source");
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The Google credentials, or the absence of them.
///
/// Holds no `reqwest::Client` of its own; `AppState` has nothing to share one with,
/// and sign-in happens once per user rather than once per request.
#[derive(Clone)]
pub struct Google {
    /// Public by design — it appears in the consent URL the browser follows — but
    /// still never logged, because a log line with one credential in it invites the
    /// next one.
    client_id: String,
    client_secret: String,
    /// Where the app is reachable, without a trailing slash. The redirect URI and
    /// the post-sign-in landing page are both built from it.
    public_url: String,
}

impl Google {
    /// Read the credentials, or decide to do without them.
    ///
    /// Absent means sign-in is off and every write is a 401, which is a usable
    /// server: reads are public. Refusing to boot would make a browsable demo
    /// impossible to run without a Google project.
    pub fn from_env() -> Option<Self> {
        let public_url = std::env::var(PUBLIC_URL_VAR).unwrap_or_default().trim().to_string();
        let client_id = std::env::var(CLIENT_ID_VAR).unwrap_or_default().trim().to_string();
        let client_secret =
            std::env::var(CLIENT_SECRET_VAR).unwrap_or_default().trim().to_string();

        if client_id.is_empty() || client_secret.is_empty() {
            tracing::info!(
                "google sign-in: disabled (no {CLIENT_ID_VAR}/{CLIENT_SECRET_VAR}) \
                 — reads are public, writes will 401"
            );
            return None;
        }

        let public_url = if public_url.is_empty() {
            tracing::warn!(
                "google sign-in: no {PUBLIC_URL_VAR}, assuming {DEFAULT_PUBLIC_URL}"
            );
            DEFAULT_PUBLIC_URL.to_string()
        } else {
            public_url.trim_end_matches('/').to_string()
        };

        // The redirect URI is not a secret, and it is the one thing that has to match
        // the Google console exactly — so it is worth printing, unlike everything
        // else here.
        tracing::info!(
            redirect_uri = %format!("{public_url}{CALLBACK_PATH}"),
            "google sign-in: enabled"
        );
        Some(Self { client_id, client_secret, public_url })
    }

    /// Credentials that are configured but point nowhere, for tests.
    ///
    /// Not `from_env`: environment variables are process-wide and the test suite runs
    /// in parallel, so one test setting them would decide another test's outcome.
    /// Nothing built this way ever reaches the network — the tests stop at the state
    /// check, which is before the token exchange.
    #[cfg(test)]
    pub fn testing() -> Self {
        Self {
            client_id: "test-client-id".into(),
            client_secret: "test-client-secret".into(),
            public_url: "http://localhost:5173".into(),
        }
    }

    fn redirect_uri(&self) -> String {
        format!("{}{CALLBACK_PATH}", self.public_url)
    }

    /// Whether the session cookie should carry `Secure`.
    ///
    /// Yes in production, which is every deployment: Caddy terminates TLS and
    /// `PUBLIC_URL` is `https://…`. No when the app is served over plain HTTP, which
    /// is only ever localhost — a `Secure` cookie there is one browsers may drop, and
    /// a sign-in that silently does nothing is worse than an insecure local cookie.
    fn secure_cookie(&self) -> bool {
        !self.public_url.starts_with("http://")
    }
}

// --- The current user ---------------------------------------------------------

/// Whoever is making this request, when anybody is.
///
/// The wire shape of `GET /api/auth/me`, and the thing every handler keys its
/// database work on. No email on the wire: the frontend has no use for it and it is
/// the one field here that is worth not shipping to a browser.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: String,
    pub name: String,
    /// "@sam", with the sigil, as every other handle in the API carries it.
    pub handle: String,
    pub avatar: Image,
}

impl From<db::AccountRow> for User {
    fn from(row: db::AccountRow) -> Self {
        Self { id: row.id, name: row.name, handle: row.handle, avatar: row.avatar }
    }
}

/// A request with a valid session — what every write extracts.
///
/// Rejects with 401 when there is none, so no handler can forget the check: a write
/// that took `Viewer` instead would compile and quietly accept anonymous traffic.
pub struct CurrentUser(pub User);

/// A request that may or may not have a session — what every read extracts.
///
/// `None` is a normal, expected state, not an error: the site is browsable without
/// an account, and an anonymous reader hydrates against an empty `Store`.
pub struct Viewer(pub Option<User>);

impl Viewer {
    /// The id to scope database work to, or `None` for an anonymous reader.
    pub fn id(&self) -> Option<&str> {
        self.0.as_ref().map(|user| user.id.as_str())
    }
}

/// Look the session up, if the request carries one.
///
/// A cookie naming a session that has expired or been revoked is treated exactly
/// like no cookie at all. So is a database error: an unreachable database makes a
/// request anonymous, which downgrades a signed-in reader to a public one rather than
/// failing the page.
fn viewer_of(parts: &Parts, state: &AppState) -> Option<User> {
    let token = cookie(parts, COOKIE)?;
    let conn = state.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match db::session_account(&conn, &token) {
        Ok(account) => account.map(User::from),
        Err(error) => {
            tracing::error!(%error, "could not read a session");
            None
        }
    }
}

impl axum::extract::FromRequestParts<AppState> for Viewer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(viewer_of(parts, state)))
    }
}

impl axum::extract::FromRequestParts<AppState> for CurrentUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        viewer_of(parts, state).map(Self).ok_or_else(unauthorized)
    }
}

/// The one 401 body in the API.
///
/// Deliberately the same `{ "error": … }` shape every other failure uses, so a
/// client can read the status and the body the same way whatever went wrong.
pub fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(Error { error: "sign in to do that".into() })).into_response()
}

#[derive(Serialize)]
struct Error {
    error: String,
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(Error { error: message.into() })).into_response()
}

/// One named cookie out of the request, if it is there.
///
/// Hand-parsed rather than through a cookie layer: one cookie is read and one is
/// written in the whole application, and `Cookie:` is a `;`-separated list of
/// `name=value` pairs. A malformed pair is skipped rather than failing the request,
/// because other software sets cookies on this host too.
fn cookie(parts: &Parts, name: &str) -> Option<String> {
    let header = parts.headers.get(header::COOKIE)?.to_str().ok()?;
    header
        .split(';')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().to_string())
}

/// The `Set-Cookie` value that starts a session.
///
/// `HttpOnly` so no script can read the token, `SameSite=Lax` so it rides a
/// top-level navigation (which is how the OAuth callback arrives) but not a
/// cross-site form post, `Path=/` because every endpoint needs it, and `Secure`
/// everywhere but plain-HTTP localhost — see `Google::secure_cookie`.
fn session_cookie(token: &str, secure: bool) -> String {
    let flags = if secure { "; Secure" } else { "" };
    format!("{COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={COOKIE_MAX_AGE}{flags}")
}

/// The `Set-Cookie` value that ends one. Same attributes, no lifetime: a cookie has
/// to match on path and flags to be replaced.
fn cleared_cookie(secure: bool) -> String {
    let flags = if secure { "; Secure" } else { "" };
    format!("{COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{flags}")
}

/// A 302 to `location`, with optional cookie work attached.
///
/// 302 rather than axum's `Redirect::to`, which sends 303: the browser is being sent
/// to Google and then to the app's own root, and 302 is what every OAuth
/// implementation and every proxy in front of one expects to see.
fn redirect(location: &str, set_cookie: Option<String>) -> Response {
    let mut response =
        Response::builder().status(StatusCode::FOUND).header(header::LOCATION, location);
    if let Some(cookie) = set_cookie {
        response = response.header(header::SET_COOKIE, cookie);
    }
    response
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// --- Handlers -----------------------------------------------------------------

/// `GET /api/auth/me` — who is signed in.
///
/// 401 when nobody is, rather than 200 with a null: the frontend's whole question is
/// "may I render the signed-in chrome", and the status answers it without the client
/// having to inspect a body.
pub async fn me(Viewer(user): Viewer) -> Response {
    match user {
        Some(user) => Json(user).into_response(),
        None => unauthorized(),
    }
}

/// `GET /api/auth/google` — off to the consent screen.
///
/// 503 when sign-in isn't configured, because that is a fact about the server rather
/// than a mistake by the caller, and the button that led here should say so.
pub async fn start(State(state): State<AppState>) -> Response {
    let Some(google) = state.google.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Error { error: "google sign-in is not configured on this server".into() }),
        )
            .into_response();
    };

    let csrf_state = random_token(TOKEN_BYTES);
    {
        let conn = state.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(error) = db::remember_auth_state(&conn, &csrf_state) {
            tracing::error!(%error, "could not store an auth state");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Error { error: "could not start sign-in — see the server log".into() }),
            )
                .into_response();
        }
    }

    let url = format!(
        "{GOOGLE_AUTH_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}\
         &access_type=online&prompt=select_account",
        urlencode(&google.client_id),
        urlencode(&google.redirect_uri()),
        urlencode(SCOPE),
        urlencode(&csrf_state),
    );
    redirect(&url, None)
}

/// What Google appends to the callback URL.
#[derive(Debug, Deserialize)]
pub struct Callback {
    /// The authorization code, absent when the user pressed "cancel".
    code: Option<String>,
    state: Option<String>,
    /// Google's own error slug — `access_denied` when consent was refused.
    error: Option<String>,
}

/// `GET /api/auth/google/callback` — finish the sign-in.
///
/// Every failure before the session exists is a 4xx or 5xx with a body, not a
/// redirect: a redirect to `/` would leave the user on a signed-out home page with no
/// idea why. Only success redirects.
pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<Callback>,
) -> Response {
    let Some(google) = state.google.as_ref() else {
        return bad_request("google sign-in is not configured on this server");
    };

    // The user declining is not an error worth a body full of detail, but it is not a
    // sign-in either.
    if let Some(error) = query.error.as_deref() {
        tracing::info!(error, "google sign-in was not completed");
        return bad_request("google sign-in was cancelled");
    }

    let (Some(code), Some(csrf_state)) = (query.code, query.state) else {
        return bad_request("the callback is missing its code or state");
    };

    // The CSRF check, before anything is exchanged or written. A mismatch means this
    // callback was not started by this server, so nothing about it is trustworthy.
    let known = {
        let conn = state.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        db::consume_auth_state(&conn, &csrf_state)
    };
    match known {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("google sign-in: rejected a callback with an unknown state");
            return bad_request("that sign-in has expired or did not start here");
        }
        Err(error) => {
            tracing::error!(%error, "could not verify an auth state");
            return bad_request("could not verify that sign-in");
        }
    }

    let profile = match fetch_profile(google, &code).await {
        Ok(profile) => profile,
        Err(error) => {
            // The code and the token are deliberately absent from this line.
            tracing::warn!(%error, "google sign-in: could not read the profile");
            return (
                StatusCode::BAD_GATEWAY,
                Json(Error { error: "google did not complete the sign-in".into() }),
            )
                .into_response();
        }
    };

    let token = random_token(TOKEN_BYTES);
    let stored = {
        let conn = state.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        db::upsert_google_account(&conn, &profile)
            .and_then(|account| db::create_session(&conn, &token, &account.id).map(|()| account))
    };
    let account = match stored {
        Ok(account) => account,
        Err(error) => {
            tracing::error!(%error, "could not create an account or a session");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Error { error: "could not finish signing you in".into() }),
            )
                .into_response();
        }
    };

    // The handle, not the email or the token: enough to tell two sign-ins apart in a
    // log without putting a credential or an address in it.
    tracing::info!(handle = %account.handle, "signed in");
    // The cached feed is keyed on the user, so nothing has to be dropped here — see
    // `cache::feed_key`.
    redirect(
        &format!("{}/", google.public_url),
        Some(session_cookie(&token, google.secure_cookie())),
    )
}

/// `POST /api/auth/logout` — revoke the session and clear the cookie.
///
/// 204 whether or not there was a session to end, because the caller's goal is "be
/// signed out" and they already are. The cookie is cleared either way, so a cookie
/// naming a row that is already gone doesn't linger.
pub async fn logout(State(state): State<AppState>, parts: Parts) -> Response {
    let secure = state.google.as_ref().is_none_or(Google::secure_cookie);

    if let Some(token) = cookie(&parts, COOKIE) {
        let conn = state.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match db::delete_session(&conn, &token) {
            Ok(true) => tracing::info!("signed out"),
            Ok(false) => {}
            Err(error) => tracing::error!(%error, "could not delete a session"),
        }
    }

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::SET_COOKIE, cleared_cookie(secure))
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// --- Talking to Google --------------------------------------------------------

/// Google's token response. Only the access token is read: the refresh token is not
/// requested (`access_type=online`), and the `id_token` carries nothing the userinfo
/// call doesn't.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Google's userinfo response, as the OpenID Connect claims name them.
#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    name: Option<String>,
    email: Option<String>,
    /// A URL on Google's CDN. Absent for an account with no photo.
    picture: Option<String>,
}

/// Exchange the code and read the profile behind it.
///
/// Two calls, both with a short timeout, both of which have to succeed before
/// anything is written: a half-finished sign-in should leave no account.
async fn fetch_profile(google: &Google, code: &str) -> Result<db::GoogleAccount, String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("could not build an HTTP client: {error}"))?;

    let token: TokenResponse = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", &google.client_id),
            ("client_secret", &google.client_secret),
            ("redirect_uri", &google.redirect_uri()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|error| format!("the token request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("google refused the code: {error}"))?
        .json()
        .await
        .map_err(|error| format!("the token response did not parse: {error}"))?;

    let info: UserInfo = client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|error| format!("the profile request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("google refused the token: {error}"))?
        .json()
        .await
        .map_err(|error| format!("the profile did not parse: {error}"))?;

    if info.sub.trim().is_empty() {
        return Err("google returned a profile with no subject id".into());
    }

    let name = info
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Cinephile")
        .to_string();

    Ok(db::GoogleAccount {
        sub: info.sub,
        email: info.email.clone(),
        handle: handle_from(info.email.as_deref(), &name),
        avatar: match info.picture.as_deref().filter(|url| !url.is_empty()) {
            Some(url) => Image::new(url, &format!("{name}'s profile picture.")),
            // The same initials monogram a TMDB user with no photo gets, so one
            // visual language covers everybody without a picture.
            None => crate::tmdb::map::monogram(&name),
        },
        name,
    })
}

/// A nickname to try for a new account.
///
/// The email's local part first, because that is what people recognise as their
/// handle; their display name otherwise. Filtered to the characters a nickname can
/// hold — it goes in a URL and is matched by friend search — and truncated, so a
/// forty-character address doesn't become a forty-character `@`. Uniqueness is
/// `db::unique_handle`'s job, not this one's.
fn handle_from(email: Option<&str>, name: &str) -> String {
    /// Long enough for a real name, short enough to render in a list.
    const MAX: usize = 20;

    let source = email
        .and_then(|address| address.split('@').next())
        .filter(|local| !local.is_empty())
        .unwrap_or(name);
    let cleaned: String = source
        .chars()
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' | '_' => Some(c),
            'A'..='Z' => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .take(MAX)
        .collect();

    // A name written entirely in a script this filter drops — the alternative is an
    // account addressed as "@".
    if cleaned.is_empty() {
        "cinephile".to_string()
    } else {
        cleaned
    }
}

/// Percent-encode one query value.
///
/// The consent URL carries a client id, a redirect URI and a state, and the URI
/// alone has a `:` and two `/` in it. Everything outside the unreserved set is
/// encoded per byte, so a multi-byte character survives. `tmdb` has its own copy of
/// this for its own query values; sharing one would couple the two modules over
/// eight lines.
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two tokens in a row must not match, and a token has to be long enough that
    /// guessing it is hopeless. Hex, so the length is twice the byte count.
    #[test]
    fn tokens_are_random_and_long() {
        let first = random_token(TOKEN_BYTES);
        let second = random_token(TOKEN_BYTES);
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        assert_ne!(first, second);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_nickname_comes_from_the_email_then_the_name() {
        assert_eq!(handle_from(Some("Sam.Okonkwo@example.com"), "Sam Okonkwo"), "samokonkwo");
        assert_eq!(handle_from(None, "Sam Okonkwo"), "samokonkwo");
        // No email local part to use.
        assert_eq!(handle_from(Some("@example.com"), "Ada Lovelace"), "adalovelace");
        // Underscores and digits survive; everything else goes.
        assert_eq!(handle_from(Some("a_1-b+c@x.com"), "n"), "a_1bc");
        // Nothing usable at all still addresses somebody.
        assert_eq!(handle_from(Some("+++@x.com"), "。。"), "cinephile");
        // Truncated rather than allowed to run on.
        assert!(handle_from(Some(&"a".repeat(60)), "n").len() <= 20);
    }

    /// The consent URL is built by hand, so the redirect URI's `:` and `/` have to be
    /// encoded or Google sees a truncated parameter.
    #[test]
    fn query_values_are_encoded() {
        assert_eq!(urlencode("https://x.test/api/auth/google/callback"),
                   "https%3A%2F%2Fx.test%2Fapi%2Fauth%2Fgoogle%2Fcallback");
        assert_eq!(urlencode("openid email profile"), "openid%20email%20profile");
        assert_eq!(urlencode("abc-123_x.y~z"), "abc-123_x.y~z");
        assert_eq!(urlencode("é"), "%C3%A9");
    }

    /// The cookie header is a list, and this app is not the only thing setting
    /// cookies on the host.
    #[test]
    fn one_cookie_is_picked_out_of_the_list() {
        let parts = |header: &str| {
            let (parts, ()) = axum::http::Request::builder()
                .header(axum::http::header::COOKIE, header)
                .body(())
                .unwrap()
                .into_parts();
            parts
        };

        assert_eq!(cookie(&parts("cj_session=abc"), COOKIE).as_deref(), Some("abc"));
        assert_eq!(
            cookie(&parts("other=1; cj_session=abc; third=2"), COOKIE).as_deref(),
            Some("abc")
        );
        // Whitespace around the pair, which is how most clients write the list.
        assert_eq!(cookie(&parts("a=1;  cj_session = abc "), COOKIE).as_deref(), Some("abc"));
        assert_eq!(cookie(&parts("other=1"), COOKIE), None);
        // A value-less pair must not be read as this cookie.
        assert_eq!(cookie(&parts("cj_session"), COOKIE), None);

        let (bare, ()) = axum::http::Request::builder().body(()).unwrap().into_parts();
        assert_eq!(cookie(&bare, COOKIE), None);
    }

    /// The three attributes the cookie must always carry, plus the one that depends
    /// on how the app is served.
    #[test]
    fn the_session_cookie_is_locked_down() {
        let secure = session_cookie("abc", true);
        for flag in ["HttpOnly", "SameSite=Lax", "Path=/", "Secure"] {
            assert!(secure.contains(flag), "{flag} is missing from {secure}");
        }
        // Plain-HTTP localhost, where a Secure cookie may be dropped.
        assert!(!session_cookie("abc", false).contains("Secure"));

        // Clearing must match on the same attributes, or the browser keeps the old
        // cookie beside the new one.
        let cleared = cleared_cookie(true);
        assert!(cleared.contains("Max-Age=0"));
        for flag in ["HttpOnly", "SameSite=Lax", "Path=/", "Secure"] {
            assert!(cleared.contains(flag), "{flag} is missing from {cleared}");
        }
    }

    /// `PUBLIC_URL` decides both, and a trailing slash on it must not double up.
    #[test]
    fn the_redirect_uri_and_the_cookie_follow_the_public_url() {
        let google = |public_url: &str| Google {
            client_id: "id".into(),
            client_secret: "secret".into(),
            public_url: public_url.trim_end_matches('/').to_string(),
        };

        assert_eq!(
            google("https://cine.example/").redirect_uri(),
            "https://cine.example/api/auth/google/callback"
        );
        assert!(google("https://cine.example").secure_cookie());
        assert!(!google("http://localhost:5173").secure_cookie());
    }
}
