//! SQLite persistence.
//!
//! Two unrelated things live here, for one reason each:
//!
//! - **The social layer** — friends, the stories rail, live-discussion rooms.
//!   TMDB has none of it: `/3/movie/{id}/reviews` returns flat prose with no
//!   reply threads, and there is no notion of a friend. It has to come from
//!   somewhere, and the export's cast of people is the obvious somewhere.
//! - **The visitor's own deltas** — watchlist, ratings, likes, posted comments
//!   and replies. Previously a `RwLock<Store>` that died on every `cargo run`.
//!
//! `hydrate` is untouched by this: `load_store` rebuilds the same `state::Store`
//! it has always taken, so the three-layer split (content / deltas / fold) holds
//! and all of `hydrate`'s tests still exercise the same shape.
//!
//! The social rows carry **no film ids**. Which film a rail entry is about is
//! decided at request time by pairing template *i* with trending film *i*, so
//! the rail can never reference a film that has since fallen out of the feed —
//! and the DB holds nothing that can rot.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{ActivityKind, Image};
use crate::state::{PostedComment, PostedReply, Store};

/// Where the database file lives, relative to the crate root (`cargo run` runs
/// from `backend/`). Override with `DATABASE_PATH`, and `:memory:` works for a
/// throwaway run.
pub const DEFAULT_PATH: &str = "cine-journal.db";

pub type Result<T> = rusqlite::Result<T>;

/// Open (or create) the database and bring the schema up to date.
pub fn open(path: &str) -> Result<Connection> {
    let conn = if path == ":memory:" {
        Connection::open_in_memory()?
    } else {
        Connection::open(path)?
    };
    prepare(&conn)?;
    Ok(conn)
}

/// Apply the schema and seed the social content. Idempotent: safe on every start.
pub fn prepare(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;

         -- Social content. Seeded once, then read-mostly.
         CREATE TABLE IF NOT EXISTS people (
             id            TEXT PRIMARY KEY,
             name          TEXT NOT NULL,
             avatar_src    TEXT NOT NULL,
             avatar_alt    TEXT NOT NULL,
             unseen        INTEGER NOT NULL DEFAULT 0,
             in_stories    INTEGER NOT NULL DEFAULT 0,
             position      INTEGER NOT NULL
         );

         -- A friend's action. No film id: paired with a trending film at request
         -- time, so it can't go stale. See the module comment.
         CREATE TABLE IF NOT EXISTS activity (
             id                TEXT PRIMARY KEY,
             person_id         TEXT NOT NULL REFERENCES people(id),
             kind              TEXT NOT NULL,
             timestamp_label   TEXT NOT NULL,
             quote             TEXT,
             rating_half_stars INTEGER,
             position          INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS discussions (
             id             TEXT PRIMARY KEY,
             blurb          TEXT NOT NULL,
             overflow_count INTEGER,
             position       INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS discussion_participants (
             discussion_id TEXT NOT NULL REFERENCES discussions(id),
             person_id     TEXT NOT NULL REFERENCES people(id),
             position      INTEGER NOT NULL,
             PRIMARY KEY (discussion_id, person_id)
         );

         -- The mobile feed's card captions ('Elena watched', 'Marcus rated').
         -- `show_rating` is a property of the caption, not the film: the export's
         -- 'added to watchlist' card draws no stars, because nobody rated it.
         CREATE TABLE IF NOT EXISTS feed_captions (
             id          TEXT PRIMARY KEY,
             caption     TEXT NOT NULL,
             show_rating INTEGER NOT NULL DEFAULT 1,
             position    INTEGER NOT NULL
         );

         -- The visitor's own state. Was state::Store, in memory.
         CREATE TABLE IF NOT EXISTS watchlist (
             movie_id TEXT PRIMARY KEY,
             added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         -- `rated_at` orders the profile's Recent Reviews tile. Nullable rather
         -- than NOT NULL DEFAULT CURRENT_TIMESTAMP, because SQLite ADD COLUMN only
         -- accepts a constant default — and this column arrived after the table
         -- did, so `migrate` has to add it to databases already on disk. Rows
         -- written before it exist sort last; `set_rating` stamps every new one.
         CREATE TABLE IF NOT EXISTS ratings (
             movie_id   TEXT PRIMARY KEY,
             half_stars INTEGER NOT NULL,
             rated_at   TEXT
         );

         CREATE TABLE IF NOT EXISTS liked_reviews (
             review_id TEXT PRIMARY KEY
         );

         CREATE TABLE IF NOT EXISTS liked_comments (
             comment_id TEXT PRIMARY KEY
         );

         -- `id` is the AUTOINCREMENT rowid rendered as 'comment-<n>'. It replaces
         -- the in-memory counter, whose ids restarted at 1 on every boot and
         -- would now collide with rows already on disk. AUTOINCREMENT (rather
         -- than a plain rowid) never reuses a number even after a delete.
         CREATE TABLE IF NOT EXISTS comments (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             review_id  TEXT NOT NULL,
             body       TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         CREATE TABLE IF NOT EXISTS replies (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             review_id  TEXT NOT NULL,
             comment_id TEXT NOT NULL,
             body       TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         CREATE INDEX IF NOT EXISTS comments_by_review ON comments(review_id);
         CREATE INDEX IF NOT EXISTS replies_by_comment ON replies(review_id, comment_id);",
    )?;

    migrate(conn)?;
    seed(conn)
}

/// Bring a database created by an earlier version up to the current schema.
///
/// `CREATE TABLE IF NOT EXISTS` is a no-op on a table that already exists, so a
/// column added to one of the definitions above never reaches a file already on
/// disk. There is no migration framework here on purpose — one visitor, one file,
/// and deleting it is a documented way to start over — but silently serving a
/// profile that can't read its own ratings is worse than four lines of this.
fn migrate(conn: &Connection) -> Result<()> {
    if !has_column(conn, "ratings", "rated_at")? {
        conn.execute_batch("ALTER TABLE ratings ADD COLUMN rated_at TEXT")?;
    }
    Ok(())
}

/// Whether a table already has a column. The table name is interpolated because
/// PRAGMA takes no bind parameters; both call sites pass a literal.
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names: Vec<String> =
        stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<_>>()?;
    Ok(names.iter().any(|name| name == column))
}

// --- Seeding ------------------------------------------------------------------

/// One friend, as the export drew them.
///
/// The avatar files are the export's own, already on disk in
/// `reference/cine-journal/img/` and served by the `/img` route — so the social
/// layer needs no new assets and looks exactly as it was designed to.
struct Person {
    id: &'static str,
    name: &'static str,
    avatar: &'static str,
    alt: &'static str,
    unseen: bool,
    in_stories: bool,
}

const PEOPLE: [Person; 11] = [
    Person {
        id: "elena",
        name: "Elena",
        avatar: "img/avatar-story-elena.jpg",
        alt: "A close up portrait of a young woman with short dark hair against a stark white background.",
        unseen: true,
        in_stories: true,
    },
    Person {
        id: "marcus",
        name: "Marcus",
        avatar: "img/avatar-story-marcus.jpg",
        alt: "A black and white portrait of a man looking thoughtfully off-camera, wearing a simple dark turtleneck.",
        unseen: true,
        in_stories: true,
    },
    Person {
        id: "sarah",
        name: "Sarah",
        avatar: "img/avatar-story-sarah.jpg",
        alt: "A casual portrait of a person with glasses, brightly lit in an airy, minimalist space.",
        unseen: false,
        in_stories: true,
    },
    Person {
        id: "david",
        name: "David",
        avatar: "img/avatar-story-david.jpg",
        alt: "A profile picture showing a silhouette of a person against a bright window.",
        unseen: false,
        in_stories: true,
    },
    Person {
        id: "anna",
        name: "Anna",
        avatar: "img/avatar-story-anna.jpg",
        alt: "A minimalist abstract avatar featuring simple geometric shapes in primary blue and slate white.",
        unseen: false,
        in_stories: true,
    },
    // The three on the desktop feed's "Friends Activity" rail, which the export
    // drew with different photos and surnames from the stories rail.
    Person {
        id: "alex-m",
        name: "Alex M.",
        avatar: "img/avatar-alex-m.jpg",
        alt: "A bright, airy profile photo of a young man smiling outdoors in soft sunlight.",
        unseen: false,
        in_stories: false,
    },
    Person {
        id: "sarah-k",
        name: "Sarah K.",
        avatar: "img/avatar-sarah-k.jpg",
        alt: "A bright, high-key studio portrait of a woman with short hair against a crisp white background.",
        unseen: false,
        in_stories: false,
    },
    Person {
        id: "david-p",
        name: "David P.",
        avatar: "img/avatar-david-p.jpg",
        alt: "A black and white portrait photo of a man looking off to the side with cinematic lighting.",
        unseen: false,
        in_stories: false,
    },
    // The live-room regulars. The export gave the discussion cards three photos of
    // their own rather than reusing the stories rail's, and they are unnamed there
    // — only the stacked avatars show — so they get ids and no display name that
    // any screen renders.
    Person {
        id: "live-a",
        name: "Nadia",
        avatar: "img/avatar-live-1.jpg",
        alt: "A black and white studio portrait of a woman looking thoughtfully off-camera.",
        unseen: false,
        in_stories: false,
    },
    Person {
        id: "live-b",
        name: "Tomas",
        avatar: "img/avatar-live-2.jpg",
        alt: "A black and white studio portrait of a man with glasses looking directly at the camera.",
        unseen: false,
        in_stories: false,
    },
    Person {
        id: "live-c",
        name: "Priya",
        avatar: "img/avatar-live-3.jpg",
        alt: "A black and white studio portrait of a woman laughing.",
        unseen: false,
        in_stories: false,
    },
];

/// One row of the friends-activity rail.
struct Activity {
    id: &'static str,
    person: &'static str,
    /// "watched" or "added_to_watchlist" — the two verbs the export drew.
    kind: &'static str,
    stamp: &'static str,
    /// The pull-quote. Absent for watchlist adds, which show no quote and no stars.
    quote: Option<&'static str>,
    rating: Option<u8>,
}

/// The rail's three rows, verbatim from the export minus the film names — those come
/// from whatever is trending when the request arrives.
const ACTIVITY: [Activity; 3] = [
    Activity {
        id: "activity-alex",
        person: "alex-m",
        kind: "watched",
        stamp: "2h ago",
        quote: Some(
            "\"A masterpiece of visual storytelling. The silence is deafening in the best way possible.\"",
        ),
        rating: Some(10),
    },
    Activity {
        id: "activity-sarah",
        person: "sarah-k",
        kind: "added_to_watchlist",
        stamp: "5h ago",
        quote: None,
        rating: None,
    },
    Activity {
        id: "activity-david",
        person: "david-p",
        kind: "watched",
        stamp: "Yesterday",
        quote: Some("\"Style over substance, perhaps. But what style it is.\""),
        rating: Some(6),
    },
];

/// The two "Live Now" rooms, verbatim from the export minus the films.
const DISCUSSIONS: [(&str, &str, Option<u32>, &[&str]); 2] = [
    (
        "live-1",
        "Join the discussion room. 142 members currently debating the ambiguous ending.",
        Some(14),
        &["live-a", "live-b"],
    ),
    ("live-2", "Live watch party starting in 10 minutes. Grab your coffee.", None, &["live-c"]),
];

/// The mobile feed's card subtitles, verbatim from the export. The film they sit
/// under is whatever is trending at position *i*.
const CAPTIONS: [(&str, &str, bool); 4] = [
    ("caption-1", "Elena watched • 4h ago", true),
    ("caption-2", "Marcus rated • 5h ago", true),
    // No stars on this one in the export — see `show_rating`.
    ("caption-3", "Anna added to watchlist", false),
    ("caption-4", "David wrote a review", true),
];

/// Insert the social content, but only into an empty database.
///
/// Guarded on `people` being empty rather than using `INSERT OR REPLACE`: a
/// restart must not duplicate the rail, and equally must not silently rewrite
/// rows an operator edited by hand.
fn seed(conn: &Connection) -> Result<()> {
    let already: i64 = conn.query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))?;
    if already > 0 {
        return Ok(());
    }

    for (position, person) in PEOPLE.iter().enumerate() {
        conn.execute(
            "INSERT INTO people (id, name, avatar_src, avatar_alt, unseen, in_stories, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                person.id,
                person.name,
                person.avatar,
                person.alt,
                person.unseen,
                person.in_stories,
                position as i64,
            ],
        )?;
    }

    for (position, row) in ACTIVITY.iter().enumerate() {
        conn.execute(
            "INSERT INTO activity
                 (id, person_id, kind, timestamp_label, quote, rating_half_stars, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.id,
                row.person,
                row.kind,
                row.stamp,
                row.quote,
                row.rating,
                position as i64,
            ],
        )?;
    }

    for (position, (id, blurb, overflow, participants)) in DISCUSSIONS.iter().enumerate() {
        conn.execute(
            "INSERT INTO discussions (id, blurb, overflow_count, position) VALUES (?1, ?2, ?3, ?4)",
            params![id, blurb, overflow, position as i64],
        )?;
        for (seat, person) in participants.iter().enumerate() {
            conn.execute(
                "INSERT INTO discussion_participants (discussion_id, person_id, position)
                 VALUES (?1, ?2, ?3)",
                params![id, person, seat as i64],
            )?;
        }
    }

    for (position, (id, caption, show_rating)) in CAPTIONS.iter().enumerate() {
        conn.execute(
            "INSERT INTO feed_captions (id, caption, show_rating, position) VALUES (?1, ?2, ?3, ?4)",
            params![id, caption, show_rating, position as i64],
        )?;
    }

    Ok(())
}

// --- Reading the social layer -------------------------------------------------

/// A friend, with the avatar already normalized to a servable path.
#[derive(Debug, Clone)]
pub struct PersonRow {
    pub id: String,
    pub name: String,
    pub avatar: Image,
    pub unseen: bool,
}

/// One activity-rail row, still missing the film it is about.
#[derive(Debug, Clone)]
pub struct ActivityRow {
    pub id: String,
    pub person: PersonRow,
    pub kind: ActivityKind,
    pub timestamp: String,
    pub quote: Option<String>,
    pub rating_half_stars: Option<u8>,
}

/// One "Live Now" room, still missing its film.
#[derive(Debug, Clone)]
pub struct DiscussionRow {
    pub id: String,
    pub blurb: String,
    pub overflow_count: Option<u32>,
    pub participants: Vec<Image>,
}

fn person_from_row(row: &rusqlite::Row<'_>) -> Result<PersonRow> {
    let src: String = row.get("avatar_src")?;
    let alt: String = row.get("avatar_alt")?;
    Ok(PersonRow {
        id: row.get("id")?,
        name: row.get("name")?,
        avatar: Image::new(&src, &alt),
        unseen: row.get("unseen")?,
    })
}

/// The stories rail, in seeded order.
pub fn stories(conn: &Connection) -> Result<Vec<PersonRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, avatar_src, avatar_alt, unseen FROM people
         WHERE in_stories = 1 ORDER BY position",
    )?;
    let rows = stmt.query_map([], person_from_row)?;
    rows.collect()
}

/// The friends-activity rail, in seeded order.
pub fn activity(conn: &Connection) -> Result<Vec<ActivityRow>> {
    let mut stmt = conn.prepare(
        "SELECT a.id AS id, a.kind AS kind, a.timestamp_label AS timestamp_label,
                a.quote AS quote, a.rating_half_stars AS rating_half_stars,
                p.id AS person_id, p.name AS name, p.avatar_src AS avatar_src,
                p.avatar_alt AS avatar_alt, p.unseen AS unseen
         FROM activity a JOIN people p ON p.id = a.person_id
         ORDER BY a.position",
    )?;

    let rows = stmt.query_map([], |row| {
        let kind: String = row.get("kind")?;
        Ok(ActivityRow {
            id: row.get("id")?,
            person: PersonRow {
                id: row.get("person_id")?,
                name: row.get("name")?,
                avatar: Image::new(&row.get::<_, String>("avatar_src")?, &row.get::<_, String>("avatar_alt")?),
                unseen: row.get("unseen")?,
            },
            // An unrecognized kind falls back to `Watched` rather than failing the
            // request: the column is a free-text enum, and the feed rendering one
            // row as the wrong verb beats the whole screen erroring.
            kind: if kind == "added_to_watchlist" {
                ActivityKind::AddedToWatchlist
            } else {
                ActivityKind::Watched
            },
            timestamp: row.get("timestamp_label")?,
            quote: row.get("quote")?,
            rating_half_stars: row.get("rating_half_stars")?,
        })
    })?;

    rows.collect()
}

/// The "Live Now" rooms, in seeded order, each with its participants' avatars.
pub fn discussions(conn: &Connection) -> Result<Vec<DiscussionRow>> {
    let mut stmt =
        conn.prepare("SELECT id, blurb, overflow_count, position FROM discussions ORDER BY position")?;
    let bare: Vec<(String, String, Option<u32>)> = stmt
        .query_map([], |row| Ok((row.get("id")?, row.get("blurb")?, row.get("overflow_count")?)))?
        .collect::<Result<_>>()?;

    let mut out = Vec::with_capacity(bare.len());
    for (id, blurb, overflow_count) in bare {
        let mut stmt = conn.prepare(
            "SELECT p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt
             FROM discussion_participants dp JOIN people p ON p.id = dp.person_id
             WHERE dp.discussion_id = ?1 ORDER BY dp.position",
        )?;
        let participants = stmt
            .query_map([&id], |row| {
                Ok(Image::new(
                    &row.get::<_, String>("avatar_src")?,
                    &row.get::<_, String>("avatar_alt")?,
                ))
            })?
            .collect::<Result<Vec<Image>>>()?;

        out.push(DiscussionRow { id, blurb, overflow_count, participants });
    }

    Ok(out)
}

/// One row of the profile's "Following" list, still missing its subtitle.
///
/// `activity` is the id of this person's activity-rail row, when they have one.
/// That row is what the subtitle is built from — and, like every other rail here,
/// which film it is about is decided at request time. See the module comment.
#[derive(Debug, Clone)]
pub struct FollowRow {
    pub person: PersonRow,
    pub activity: Option<String>,
}

/// Everyone the visitor follows: the stories rail plus the friends-activity rail.
///
/// Those two rails *are* the app's notion of a friend — there is no follow table,
/// because there is no second account to follow (see `state`). The live-room
/// regulars are excluded: the export never names them, only stacks their avatars,
/// so listing them by name would put invented copy on screen.
pub fn following(conn: &Connection) -> Result<Vec<FollowRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id AS id, p.name AS name, p.avatar_src AS avatar_src,
                p.avatar_alt AS avatar_alt, p.unseen AS unseen,
                a.id AS activity_id
         FROM people p LEFT JOIN activity a ON a.person_id = p.id
         WHERE p.in_stories = 1 OR a.id IS NOT NULL
         ORDER BY p.position",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FollowRow { person: person_from_row(row)?, activity: row.get("activity_id")? })
    })?;
    rows.collect()
}

/// The visitor's watchlist, **most recently added first** — the order the profile
/// grid wants, and the reverse of `load_store`'s.
///
/// `movie_id` breaks the tie because `added_at` has one-second resolution, so two
/// films logged in the same second would otherwise come back in an arbitrary
/// order that flips between requests.
pub fn watchlist_recent_first(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT movie_id FROM watchlist ORDER BY added_at DESC, movie_id DESC")?;
    let ids = stmt.query_map([], |row| row.get(0))?.collect();
    ids
}

/// Every film the visitor rated, newest first. `rated_at` is NULL for rows
/// written before that column existed, and NULLs sort last under DESC.
pub fn ratings_recent_first(conn: &Connection) -> Result<Vec<(String, u8)>> {
    let mut stmt = conn.prepare(
        "SELECT movie_id, half_stars FROM ratings ORDER BY rated_at DESC, movie_id DESC",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
    rows
}

/// The visitor's highest-rated films, best first, most recent of equals first.
pub fn ratings_best_first(conn: &Connection) -> Result<Vec<(String, u8)>> {
    let mut stmt = conn.prepare(
        "SELECT movie_id, half_stars FROM ratings
         ORDER BY half_stars DESC, rated_at DESC, movie_id DESC",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
    rows
}

/// One mobile-feed card's caption, still missing its film.
#[derive(Debug, Clone)]
pub struct CaptionRow {
    pub id: String,
    pub caption: String,
    pub show_rating: bool,
}

/// The mobile feed's card captions, in seeded order.
pub fn captions(conn: &Connection) -> Result<Vec<CaptionRow>> {
    let mut stmt =
        conn.prepare("SELECT id, caption, show_rating FROM feed_captions ORDER BY position")?;
    let rows = stmt.query_map([], |row| {
        Ok(CaptionRow {
            id: row.get("id")?,
            caption: row.get("caption")?,
            show_rating: row.get("show_rating")?,
        })
    })?;
    rows.collect()
}

// --- The visitor's state ------------------------------------------------------

/// Rebuild the whole `Store` from disk.
///
/// One snapshot per request, which is what keeps `hydrate` unchanged: it takes a
/// `&Store` and has no idea a database exists. The tables hold a handful of rows
/// for a single visitor, so reading all of them is cheaper than the six
/// finer-grained queries that would replace it.
pub fn load_store(conn: &Connection) -> Result<Store> {
    let mut store = Store::default();

    let mut stmt = conn.prepare("SELECT movie_id FROM watchlist ORDER BY added_at, movie_id")?;
    for id in stmt.query_map([], |row| row.get::<_, String>(0))? {
        store.watchlist.insert(id?);
    }

    let mut stmt = conn.prepare("SELECT movie_id, half_stars FROM ratings")?;
    for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?)))? {
        let (id, half_stars) = row?;
        store.ratings.insert(id, half_stars);
    }

    let mut stmt = conn.prepare("SELECT review_id FROM liked_reviews")?;
    for id in stmt.query_map([], |row| row.get::<_, String>(0))? {
        store.liked_reviews.insert(id?);
    }

    let mut stmt = conn.prepare("SELECT comment_id FROM liked_comments")?;
    for id in stmt.query_map([], |row| row.get::<_, String>(0))? {
        store.liked_comments.insert(id?);
    }

    let mut stmt =
        conn.prepare("SELECT review_id, id, body FROM comments ORDER BY id")?;
    let mut posted: BTreeMap<String, Vec<PostedComment>> = BTreeMap::new();
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
    })? {
        let (review_id, id, body) = row?;
        posted.entry(review_id).or_default().push(PostedComment { id: comment_id(id), body });
    }
    store.posted_comments = posted;

    let mut stmt =
        conn.prepare("SELECT review_id, comment_id, id, body FROM replies ORDER BY id")?;
    let mut replies: BTreeMap<(String, String), Vec<PostedReply>> = BTreeMap::new();
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        let (review_id, parent, id, body) = row?;
        replies.entry((review_id, parent)).or_default().push(PostedReply { id: reply_id(id), body });
    }
    store.posted_replies = replies;

    Ok(store)
}

/// Rowid 3 -> "comment-3". The wire ids the frontend already sends back.
fn comment_id(rowid: i64) -> String {
    format!("comment-{rowid}")
}

fn reply_id(rowid: i64) -> String {
    format!("reply-{rowid}")
}

/// Add, remove, or toggle a film on the watchlist. Returns the resulting state.
pub fn set_watchlist(conn: &Connection, movie_id: &str, target: Option<bool>) -> Result<bool> {
    let present: bool = conn
        .query_row("SELECT 1 FROM watchlist WHERE movie_id = ?1", [movie_id], |_| Ok(()))
        .optional()?
        .is_some();

    let target = target.unwrap_or(!present);
    if target {
        // Idempotent, so a double-click can't desync the button from the store —
        // and `added_at` keeps its original value rather than jumping.
        conn.execute("INSERT OR IGNORE INTO watchlist (movie_id) VALUES (?1)", [movie_id])?;
    } else {
        conn.execute("DELETE FROM watchlist WHERE movie_id = ?1", [movie_id])?;
    }
    Ok(target)
}

/// Set the visitor's rating; `0` clears it, which is how the UI un-rates a film.
pub fn set_rating(conn: &Connection, movie_id: &str, half_stars: u8) -> Result<Option<u8>> {
    if half_stars == 0 {
        conn.execute("DELETE FROM ratings WHERE movie_id = ?1", [movie_id])?;
        return Ok(None);
    }
    // Re-rating a film moves it to the top of the profile's "Recent Reviews",
    // which is what "recent" has to mean — the *rating* is the event, not the row.
    conn.execute(
        "INSERT INTO ratings (movie_id, half_stars, rated_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(movie_id) DO UPDATE
             SET half_stars = excluded.half_stars, rated_at = excluded.rated_at",
        params![movie_id, half_stars],
    )?;
    Ok(Some(half_stars))
}

/// Toggle a like. Returns whether it is now liked.
pub fn toggle_review_like(conn: &Connection, review_id: &str) -> Result<bool> {
    let removed = conn.execute("DELETE FROM liked_reviews WHERE review_id = ?1", [review_id])?;
    if removed > 0 {
        return Ok(false);
    }
    conn.execute("INSERT INTO liked_reviews (review_id) VALUES (?1)", [review_id])?;
    Ok(true)
}

pub fn toggle_comment_like(conn: &Connection, comment_id: &str) -> Result<bool> {
    let removed = conn.execute("DELETE FROM liked_comments WHERE comment_id = ?1", [comment_id])?;
    if removed > 0 {
        return Ok(false);
    }
    conn.execute("INSERT INTO liked_comments (comment_id) VALUES (?1)", [comment_id])?;
    Ok(true)
}

/// Store a comment and return its wire id.
pub fn add_comment(conn: &Connection, review_id: &str, body: &str) -> Result<String> {
    conn.execute(
        "INSERT INTO comments (review_id, body) VALUES (?1, ?2)",
        params![review_id, body],
    )?;
    Ok(comment_id(conn.last_insert_rowid()))
}

pub fn add_reply(
    conn: &Connection,
    review_id: &str,
    comment_id: &str,
    body: &str,
) -> Result<String> {
    conn.execute(
        "INSERT INTO replies (review_id, comment_id, body) VALUES (?1, ?2, ?3)",
        params![review_id, comment_id, body],
    )?;
    Ok(reply_id(conn.last_insert_rowid()))
}

/// Whether the visitor posted this comment — needed before accepting a reply to
/// it or a like on it, since it isn't in the upstream content.
pub fn comment_exists(conn: &Connection, review_id: &str, comment_id: &str) -> Result<bool> {
    let rowid = match comment_id.strip_prefix("comment-").and_then(|n| n.parse::<i64>().ok()) {
        Some(rowid) => rowid,
        None => return Ok(false),
    };
    Ok(conn
        .query_row(
            "SELECT 1 FROM comments WHERE id = ?1 AND review_id = ?2",
            params![rowid, review_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        open(":memory:").expect("in-memory database")
    }

    #[test]
    fn the_schema_applies_twice_without_duplicating_the_seed() {
        let conn = db();
        let people = |conn: &Connection| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM people", [], |r| r.get(0)).unwrap()
        };
        let before = people(&conn);
        assert_eq!(before, PEOPLE.len() as i64);

        // What a restart does.
        prepare(&conn).unwrap();
        assert_eq!(people(&conn), before, "seeding twice duplicated the rail");
    }

    #[test]
    fn the_social_rails_come_back_in_seeded_order() {
        let conn = db();

        let stories = stories(&conn).unwrap();
        assert_eq!(stories.len(), 5, "the export's stories rail holds five");
        // Only the rail's five are `in_stories`; the activity and live-room people
        // share the table but must not appear as story circles.
        assert!(stories.len() < PEOPLE.len());
        assert_eq!(stories[0].name, "Elena");
        assert!(stories[0].unseen);
        assert!(!stories[4].unseen);
        // `Image::new` has made the export's relative path servable.
        assert_eq!(stories[0].avatar.src, "/img/avatar-story-elena.jpg");

        let activity = activity(&conn).unwrap();
        assert_eq!(activity.len(), 3);
        assert_eq!(activity[0].person.name, "Alex M.");
        assert_eq!(activity[0].rating_half_stars, Some(10));
        assert!(activity[0].quote.is_some());
        assert!(matches!(activity[1].kind, ActivityKind::AddedToWatchlist));
        assert_eq!(activity[1].quote, None);

        let discussions = discussions(&conn).unwrap();
        assert_eq!(discussions.len(), 2);
        assert_eq!(discussions[0].participants.len(), 2);
        assert_eq!(discussions[0].participants[0].src, "/img/avatar-live-1.jpg");
        assert_eq!(discussions[0].participants[1].src, "/img/avatar-live-2.jpg");
        assert_eq!(discussions[0].overflow_count, Some(14));
        assert_eq!(discussions[1].participants.len(), 1);
        assert_eq!(discussions[1].overflow_count, None);

        let captions = captions(&conn).unwrap();
        assert_eq!(captions.len(), 4);
        assert_eq!(captions[0].caption, "Elena watched • 4h ago");
        assert!(captions[0].show_rating);
        assert!(!captions[2].show_rating, "the watchlist card draws no stars");
    }

    #[test]
    fn watchlist_toggles_and_is_idempotent_when_told_the_target() {
        let conn = db();
        assert!(set_watchlist(&conn, "157336-interstellar", None).unwrap());
        assert!(!set_watchlist(&conn, "157336-interstellar", None).unwrap());

        // Stating the target twice must not flip it — that's what protects a
        // double-click from desyncing the button.
        assert!(set_watchlist(&conn, "x", Some(true)).unwrap());
        assert!(set_watchlist(&conn, "x", Some(true)).unwrap());
        assert!(!set_watchlist(&conn, "x", Some(false)).unwrap());
        assert!(!set_watchlist(&conn, "x", Some(false)).unwrap());

        assert_eq!(load_store(&conn).unwrap().watchlist.len(), 0);
    }

    #[test]
    fn ratings_round_trip_and_zero_clears() {
        let conn = db();
        assert_eq!(set_rating(&conn, "m", 7).unwrap(), Some(7));
        assert_eq!(load_store(&conn).unwrap().ratings.get("m"), Some(&7));

        // Re-rating replaces rather than conflicting.
        assert_eq!(set_rating(&conn, "m", 9).unwrap(), Some(9));
        assert_eq!(load_store(&conn).unwrap().ratings.get("m"), Some(&9));

        assert_eq!(set_rating(&conn, "m", 0).unwrap(), None);
        assert!(load_store(&conn).unwrap().ratings.is_empty());
    }

    #[test]
    fn likes_toggle() {
        let conn = db();
        assert!(toggle_review_like(&conn, "r").unwrap());
        assert!(load_store(&conn).unwrap().liked_reviews.contains("r"));
        assert!(!toggle_review_like(&conn, "r").unwrap());
        assert!(load_store(&conn).unwrap().liked_reviews.is_empty());

        assert!(toggle_comment_like(&conn, "comment-1").unwrap());
        assert!(load_store(&conn).unwrap().liked_comments.contains("comment-1"));
    }

    /// The ids `hydrate` and the frontend exchange, rebuilt from rowids.
    #[test]
    fn posted_comments_and_replies_reload_with_their_ids() {
        let conn = db();
        let first = add_comment(&conn, "review-a", "Mine").unwrap();
        let second = add_comment(&conn, "review-a", "Also mine").unwrap();
        let elsewhere = add_comment(&conn, "review-b", "Different review").unwrap();
        assert_eq!(first, "comment-1");
        assert_eq!(second, "comment-2");
        assert_eq!(elsewhere, "comment-3");

        let reply = add_reply(&conn, "review-a", &first, "A follow-up").unwrap();
        assert_eq!(reply, "reply-1");

        let store = load_store(&conn).unwrap();
        let on_a = &store.posted_comments["review-a"];
        assert_eq!(on_a.len(), 2);
        assert_eq!(on_a[0].id, "comment-1");
        assert_eq!(on_a[0].body, "Mine");
        assert_eq!(on_a[1].id, "comment-2");
        // Scoped to their review, exactly as the in-memory store was.
        assert_eq!(store.posted_comments["review-b"].len(), 1);

        let thread = &store.posted_replies[&("review-a".to_string(), "comment-1".to_string())];
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].id, "reply-1");
    }

    /// The bug the rowid scheme exists to prevent: the old in-memory counter
    /// restarted at 1 every boot, so a fresh comment would reuse a stored id.
    #[test]
    fn comment_ids_do_not_restart_after_a_reload() {
        let conn = db();
        add_comment(&conn, "r", "one").unwrap();
        add_comment(&conn, "r", "two").unwrap();

        // Reopening the same connection is as close as an in-memory database gets
        // to a restart; AUTOINCREMENT keeps its high-water mark in the file.
        let third = add_comment(&conn, "r", "three").unwrap();
        assert_eq!(third, "comment-3");

        // Even after a delete, the number is not reused — that's AUTOINCREMENT
        // rather than a bare rowid.
        conn.execute("DELETE FROM comments WHERE id = 3", []).unwrap();
        assert_eq!(add_comment(&conn, "r", "four").unwrap(), "comment-4");
    }

    #[test]
    fn comment_existence_is_scoped_to_the_review() {
        let conn = db();
        let id = add_comment(&conn, "review-a", "Mine").unwrap();
        assert!(comment_exists(&conn, "review-a", &id).unwrap());
        assert!(!comment_exists(&conn, "review-b", &id).unwrap());
        // A malformed or upstream id is simply not ours.
        assert!(!comment_exists(&conn, "review-a", "comment-marcus").unwrap());
        assert!(!comment_exists(&conn, "review-a", "nonsense").unwrap());
    }

    #[test]
    fn an_empty_database_loads_an_empty_store() {
        let store = load_store(&db()).unwrap();
        assert!(store.watchlist.is_empty());
        assert!(store.ratings.is_empty());
        assert!(store.liked_reviews.is_empty());
        assert!(store.liked_comments.is_empty());
        assert!(store.posted_comments.is_empty());
        assert!(store.posted_replies.is_empty());
    }

    /// The profile grid shows the newest first, which is the reverse of what
    /// `load_store` hands `hydrate`.
    #[test]
    fn the_watchlist_reads_back_newest_first() {
        let conn = db();
        // `added_at` has one-second resolution, so three films logged in the same
        // second tie — and the id breaks the tie deterministically rather than
        // letting the order flip between requests.
        for id in ["a-film", "b-film", "c-film"] {
            set_watchlist(&conn, id, Some(true)).unwrap();
        }
        assert_eq!(watchlist_recent_first(&conn).unwrap(), ["c-film", "b-film", "a-film"]);
        // `load_store`'s order is unchanged, which is what keeps `hydrate` honest.
        let store: Vec<String> = load_store(&conn).unwrap().watchlist.into_iter().collect();
        assert_eq!(store, ["a-film", "b-film", "c-film"]);
    }

    #[test]
    fn ratings_sort_by_score_and_by_recency() {
        let conn = db();
        set_rating(&conn, "middling", 6).unwrap();
        set_rating(&conn, "great", 10).unwrap();
        set_rating(&conn, "good", 8).unwrap();

        let best: Vec<String> =
            ratings_best_first(&conn).unwrap().into_iter().map(|(id, _)| id).collect();
        assert_eq!(best, ["great", "good", "middling"]);

        // Same second for all three, so the id tiebreak decides — what matters is
        // that re-rating moves a film to the front, since the rating is the event.
        set_rating(&conn, "middling", 7).unwrap();
        let recent = ratings_recent_first(&conn).unwrap();
        assert_eq!(recent[0], ("middling".to_string(), 7));
    }

    /// A rating written before `rated_at` existed must still come back — it sorts
    /// last rather than disappearing from the profile.
    #[test]
    fn ratings_predating_the_timestamp_column_still_load() {
        let conn = db();
        conn.execute("INSERT INTO ratings (movie_id, half_stars) VALUES ('legacy', 9)", [])
            .unwrap();
        set_rating(&conn, "current", 4).unwrap();

        let recent: Vec<String> =
            ratings_recent_first(&conn).unwrap().into_iter().map(|(id, _)| id).collect();
        assert_eq!(recent, ["current", "legacy"], "NULL rated_at sorts last, not out");
        assert_eq!(load_store(&conn).unwrap().ratings.get("legacy"), Some(&9));
    }

    /// `migrate` has to add the column to a file created without it, since
    /// `CREATE TABLE IF NOT EXISTS` won't.
    #[test]
    fn the_ratings_timestamp_is_added_to_an_older_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ratings (movie_id TEXT PRIMARY KEY, half_stars INTEGER NOT NULL)",
        )
        .unwrap();
        assert!(!has_column(&conn, "ratings", "rated_at").unwrap());

        prepare(&conn).unwrap();
        assert!(has_column(&conn, "ratings", "rated_at").unwrap());
        // And twice is still fine.
        prepare(&conn).unwrap();
        assert_eq!(set_rating(&conn, "m", 8).unwrap(), Some(8));
    }

    /// The follow list is the two rails, and deliberately not the unnamed
    /// live-room regulars.
    #[test]
    fn following_covers_both_rails_but_not_the_live_rooms() {
        let conn = db();
        let ids: Vec<String> =
            following(&conn).unwrap().into_iter().map(|row| row.person.id).collect();

        assert!(ids.contains(&"elena".to_string()), "the stories rail is followed");
        assert!(ids.contains(&"alex-m".to_string()), "so is the activity rail");
        for unnamed in ["live-a", "live-b", "live-c"] {
            assert!(!ids.contains(&unnamed.to_string()), "{unnamed} is never named on screen");
        }

        // Only the activity-rail people carry a row to build a subtitle from.
        let with_activity: Vec<String> = following(&conn)
            .unwrap()
            .into_iter()
            .filter(|row| row.activity.is_some())
            .map(|row| row.person.id)
            .collect();
        assert_eq!(with_activity, ["alex-m", "sarah-k", "david-p"]);
    }
}
