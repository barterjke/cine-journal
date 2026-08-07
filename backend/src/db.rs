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
//! The *export's* social rows carry **no film ids**. Which film a rail entry is
//! about is decided at request time by pairing template *i* with trending film
//! *i*, so the rail can never reference a film that has since fallen out of the
//! feed — and the DB holds nothing that can rot.
//!
//! The **social graph** added later works the other way round, and has to. A
//! person's review is *of a specific film*, so `user_reviews.movie_id` is a real
//! app id and the pairing trick is unavailable: a review that slid onto whatever
//! is trending today would say a different thing every week. The cost is that a
//! seeded review can outlive the visitor's interest in that film, which is
//! fine — it is what a review does.
//!
//! These people are **ours**, not TMDB's. TMDB seeds them (`seed_graph` borrows
//! real nicknames, ratings and prose to have something to test against), but the
//! rows are the app's own afterwards: following writes to `follows`, and nothing
//! about a person is re-fetched. See `content::people`.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::Image;
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

         -- The app's users. Seeded once from `content::harvest_graph`, then
         -- read-mostly.
         --
         -- `handle` is the nickname friend search matches on, and is UNIQUE
         -- because it is how a person is addressed ('@msbreviews').
         --
         -- Every row here is now a user, so `is_user` is always 1 and the column
         -- survives only for the queries that still filter on it and for databases
         -- already on disk. It used to separate these from the export's decorative
         -- cast — eleven people who existed to populate a stories rail and a
         -- 'Friends Activity' sidebar, both of which are gone: their entries
         -- described actions nothing recorded. Anyone on screen is now somebody the
         -- visitor follows, and the rails come from what those people wrote.
         --
         -- `follows_visitor` is stored rather than derived: with one visitor
         -- nobody can really press follow on you, so this is seeded (a little
         -- under half of them) and thereafter left alone. It is the one field
         -- here that describes something that did not happen, and it is a
         -- column rather than a hash so a test can set it.
         --
         -- `unseen`, `in_stories` and `position` are likewise vestigial, kept
         -- because SQLite has no DROP COLUMN before 3.35 and rewriting the table
         -- would risk a visitor's own follows for tidiness. Nothing reads them.
         CREATE TABLE IF NOT EXISTS people (
             id            TEXT PRIMARY KEY,
             name          TEXT NOT NULL,
             avatar_src    TEXT NOT NULL,
             avatar_alt    TEXT NOT NULL,
             unseen        INTEGER NOT NULL DEFAULT 0,
             in_stories    INTEGER NOT NULL DEFAULT 0,
             position      INTEGER NOT NULL DEFAULT 0,
             handle        TEXT UNIQUE,
             bio           TEXT,
             is_user       INTEGER NOT NULL DEFAULT 0,
             follows_visitor INTEGER NOT NULL DEFAULT 0
         );

         -- Who the visitor follows. One row per follow, written by the button;
         -- deleting the row unfollows. Not symmetric with `follows_visitor`:
         -- that one is about them, this one is the visitor's own action, and
         -- conflating the two would make an unfollow silently rewrite history.
         CREATE TABLE IF NOT EXISTS follows (
             person_id  TEXT PRIMARY KEY REFERENCES people(id),
             followed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         -- A user's review of one specific film.
         --
         -- `movie_id` is a real app id, unlike `activity` above: a review is of a
         -- film, and pairing it with whatever trends today would change what it
         -- says. One review per person per film, which is what the PK enforces.
         CREATE TABLE IF NOT EXISTS user_reviews (
             person_id  TEXT NOT NULL REFERENCES people(id),
             movie_id   TEXT NOT NULL,
             half_stars INTEGER NOT NULL,
             body       TEXT NOT NULL,
             created_at TEXT NOT NULL,
             PRIMARY KEY (person_id, movie_id)
         );

         CREATE INDEX IF NOT EXISTS user_reviews_by_movie ON user_reviews(movie_id);

         -- A user's favourite films and their watchlist, so their page shows the
         -- same sections the visitor's own profile does rather than reviews alone.
         --
         -- `position` rather than a timestamp: these are seeded in one pass and
         -- nobody but the visitor can add to them, so the order they were written in
         -- *is* the order — and a seeded `added_at` would claim a moment that never
         -- happened. The visitor's own two tables (`favorites`, `watchlist`) keep
         -- their timestamps, because those record real presses.
         CREATE TABLE IF NOT EXISTS user_favorites (
             person_id TEXT NOT NULL REFERENCES people(id),
             movie_id  TEXT NOT NULL,
             position  INTEGER NOT NULL,
             PRIMARY KEY (person_id, movie_id)
         );

         CREATE TABLE IF NOT EXISTS user_watchlist (
             person_id TEXT NOT NULL REFERENCES people(id),
             movie_id  TEXT NOT NULL,
             position  INTEGER NOT NULL,
             PRIMARY KEY (person_id, movie_id)
         );

         -- The visitor's own state. Was state::Store, in memory.
         CREATE TABLE IF NOT EXISTS watchlist (
             movie_id TEXT PRIMARY KEY,
             added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         -- Films the visitor marked as favourites, by pressing the heart on a
         -- film's page.
         --
         -- A separate table from `ratings` rather than the visitor's highest-rated
         -- films, which is what the profile's Favorite Films strip used to mean.
         -- Those are different statements: a five-star rating says the film is
         -- good, a favourite says it is *yours*, and deriving one from the other
         -- made the strip change behind your back every time you rated something.
         CREATE TABLE IF NOT EXISTS favorites (
             movie_id TEXT PRIMARY KEY,
             added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         -- The visitor's own prose about a film. One review per film, which is
         -- what the PK enforces — writing again edits what's there.
         --
         -- Not a column on `ratings`, because the two are independent: clearing a
         -- rating must not delete what you wrote, and you can write about a film
         -- without scoring it. `user_reviews` is the other people's equivalent; it
         -- is keyed on `people(id)` and the visitor has no row there (see
         -- `models::Profile`), so this is the visitor-shaped copy of it — the same
         -- split `watchlist` and `ratings` already have.
         CREATE TABLE IF NOT EXISTS visitor_reviews (
             movie_id   TEXT PRIMARY KEY,
             body       TEXT NOT NULL,
             written_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         -- Editable scalars belonging to the visitor — at present just their bio.
         --
         -- A key/value table rather than a one-row `visitor` table with a column
         -- per field: the rest of their identity (name, handle, avatar, the joined
         -- line) is still the export's, held in `hydrate` as constants, and a table
         -- with one editable column and four decorative ones would imply an account
         -- system that still doesn't exist.
         CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
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

    migrate(conn)
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
    // The social graph's four columns on `people`. `handle` cannot be added with
    // its UNIQUE constraint — SQLite's ADD COLUMN rejects that — so the index
    // carries it instead, which is the same guarantee under a different name.
    if !has_column(conn, "people", "handle")? {
        conn.execute_batch(
            "ALTER TABLE people ADD COLUMN handle TEXT;
             ALTER TABLE people ADD COLUMN bio TEXT;
             ALTER TABLE people ADD COLUMN is_user INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE people ADD COLUMN follows_visitor INTEGER NOT NULL DEFAULT 0;
             CREATE UNIQUE INDEX IF NOT EXISTS people_by_handle ON people(handle);",
        )?;
    }
    Ok(())
}

/// Whether a table already has a column. The table name is interpolated because
/// PRAGMA takes no bind parameters; every call site passes a literal.
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names: Vec<String> =
        stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<_>>()?;
    Ok(names.iter().any(|name| name == column))
}

// --- Seeding ------------------------------------------------------------------

/// One person to seed into the social graph, and their reviews.
///
/// Built by `content::harvest_graph` from TMDB in one pass at startup, or by
/// `demo_graph` below without a token. `db` does no networking, so this is the
/// shape the two sources agree on.
#[derive(Debug, Clone)]
pub struct SeedUser {
    pub id: String,
    /// Without the `@`; `seed_graph` adds it, so no caller can store a bare one.
    pub handle: String,
    pub name: String,
    pub avatar: Image,
    pub bio: Option<String>,
    pub follows_visitor: bool,
    /// Whether the visitor already follows them, so the app has friends on first run.
    pub followed_by_visitor: bool,
    /// `(movie_id, half_stars, body, created_at)`.
    pub reviews: Vec<(String, u8, String, String)>,
    /// Films they call favourites, best first. Derived from their own reviews by
    /// both callers rather than invented: a favourite they never wrote about would
    /// be a poster with nothing behind it, and their page shows both.
    pub favorites: Vec<String>,
    /// Films they mean to watch. Everything they *haven't* reviewed, so a person's
    /// watchlist and their reviews never name the same film — which is what makes
    /// the two strips on their page say different things.
    pub watchlist: Vec<String>,
}

/// Whether the graph still needs seeding.
///
/// Asked *before* the harvest, not just inside it: the harvest is a dozen HTTP
/// calls, and every restart after the first would otherwise make all of them only
/// for `seed_graph` to discard the results.
pub fn needs_graph_seed(conn: &Connection) -> Result<bool> {
    let users: i64 =
        conn.query_row("SELECT COUNT(*) FROM people WHERE is_user = 1", [], |row| row.get(0))?;
    Ok(users == 0)
}

/// Populate the social graph, once, into a database that has none.
///
/// Runs after `prepare`, not inside it, because it needs the network and the schema
/// is applied before the TMDB client exists. Guarded on `is_user` rather than on
/// `people` being non-empty, because a database written by an earlier build already
/// holds the export's eleven decorative rows and would otherwise never seed. Once
/// anyone is in there the graph is the visitor's, and seeding again would talk over
/// their follows — so this is one shot, and returns `Ok(0)` afterwards.
///
/// `INSERT OR IGNORE` on every row, for collisions *within* one list rather than
/// across runs: ids come from slugified TMDB nicknames, and two distinct
/// nicknames can slug to the same thing ("MSBReviews" and "msbreviews"). Ignoring
/// the duplicate folds them into one person, which is wrong-but-harmless; failing
/// the whole seed on it would leave the app with no friends at all.
pub fn seed_graph(conn: &Connection, users: &[SeedUser]) -> Result<usize> {
    if !needs_graph_seed(conn)? {
        return Ok(0);
    }

    let mut written = 0;
    for (offset, user) in users.iter().enumerate() {
        conn.execute(
            // `position` is written even though nothing reads it any more: a database
            // created by an earlier build declares it `NOT NULL` with no default, and
            // `CREATE TABLE IF NOT EXISTS` cannot add the default this schema now
            // gives it. Omitting it there would make `OR IGNORE` swallow a NOT NULL
            // violation on every person — seeding no-one, then failing the follow
            // below on a foreign key. Which is exactly what it did.
            "INSERT OR IGNORE INTO people
                 (id, name, avatar_src, avatar_alt, position,
                  handle, bio, is_user, follows_visitor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
            params![
                user.id,
                user.name,
                user.avatar.src,
                user.avatar.alt,
                offset as i64,
                format!("@{}", user.handle.trim_start_matches('@')),
                user.bio,
                user.follows_visitor,
            ],
        )?;
        if user.followed_by_visitor {
            conn.execute(
                "INSERT OR IGNORE INTO follows (person_id) VALUES (?1)",
                params![user.id],
            )?;
        }
        for (movie_id, half_stars, body, created_at) in &user.reviews {
            conn.execute(
                "INSERT OR IGNORE INTO user_reviews
                     (person_id, movie_id, half_stars, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![user.id, movie_id, half_stars, body, created_at],
            )?;
        }
        for (rank, movie_id) in user.favorites.iter().enumerate() {
            conn.execute(
                "INSERT OR IGNORE INTO user_favorites (person_id, movie_id, position)
                 VALUES (?1, ?2, ?3)",
                params![user.id, movie_id, rank as i64],
            )?;
        }
        for (rank, movie_id) in user.watchlist.iter().enumerate() {
            conn.execute(
                "INSERT OR IGNORE INTO user_watchlist (person_id, movie_id, position)
                 VALUES (?1, ?2, ?3)",
                params![user.id, movie_id, rank as i64],
            )?;
        }
        written += 1;
    }

    Ok(written)
}

/// The graph to seed with when there is no token.
///
/// Demo mode has to exercise the same screens — a friend directory you can search,
/// people whose pages open, a film with several reviews ranked friends-first — so
/// it needs its own graph rather than an empty one. These eight take the first names
/// of the export's own cast, given surnames, handles and opinions on the demo
/// catalogue's films — so they are people with pages rather than the decorative rows
/// that used to fill the stories rail.
///
/// The film ids here are `data::catalogue` ids, which is the one place the demo
/// graph has to stay in step with `data`; `content`'s tests assert the overlap.
/// Their avatars are monograms rather than the export's photographs, matching how
/// TMDB-seeded users with no `avatar_path` are drawn — one visual language for
/// "user of the app", distinct from the export's decorative rails.
pub fn demo_graph() -> Vec<SeedUser> {
    // The demo catalogue is the pool their watchlists are drawn from, the same way
    // the harvest draws from the trending films it read reviews off.
    let pool: Vec<String> = crate::data::catalogue().into_iter().map(|entry| entry.id).collect();

    GRAPH
        .iter()
        .enumerate()
        .map(|(seat, person)| {
            let reviews: Vec<(String, u8, String, String)> = person
                .reviews
                .iter()
                .enumerate()
                .map(|(i, (film, stars, body))| {
                    ((*film).into(), *stars, (*body).into(), REVIEW_DATES[i].into())
                })
                .collect();
            let (favorites, watchlist) = derive_taste(&reviews, &pool, seat);

            SeedUser {
                id: format!("user-{}", person.handle),
                handle: person.handle.into(),
                name: person.name.into(),
                avatar: crate::tmdb::map::monogram(person.name),
                bio: Some(person.bio.into()),
                follows_visitor: person.follows_visitor,
                followed_by_visitor: person.followed,
                reviews,
                favorites,
                watchlist,
            }
        })
        .collect()
}

/// How many favourites and watchlist films a seeded person gets. Matches what the
/// profile's two strips draw, so a person's page is full rather than half-empty.
pub const TASTE_FAVORITES: usize = 4;
pub const TASTE_WATCHLIST: usize = 6;

/// The lowest rating that can make a film someone's favourite — 3½ stars.
///
/// Without a floor, "their best-rated films" would make a favourite out of a two-star
/// review for anyone who has only panned things, and a page claiming someone's
/// favourite film is one they called "an idea held at arm's length" is worse than a
/// page saying they have none. A hard-to-please seeded person legitimately has an
/// empty strip, which is also the only way to see that empty state on a person's page.
const FAVORITE_FLOOR: u8 = 7;

/// Turn one person's reviews into the two strips their page shows.
///
/// Shared by `demo_graph` and `content::harvest_graph` because neither source has
/// these upstream — TMDB has no favourites and no watchlists — and two derivations
/// would drift. Favourites come from what they wrote, so every poster on their page
/// has an opinion behind it; the watchlist is films they have *not* written about, so
/// the two strips never name the same film.
///
/// `seat` is their position in the graph, and rotates where in the pool their
/// watchlist starts: without it every seeded person would want to watch the same six
/// films, which reads as a placeholder the moment you open two pages.
pub fn derive_taste(
    reviews: &[(String, u8, String, String)],
    pool: &[String],
    seat: usize,
) -> (Vec<String>, Vec<String>) {
    let mut best: Vec<&(String, u8, String, String)> =
        reviews.iter().filter(|(_, stars, _, _)| *stars >= FAVORITE_FLOOR).collect();
    // Highest first; `reviews` is already newest-first, and a stable sort keeps that
    // as the tiebreak, so two 9s come back most-recently-written first.
    best.sort_by(|a, b| b.1.cmp(&a.1));
    let favorites: Vec<String> =
        best.into_iter().take(TASTE_FAVORITES).map(|(id, _, _, _)| id.clone()).collect();

    let reviewed: std::collections::HashSet<&str> =
        reviews.iter().map(|(id, _, _, _)| id.as_str()).collect();
    let watchlist: Vec<String> = if pool.is_empty() {
        Vec::new()
    } else {
        // Coprime with most pool sizes, so consecutive seats start well apart rather
        // than one film apart.
        let start = (seat * 5) % pool.len();
        pool.iter()
            .cycle()
            .skip(start)
            .take(pool.len())
            .filter(|id| !reviewed.contains(id.as_str()))
            .take(TASTE_WATCHLIST)
            .cloned()
            .collect()
    };

    (favorites, watchlist)
}

/// One invented user of the app, for demo mode.
struct DemoUser {
    /// Without the `@`, matching `SeedUser`.
    handle: &'static str,
    name: &'static str,
    bio: &'static str,
    follows_visitor: bool,
    /// Whether the visitor starts out following them.
    followed: bool,
    /// `(film id from `data::catalogue`, half-stars, prose)`.
    reviews: &'static [(&'static str, u8, &'static str)],
}

/// Fixed dates rather than "now", so a deleted database comes back identical and a
/// test can assert on the ordering. Newest first, since `reviews_by_person` sorts
/// on `created_at` and a person's page should open on their latest. Long enough for
/// the most prolific person below, and `demo_graph` indexes it directly so adding a
/// sixth review to someone is a compile-time-obvious change rather than a silent
/// duplicate date.
const REVIEW_DATES: [&str; 5] = [
    "2024-03-15T10:00:00Z",
    "2024-02-02T10:00:00Z",
    "2023-12-19T10:00:00Z",
    "2023-11-04T10:00:00Z",
    "2023-09-21T10:00:00Z",
];

/// The demo graph's cast, sharing the export's first names with `PEOPLE` above.
///
/// Four of the eight are followed and four follow back in a deliberately uneven
/// pattern: one mutual, one you follow who doesn't follow back, one who follows you
/// unrequited, and one stranger — every combination the follow button and the
/// "follows you" badge have to render.
const GRAPH: [DemoUser; 8] = [
    DemoUser {
        handle: "elenarostova",
        name: "Elena Rostova",
        bio: "5 films reviewed · generous ratings",
        follows_visitor: true,
        followed: true,
        reviews: &[
            ("dune-part-two", 9, "Villeneuve builds a world you can feel the grit of. The Harkonnen arena sequence is the most striking twenty minutes of the decade."),
            ("silence-of-space", 9, "A film about absence that never once feels empty. The long silences do the work three pages of dialogue would have fumbled."),
            ("neon-reverie", 8, "Gorgeous, and about half an hour too in love with its own reflections."),
            ("le-souffle", 10, "Sixty years on and it still feels like it was shot yesterday afternoon."),
            ("blue-notes", 7, "The music is extraordinary. The talking heads around it are less so."),
        ],
    },
    DemoUser {
        handle: "marcusdrey",
        name: "Marcus Drey",
        bio: "4 films reviewed · hard to please",
        follows_visitor: true,
        followed: true,
        reviews: &[
            ("dune-part-two", 7, "Completely agree about the sound design. Less sure about the sietch politics, which stall the middle hour."),
            ("the-drop", 4, "Ninety minutes of a very good cinematographer photographing a script nobody finished."),
            ("neon-reverie", 6, "Style as substance only works if the style is saying something. Here it is mostly saying \"look\"."),
            ("endless", 9, "The last shot recontextualises everything before it, which is a trick that only works once and works completely here."),
        ],
    },
    DemoUser {
        handle: "sarahkline",
        name: "Sarah Kline",
        bio: "4 films reviewed · generous ratings",
        follows_visitor: false,
        followed: true,
        reviews: &[
            ("morning-haze", 10, "I have watched this four times and found a different film each time. The dawn sequence is perfect."),
            ("silence-of-space", 8, "Cold on the surface, enormously warm underneath. The score does a lot of that."),
            ("estate-of-mind", 9, "A period drama that trusts you to notice things. Rare and welcome."),
            ("fractured", 7, "Beautifully acted, a little too pleased with its own restraint."),
        ],
    },
    DemoUser {
        handle: "davidpell",
        name: "David Pell",
        bio: "3 films reviewed · middling ratings",
        follows_visitor: true,
        followed: true,
        reviews: &[
            ("le-souffle", 9, "The blocking in the café scene is a masterclass. Everything the New Wave was for, in four minutes."),
            ("the-horizon", 6, "A striking image stretched to feature length."),
            ("red-shift", 7, "Cheap in all the right places and expensive in exactly one, which is the correct way round."),
        ],
    },
    DemoUser {
        handle: "annaveil",
        name: "Anna Veil",
        bio: "3 films reviewed · generous ratings",
        follows_visitor: false,
        followed: true,
        reviews: &[
            ("architecture-of-silence", 9, "Buildings as biography. I did not expect to be moved by concrete and I was."),
            ("blue-notes", 8, "The interviews are thin but the archive footage is worth the ticket twice over."),
            ("void-geometry", 8, "Tense in a way that has nothing to do with the plot and everything to do with the framing."),
        ],
    },
    DemoUser {
        handle: "tomasrey",
        name: "Tomas Rey",
        bio: "3 films reviewed · hard to please",
        follows_visitor: true,
        followed: false,
        reviews: &[
            ("void-geometry", 5, "An idea, held at arm's length for a hundred minutes."),
            ("event-horizon-echoes", 6, "Handsome, derivative, and perfectly watchable at eleven at night."),
            ("project-kepler", 4, "Someone should tell them the mystery only works if there is an answer."),
        ],
    },
    DemoUser {
        handle: "nadiacourt",
        name: "Nadia Court",
        bio: "2 films reviewed · generous ratings",
        follows_visitor: true,
        followed: false,
        reviews: &[
            ("solitude-of-orbits", 9, "The quietest science fiction film I have seen, and the one I have thought about longest."),
            ("morning-haze", 9, "Two people not saying what they mean, for ninety minutes, brilliantly."),
        ],
    },
    DemoUser {
        handle: "priyanaidu",
        name: "Priya Naidu",
        bio: "2 films reviewed · middling ratings",
        follows_visitor: false,
        followed: false,
        reviews: &[
            ("dune-part-two", 8, "Enormous, and it earns the size. I could have used one fewer sandworm."),
            ("estate-of-mind", 6, "The fog is doing a great deal of the acting."),
        ],
    },
];

// --- Reading the social layer -------------------------------------------------

/// One row of the profile's "Following" list.
///
/// It used to carry the id of this person's activity-rail row, which was where its
/// subtitle came from. That rail is gone — its verbs described events nothing
/// recorded — so the subtitle is now built from `bio` and `review_count`, both of
/// which are facts about what this person actually wrote.
#[derive(Debug, Clone)]
pub struct FollowRow {
    pub id: String,
    pub name: String,
    pub avatar: Image,
    /// How the UI links to their page. `Option` because the column is nullable for
    /// databases seeded before every person here was a user.
    pub handle: Option<String>,
    /// "5 films reviewed · generous ratings", written at harvest.
    pub bio: Option<String>,
    /// Counted live rather than read off `bio`, so it stays true if they write more.
    pub review_count: u32,
}

/// The profile's "Following" list: the people the visitor really follows, newest
/// first.
///
/// Only `follows` rows. It used to also list everyone on the export's stories and
/// activity rails, on the reasoning that the rails were the only friends the app
/// had — but now that following is a real, clickable act, a profile counting twelve
/// while the friend directory counts five is just a lie about the same fact. Those
/// rails are gone entirely, and the feed's rails are built from this same list.
pub fn following(conn: &Connection) -> Result<Vec<FollowRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id AS id, p.name AS name, p.avatar_src AS avatar_src,
                p.avatar_alt AS avatar_alt, p.handle AS handle, p.bio AS bio,
                (SELECT COUNT(*) FROM user_reviews r WHERE r.person_id = p.id)
                    AS review_count
         FROM people p
         JOIN follows f ON f.person_id = p.id
         ORDER BY f.followed_at DESC, p.name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FollowRow {
            id: row.get("id")?,
            name: row.get("name")?,
            avatar: Image::new(
                &row.get::<_, String>("avatar_src")?,
                &row.get::<_, String>("avatar_alt")?,
            ),
            handle: row.get("handle")?,
            bio: row.get("bio")?,
            review_count: row.get("review_count")?,
        })
    })?;
    rows.collect()
}

/// One user of the app: a followable person with a page of their own.
#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Image,
    pub bio: Option<String>,
    pub following: bool,
    pub follows_you: bool,
    pub review_count: u32,
}

/// The columns every user query selects, and the joins that compute the two
/// relationship flags. Shared as a string because four queries differ only in
/// their `WHERE` and `ORDER BY`, and a divergence between them would show up as
/// a follow button that disagrees with itself between two screens.
const USER_SELECT: &str = "SELECT p.id AS id, p.name AS name, p.handle AS handle,
            p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt, p.bio AS bio,
            p.follows_visitor AS follows_visitor,
            f.person_id IS NOT NULL AS following,
            (SELECT COUNT(*) FROM user_reviews r WHERE r.person_id = p.id) AS review_count
     FROM people p LEFT JOIN follows f ON f.person_id = p.id";

fn user_from_row(row: &rusqlite::Row) -> Result<UserRow> {
    Ok(UserRow {
        id: row.get("id")?,
        name: row.get("name")?,
        handle: row.get("handle")?,
        avatar: Image::new(&row.get::<_, String>("avatar_src")?, &row.get::<_, String>("avatar_alt")?),
        bio: row.get("bio")?,
        following: row.get("following")?,
        follows_you: row.get("follows_visitor")?,
        review_count: row.get("review_count")?,
    })
}

/// Find users by nickname or by name.
///
/// Matches `handle` and `name` both, because a nickname is what you know someone
/// by but a name is what you remember — searching "@msb" and "Sofia" should each
/// work. `LIKE` with an escaped pattern rather than FTS: the directory is dozens
/// of rows, not thousands.
///
/// An empty query lists everyone, so the screen opens on a browsable directory
/// rather than a blank slate.
pub fn search_people(conn: &Connection, query: &str) -> Result<Vec<UserRow>> {
    // `_` and `%` in a user's query would otherwise act as wildcards, so "a_b"
    // would match "axb". ESCAPE makes them literal.
    let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{}%", escaped.trim_start_matches('@'));
    let mut stmt = conn.prepare(&format!(
        "{USER_SELECT}
         WHERE p.is_user = 1
           AND (?1 = '' OR p.handle LIKE ?2 ESCAPE '\\' OR p.name LIKE ?2 ESCAPE '\\')
         ORDER BY following DESC, review_count DESC, p.name"
    ))?;
    let rows = stmt.query_map(params![query.trim(), pattern], user_from_row)?.collect();
    rows
}

/// One user by nickname, with or without the leading `@`.
pub fn person_by_handle(conn: &Connection, handle: &str) -> Result<Option<UserRow>> {
    let handle = format!("@{}", handle.trim_start_matches('@'));
    let mut stmt = conn.prepare(&format!("{USER_SELECT} WHERE p.handle = ?1"))?;
    let row = stmt.query_row(params![handle], user_from_row).optional()?;
    Ok(row)
}

/// One user by id — what the follow endpoint takes, since a button knows the id.
pub fn person_by_id(conn: &Connection, id: &str) -> Result<Option<UserRow>> {
    let mut stmt = conn.prepare(&format!("{USER_SELECT} WHERE p.id = ?1 AND p.is_user = 1"))?;
    let row = stmt.query_row(params![id], user_from_row).optional()?;
    Ok(row)
}

/// The users the visitor follows, most recently followed first.
pub fn followed_users(conn: &Connection) -> Result<Vec<UserRow>> {
    let mut stmt = conn.prepare(&format!(
        "{USER_SELECT} WHERE f.person_id IS NOT NULL ORDER BY f.followed_at DESC, p.name"
    ))?;
    let rows = stmt.query_map([], user_from_row)?.collect();
    rows
}

/// The users who follow the visitor.
pub fn followers(conn: &Connection) -> Result<Vec<UserRow>> {
    let mut stmt = conn.prepare(&format!(
        "{USER_SELECT} WHERE p.follows_visitor = 1 AND p.is_user = 1
         ORDER BY following DESC, p.name"
    ))?;
    let rows = stmt.query_map([], user_from_row)?.collect();
    rows
}

/// Follow or unfollow one person. Returns the new state, or `None` if no such user.
///
/// `target` makes it idempotent, as `set_watchlist` is: the button sends the state
/// it wants rather than "flip it", so a double-tap or a retried request can't land
/// the UI and the DB on opposite answers. Pass `None` to toggle.
pub fn set_follow(conn: &Connection, person_id: &str, target: Option<bool>) -> Result<Option<bool>> {
    if person_by_id(conn, person_id)?.is_none() {
        return Ok(None);
    }
    let now: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM follows WHERE person_id = ?1)",
        params![person_id],
        |row| row.get(0),
    )?;
    let next = target.unwrap_or(!now);
    if next {
        conn.execute("INSERT OR IGNORE INTO follows (person_id) VALUES (?1)", params![person_id])?;
    } else {
        conn.execute("DELETE FROM follows WHERE person_id = ?1", params![person_id])?;
    }
    Ok(Some(next))
}

/// How many people the visitor follows.
pub fn follow_count(conn: &Connection) -> Result<u32> {
    conn.query_row("SELECT COUNT(*) FROM follows", [], |row| row.get(0))
}

/// One seeded review, with its author's details joined in.
#[derive(Debug, Clone)]
pub struct UserReviewRow {
    pub person_id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Image,
    pub followed: bool,
    pub movie_id: String,
    pub half_stars: u8,
    pub body: String,
    pub created_at: String,
}

fn review_from_row(row: &rusqlite::Row) -> Result<UserReviewRow> {
    Ok(UserReviewRow {
        person_id: row.get("person_id")?,
        name: row.get("name")?,
        handle: row.get("handle")?,
        avatar: Image::new(&row.get::<_, String>("avatar_src")?, &row.get::<_, String>("avatar_alt")?),
        followed: row.get("followed")?,
        movie_id: row.get("movie_id")?,
        half_stars: row.get("half_stars")?,
        body: row.get("body")?,
        created_at: row.get("created_at")?,
    })
}

const REVIEW_SELECT: &str = "SELECT r.person_id AS person_id, p.name AS name, p.handle AS handle,
            p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
            f.person_id IS NOT NULL AS followed,
            r.movie_id AS movie_id, r.half_stars AS half_stars,
            r.body AS body, r.created_at AS created_at
     FROM user_reviews r
     JOIN people p ON p.id = r.person_id
     LEFT JOIN follows f ON f.person_id = r.person_id";

/// The reviews of one film, **the people the visitor follows first**.
///
/// That ordering is the whole point of the film page's section: a friend's opinion
/// outranks a stranger's, and within each group the highest-rated comes first so
/// what you see is someone recommending the film rather than the most recent
/// passer-by. `person_id` breaks the final tie, since `created_at` is seeded at
/// one-day resolution.
pub fn reviews_for_movie(conn: &Connection, movie_id: &str) -> Result<Vec<UserReviewRow>> {
    let mut stmt = conn.prepare(&format!(
        "{REVIEW_SELECT} WHERE r.movie_id = ?1
         ORDER BY followed DESC, r.half_stars DESC, r.created_at DESC, r.person_id"
    ))?;
    let rows = stmt.query_map(params![movie_id], review_from_row)?.collect();
    rows
}

/// One person's reviews, newest first.
pub fn reviews_by_person(conn: &Connection, person_id: &str) -> Result<Vec<UserReviewRow>> {
    let mut stmt = conn.prepare(&format!(
        "{REVIEW_SELECT} WHERE r.person_id = ?1 ORDER BY r.created_at DESC, r.movie_id"
    ))?;
    let rows = stmt.query_map(params![person_id], review_from_row)?.collect();
    rows
}

/// A review's wire id. One place, because a card, the page it opens and every
/// mutation on that page all have to agree on the string.
pub fn review_id(person_id: &str, movie_id: &str) -> String {
    format!("{person_id}-{movie_id}")
}

/// One review by its wire id — `<person_id>-<movie_id>`, as `review_id` mints it.
///
/// The join is done in SQL rather than by splitting the string, and that is not a
/// stylistic choice: both halves contain hyphens (`user-elenarostova` and
/// `dune-part-two`), so no split position is knowable from the id alone. Matching
/// the concatenation lets the primary key decide, which is the only authority that
/// can.
pub fn review_by_id(conn: &Connection, review_id: &str) -> Result<Option<UserReviewRow>> {
    let mut stmt =
        conn.prepare(&format!("{REVIEW_SELECT} WHERE r.person_id || '-' || r.movie_id = ?1"))?;
    stmt.query_row(params![review_id], review_from_row).optional()
}

/// The newest reviews across the whole graph, **the people the visitor follows
/// first** — what the review screen opens on when no id names one.
pub fn recent_reviews(conn: &Connection, limit: u32) -> Result<Vec<UserReviewRow>> {
    let mut stmt = conn.prepare(&format!(
        "{REVIEW_SELECT}
         ORDER BY followed DESC, r.created_at DESC, r.half_stars DESC, r.person_id, r.movie_id
         LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![limit], review_from_row)?.collect();
    rows
}

/// The newest reviews by **the people the visitor follows**, and nobody else.
///
/// Strictly a `JOIN` on `follows`, unlike `recent_reviews`'s ordering trick: the feed
/// says these are your friends' reviews, so a stranger's appearing there because the
/// graph was thin would make the heading a lie. An empty result is the honest answer
/// for someone who follows nobody, and the screen says so.
pub fn reviews_from_followed(conn: &Connection, limit: u32) -> Result<Vec<UserReviewRow>> {
    let mut stmt = conn.prepare(&format!(
        "{REVIEW_SELECT}
         WHERE f.person_id IS NOT NULL
         ORDER BY r.created_at DESC, r.half_stars DESC, r.person_id, r.movie_id
         LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![limit], review_from_row)?.collect();
    rows
}

/// One followed person and the id of their newest review — a stories circle.
#[derive(Debug, Clone)]
pub struct StoryRow {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Image,
    /// `<person_id>-<movie_id>`, or `None` for someone who hasn't written anything.
    pub newest_review: Option<String>,
}

/// The people the visitor follows, each with their newest review, most recently
/// followed first.
///
/// The rail is the follow list — not a separate seeded `in_stories` set, which is what
/// it used to be and which meant the circles on screen had no relationship to
/// anyone you'd actually chosen. People with something to show come first, because a
/// rail of dimmed unlinked circles is the one arrangement that teaches nothing.
pub fn followed_with_newest_review(conn: &Connection, limit: u32) -> Result<Vec<StoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id AS id, p.name AS name, p.handle AS handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
                (SELECT r.person_id || '-' || r.movie_id FROM user_reviews r
                 WHERE r.person_id = p.id
                 ORDER BY r.created_at DESC, r.movie_id LIMIT 1) AS newest_review
         FROM people p JOIN follows f ON f.person_id = p.id
         WHERE p.is_user = 1
         ORDER BY newest_review IS NULL, f.followed_at DESC, p.name
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(StoryRow {
            id: row.get("id")?,
            name: row.get("name")?,
            handle: row.get("handle")?,
            avatar: Image::new(
                &row.get::<_, String>("avatar_src")?,
                &row.get::<_, String>("avatar_alt")?,
            ),
            newest_review: row.get("newest_review")?,
        })
    })?;
    rows.collect()
}

/// One person's favourite films, in the order they were seeded.
pub fn favorites_by_person(conn: &Connection, person_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT movie_id FROM user_favorites WHERE person_id = ?1 ORDER BY position")?;
    let ids = stmt.query_map(params![person_id], |row| row.get(0))?.collect();
    ids
}

/// One person's watchlist, in the order it was seeded.
pub fn watchlist_by_person(conn: &Connection, person_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT movie_id FROM user_watchlist WHERE person_id = ?1 ORDER BY position")?;
    let ids = stmt.query_map(params![person_id], |row| row.get(0))?.collect();
    ids
}

// No per-person follower/following counts here, deliberately. The graph stores the
// visitor's own edges and nothing else — nobody follows anybody but the visitor —
// so any such count is 0 or 1, and a person's page printing "1 followers" would be
// dressing up `follows_visitor` as a statistic. See `models::PersonProfile`.

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

/// One entry in the profile's "Recent Reviews" tile: a film the visitor rated, or
/// wrote about, or both.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalRow {
    pub movie_id: String,
    /// `None` for a film they wrote about without scoring.
    pub half_stars: Option<u8>,
    /// `None` for a film they scored without writing about.
    pub body: Option<String>,
}

/// Everything the visitor has logged about a film, newest first.
///
/// The union of `ratings` and `visitor_reviews` rather than either alone, because
/// both are events worth listing and neither implies the other: rating a film and
/// writing about it are separate acts, and a tile driven by only one of them would
/// silently drop half of what the visitor did.
///
/// Ordered on whichever of the two happened later, so editing a review moves the
/// film to the top exactly as re-rating it does. `rated_at` is NULL for rows written
/// before that column existed; `COALESCE` to the empty string makes those sort last
/// rather than dropping the row.
pub fn journal_recent_first(conn: &Connection) -> Result<Vec<JournalRow>> {
    let mut stmt = conn.prepare(
        "SELECT ids.movie_id AS movie_id, r.half_stars AS half_stars, v.body AS body,
                MAX(COALESCE(r.rated_at, ''), COALESCE(v.written_at, '')) AS logged_at
         FROM (SELECT movie_id FROM ratings UNION SELECT movie_id FROM visitor_reviews) ids
         LEFT JOIN ratings r ON r.movie_id = ids.movie_id
         LEFT JOIN visitor_reviews v ON v.movie_id = ids.movie_id
         ORDER BY logged_at DESC, ids.movie_id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(JournalRow {
            movie_id: row.get("movie_id")?,
            half_stars: row.get("half_stars")?,
            body: row.get("body")?,
        })
    })?;
    rows.collect()
}

/// The visitor's favourite films, most recently added first — the same order the
/// watchlist strip beside it uses.
pub fn favorites_recent_first(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT movie_id FROM favorites ORDER BY added_at DESC, movie_id DESC")?;
    let ids = stmt.query_map([], |row| row.get(0))?.collect();
    ids
}

/// Add, remove, or toggle a favourite. Returns the resulting state.
///
/// Idempotent on a stated target, as `set_watchlist` is — and for the same reason:
/// the heart sends the state it wants, so a double-tap can't land the button and the
/// row on opposite answers.
pub fn set_favorite(conn: &Connection, movie_id: &str, target: Option<bool>) -> Result<bool> {
    let present: bool = conn
        .query_row("SELECT 1 FROM favorites WHERE movie_id = ?1", [movie_id], |_| Ok(()))
        .optional()?
        .is_some();

    let target = target.unwrap_or(!present);
    if target {
        conn.execute("INSERT OR IGNORE INTO favorites (movie_id) VALUES (?1)", [movie_id])?;
    } else {
        conn.execute("DELETE FROM favorites WHERE movie_id = ?1", [movie_id])?;
    }
    Ok(target)
}

/// Write or rewrite the visitor's review of a film. An empty body deletes it, which
/// is how the composer clears one.
///
/// `written_at` is refreshed on a rewrite, so an edited review moves to the top of
/// the profile: the writing is the event, exactly as `set_rating` treats the rating.
pub fn set_visitor_review(conn: &Connection, movie_id: &str, body: &str) -> Result<Option<String>> {
    let body = body.trim();
    if body.is_empty() {
        conn.execute("DELETE FROM visitor_reviews WHERE movie_id = ?1", [movie_id])?;
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO visitor_reviews (movie_id, body, written_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(movie_id) DO UPDATE
             SET body = excluded.body, written_at = excluded.written_at",
        params![movie_id, body],
    )?;
    Ok(Some(body.to_string()))
}

/// The `settings` key the visitor's bio is stored under.
const BIO_KEY: &str = "visitor_bio";

/// The visitor's bio, or `None` if they've never edited it — in which case the
/// caller uses `hydrate::VISITOR_BIO`, the export's own line. Storing the default
/// eagerly would make "never edited" and "edited back to the original" the same
/// state, and the second one should stick even if the constant later changes.
pub fn visitor_bio(conn: &Connection) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [BIO_KEY], |row| row.get(0))
        .optional()
}

/// Store the visitor's bio. An empty string clears it, restoring the export's line.
pub fn set_visitor_bio(conn: &Connection, bio: &str) -> Result<Option<String>> {
    let bio = bio.trim();
    if bio.is_empty() {
        conn.execute("DELETE FROM settings WHERE key = ?1", [BIO_KEY])?;
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![BIO_KEY, bio],
    )?;
    Ok(Some(bio.to_string()))
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

    let mut stmt = conn.prepare("SELECT movie_id FROM favorites ORDER BY added_at, movie_id")?;
    for id in stmt.query_map([], |row| row.get::<_, String>(0))? {
        store.favorites.insert(id?);
    }

    let mut stmt = conn.prepare("SELECT movie_id, half_stars FROM ratings")?;
    for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?)))? {
        let (id, half_stars) = row?;
        store.ratings.insert(id, half_stars);
    }

    let mut stmt = conn.prepare("SELECT movie_id, body FROM visitor_reviews")?;
    for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (id, body) = row?;
        store.written_reviews.insert(id, body);
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

    /// A fresh database has no people at all now — the eleven decorative rows the
    /// old `seed` wrote were there to fill a stories rail and an activity sidebar,
    /// and both are gone. Applying the schema twice must still be a no-op.
    #[test]
    fn the_schema_applies_twice_and_starts_with_nobody() {
        let conn = db();
        let people = |conn: &Connection| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM people", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(people(&conn), 0, "a fresh database invents nobody");
        assert!(needs_graph_seed(&conn).unwrap());

        // What a restart does.
        prepare(&conn).unwrap();
        assert_eq!(people(&conn), 0);
    }

    /// The two feed rails, which replaced the invented ones. Both read the follow
    /// graph, and both must be empty for a visitor who follows nobody rather than
    /// falling back to strangers — the headings name your friends.
    #[test]
    fn the_feed_rails_are_the_follow_graph() {
        let conn = db();
        assert!(reviews_from_followed(&conn, 10).unwrap().is_empty());
        assert!(followed_with_newest_review(&conn, 10).unwrap().is_empty());

        seed_graph(&conn, &demo_graph()).unwrap();

        // `demo_graph` has the visitor following some but not all of its cast, and
        // only the followed ones may appear.
        let followed: Vec<String> =
            following(&conn).unwrap().into_iter().map(|row| row.id).collect();
        assert!(!followed.is_empty(), "the demo graph seeds some follows");

        let reviews = reviews_from_followed(&conn, 50).unwrap();
        assert!(!reviews.is_empty());
        for review in &reviews {
            assert!(
                followed.contains(&review.person_id),
                "{} is not followed but appears in the friends rail",
                review.person_id
            );
        }
        // Newest first, which is what makes the rail a feed.
        let dates: Vec<&str> = reviews.iter().map(|r| r.created_at.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(dates, sorted);

        // A story circle per followed person, people with something to open first,
        // and `Image::new` has made the avatar path servable.
        let stories = followed_with_newest_review(&conn, 50).unwrap();
        assert_eq!(stories.len(), followed.len());
        assert!(stories[0].newest_review.is_some());
        assert!(stories[0].avatar.src.starts_with('/') || stories[0].avatar.src.contains("://"));
        let first_blank = stories.iter().position(|s| s.newest_review.is_none());
        if let Some(at) = first_blank {
            assert!(
                stories[at..].iter().all(|s| s.newest_review.is_none()),
                "linkable circles must all come before the dimmed ones"
            );
        }
        // The id a circle opens is a review that really exists.
        let review_id = stories[0].newest_review.clone().unwrap();
        assert!(review_by_id(&conn, &review_id).unwrap().is_some());
    }

    /// The "Following" list's subtitle is built from these two, so both have to
    /// survive the round trip.
    #[test]
    fn following_carries_a_bio_and_a_live_review_count() {
        let conn = db();
        seed_graph(&conn, &demo_graph()).unwrap();

        let rows = following(&conn).unwrap();
        let row = rows.iter().find(|r| r.review_count > 0).expect("someone reviewed something");
        assert!(row.handle.as_deref().unwrap_or_default().starts_with('@'));
        assert!(row.bio.is_some());

        let counted: u32 =
            reviews_by_person(&conn, &row.id).unwrap().len().try_into().unwrap();
        assert_eq!(row.review_count, counted);
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

    /// Re-rating moves a film to the front of the journal: the rating is the event.
    #[test]
    fn re_rating_a_film_moves_it_to_the_front_of_the_journal() {
        let conn = db();
        // Same second for all three, so the id tiebreak decides the initial order —
        // deterministic rather than whatever SQLite feels like.
        set_rating(&conn, "middling", 6).unwrap();
        set_rating(&conn, "great", 10).unwrap();
        set_rating(&conn, "good", 8).unwrap();
        assert_eq!(
            journal_recent_first(&conn).unwrap().iter().map(|r| r.movie_id.as_str()).collect::<Vec<_>>(),
            ["middling", "great", "good"]
        );

        set_rating(&conn, "middling", 7).unwrap();
        let recent = journal_recent_first(&conn).unwrap();
        assert_eq!(recent[0].movie_id, "middling");
        assert_eq!(recent[0].half_stars, Some(7));
        assert_eq!(recent[0].body, None, "a rating alone carries no prose");
    }

    /// The heart is its own act, independent of the rating and of the watchlist.
    #[test]
    fn favouriting_toggles_and_is_idempotent_when_told_the_target() {
        let conn = db();
        assert!(favorites_recent_first(&conn).unwrap().is_empty());

        assert!(set_favorite(&conn, "le-souffle", Some(true)).unwrap());
        // Twice is still favourited, and still one row — the PK sees to that.
        assert!(set_favorite(&conn, "le-souffle", Some(true)).unwrap());
        assert_eq!(favorites_recent_first(&conn).unwrap(), ["le-souffle"]);

        assert!(!set_favorite(&conn, "le-souffle", Some(false)).unwrap());
        assert!(!set_favorite(&conn, "le-souffle", Some(false)).unwrap());
        assert!(favorites_recent_first(&conn).unwrap().is_empty());

        // No body toggles.
        assert!(set_favorite(&conn, "le-souffle", None).unwrap());
        assert!(!set_favorite(&conn, "le-souffle", None).unwrap());

        // And it left the neighbouring tables alone: a favourite is not a rating
        // and not a watchlist entry, which is the whole reason it has its own table.
        set_favorite(&conn, "le-souffle", Some(true)).unwrap();
        let store = load_store(&conn).unwrap();
        assert!(store.favorites.contains("le-souffle"));
        assert!(store.ratings.is_empty() && store.watchlist.is_empty());
    }

    #[test]
    fn favourites_read_back_newest_first() {
        let conn = db();
        for id in ["a-film", "b-film", "c-film"] {
            set_favorite(&conn, id, Some(true)).unwrap();
        }
        assert_eq!(favorites_recent_first(&conn).unwrap(), ["c-film", "b-film", "a-film"]);
    }

    /// Writing, rewriting and clearing. Blank deletes rather than storing an empty
    /// review, so a cleared composer leaves no trace on the profile.
    #[test]
    fn a_written_review_can_be_edited_and_cleared() {
        let conn = db();
        assert_eq!(set_visitor_review(&conn, "le-souffle", "  First pass.  ").unwrap(),
                   Some("First pass.".into()), "the body is trimmed on the way in");

        let journal = journal_recent_first(&conn).unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].body.as_deref(), Some("First pass."));
        assert_eq!(journal[0].half_stars, None, "prose without a score is allowed");

        assert_eq!(set_visitor_review(&conn, "le-souffle", "Second thoughts.").unwrap(),
                   Some("Second thoughts.".into()));
        assert_eq!(journal_recent_first(&conn).unwrap().len(), 1, "editing wrote a second row");

        assert_eq!(set_visitor_review(&conn, "le-souffle", "   ").unwrap(), None);
        assert!(journal_recent_first(&conn).unwrap().is_empty());
        assert!(load_store(&conn).unwrap().written_reviews.is_empty());
    }

    /// A rating and a review of the same film are one journal entry, and clearing
    /// either one leaves the other standing.
    #[test]
    fn a_rating_and_a_review_of_one_film_are_one_entry() {
        let conn = db();
        set_rating(&conn, "le-souffle", 9).unwrap();
        set_visitor_review(&conn, "le-souffle", "Worth the hype.").unwrap();

        let journal = journal_recent_first(&conn).unwrap();
        assert_eq!(journal.len(), 1, "the union double-counted the film");
        assert_eq!(journal[0].half_stars, Some(9));
        assert_eq!(journal[0].body.as_deref(), Some("Worth the hype."));

        // Clearing the rating must not delete what was written about it.
        set_rating(&conn, "le-souffle", 0).unwrap();
        let after = journal_recent_first(&conn).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].half_stars, None);
        assert_eq!(after[0].body.as_deref(), Some("Worth the hype."));

        // And the other way round.
        set_rating(&conn, "le-souffle", 6).unwrap();
        set_visitor_review(&conn, "le-souffle", "").unwrap();
        let last = journal_recent_first(&conn).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].half_stars, Some(6));
        assert_eq!(last[0].body, None);
    }

    /// An edit is an event too, so a rewritten review outranks a newer rating.
    #[test]
    fn the_journal_orders_on_whichever_happened_later() {
        let conn = db();
        // Written far enough apart to beat CURRENT_TIMESTAMP's one-second floor,
        // which is why these go in by hand rather than through the setters.
        conn.execute_batch(
            "INSERT INTO ratings (movie_id, half_stars, rated_at)
                 VALUES ('rated-today', 8, '2026-08-04 10:00:00');
             INSERT INTO visitor_reviews (movie_id, body, written_at)
                 VALUES ('written-later', 'Still thinking about it.', '2026-08-04 11:00:00');",
        )
        .unwrap();
        let ids: Vec<String> =
            journal_recent_first(&conn).unwrap().into_iter().map(|r| r.movie_id).collect();
        assert_eq!(ids, ["written-later", "rated-today"]);

        // Now rewrite the older film's review: the film moves to the front even
        // though its rating is untouched and older than the other entry.
        set_visitor_review(&conn, "rated-today", "Came back to it.").unwrap();
        let after: Vec<String> =
            journal_recent_first(&conn).unwrap().into_iter().map(|r| r.movie_id).collect();
        assert_eq!(after, ["rated-today", "written-later"]);
    }

    /// The bio is the one identity field the visitor owns. `None` means untouched,
    /// which is what lets `content` supply the export's line.
    #[test]
    fn the_bio_is_stored_and_clearing_it_restores_the_default() {
        let conn = db();
        assert_eq!(visitor_bio(&conn).unwrap(), None, "an untouched bio is absent, not blank");

        assert_eq!(set_visitor_bio(&conn, "  Watches too much. ").unwrap(),
                   Some("Watches too much.".into()));
        assert_eq!(visitor_bio(&conn).unwrap(), Some("Watches too much.".into()));

        // Editing replaces rather than accumulating: it's a key/value row.
        set_visitor_bio(&conn, "Second draft.").unwrap();
        assert_eq!(visitor_bio(&conn).unwrap(), Some("Second draft.".into()));
        let rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1);

        assert_eq!(set_visitor_bio(&conn, "  ").unwrap(), None);
        assert_eq!(visitor_bio(&conn).unwrap(), None);
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
            journal_recent_first(&conn).unwrap().into_iter().map(|row| row.movie_id).collect();
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

    /// The follow list is only real follows, and it is now the *only* list of people
    /// any screen draws — the export's rails, which listed people you had never
    /// chosen, are gone.
    #[test]
    fn following_is_only_who_you_really_follow() {
        let conn = db();
        assert!(following(&conn).unwrap().is_empty(), "no follows, no rows");

        seed_graph(&conn, &demo_graph()).unwrap();
        let rows = following(&conn).unwrap();
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids.len(), follow_count(&conn).unwrap() as usize);
        assert_eq!(ids.len(), GRAPH.iter().filter(|u| u.followed).count());

        // Nobody from the export's cast, named or not.
        for decorative in ["elena", "alex-m", "sarah-k", "live-a"] {
            assert!(!ids.contains(&decorative), "{decorative} was never followed");
        }
        // Every row links to a page and carries the line the profile prints under
        // their name, which used to come from the invented activity rail.
        assert!(rows.iter().all(|row| row.handle.is_some()));
        assert!(rows.iter().all(|row| row.bio.is_some()));
    }

    // --- The social graph -----------------------------------------------------

    /// A database with the demo graph in it, which is what demo mode boots with.
    fn graph() -> Connection {
        let conn = db();
        seed_graph(&conn, &demo_graph()).unwrap();
        conn
    }

    #[test]
    fn the_graph_seeds_once() {
        let conn = db();

        assert!(needs_graph_seed(&conn).unwrap());
        assert_eq!(seed_graph(&conn, &demo_graph()).unwrap(), GRAPH.len());
        assert!(!needs_graph_seed(&conn).unwrap());

        // A restart must not duplicate anyone. It must also not top anyone up: the
        // follows are the visitor's by then.
        assert_eq!(seed_graph(&conn, &demo_graph()).unwrap(), 0);
        assert_eq!(search_people(&conn, "").unwrap().len(), GRAPH.len());

        // Everyone the seed wrote is a user with a page. Nothing else is in the table
        // — the eleven decorative rows the old `seed` added are gone.
        let non_users: i64 =
            conn.query_row("SELECT COUNT(*) FROM people WHERE is_user = 0", [], |r| r.get(0))
                .unwrap();
        assert_eq!(non_users, 0);
    }

    /// A half-finished harvest leaves a usable graph, and a later run leaves it
    /// alone rather than talking over the visitor's follows.
    #[test]
    fn a_partial_seed_stands_and_is_not_topped_up() {
        let conn = db();
        let all = demo_graph();
        seed_graph(&conn, &all[..2]).unwrap();
        assert_eq!(search_people(&conn, "").unwrap().len(), 2);

        assert_eq!(seed_graph(&conn, &all).unwrap(), 0, "the graph was re-seeded");
        assert_eq!(search_people(&conn, "").unwrap().len(), 2);
        // The two who did land are complete — followable, with their reviews.
        assert!(!reviews_by_person(&conn, &all[0].id).unwrap().is_empty());
        assert_eq!(set_follow(&conn, &all[1].id, Some(true)).unwrap(), Some(true));
    }

    /// Two TMDB nicknames can slug to the same id ("MSBReviews" / "msbreviews").
    /// One of them wins and the seed completes; it must not fail outright and leave
    /// the app with no friends.
    #[test]
    fn colliding_ids_within_one_seed_do_not_abort_it() {
        let conn = db();
        let mut users = demo_graph();
        users[1].id = users[0].id.clone();
        users[1].handle = "@elenarostova".into();

        let written = seed_graph(&conn, &users).unwrap();
        assert_eq!(written, users.len(), "the collision aborted the seed");
        // The duplicate folded into the first, and everyone after it still landed.
        let stored = search_people(&conn, "").unwrap();
        assert_eq!(stored.len(), users.len() - 1);
        assert!(stored.iter().any(|u| u.handle == "@priyanaidu"));
    }

    #[test]
    fn handles_are_stored_with_an_at_sign_however_they_arrive() {
        let conn = db();
        let mut users = demo_graph();
        users[0].handle = "@alreadyprefixed".into();
        seed_graph(&conn, &users[..1]).unwrap();

        let stored = search_people(&conn, "").unwrap();
        assert_eq!(stored[0].handle, "@alreadyprefixed", "the `@` was doubled");
        // And it's findable either way, because `person_by_handle` normalizes too.
        assert!(person_by_handle(&conn, "alreadyprefixed").unwrap().is_some());
        assert!(person_by_handle(&conn, "@alreadyprefixed").unwrap().is_some());
    }

    #[test]
    fn search_matches_nickname_and_name() {
        let conn = graph();

        let by_handle: Vec<String> =
            search_people(&conn, "kline").unwrap().into_iter().map(|u| u.handle).collect();
        assert_eq!(by_handle, ["@sarahkline"]);

        // The display name works too, including the space the handle doesn't have.
        let by_name: Vec<String> =
            search_people(&conn, "Sarah K").unwrap().into_iter().map(|u| u.name).collect();
        assert_eq!(by_name, ["Sarah Kline"]);

        // Case-insensitive, and a partial prefix is enough.
        assert_eq!(search_people(&conn, "ELENA").unwrap().len(), 1);
        assert!(search_people(&conn, "nobodyatall").unwrap().is_empty());
        // Empty lists everyone: that's what the screen shows before you type.
        assert_eq!(search_people(&conn, "").unwrap().len(), GRAPH.len());
    }

    /// The wildcards are escaped, so a search for them finds nothing rather than
    /// everyone.
    #[test]
    fn search_treats_wildcards_as_text() {
        let conn = graph();
        for pattern in ["%", "_", "\\", "%%", "a%"] {
            assert!(
                search_people(&conn, pattern).unwrap().is_empty(),
                "'{pattern}' matched somebody"
            );
        }
    }

    /// People the visitor follows sort above strangers, so the friends you have are
    /// the first thing the directory shows.
    #[test]
    fn search_puts_followed_people_first() {
        let conn = graph();
        let results = search_people(&conn, "").unwrap();
        let boundary = results.iter().position(|u| !u.following).unwrap_or(results.len());
        assert!(results[..boundary].iter().all(|u| u.following));
        assert!(results[boundary..].iter().all(|u| !u.following));
        let followed = GRAPH.iter().filter(|u| u.followed).count();
        assert_eq!(boundary, followed);
        assert!(boundary > 0 && boundary < results.len(), "the fixture needs both kinds");
    }

    #[test]
    fn following_and_followers_are_different_lists() {
        let conn = graph();
        let followed: Vec<String> =
            followed_users(&conn).unwrap().into_iter().map(|u| u.handle).collect();
        let followers: Vec<String> =
            followers(&conn).unwrap().into_iter().map(|u| u.handle).collect();

        // Every combination the badges have to render is present.
        assert!(followed.contains(&"@elenarostova".into()) && followers.contains(&"@elenarostova".into()));
        assert!(followed.contains(&"@sarahkline".into()) && !followers.contains(&"@sarahkline".into()));
        assert!(!followed.contains(&"@tomasrey".into()) && followers.contains(&"@tomasrey".into()));
        assert!(!followed.contains(&"@priyanaidu".into()) && !followers.contains(&"@priyanaidu".into()));
    }

    #[test]
    fn following_is_idempotent_when_told_the_target() {
        let conn = graph();
        let id = "user-priyanaidu";
        let before = follow_count(&conn).unwrap();

        assert_eq!(set_follow(&conn, id, Some(true)).unwrap(), Some(true));
        assert_eq!(follow_count(&conn).unwrap(), before + 1);
        // Twice is still followed, and still one row.
        assert_eq!(set_follow(&conn, id, Some(true)).unwrap(), Some(true));
        assert_eq!(follow_count(&conn).unwrap(), before + 1);

        assert_eq!(set_follow(&conn, id, Some(false)).unwrap(), Some(false));
        assert_eq!(set_follow(&conn, id, Some(false)).unwrap(), Some(false));
        assert_eq!(follow_count(&conn).unwrap(), before);

        // No body toggles.
        assert_eq!(set_follow(&conn, id, None).unwrap(), Some(true));
        assert_eq!(set_follow(&conn, id, None).unwrap(), Some(false));
    }

    /// Unfollowing must not touch `follows_visitor` — one is the visitor's action,
    /// the other is about them, and conflating them would rewrite history.
    #[test]
    fn unfollowing_leaves_their_side_of_the_graph_alone() {
        let conn = graph();
        let handle = "elenarostova";
        assert!(person_by_handle(&conn, handle).unwrap().unwrap().follows_you);

        set_follow(&conn, "user-elenarostova", Some(false)).unwrap();
        let after = person_by_handle(&conn, handle).unwrap().unwrap();
        assert!(!after.following);
        assert!(after.follows_you, "they stopped following the visitor too");
    }

    /// Only real users are followable. The export's decorative cast has no page and
    /// no follow button, and a stray id must not create a dangling row.
    #[test]
    fn the_export_cast_is_not_followable_or_findable() {
        let conn = graph();
        let before = follow_count(&conn).unwrap();
        assert_eq!(set_follow(&conn, "elena", Some(true)).unwrap(), None);
        assert_eq!(set_follow(&conn, "no-such-person", Some(true)).unwrap(), None);
        assert_eq!(follow_count(&conn).unwrap(), before, "a dangling follow row was written");

        assert!(person_by_id(&conn, "elena").unwrap().is_none());
        assert!(person_by_handle(&conn, "elena").unwrap().is_none());
        // And "Marcus" finds the user, not the export's story-rail Marcus.
        let found: Vec<String> =
            search_people(&conn, "Marcus").unwrap().into_iter().map(|u| u.id).collect();
        assert_eq!(found, ["user-marcusdrey"]);
    }

    /// The film page's ordering: friends first, then the best-rated stranger.
    #[test]
    fn a_films_reviews_put_friends_first_then_the_highest_rated() {
        let conn = graph();
        let reviews = reviews_for_movie(&conn, "dune-part-two").unwrap();
        let handles: Vec<&str> = reviews.iter().map(|r| r.handle.as_str()).collect();

        // Elena (9, followed) and Marcus (7, followed) outrank Priya (8, stranger)
        // even though she rated it higher than Marcus did.
        assert_eq!(handles, ["@elenarostova", "@marcusdrey", "@priyanaidu"]);
        assert!(reviews[0].followed && reviews[1].followed && !reviews[2].followed);

        // Unfollow the top one and she drops behind her own friend, then below the
        // stranger who rated it higher than she did — this is the fallback the
        // film page relies on when you have no friends who reviewed it.
        set_follow(&conn, "user-elenarostova", Some(false)).unwrap();
        let after: Vec<String> =
            reviews_for_movie(&conn, "dune-part-two").unwrap().into_iter().map(|r| r.handle).collect();
        assert_eq!(after, ["@marcusdrey", "@elenarostova", "@priyanaidu"]);
    }

    /// With no friends at all it's purely best-rated first — "random people's
    /// reviews or high-star reviews", which is the whole point of the fallback.
    #[test]
    fn with_no_friends_a_films_reviews_are_best_rated_first() {
        let conn = graph();
        conn.execute("DELETE FROM follows", []).unwrap();

        let stars: Vec<u8> =
            reviews_for_movie(&conn, "dune-part-two").unwrap().iter().map(|r| r.half_stars).collect();
        assert_eq!(stars, [9, 8, 7]);
    }

    #[test]
    fn a_films_reviews_are_empty_for_a_film_nobody_reviewed() {
        let conn = graph();
        assert!(reviews_for_movie(&conn, "no-such-film").unwrap().is_empty());
    }

    #[test]
    fn a_persons_reviews_come_back_newest_first() {
        let conn = graph();
        let reviews = reviews_by_person(&conn, "user-elenarostova").unwrap();
        assert_eq!(reviews.len(), 5);

        let dates: Vec<&str> = reviews.iter().map(|r| r.created_at.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(dates, sorted, "a person's page should open on their latest");
        // Every row carries its author, so one shape serves both screens.
        assert!(reviews.iter().all(|r| r.name == "Elena Rostova" && r.followed));
    }

    /// The count and the list are the same rows, so a page that clamps the list
    /// still prints a true number.
    #[test]
    fn review_counts_match_the_rows() {
        let conn = graph();
        for user in demo_graph() {
            let stored = person_by_handle(&conn, &user.handle).unwrap().unwrap();
            assert_eq!(stored.review_count as usize, user.reviews.len());
            assert_eq!(reviews_by_person(&conn, &stored.id).unwrap().len(), user.reviews.len());
        }
    }

    /// The migration path: an existing database predating the graph gains the
    /// columns rather than silently keeping the old shape.
    #[test]
    fn the_graph_columns_are_added_to_an_older_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL,
                 avatar_src TEXT NOT NULL, avatar_alt TEXT NOT NULL,
                 unseen INTEGER NOT NULL DEFAULT 0,
                 in_stories INTEGER NOT NULL DEFAULT 0,
                 position INTEGER NOT NULL);
             INSERT INTO people VALUES ('elena', 'Elena', 'img/a.jpg', 'alt', 1, 1, 0);",
        )
        .unwrap();
        for column in ["handle", "bio", "is_user", "follows_visitor"] {
            assert!(!has_column(&conn, "people", column).unwrap());
        }

        prepare(&conn).unwrap();
        for column in ["handle", "bio", "is_user", "follows_visitor"] {
            assert!(has_column(&conn, "people", column).unwrap(), "{column} was not added");
        }
        // The pre-existing row survived but is not a user, so it reaches no screen:
        // it is one of the export's decorative eleven, and every screen now shows
        // only people you follow.
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM people", [], |r| r.get(0)).unwrap(),
            1
        );
        assert!(search_people(&conn, "").unwrap().is_empty());
        assert!(followed_with_newest_review(&conn, 10).unwrap().is_empty());

        // And twice is still fine, on the DB and on the unique index.
        prepare(&conn).unwrap();
        assert_eq!(seed_graph(&conn, &demo_graph()).unwrap(), GRAPH.len());
    }

    /// Two people can't share a nickname — it's how a person's page is addressed.
    #[test]
    fn nicknames_are_unique() {
        let conn = graph();
        let clash = conn.execute(
            "INSERT INTO people (id, name, avatar_src, avatar_alt, position, handle, is_user)
             VALUES ('impostor', 'Elena', 'img/a.jpg', 'alt', 99, '@elenarostova', 1)",
            [],
        );
        assert!(clash.is_err(), "a duplicate nickname was accepted");
    }

    /// Favourites come from what someone rated highly; the watchlist from what they
    /// haven't written about. The two strips must never name the same film.
    #[test]
    fn taste_is_derived_from_reviews_and_never_overlaps_the_watchlist() {
        let review = |id: &str, stars: u8| (id.into(), stars, "Words.".into(), "2026-01-01".into());
        let pool: Vec<String> = (0..12).map(|n| format!("film-{n}")).collect();
        let reviews =
            vec![review("film-0", 6), review("film-1", 10), review("film-2", 8), review("film-3", 7)];

        let (favorites, watchlist) = derive_taste(&reviews, &pool, 0);
        // Best first, and `film-0` is under the floor rather than a reluctant favourite.
        assert_eq!(favorites, ["film-1", "film-2", "film-3"]);
        assert_eq!(watchlist.len(), TASTE_WATCHLIST);
        for id in &watchlist {
            assert!(!reviews.iter().any(|(r, ..)| r == id), "{id} is both reviewed and wanted");
        }
    }

    /// Someone who has panned everything has no favourites — better an empty strip
    /// than a page claiming a two-star film is their favourite.
    #[test]
    fn a_hard_to_please_person_has_no_favourites() {
        let review = |id: &str, stars: u8| (id.into(), stars, "Words.".into(), "2026-01-01".into());
        let pool: Vec<String> = (0..12).map(|n| format!("film-{n}")).collect();

        let (favorites, watchlist) =
            derive_taste(&[review("film-0", 4), review("film-1", 6)], &pool, 3);
        assert!(favorites.is_empty());
        assert_eq!(watchlist.len(), TASTE_WATCHLIST, "they still have things to watch");
    }

    /// The cap holds, and equal ratings fall back to newest-written.
    #[test]
    fn favourites_are_capped_and_ties_break_on_recency() {
        let review = |id: &str, stars: u8| (id.into(), stars, "Words.".into(), "2026-01-01".into());
        // `reviews` arrives newest-first, so among the four 9s the first one listed
        // is the most recent and should lead.
        let reviews: Vec<_> = (0..6).map(|n| review(&format!("film-{n}"), 9)).collect();
        let (favorites, _) = derive_taste(&reviews, &[], 0);
        assert_eq!(favorites, ["film-0", "film-1", "film-2", "film-3"]);
    }

    /// Consecutive seats must not all want the same six films — two adjacent pages
    /// showing an identical strip reads as a placeholder.
    ///
    /// The stride is 5 and the strip is 6 long, so neighbours do share their last
    /// film with the next seat's first. Mostly-different is the claim, not disjoint.
    #[test]
    fn consecutive_seats_get_different_watchlists() {
        let pool: Vec<String> = (0..20).map(|n| format!("film-{n}")).collect();
        let first = derive_taste(&[], &pool, 0).1;
        let second = derive_taste(&[], &pool, 1).1;
        assert_ne!(first[0], second[0], "two seats opened on the same film");
        let shared = first.iter().filter(|id| second.contains(id)).count();
        assert!(shared <= 1, "{shared} of {TASTE_WATCHLIST} films are shared");
    }

    /// An empty pool is the harvest failing to read any films. No watchlist rather
    /// than a panic on the modulo.
    #[test]
    fn an_empty_film_pool_yields_an_empty_watchlist() {
        assert_eq!(derive_taste(&[], &[], 7), (Vec::new(), Vec::new()));
    }

    /// The seed writes both strips, and they read back in seeded order — which is
    /// what makes a person's page look like the visitor's own.
    #[test]
    fn a_seeded_person_has_both_strips() {
        let conn = graph();
        let seeded = &demo_graph()[0];
        let stored = person_by_handle(&conn, &seeded.handle).unwrap().unwrap();

        assert_eq!(favorites_by_person(&conn, &stored.id).unwrap(), seeded.favorites);
        assert_eq!(watchlist_by_person(&conn, &stored.id).unwrap(), seeded.watchlist);
        assert!(!seeded.favorites.is_empty() && !seeded.watchlist.is_empty());

        // Nobody's two strips name the same film, and everyone has something.
        for user in demo_graph() {
            let id = person_by_handle(&conn, &user.handle).unwrap().unwrap().id;
            let favorites = favorites_by_person(&conn, &id).unwrap();
            let watchlist = watchlist_by_person(&conn, &id).unwrap();
            assert!(!watchlist.is_empty(), "{} has nothing to watch", user.handle);
            assert!(favorites.iter().all(|f| !watchlist.contains(f)));
        }
    }

    /// The seed is idempotent on these two tables as well — a restart must not
    /// duplicate a strip, and `INSERT OR IGNORE` on the composite PK is why.
    #[test]
    fn re_seeding_does_not_duplicate_a_strip() {
        let conn = graph();
        let id = person_by_handle(&conn, &demo_graph()[0].handle).unwrap().unwrap().id;
        let before = favorites_by_person(&conn, &id).unwrap();

        seed_graph(&conn, &demo_graph()).unwrap();
        assert_eq!(favorites_by_person(&conn, &id).unwrap(), before);
    }

    /// The demo graph reviews films that actually exist, so no demo user's page is
    /// a list of "Untitled".
    #[test]
    fn the_demo_graph_reviews_films_from_the_catalogue() {
        let catalogue: Vec<String> =
            crate::data::catalogue().into_iter().map(|entry| entry.id).collect();
        for user in demo_graph() {
            for (movie_id, half_stars, body, _) in &user.reviews {
                assert!(catalogue.contains(movie_id), "{} reviewed '{movie_id}'", user.handle);
                assert!((1..=10).contains(half_stars), "'{movie_id}' has {half_stars} half-stars");
                assert!(!body.trim().is_empty());
            }
            assert!(user.reviews.len() <= REVIEW_DATES.len());
        }
    }
}
