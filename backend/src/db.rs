//! SQLite persistence.
//!
//! Three things live here:
//!
//! - **The social layer** — the people, their reviews, their taste. TMDB has none
//!   of it: `/3/movie/{id}/reviews` returns flat prose with no reply threads, and
//!   there is no notion of a friend. It has to come from somewhere, and the
//!   export's cast of people is the obvious somewhere.
//! - **Each user's own deltas** — watchlist, ratings, likes, posted comments and
//!   replies. Every one of those tables is keyed on `user_id`, and that key is the
//!   only thing keeping one person's screen out of another's. See `PER_USER_TABLES`.
//! - **Accounts and sessions.** An account is a `people` row with a `google_sub`,
//!   so it has a page and a nickname like anyone else; a session is a row in
//!   `sessions` naming an opaque token. `auth` drives both.
//!
//! `hydrate` is untouched by all of this: `load_store` rebuilds the same
//! `state::Store` it has always taken, so the three-layer split (content / deltas /
//! fold) holds and all of `hydrate`'s tests still exercise the same shape. What
//! changed is that the snapshot is now one person's rather than everyone's.
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

use rusqlite::{params, Connection, OptionalExtension};

use crate::auth::random_token;
use crate::models::Image;
use crate::state::Store;

/// Where the database file lives, relative to the crate root (`cargo run` runs
/// from `backend/`). Override with `DATABASE_PATH`, and `:memory:` works for a
/// throwaway run.
pub const DEFAULT_PATH: &str = "cine-journal.db";

/// The account that inherits every row written before sign-in existed.
///
/// The app used to have one visitor and one row set. Those rows are somebody's
/// watchlist and somebody's ratings, so `migrate` gives them an owner rather than
/// deleting them. It is an ordinary `people` row — it has a page at
/// `/api/people/alexm_cinema` and can be followed — but it has no `google_sub`, so
/// nobody can sign in as it. To hand its rows to a real account, run
/// `UPDATE <table> SET user_id = '<account id>' WHERE user_id = 'user-legacy-visitor'`
/// over the per-user tables (`PER_USER_TABLES`, plus `comments` and `replies`).
pub const LEGACY_USER_ID: &str = "user-legacy-visitor";

/// The user id that owns nothing — an anonymous reader.
///
/// Every per-user query takes an id, and a reader with no session still has to be
/// answered. No account can hold this value (ids are minted with a prefix), so the
/// personal tables read back empty for it and reads need no second code path.
pub const ANONYMOUS: &str = "";

/// How long a session lives, in days.
///
/// Long enough that signing in feels like staying signed in, short enough that a
/// leaked cookie stops working. The row is what grants access, so shortening this
/// takes effect immediately rather than after old tokens expire.
const SESSION_DAYS: u32 = 30;

/// How long a pending OAuth `state` value is accepted, in minutes.
///
/// The consent screen is a few clicks. Anything older is a stale tab or a replay,
/// and either way the sign-in should be restarted rather than completed.
const STATE_MINUTES: u32 = 10;

/// How many suffixed nicknames to try before falling back to a random one.
const HANDLE_TRIES: u32 = 50;

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
    conn.execute_batch(SCHEMA)?;
    // The per-user tables come from one list, because `migrate` rebuilds them from
    // the same strings — see `adopt_pre_account_rows`.
    for table in &PER_USER_TABLES {
        conn.execute_batch(table.create)?;
    }
    migrate(conn)?;
    // After `migrate`, not before: it rebuilds the two tables these are on, and a
    // rebuild takes the old table's indexes away with it.
    conn.execute_batch(PER_USER_INDEXES)
}

/// Indexes on tables `migrate` may rebuild.
///
/// Both serve reads that were per-user before and are shared now: `visitor_reviews`
/// is scanned by film for a film's review list, and `liked_comments` by comment to
/// count a comment's likes. Their primary keys lead with `user_id`, so neither lookup
/// could use one.
const PER_USER_INDEXES: &str =
    "CREATE INDEX IF NOT EXISTS visitor_reviews_by_movie ON visitor_reviews(movie_id);
     CREATE INDEX IF NOT EXISTS liked_comments_by_comment ON liked_comments(comment_id);";

/// Everything that is not per-user: people, their seeded content, sessions, and the
/// two posting tables whose ids the frontend holds.
const SCHEMA: &str = "PRAGMA foreign_keys = ON;
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
         -- `follows_visitor` is stored rather than derived: it is seeded on a
         -- little under half of the harvested people and thereafter left alone,
         -- so it now means 'follows whoever is signed in'. Real accounts have it
         -- at 0 and follow each other through `follows` instead, which is why
         -- `USER_SELECT` reads both.
         --
         -- Sign-in added five columns. A real account is a `people` row like any
         -- other — it has a page, a nickname and an avatar, and it can be
         -- followed — and these are what make it reachable:
         --
         --   `google_sub`     Google's stable subject id, the login key. UNIQUE
         --                    through `people_by_google_sub`, since ADD COLUMN
         --                    cannot carry the constraint. NULL for seeded people,
         --                    and NULLs do not collide in a SQLite unique index.
         --   `email`          What Google reported. Never used as a key: Google
         --                    lets an address move between accounts, `sub` does not.
         --   `is_account`     1 for a real account, 0 for a seeded person. This is
         --                    what keeps `needs_graph_seed` from thinking a sign-up
         --                    was the harvest.
         --   `joined_at`      When the account was created, for 'Cinephile since'.
         --                    NULL for the legacy visitor, whose rows predate this.
         --   `starter_follow` Whether a new account starts out following them. Set
         --                    by the seed, read by `grant_starter_follows`, so a
         --                    fresh sign-in opens on a feed rather than a blank.
         --
         -- `unseen`, `in_stories` and `position` are vestigial, kept because SQLite
         -- has no DROP COLUMN before 3.35 and rewriting the table would risk a
         -- visitor's own follows for tidiness. Nothing reads them.
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
             follows_visitor INTEGER NOT NULL DEFAULT 0,
             google_sub    TEXT,
             email         TEXT,
             is_account    INTEGER NOT NULL DEFAULT 0,
             joined_at     TEXT,
             starter_follow INTEGER NOT NULL DEFAULT 0
         );

         -- A signed-in browser, by opaque token.
         --
         -- A table rather than a signed cookie, so logging out can revoke: deleting
         -- the row ends the session everywhere, which a self-contained token cannot
         -- do without a blocklist that is this table under another name.
         --
         -- `expires_at` is written as `datetime('now', '+N days')`, the same format
         -- CURRENT_TIMESTAMP produces, so the lookups can compare it as a string.
         CREATE TABLE IF NOT EXISTS sessions (
             token      TEXT PRIMARY KEY,
             user_id    TEXT NOT NULL REFERENCES people(id),
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
             expires_at TEXT NOT NULL
         );

         CREATE INDEX IF NOT EXISTS sessions_by_user ON sessions(user_id);

         -- Pending OAuth `state` values — the CSRF check.
         --
         -- Written when a sign-in starts, deleted when the callback presents it.
         -- The delete is the check: a state that was already spent, was never
         -- issued, or was issued too long ago deletes nothing, and the callback
         -- refuses. Stored server-side rather than in a cookie so a state cannot be
         -- forged by anyone who can set cookies for this host.
         CREATE TABLE IF NOT EXISTS auth_states (
             state      TEXT PRIMARY KEY,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
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

         -- Vestigial. It held the one visitor's bio under a single key; a bio now
         -- lives on the account's own `people` row, because an account is a person.
         -- The table stays so `migrate` can read the old value out of it, and the
         -- row stays after that rather than being deleted.
         CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );

         -- `id` is the AUTOINCREMENT rowid rendered as 'comment-<n>'. It replaces
         -- the in-memory counter, whose ids restarted at 1 on every boot and
         -- would now collide with rows already on disk. AUTOINCREMENT (rather
         -- than a plain rowid) never reuses a number even after a delete.
         --
         -- These two are the only per-user tables `migrate` does not rebuild: the
         -- frontend holds their ids, so the rowid has to survive, and a plain
         -- `user_id` column is enough because the id already keys the row. The
         -- default is there for the ADD COLUMN path and never used by a write.
         CREATE TABLE IF NOT EXISTS comments (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             user_id    TEXT NOT NULL DEFAULT '',
             review_id  TEXT NOT NULL,
             body       TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         CREATE TABLE IF NOT EXISTS replies (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             user_id    TEXT NOT NULL DEFAULT '',
             review_id  TEXT NOT NULL,
             comment_id TEXT NOT NULL,
             body       TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );

         CREATE INDEX IF NOT EXISTS comments_by_review ON comments(review_id);
         CREATE INDEX IF NOT EXISTS replies_by_comment ON replies(review_id, comment_id);";

/// One table that used to hold the single visitor's rows and now holds everybody's.
///
/// `user_id` is part of the primary key, and that is the point: two people have to
/// be able to watchlist the same film. SQLite cannot add a column to a primary key,
/// so `adopt_pre_account_rows` rebuilds these tables on a database already on disk
/// rather than altering them.
struct PerUser {
    name: &'static str,
    /// The current definition. Used by `prepare` and by the rebuild, so the fresh
    /// shape and the migrated shape cannot drift apart.
    create: &'static str,
    /// The columns the pre-accounts version had, for the rebuild's copy.
    carried: &'static str,
}

/// Every table keyed on the person who wrote its rows.
const PER_USER_TABLES: [PerUser; 7] = [
    // The visitor's own state. Was `state::Store`, in memory.
    PerUser {
        name: "watchlist",
        create: "CREATE TABLE IF NOT EXISTS watchlist (
                     user_id  TEXT NOT NULL,
                     movie_id TEXT NOT NULL,
                     added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                     PRIMARY KEY (user_id, movie_id)
                 )",
        carried: "movie_id, added_at",
    },
    // Films marked as favourites, by pressing the heart on a film's page.
    //
    // A separate table from `ratings` rather than somebody's highest-rated films,
    // which is what the profile's Favorite Films strip used to mean. Those are
    // different statements: a five-star rating says the film is good, a favourite
    // says it is *yours*, and deriving one from the other made the strip change
    // behind your back every time you rated something.
    PerUser {
        name: "favorites",
        create: "CREATE TABLE IF NOT EXISTS favorites (
                     user_id  TEXT NOT NULL,
                     movie_id TEXT NOT NULL,
                     added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                     PRIMARY KEY (user_id, movie_id)
                 )",
        carried: "movie_id, added_at",
    },
    // `rated_at` orders the profile's Recent Reviews tile. Nullable rather than
    // NOT NULL DEFAULT CURRENT_TIMESTAMP, because SQLite ADD COLUMN only accepts a
    // constant default — and this column arrived after the table did, so `migrate`
    // has to add it to databases already on disk. Rows written before it existed
    // sort last; `set_rating` stamps every new one.
    PerUser {
        name: "ratings",
        create: "CREATE TABLE IF NOT EXISTS ratings (
                     user_id    TEXT NOT NULL,
                     movie_id   TEXT NOT NULL,
                     half_stars INTEGER NOT NULL,
                     rated_at   TEXT,
                     PRIMARY KEY (user_id, movie_id)
                 )",
        carried: "movie_id, half_stars, rated_at",
    },
    // A signed-in person's own prose about a film. One review per film per person,
    // which is what the PK enforces — writing again edits what's there.
    //
    // Not a column on `ratings`, because the two are independent: clearing a rating
    // must not delete what you wrote, and you can write about a film without
    // scoring it. `user_reviews` is the seeded people's equivalent, keyed on
    // `people(id)`; this is the accounts' copy of it. The name is the old one, kept
    // because renaming it would buy nothing and cost a second migration.
    PerUser {
        name: "visitor_reviews",
        create: "CREATE TABLE IF NOT EXISTS visitor_reviews (
                     user_id    TEXT NOT NULL,
                     movie_id   TEXT NOT NULL,
                     body       TEXT NOT NULL,
                     written_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                     PRIMARY KEY (user_id, movie_id)
                 )",
        carried: "movie_id, body, written_at",
    },
    PerUser {
        name: "liked_reviews",
        create: "CREATE TABLE IF NOT EXISTS liked_reviews (
                     user_id   TEXT NOT NULL,
                     review_id TEXT NOT NULL,
                     PRIMARY KEY (user_id, review_id)
                 )",
        carried: "review_id",
    },
    PerUser {
        name: "liked_comments",
        create: "CREATE TABLE IF NOT EXISTS liked_comments (
                     user_id    TEXT NOT NULL,
                     comment_id TEXT NOT NULL,
                     PRIMARY KEY (user_id, comment_id)
                 )",
        carried: "comment_id",
    },
    // Who one account follows. One row per follow, written by the button; deleting
    // the row unfollows. Not symmetric with `people.follows_visitor`: that one is
    // about them, this one is somebody's own action, and conflating the two would
    // make an unfollow silently rewrite history.
    PerUser {
        name: "follows",
        create: "CREATE TABLE IF NOT EXISTS follows (
                     user_id     TEXT NOT NULL,
                     person_id   TEXT NOT NULL REFERENCES people(id),
                     followed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                     PRIMARY KEY (user_id, person_id)
                 )",
        carried: "person_id, followed_at",
    },
];

/// Bring a database created by an earlier version up to the current schema.
///
/// `CREATE TABLE IF NOT EXISTS` is a no-op on a table that already exists, so a
/// column added to one of the definitions above never reaches a file already on
/// disk. There is no migration framework here on purpose — but silently serving a
/// profile that can't read its own ratings is worse than a few lines of this.
///
/// Order matters. `rated_at` is added before the rebuild below copies it.
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
    // Sign-in's five columns.
    if !has_column(conn, "people", "google_sub")? {
        conn.execute_batch(
            "ALTER TABLE people ADD COLUMN google_sub TEXT;
             ALTER TABLE people ADD COLUMN email TEXT;
             ALTER TABLE people ADD COLUMN is_account INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE people ADD COLUMN joined_at TEXT;
             ALTER TABLE people ADD COLUMN starter_follow INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // Here rather than in `SCHEMA`, because it has to run after the ALTER above: on a
    // database that already had `people`, the column does not exist until then. Same
    // trick `handle` uses — ADD COLUMN cannot carry a UNIQUE constraint, so the index
    // carries it, which is the same guarantee under a different name.
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS people_by_google_sub ON people(google_sub)",
    )?;

    adopt_pre_account_rows(conn)
}

/// Give the pre-accounts rows an owner.
///
/// Before sign-in there was one visitor, so `watchlist` was keyed on `movie_id`
/// alone. Two people now have to be able to watchlist the same film, which means
/// `user_id` has to join the primary key, which SQLite cannot do in place — so each
/// of these tables is rebuilt and its rows copied across to `LEGACY_USER_ID`.
/// Copied, not dropped: those rows are a real person's watchlist.
///
/// A fresh database never reaches the body: `prepare` has just created every table
/// with `user_id` in it, so nothing is pending and no legacy account is invented.
fn adopt_pre_account_rows(conn: &Connection) -> Result<()> {
    let mut pending: Vec<&PerUser> = Vec::new();
    for table in &PER_USER_TABLES {
        if !has_column(conn, table.name, "user_id")? {
            pending.push(table);
        }
    }
    let stamp_posts = !has_column(conn, "comments", "user_id")?;
    if pending.is_empty() && !stamp_posts {
        return Ok(());
    }

    // All of it or none of it. SQLite's DDL is transactional, and the alternative is a
    // process killed between the rename and the copy leaving somebody's watchlist in a
    // table called `watchlist_pre_accounts` that nothing reads. Dropping the guard
    // without committing rolls the whole thing back.
    let conn = conn.unchecked_transaction()?;

    // Only when there is something to adopt. A database that had the old schema but
    // no rows in it gains no account, so the migration invents no people.
    let adopting = pre_account_rows(&conn, &pending)?;
    if adopting {
        ensure_legacy_account(&conn)?;
    }

    for table in &pending {
        conn.execute_batch(&format!(
            "ALTER TABLE {name} RENAME TO {name}_pre_accounts;
             {create};
             INSERT INTO {name} (user_id, {carried})
                 SELECT '{LEGACY_USER_ID}', {carried} FROM {name}_pre_accounts;
             DROP TABLE {name}_pre_accounts;",
            name = table.name,
            create = table.create,
            carried = table.carried,
        ))?;
    }

    if stamp_posts {
        conn.execute_batch(
            "ALTER TABLE comments ADD COLUMN user_id TEXT NOT NULL DEFAULT '';
             ALTER TABLE replies  ADD COLUMN user_id TEXT NOT NULL DEFAULT '';",
        )?;
        for table in ["comments", "replies"] {
            conn.execute(
                &format!("UPDATE {table} SET user_id = ?1 WHERE user_id = ''"),
                [LEGACY_USER_ID],
            )?;
        }
    }

    if !adopting {
        return conn.commit();
    }

    // Whoever the visitor followed becomes the starter set every new account gets,
    // so a database with a graph in it still opens on a feed after somebody signs
    // in for the first time.
    conn.execute(
        "UPDATE people SET starter_follow = 1
         WHERE id IN (SELECT person_id FROM follows WHERE user_id = ?1)",
        [LEGACY_USER_ID],
    )?;
    // And their bio moves onto their own `people` row, which is where a bio lives
    // now. The `settings` row is left behind rather than deleted.
    conn.execute(
        "UPDATE people SET bio = (SELECT value FROM settings WHERE key = ?2)
         WHERE id = ?1 AND EXISTS(SELECT 1 FROM settings WHERE key = ?2)",
        params![LEGACY_USER_ID, BIO_KEY],
    )?;
    conn.commit()
}

/// Whether a pre-accounts database holds any of the single visitor's rows.
///
/// `comments`, `replies` and `settings` are counted too even though they are not
/// rebuilt: a database whose only content is a posted comment still has data worth
/// an owner.
fn pre_account_rows(conn: &Connection, pending: &[&PerUser]) -> Result<bool> {
    let names = pending.iter().map(|table| table.name).chain(["comments", "replies", "settings"]);
    for name in names {
        let rows: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| row.get(0))?;
        if rows > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Create the account the pre-accounts rows belong to, if it isn't there.
///
/// It wears the export's identity, because that is whose profile those rows were
/// shown under: the name, nickname and avatar `hydrate` has always printed. No
/// `google_sub`, so it is unreachable by sign-in — see `LEGACY_USER_ID`.
fn ensure_legacy_account(conn: &Connection) -> Result<()> {
    let present: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM people WHERE id = ?1)",
        [LEGACY_USER_ID],
        |row| row.get(0),
    )?;
    if present {
        return Ok(());
    }

    let avatar = crate::hydrate::visitor_avatar();
    let handle = unique_handle(conn, crate::hydrate::VISITOR_HANDLE)?;
    conn.execute(
        // `position` is written because a database created by an earlier build
        // declares it NOT NULL with no default.
        "INSERT INTO people
             (id, name, avatar_src, avatar_alt, position, handle, is_user, is_account)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, 1, 1)",
        params![LEGACY_USER_ID, crate::hydrate::VISITOR_NAME, avatar.src, avatar.alt, handle],
    )?;
    Ok(())
}

/// A nickname nobody has yet.
///
/// `people.handle` is unique because it addresses a page, and two Google accounts
/// can easily want the same one. The wanted nickname wins if it is free, otherwise
/// a number is appended — "@sam", "@sam2", "@sam3". The random fallback exists so
/// this cannot fail on a server with fifty Sams.
fn unique_handle(conn: &Connection, wanted: &str) -> Result<String> {
    let base = format!("@{}", wanted.trim_start_matches('@'));
    for attempt in 1..=HANDLE_TRIES {
        let candidate = if attempt == 1 { base.clone() } else { format!("{base}{attempt}") };
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM people WHERE handle = ?1)",
            [&candidate],
            |row| row.get(0),
        )?;
        if !taken {
            return Ok(candidate);
        }
    }
    Ok(format!("{base}-{}", random_token(4)))
}

/// Whether a table already has a column. The table name is interpolated because
/// PRAGMA takes no bind parameters; every call site passes a literal.
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names: Vec<String> =
        stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<_>>()?;
    Ok(names.iter().any(|name| name == column))
}

// --- Accounts and sessions ----------------------------------------------------

/// One account, as `/api/auth/me` and the profile header draw it.
///
/// A thin read of the account's own `people` row. Everything on it is either
/// Google's (name, avatar) or the user's own (nickname, bio). No email: it is stored
/// so an operator can tell two accounts apart, and read by nothing, because no screen
/// shows it and a browser has no use for it.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Image,
    pub bio: Option<String>,
    /// When the account was created. `None` for the legacy visitor, whose rows
    /// predate sign-in and who therefore has no join date to claim.
    pub joined_at: Option<String>,
}

const ACCOUNT_SELECT: &str =
    "SELECT id, name, handle, avatar_src, avatar_alt, bio, joined_at FROM people";

fn account_from_row(row: &rusqlite::Row) -> Result<AccountRow> {
    Ok(AccountRow {
        id: row.get("id")?,
        name: row.get("name")?,
        // Every account is written with one, so a NULL here would be a bug rather
        // than a state to render.
        handle: row.get::<_, Option<String>>("handle")?.unwrap_or_default(),
        avatar: Image::new(
            &row.get::<_, String>("avatar_src")?,
            &row.get::<_, String>("avatar_alt")?,
        ),
        bio: row.get("bio")?,
        joined_at: row.get("joined_at")?,
    })
}

/// One account by id.
pub fn account(conn: &Connection, id: &str) -> Result<Option<AccountRow>> {
    let mut stmt = conn.prepare(&format!("{ACCOUNT_SELECT} WHERE id = ?1 AND is_account = 1"))?;
    stmt.query_row([id], account_from_row).optional()
}

/// Whose session this token is, or `None` for a token that is unknown or expired.
///
/// Expiry is checked in the query rather than after it, so an expired row reads as
/// no session at all and the request is anonymous.
pub fn session_account(conn: &Connection, token: &str) -> Result<Option<AccountRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id AS id, p.name AS name, p.handle AS handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
                p.bio AS bio, p.joined_at AS joined_at
         FROM sessions s JOIN people p ON p.id = s.user_id
         WHERE s.token = ?1 AND s.expires_at > datetime('now')",
    )?;
    stmt.query_row([token], account_from_row).optional()
}

/// Start a session for one account.
///
/// Expired rows are cleared on the way in. That is the only sweep there is: nothing
/// runs on a timer, and sign-in is the moment a sweep is both cheap and certain to
/// happen. An expired row is inert until then, because `session_account` filters it.
pub fn create_session(conn: &Connection, token: &str, user_id: &str) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE expires_at <= datetime('now')", [])?;
    conn.execute(
        "INSERT INTO sessions (token, user_id, expires_at)
         VALUES (?1, ?2, datetime('now', ?3))",
        params![token, user_id, format!("+{SESSION_DAYS} days")],
    )?;
    Ok(())
}

/// End one session. `true` when a row was really removed.
///
/// This is what makes logout a revocation rather than a suggestion: the token stops
/// working for every browser holding it, not just the one that cleared its cookie.
pub fn delete_session(conn: &Connection, token: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM sessions WHERE token = ?1", [token])? > 0)
}

/// Remember a `state` value so the callback can prove it issued it.
pub fn remember_auth_state(conn: &Connection, state: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM auth_states WHERE created_at <= datetime('now', ?1)",
        [format!("-{STATE_MINUTES} minutes")],
    )?;
    conn.execute("INSERT OR REPLACE INTO auth_states (state) VALUES (?1)", [state])?;
    Ok(())
}

/// Spend a `state` value. `false` means the callback must be refused.
///
/// The delete *is* the check, and it is atomic, so a state cannot be spent twice —
/// a replayed callback finds nothing to delete. `false` covers all three failures
/// worth refusing: never issued, already used, and older than `STATE_MINUTES`.
pub fn consume_auth_state(conn: &Connection, state: &str) -> Result<bool> {
    let spent = conn.execute(
        "DELETE FROM auth_states
         WHERE state = ?1 AND created_at > datetime('now', ?2)",
        params![state, format!("-{STATE_MINUTES} minutes")],
    )?;
    Ok(spent > 0)
}

/// What Google told us about somebody signing in.
///
/// Built by `auth` from the userinfo response. `db` does no networking, so this is
/// the shape the two agree on — the same arrangement `SeedUser` has.
#[derive(Debug, Clone)]
pub struct GoogleAccount {
    /// Google's `sub`: stable, and the only field safe to key on. An email address
    /// can move between Google accounts; a `sub` cannot.
    pub sub: String,
    pub email: Option<String>,
    pub name: String,
    pub avatar: Image,
    /// The nickname to try first, derived from the Google profile. Made unique here.
    pub handle: String,
}

/// Find or create the account behind a Google profile.
///
/// On every sign-in the name, avatar and email are refreshed, because Google owns
/// those. The nickname and the bio are not: the user may have been followed under
/// that nickname, and the bio is something they wrote.
///
/// A new account starts out following whoever the seed marked `starter_follow`, so
/// the first screen after sign-in has something on it rather than being the empty
/// state.
pub fn upsert_google_account(conn: &Connection, profile: &GoogleAccount) -> Result<AccountRow> {
    let existing: Option<String> = conn
        .query_row("SELECT id FROM people WHERE google_sub = ?1", [&profile.sub], |row| row.get(0))
        .optional()?;

    let id = match existing {
        Some(id) => {
            conn.execute(
                "UPDATE people
                    SET name = ?2, avatar_src = ?3, avatar_alt = ?4, email = ?5
                  WHERE id = ?1",
                params![
                    id,
                    profile.name,
                    profile.avatar.src,
                    profile.avatar.alt,
                    profile.email
                ],
            )?;
            id
        }
        None => {
            let id = account_id(&profile.sub);
            let handle = unique_handle(conn, &profile.handle)?;
            conn.execute(
                "INSERT INTO people
                     (id, name, avatar_src, avatar_alt, position, handle, is_user,
                      google_sub, email, is_account, joined_at)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, 1, ?6, ?7, 1, CURRENT_TIMESTAMP)",
                params![
                    id,
                    profile.name,
                    profile.avatar.src,
                    profile.avatar.alt,
                    handle,
                    profile.sub,
                    profile.email
                ],
            )?;
            grant_starter_follows(conn, &id)?;
            id
        }
    };

    // Read back rather than assemble from `profile`: the nickname and bio come from
    // the row, and the row is what every other screen will show.
    account(conn, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// The account id minted for one Google subject.
///
/// Prefixed, so it can never collide with a seeded `user-...` id or with
/// `ANONYMOUS`. Filtered down to ASCII alphanumerics because this string ends up in
/// URLs and in Redis keys — see `cache::feed_key`.
fn account_id(sub: &str) -> String {
    let clean: String = sub.chars().filter(char::is_ascii_alphanumeric).take(64).collect();
    format!("account-{clean}")
}

/// Follow, for one account, everybody the seed marked as a starting friend.
///
/// Returns how many rows it wrote. `INSERT OR IGNORE`, so calling it twice is a
/// no-op rather than an error.
pub fn grant_starter_follows(conn: &Connection, user_id: &str) -> Result<usize> {
    conn.execute(
        "INSERT OR IGNORE INTO follows (user_id, person_id)
         SELECT ?1, id FROM people WHERE starter_follow = 1 AND id <> ?1",
        [user_id],
    )
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
    /// Whether a new account starts out following them, so the app has friends on
    /// first sign-in. Stored on `people.starter_follow` rather than written as a
    /// follow row, because the seed runs before anybody has an account.
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
///
/// Real accounts are excluded, which matters twice: the first person to sign in must
/// not suppress the seed, and a database migrated from before sign-in must not be
/// re-seeded because it gained a legacy account.
pub fn needs_graph_seed(conn: &Connection) -> Result<bool> {
    let users: i64 = conn.query_row(
        "SELECT COUNT(*) FROM people WHERE is_user = 1 AND is_account = 0",
        [],
        |row| row.get(0),
    )?;
    Ok(users == 0)
}

/// Populate the social graph, once, into a database that has none.
///
/// Runs after `prepare`, not inside it, because it needs the network and the schema
/// is applied before the TMDB client exists. Guarded on `is_user` rather than on
/// `people` being non-empty, because a database written by an earlier build already
/// holds the export's eleven decorative rows and would otherwise never seed. Once
/// anyone is in there the graph is the users', and seeding again would talk over
/// their follows — so this is one shot, and returns `Ok(0)` afterwards.
///
/// It writes no `follows` rows: there is nobody to own one yet. `starter_follow`
/// records who a new account should follow, and `grant_starter_follows` writes the
/// rows once there is an account to write them for.
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
                  handle, bio, is_user, follows_visitor, starter_follow)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)",
            params![
                user.id,
                user.name,
                user.avatar.src,
                user.avatar.alt,
                offset as i64,
                format!("@{}", user.handle.trim_start_matches('@')),
                user.bio,
                user.follows_visitor,
                user.followed_by_visitor,
            ],
        )?;
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
    best.sort_by_key(|b| std::cmp::Reverse(b.1));
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

/// The profile's "Following" list: the people one account really follows, newest
/// first.
///
/// Only `follows` rows, and only this account's. It used to also list everyone on
/// the export's stories and activity rails, on the reasoning that the rails were the
/// only friends the app had — but now that following is a real, clickable act, a
/// profile counting twelve while the friend directory counts five is just a lie
/// about the same fact. Those rails are gone entirely, and the feed's rails are
/// built from this same list.
pub fn following(conn: &Connection, user_id: &str) -> Result<Vec<FollowRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT p.id AS id, p.name AS name, p.avatar_src AS avatar_src,
                p.avatar_alt AS avatar_alt, p.handle AS handle, p.bio AS bio,
                {REVIEW_COUNT} AS review_count
         FROM people p
         JOIN follows f ON f.person_id = p.id AND f.user_id = ?1
         ORDER BY f.followed_at DESC, p.name"
    ))?;
    let rows = stmt.query_map([user_id], |row| {
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
    /// Whether this is a real account rather than a seeded person. It decides which
    /// tables their favourites and watchlist come from: an account writes to
    /// `favorites` and `watchlist`, a seeded person was filled into `user_favorites`
    /// and `user_watchlist` by the harvest.
    pub is_account: bool,
}

/// The columns every user query selects, and the joins that compute the two
/// relationship flags. Shared as a string because four queries differ only in
/// their `WHERE` and `ORDER BY`, and a divergence between them would show up as
/// a follow button that disagrees with itself between two screens.
///
/// `?1` is the account asking. Every query built from this reserves it, so their own
/// parameters start at `?2`. An anonymous reader passes `ANONYMOUS`, which matches
/// no row, so both flags come back false rather than showing somebody else's graph.
///
/// `follows_you` is two things at once, and has to be. A seeded person carries a
/// static `follows_visitor` flag, since nobody could really press follow on you when
/// there was one visitor; a real account follows you by writing a row. Reading only
/// the flag would make one account's follow invisible to the other. Both halves are
/// gated on there *being* somebody asking (`?1 <> ''`, which is `ANONYMOUS`) —
/// otherwise the seeded flag would tell a signed-out reader that eight people follow
/// them.
fn user_select() -> String {
    format!(
        "SELECT p.id AS id, p.name AS name, p.handle AS handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt, p.bio AS bio,
                p.is_account AS is_account,
                f.user_id IS NOT NULL AS following,
                (?1 <> '' AND (p.follows_visitor = 1
                               OR EXISTS(SELECT 1 FROM follows b
                                         WHERE b.user_id = p.id
                                           AND b.person_id = ?1))) AS follows_you,
                {REVIEW_COUNT} AS review_count
         FROM people p LEFT JOIN follows f ON f.person_id = p.id AND f.user_id = ?1"
    )
}

fn user_from_row(row: &rusqlite::Row) -> Result<UserRow> {
    Ok(UserRow {
        id: row.get("id")?,
        name: row.get("name")?,
        handle: row.get("handle")?,
        avatar: Image::new(&row.get::<_, String>("avatar_src")?, &row.get::<_, String>("avatar_alt")?),
        bio: row.get("bio")?,
        following: row.get("following")?,
        follows_you: row.get("follows_you")?,
        review_count: row.get("review_count")?,
        is_account: row.get("is_account")?,
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
/// rather than a blank slate. The asker is excluded, because a row offering to
/// follow yourself is a button that cannot do anything.
pub fn search_people(conn: &Connection, user_id: &str, query: &str) -> Result<Vec<UserRow>> {
    // `_` and `%` in a user's query would otherwise act as wildcards, so "a_b"
    // would match "axb". ESCAPE makes them literal.
    let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{}%", escaped.trim_start_matches('@'));
    let select = user_select();
    let mut stmt = conn.prepare(&format!(
        "{select}
         WHERE p.is_user = 1 AND p.id <> ?1
           AND (?2 = '' OR p.handle LIKE ?3 ESCAPE '\\' OR p.name LIKE ?3 ESCAPE '\\')
         ORDER BY following DESC, review_count DESC, p.name"
    ))?;
    let rows =
        stmt.query_map(params![user_id, query.trim(), pattern], user_from_row)?.collect();
    rows
}

/// One user by nickname, with or without the leading `@`.
pub fn person_by_handle(
    conn: &Connection,
    user_id: &str,
    handle: &str,
) -> Result<Option<UserRow>> {
    let handle = format!("@{}", handle.trim_start_matches('@'));
    let mut stmt = conn.prepare(&format!("{} WHERE p.handle = ?2", user_select()))?;
    let row = stmt.query_row(params![user_id, handle], user_from_row).optional()?;
    Ok(row)
}

/// One user by id — what the follow endpoint takes, since a button knows the id.
pub fn person_by_id(conn: &Connection, user_id: &str, id: &str) -> Result<Option<UserRow>> {
    let mut stmt =
        conn.prepare(&format!("{} WHERE p.id = ?2 AND p.is_user = 1", user_select()))?;
    let row = stmt.query_row(params![user_id, id], user_from_row).optional()?;
    Ok(row)
}

/// The users one account follows, most recently followed first.
pub fn followed_users(conn: &Connection, user_id: &str) -> Result<Vec<UserRow>> {
    let select = user_select();
    let mut stmt = conn.prepare(&format!(
        "{select} WHERE f.user_id IS NOT NULL ORDER BY f.followed_at DESC, p.name"
    ))?;
    let rows = stmt.query_map([user_id], user_from_row)?.collect();
    rows
}

/// The users who follow one account — the seeded people who follow everybody, plus
/// the real accounts that wrote a row.
pub fn followers(conn: &Connection, user_id: &str) -> Result<Vec<UserRow>> {
    let select = user_select();
    let mut stmt = conn.prepare(&format!(
        // The `follows_you` expression again rather than the alias: SQLite would
        // accept the alias here, but only SQLite would.
        "{select}
         WHERE p.is_user = 1 AND p.id <> ?1
           AND ?1 <> '' AND (p.follows_visitor = 1
                             OR EXISTS(SELECT 1 FROM follows b
                                       WHERE b.user_id = p.id AND b.person_id = ?1))
         ORDER BY following DESC, p.name"
    ))?;
    let rows = stmt.query_map([user_id], user_from_row)?.collect();
    rows
}

/// Follow or unfollow one person. Returns the new state, or `None` if no such user.
///
/// `target` makes it idempotent, as `set_watchlist` is: the button sends the state
/// it wants rather than "flip it", so a double-tap or a retried request can't land
/// the UI and the DB on opposite answers. Pass `None` to toggle.
///
/// Following yourself is `None` rather than a row: a self-edge would put you in your
/// own feed and your own follower list.
pub fn set_follow(
    conn: &Connection,
    user_id: &str,
    person_id: &str,
    target: Option<bool>,
) -> Result<Option<bool>> {
    if user_id == person_id || person_by_id(conn, user_id, person_id)?.is_none() {
        return Ok(None);
    }
    let now: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM follows WHERE user_id = ?1 AND person_id = ?2)",
        params![user_id, person_id],
        |row| row.get(0),
    )?;
    let next = target.unwrap_or(!now);
    if next {
        conn.execute(
            "INSERT OR IGNORE INTO follows (user_id, person_id) VALUES (?1, ?2)",
            params![user_id, person_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM follows WHERE user_id = ?1 AND person_id = ?2",
            params![user_id, person_id],
        )?;
    }
    Ok(Some(next))
}

/// How many people one account follows.
pub fn follow_count(conn: &Connection, user_id: &str) -> Result<u32> {
    conn.query_row("SELECT COUNT(*) FROM follows WHERE user_id = ?1", [user_id], |row| row.get(0))
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
    /// `None` for prose written without a score. A seeded review always has one —
    /// the harvest never separated them — but an account can write about a film
    /// without rating it, and showing that as zero stars would be a lie about it.
    pub half_stars: Option<u8>,
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

/// Every review anybody wrote, from the two tables that hold them.
///
/// There are two because the app keeps a *rating* and the *prose* about a film apart:
/// clearing a score must not delete what you wrote. A seeded person's review arrives
/// from the harvest with both in one `user_reviews` row; an account writes prose to
/// `visitor_reviews` and a score to `ratings`, separately, so its review is assembled
/// here by joining them.
///
/// Without this union an account's reviews were invisible to everybody — including
/// the people following them, which is the whole point of following somebody.
///
/// `NOT EXISTS` keeps `UNION ALL` honest: nobody has rows in both tables for the same
/// film today, and if anybody ever did the review would otherwise be listed twice.
/// `UNION ALL` rather than `UNION` because that guard already rules out duplicates,
/// and `UNION` would pay for a sort to prove it.
const REVIEW_SOURCE: &str = "SELECT person_id, movie_id, half_stars, body, created_at
       FROM user_reviews
     UNION ALL
     SELECT v.user_id AS person_id, v.movie_id AS movie_id, r.half_stars AS half_stars,
            v.body AS body, v.written_at AS created_at
       FROM visitor_reviews v
       LEFT JOIN ratings r ON r.user_id = v.user_id AND r.movie_id = v.movie_id
      WHERE NOT EXISTS(SELECT 1 FROM user_reviews u
                       WHERE u.person_id = v.user_id AND u.movie_id = v.movie_id)";

/// How many reviews one person has written, counted the same way `REVIEW_SOURCE`
/// lists them. `p.id` is the person, so this only reads inside `USER_SELECT` and
/// `following`.
const REVIEW_COUNT: &str = "((SELECT COUNT(*) FROM user_reviews r WHERE r.person_id = p.id)
      + (SELECT COUNT(*) FROM visitor_reviews v
         WHERE v.user_id = p.id
           AND NOT EXISTS(SELECT 1 FROM user_reviews u
                          WHERE u.person_id = v.user_id AND u.movie_id = v.movie_id)))";

/// The columns every review query selects, over both source tables.
///
/// A function rather than a const because it interpolates `REVIEW_SOURCE`, and a const
/// cannot call `format!`. One copy of that union, so the six queries built from this
/// cannot drift apart.
///
/// As `USER_SELECT`: `?1` is the account asking, and every query below starts its own
/// parameters at `?2`.
///
/// The `JOIN people` is what publishes a review: an author has to be somebody with a
/// page. Every account is, so an account's reviews reach the same screens a seeded
/// person's do, with the same attribution.
fn review_select() -> String {
    format!(
        "SELECT r.person_id AS person_id, p.name AS name, p.handle AS handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
                f.user_id IS NOT NULL AS followed,
                r.movie_id AS movie_id, r.half_stars AS half_stars,
                r.body AS body, r.created_at AS created_at
         FROM ({REVIEW_SOURCE}) r
         JOIN people p ON p.id = r.person_id
         LEFT JOIN follows f ON f.person_id = r.person_id AND f.user_id = ?1"
    )
}

/// The reviews of one film, **the people the visitor follows first**.
///
/// That ordering is the whole point of the film page's section: a friend's opinion
/// outranks a stranger's, and within each group the highest-rated comes first so
/// what you see is someone recommending the film rather than the most recent
/// passer-by. `person_id` breaks the final tie, since `created_at` is seeded at
/// one-day resolution.
pub fn reviews_for_movie(
    conn: &Connection,
    user_id: &str,
    movie_id: &str,
) -> Result<Vec<UserReviewRow>> {
    let select = review_select();
    // `half_stars` is NULL for prose written without a score. SQLite sorts NULLs first
    // ascending, so DESC puts an unrated review below every rated one — which is the
    // right place for it in a list meant to lead with a recommendation.
    let mut stmt = conn.prepare(&format!(
        "{select} WHERE r.movie_id = ?2
         ORDER BY followed DESC, r.half_stars DESC, r.created_at DESC, r.person_id"
    ))?;
    let rows = stmt.query_map(params![user_id, movie_id], review_from_row)?.collect();
    rows
}

/// One person's reviews, newest first.
pub fn reviews_by_person(
    conn: &Connection,
    user_id: &str,
    person_id: &str,
) -> Result<Vec<UserReviewRow>> {
    let select = review_select();
    let mut stmt = conn.prepare(&format!(
        "{select} WHERE r.person_id = ?2 ORDER BY r.created_at DESC, r.movie_id"
    ))?;
    let rows = stmt.query_map(params![user_id, person_id], review_from_row)?.collect();
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
pub fn review_by_id(
    conn: &Connection,
    user_id: &str,
    review_id: &str,
) -> Result<Option<UserReviewRow>> {
    let select = review_select();
    let mut stmt =
        conn.prepare(&format!("{select} WHERE r.person_id || '-' || r.movie_id = ?2"))?;
    stmt.query_row(params![user_id, review_id], review_from_row).optional()
}

/// The newest reviews across the whole graph, **the people this account follows
/// first** — what the review screen opens on when no id names one.
///
/// Public content: it is every user's reviews, not one account's, so this is also
/// what an anonymous reader's feed is built from.
pub fn recent_reviews(conn: &Connection, user_id: &str, limit: u32) -> Result<Vec<UserReviewRow>> {
    let select = review_select();
    let mut stmt = conn.prepare(&format!(
        "{select}
         ORDER BY followed DESC, r.created_at DESC, r.half_stars DESC, r.person_id, r.movie_id
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![user_id, limit], review_from_row)?.collect();
    rows
}

/// The newest reviews by **the people the visitor follows**, and nobody else.
///
/// Strictly a `JOIN` on `follows`, unlike `recent_reviews`'s ordering trick: the feed
/// says these are your friends' reviews, so a stranger's appearing there because the
/// graph was thin would make the heading a lie. An empty result is the honest answer
/// for someone who follows nobody, and the screen says so.
pub fn reviews_from_followed(
    conn: &Connection,
    user_id: &str,
    limit: u32,
) -> Result<Vec<UserReviewRow>> {
    let select = review_select();
    let mut stmt = conn.prepare(&format!(
        "{select}
         WHERE f.user_id IS NOT NULL
         ORDER BY r.created_at DESC, r.half_stars DESC, r.person_id, r.movie_id
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![user_id, limit], review_from_row)?.collect();
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
pub fn followed_with_newest_review(
    conn: &Connection,
    user_id: &str,
    limit: u32,
) -> Result<Vec<StoryRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT p.id AS id, p.name AS name, p.handle AS handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
                (SELECT r.person_id || '-' || r.movie_id FROM ({REVIEW_SOURCE}) r
                 WHERE r.person_id = p.id
                 ORDER BY r.created_at DESC, r.movie_id LIMIT 1) AS newest_review
         FROM people p JOIN follows f ON f.person_id = p.id AND f.user_id = ?1
         WHERE p.is_user = 1
         ORDER BY newest_review IS NULL, f.followed_at DESC, p.name
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![user_id, limit], |row| {
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

/// One account's watchlist, **most recently added first** — the order the profile
/// grid wants, and the reverse of `load_store`'s.
///
/// `movie_id` breaks the tie because `added_at` has one-second resolution, so two
/// films logged in the same second would otherwise come back in an arbitrary
/// order that flips between requests.
pub fn watchlist_recent_first(conn: &Connection, user_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT movie_id FROM watchlist WHERE user_id = ?1
         ORDER BY added_at DESC, movie_id DESC",
    )?;
    let ids = stmt.query_map([user_id], |row| row.get(0))?.collect();
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
pub fn journal_recent_first(conn: &Connection, user_id: &str) -> Result<Vec<JournalRow>> {
    let mut stmt = conn.prepare(
        "SELECT ids.movie_id AS movie_id, r.half_stars AS half_stars, v.body AS body,
                MAX(COALESCE(r.rated_at, ''), COALESCE(v.written_at, '')) AS logged_at
         FROM (SELECT movie_id FROM ratings WHERE user_id = ?1
               UNION SELECT movie_id FROM visitor_reviews WHERE user_id = ?1) ids
         LEFT JOIN ratings r ON r.movie_id = ids.movie_id AND r.user_id = ?1
         LEFT JOIN visitor_reviews v ON v.movie_id = ids.movie_id AND v.user_id = ?1
         ORDER BY logged_at DESC, ids.movie_id DESC",
    )?;
    let rows = stmt.query_map([user_id], |row| {
        Ok(JournalRow {
            movie_id: row.get("movie_id")?,
            half_stars: row.get("half_stars")?,
            body: row.get("body")?,
        })
    })?;
    rows.collect()
}

/// One account's favourite films, most recently added first — the same order the
/// watchlist strip beside it uses.
pub fn favorites_recent_first(conn: &Connection, user_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT movie_id FROM favorites WHERE user_id = ?1
         ORDER BY added_at DESC, movie_id DESC",
    )?;
    let ids = stmt.query_map([user_id], |row| row.get(0))?.collect();
    ids
}

/// Add, remove, or toggle a favourite. Returns the resulting state.
///
/// Idempotent on a stated target, as `set_watchlist` is — and for the same reason:
/// the heart sends the state it wants, so a double-tap can't land the button and the
/// row on opposite answers.
pub fn set_favorite(
    conn: &Connection,
    user_id: &str,
    movie_id: &str,
    target: Option<bool>,
) -> Result<bool> {
    let present: bool = conn
        .query_row(
            "SELECT 1 FROM favorites WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let target = target.unwrap_or(!present);
    if target {
        conn.execute(
            "INSERT OR IGNORE INTO favorites (user_id, movie_id) VALUES (?1, ?2)",
            [user_id, movie_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM favorites WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
        )?;
    }
    Ok(target)
}

/// Write or rewrite one account's review of a film. An empty body deletes it, which
/// is how the composer clears one.
///
/// `written_at` is refreshed on a rewrite, so an edited review moves to the top of
/// the profile: the writing is the event, exactly as `set_rating` treats the rating.
pub fn set_user_review(
    conn: &Connection,
    user_id: &str,
    movie_id: &str,
    body: &str,
) -> Result<Option<String>> {
    let body = body.trim();
    if body.is_empty() {
        conn.execute(
            "DELETE FROM visitor_reviews WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
        )?;
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO visitor_reviews (user_id, movie_id, body, written_at)
         VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
         ON CONFLICT(user_id, movie_id) DO UPDATE
             SET body = excluded.body, written_at = excluded.written_at",
        params![user_id, movie_id, body],
    )?;
    Ok(Some(body.to_string()))
}

/// The `settings` key the one visitor's bio used to be stored under. Read once by
/// `adopt_pre_account_rows`, which moves the value onto their `people` row.
const BIO_KEY: &str = "visitor_bio";

/// Store one account's bio. An empty string clears it, restoring the default.
///
/// It lives on `people.bio`, the same column a seeded person's harvested line does,
/// because an account *is* a person now: somebody else opening your page reads it
/// through the same query as anybody else's, and `account` reads it back for the
/// owner. `None` — never written, or written back to blank — is what lets the caller
/// supply a default; storing the default eagerly would make "never edited" and
/// "edited back to the original" the same state.
pub fn set_user_bio(conn: &Connection, user_id: &str, bio: &str) -> Result<Option<String>> {
    let bio = bio.trim();
    let stored = (!bio.is_empty()).then(|| bio.to_string());
    conn.execute("UPDATE people SET bio = ?2 WHERE id = ?1", params![user_id, stored])?;
    Ok(stored)
}

// --- Comment threads ----------------------------------------------------------

/// One comment on a review, with its author and its likes joined in.
///
/// Comments are **content**, like reviews: everybody sees the whole thread. They used
/// to be per-viewer deltas in `state::Store`, which meant you saw only your own and
/// every one of them was captioned "You" — true only while there was one visitor.
#[derive(Debug, Clone)]
pub struct CommentRow {
    /// The wire id, `comment-<rowid>`.
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    pub body: String,
    pub created_at: String,
    /// How many people have liked it, the reader included.
    ///
    /// No "did *you* like it" flag beside it: that comes from the reader's own
    /// `Store`, which is already loaded, and `hydrate::review` stamps it on. One place
    /// for every per-viewer flag rather than two that can disagree.
    pub like_count: u32,
    /// Its replies, oldest first.
    pub replies: Vec<ReplyRow>,
}

/// One reply under a comment. No likes: the export drew no like button on a reply.
#[derive(Debug, Clone)]
pub struct ReplyRow {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub author_handle: String,
    pub author_avatar: Image,
    pub body: String,
    pub created_at: String,
}

/// The whole conversation on one review, oldest first.
///
/// Two queries rather than one join, because a join would repeat every comment once
/// per reply and the grouping would have to be undone in Rust anyway. Both are ordered
/// by rowid, which is the order things were written in.
///
/// The same thread for everybody, signed in or not: a reader with no account can
/// follow a conversation, they just cannot join it.
pub fn thread(conn: &Connection, review_id: &str) -> Result<Vec<CommentRow>> {
    // `JOIN people` for the same reason `review_select` does it: an author is somebody
    // with a page. Only accounts can post, and every account has a row.
    //
    // `'comment-' || c.id` rebuilds the wire id `comment_id` mints, because that is
    // what `liked_comments` stores — the id the frontend sends back, not the rowid.
    let mut stmt = conn.prepare(
        "SELECT c.id AS id, c.body AS body, c.created_at AS created_at,
                p.id AS author_id, p.name AS author_name, p.handle AS author_handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt,
                (SELECT COUNT(*) FROM liked_comments l
                 WHERE l.comment_id = 'comment-' || c.id) AS like_count
         FROM comments c
         JOIN people p ON p.id = c.user_id
         WHERE c.review_id = ?1
         ORDER BY c.id",
    )?;
    let mut comments: Vec<CommentRow> = stmt
        .query_map([review_id], |row| {
            Ok(CommentRow {
                id: comment_id(row.get("id")?),
                author_id: row.get("author_id")?,
                author_name: row.get("author_name")?,
                author_handle: row.get::<_, Option<String>>("author_handle")?.unwrap_or_default(),
                author_avatar: Image::new(
                    &row.get::<_, String>("avatar_src")?,
                    &row.get::<_, String>("avatar_alt")?,
                ),
                body: row.get("body")?,
                created_at: row.get("created_at")?,
                like_count: row.get("like_count")?,
                replies: Vec::new(),
            })
        })?
        .collect::<Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT r.id AS id, r.comment_id AS comment_id, r.body AS body,
                r.created_at AS created_at,
                p.id AS author_id, p.name AS author_name, p.handle AS author_handle,
                p.avatar_src AS avatar_src, p.avatar_alt AS avatar_alt
         FROM replies r
         JOIN people p ON p.id = r.user_id
         WHERE r.review_id = ?1
         ORDER BY r.id",
    )?;
    let replies = stmt.query_map([review_id], |row| {
        Ok((
            row.get::<_, String>("comment_id")?,
            ReplyRow {
                id: reply_id(row.get("id")?),
                author_id: row.get("author_id")?,
                author_name: row.get("author_name")?,
                author_handle: row.get::<_, Option<String>>("author_handle")?.unwrap_or_default(),
                author_avatar: Image::new(
                    &row.get::<_, String>("avatar_src")?,
                    &row.get::<_, String>("avatar_alt")?,
                ),
                body: row.get("body")?,
                created_at: row.get("created_at")?,
            },
        ))
    })?;

    for row in replies {
        let (parent, reply) = row?;
        // A reply whose parent is gone is dropped rather than shown at the top level,
        // where it would read as a comment somebody never wrote.
        if let Some(comment) = comments.iter_mut().find(|comment| comment.id == parent) {
            comment.replies.push(reply);
        }
    }

    Ok(comments)
}

// --- The visitor's state ------------------------------------------------------

/// Rebuild one account's whole `Store` from disk.
///
/// One snapshot per request, which is what keeps `hydrate` unchanged: it takes a
/// `&Store` and has no idea a database exists. Every query is scoped to `user_id`,
/// which is the only thing keeping one person's watchlist out of another's screen.
/// Pass `ANONYMOUS` and every one of them comes back empty.
pub fn load_store(conn: &Connection, user_id: &str) -> Result<Store> {
    let mut store = Store::default();

    let mut stmt = conn.prepare(
        "SELECT movie_id FROM watchlist WHERE user_id = ?1 ORDER BY added_at, movie_id",
    )?;
    for id in stmt.query_map([user_id], |row| row.get::<_, String>(0))? {
        store.watchlist.insert(id?);
    }

    let mut stmt = conn.prepare(
        "SELECT movie_id FROM favorites WHERE user_id = ?1 ORDER BY added_at, movie_id",
    )?;
    for id in stmt.query_map([user_id], |row| row.get::<_, String>(0))? {
        store.favorites.insert(id?);
    }

    let mut stmt = conn.prepare("SELECT movie_id, half_stars FROM ratings WHERE user_id = ?1")?;
    for row in
        stmt.query_map([user_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?)))?
    {
        let (id, half_stars) = row?;
        store.ratings.insert(id, half_stars);
    }

    let mut stmt = conn.prepare("SELECT movie_id, body FROM visitor_reviews WHERE user_id = ?1")?;
    for row in
        stmt.query_map([user_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
    {
        let (id, body) = row?;
        store.written_reviews.insert(id, body);
    }

    let mut stmt = conn.prepare("SELECT review_id FROM liked_reviews WHERE user_id = ?1")?;
    for id in stmt.query_map([user_id], |row| row.get::<_, String>(0))? {
        store.liked_reviews.insert(id?);
    }

    let mut stmt = conn.prepare("SELECT comment_id FROM liked_comments WHERE user_id = ?1")?;
    for id in stmt.query_map([user_id], |row| row.get::<_, String>(0))? {
        store.liked_comments.insert(id?);
    }

    // Nothing about comments themselves. They used to be loaded here, per user, and
    // `hydrate` appended them to a review as the whole thread — so you saw only your
    // own. They are content now: `thread` reads everybody's, and what stays in the
    // store is the one part that really is per-viewer, which likes you pressed.
    Ok(store)
}

/// Rowid 3 -> "comment-3". The wire ids the frontend already sends back.
fn comment_id(rowid: i64) -> String {
    format!("comment-{rowid}")
}

fn reply_id(rowid: i64) -> String {
    format!("reply-{rowid}")
}

/// Add, remove, or toggle a film on one account's watchlist. Returns the resulting
/// state.
pub fn set_watchlist(
    conn: &Connection,
    user_id: &str,
    movie_id: &str,
    target: Option<bool>,
) -> Result<bool> {
    let present: bool = conn
        .query_row(
            "SELECT 1 FROM watchlist WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let target = target.unwrap_or(!present);
    if target {
        // Idempotent, so a double-click can't desync the button from the store —
        // and `added_at` keeps its original value rather than jumping.
        conn.execute(
            "INSERT OR IGNORE INTO watchlist (user_id, movie_id) VALUES (?1, ?2)",
            [user_id, movie_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM watchlist WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
        )?;
    }
    Ok(target)
}

/// Set one account's rating; `0` clears it, which is how the UI un-rates a film.
pub fn set_rating(
    conn: &Connection,
    user_id: &str,
    movie_id: &str,
    half_stars: u8,
) -> Result<Option<u8>> {
    if half_stars == 0 {
        conn.execute(
            "DELETE FROM ratings WHERE user_id = ?1 AND movie_id = ?2",
            [user_id, movie_id],
        )?;
        return Ok(None);
    }
    // Re-rating a film moves it to the top of the profile's "Recent Reviews",
    // which is what "recent" has to mean — the *rating* is the event, not the row.
    conn.execute(
        "INSERT INTO ratings (user_id, movie_id, half_stars, rated_at)
         VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
         ON CONFLICT(user_id, movie_id) DO UPDATE
             SET half_stars = excluded.half_stars, rated_at = excluded.rated_at",
        params![user_id, movie_id, half_stars],
    )?;
    Ok(Some(half_stars))
}

/// Toggle one account's like on a review. Returns whether *they* now like it, and how
/// many people do in total.
///
/// Both, from one call, because the button needs both and asking separately would let
/// a handler return a count that disagrees with the state it just wrote. The total
/// includes the caller's own like, so nothing downstream adds one.
pub fn toggle_review_like(
    conn: &Connection,
    user_id: &str,
    review_id: &str,
) -> Result<(bool, u32)> {
    let removed = conn.execute(
        "DELETE FROM liked_reviews WHERE user_id = ?1 AND review_id = ?2",
        [user_id, review_id],
    )?;
    if removed == 0 {
        conn.execute(
            "INSERT INTO liked_reviews (user_id, review_id) VALUES (?1, ?2)",
            [user_id, review_id],
        )?;
    }
    let total: u32 = conn.query_row(
        "SELECT COUNT(*) FROM liked_reviews WHERE review_id = ?1",
        [review_id],
        |row| row.get(0),
    )?;
    Ok((removed == 0, total))
}

/// The same, for a comment.
pub fn toggle_comment_like(
    conn: &Connection,
    user_id: &str,
    comment_id: &str,
) -> Result<(bool, u32)> {
    let removed = conn.execute(
        "DELETE FROM liked_comments WHERE user_id = ?1 AND comment_id = ?2",
        [user_id, comment_id],
    )?;
    if removed == 0 {
        conn.execute(
            "INSERT INTO liked_comments (user_id, comment_id) VALUES (?1, ?2)",
            [user_id, comment_id],
        )?;
    }
    let total: u32 = conn.query_row(
        "SELECT COUNT(*) FROM liked_comments WHERE comment_id = ?1",
        [comment_id],
        |row| row.get(0),
    )?;
    Ok((removed == 0, total))
}

/// How many people have liked one review. What the review page prints beside the
/// button before anybody presses it.
pub fn review_like_count(conn: &Connection, review_id: &str) -> Result<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM liked_reviews WHERE review_id = ?1",
        [review_id],
        |row| row.get(0),
    )
}

/// Store a comment and return its wire id.
pub fn add_comment(
    conn: &Connection,
    user_id: &str,
    review_id: &str,
    body: &str,
) -> Result<String> {
    conn.execute(
        "INSERT INTO comments (user_id, review_id, body) VALUES (?1, ?2, ?3)",
        params![user_id, review_id, body],
    )?;
    Ok(comment_id(conn.last_insert_rowid()))
}

pub fn add_reply(
    conn: &Connection,
    user_id: &str,
    review_id: &str,
    comment_id: &str,
    body: &str,
) -> Result<String> {
    conn.execute(
        "INSERT INTO replies (user_id, review_id, comment_id, body) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, review_id, comment_id, body],
    )?;
    Ok(reply_id(conn.last_insert_rowid()))
}

/// Whether this comment exists on this review — the guard in front of a reply or a
/// like, since a comment is not in the upstream content and a bogus id would be
/// stored under a key nothing renders.
///
/// Scoped to the review and **not** to the asker. A thread is shared, so anybody who
/// can read a comment can reply to it and like it, which is what makes it a
/// conversation. There is deliberately no endpoint that edits or deletes one, so
/// "can act on it" never means "can change somebody else's words".
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

    /// The account every test below acts as.
    ///
    /// Its id is derived from the Google subject, so it is knowable in advance and a
    /// test can name it without threading a variable through every assertion.
    const ME: &str = "account-1001";

    /// Sign somebody in.
    ///
    /// Through `upsert_google_account`, the same path a real callback takes, rather
    /// than a hand-written INSERT — so a change to account creation is felt by the
    /// tests that depend on having an account.
    fn sign_in(conn: &Connection, sub: &str, handle: &str) -> AccountRow {
        upsert_google_account(
            conn,
            &GoogleAccount {
                sub: sub.into(),
                email: Some(format!("{handle}@example.com")),
                name: format!("{handle} the viewer"),
                avatar: Image::new("img/avatar-test.jpg", "A test avatar."),
                handle: handle.into(),
            },
        )
        .expect("an account")
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
        assert!(reviews_from_followed(&conn, ME, 10).unwrap().is_empty());
        assert!(followed_with_newest_review(&conn, ME, 10).unwrap().is_empty());

        let conn = graph();

        // `demo_graph` marks some but not all of its cast as starter follows, and
        // only the followed ones may appear.
        let followed: Vec<String> =
            following(&conn, ME).unwrap().into_iter().map(|row| row.id).collect();
        assert!(!followed.is_empty(), "the demo graph seeds some follows");

        let reviews = reviews_from_followed(&conn, ME, 50).unwrap();
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
        let stories = followed_with_newest_review(&conn, ME, 50).unwrap();
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
        assert!(review_by_id(&conn, ME, &review_id).unwrap().is_some());
    }

    /// The "Following" list's subtitle is built from these two, so both have to
    /// survive the round trip.
    #[test]
    fn following_carries_a_bio_and_a_live_review_count() {
        let conn = graph();

        let rows = following(&conn, ME).unwrap();
        let row = rows.iter().find(|r| r.review_count > 0).expect("someone reviewed something");
        assert!(row.handle.as_deref().unwrap_or_default().starts_with('@'));
        assert!(row.bio.is_some());

        let counted: u32 =
            reviews_by_person(&conn, ME, &row.id).unwrap().len().try_into().unwrap();
        assert_eq!(row.review_count, counted);
    }

    #[test]
    fn watchlist_toggles_and_is_idempotent_when_told_the_target() {
        let conn = db();
        assert!(set_watchlist(&conn, ME, "157336-interstellar", None).unwrap());
        assert!(!set_watchlist(&conn, ME, "157336-interstellar", None).unwrap());

        // Stating the target twice must not flip it — that's what protects a
        // double-click from desyncing the button.
        assert!(set_watchlist(&conn, ME, "x", Some(true)).unwrap());
        assert!(set_watchlist(&conn, ME, "x", Some(true)).unwrap());
        assert!(!set_watchlist(&conn, ME, "x", Some(false)).unwrap());
        assert!(!set_watchlist(&conn, ME, "x", Some(false)).unwrap());

        assert_eq!(load_store(&conn, ME).unwrap().watchlist.len(), 0);
    }

    #[test]
    fn ratings_round_trip_and_zero_clears() {
        let conn = db();
        assert_eq!(set_rating(&conn, ME, "m", 7).unwrap(), Some(7));
        assert_eq!(load_store(&conn, ME).unwrap().ratings.get("m"), Some(&7));

        // Re-rating replaces rather than conflicting.
        assert_eq!(set_rating(&conn, ME, "m", 9).unwrap(), Some(9));
        assert_eq!(load_store(&conn, ME).unwrap().ratings.get("m"), Some(&9));

        assert_eq!(set_rating(&conn, ME, "m", 0).unwrap(), None);
        assert!(load_store(&conn, ME).unwrap().ratings.is_empty());
    }

    /// A toggle reports the presser's own state and the shared total together.
    #[test]
    fn likes_toggle_and_report_the_real_total() {
        let conn = db();
        assert_eq!(toggle_review_like(&conn, ME, "r").unwrap(), (true, 1));
        assert!(load_store(&conn, ME).unwrap().liked_reviews.contains("r"));
        assert_eq!(toggle_review_like(&conn, ME, "r").unwrap(), (false, 0));
        assert!(load_store(&conn, ME).unwrap().liked_reviews.is_empty());

        assert_eq!(toggle_comment_like(&conn, ME, "comment-1").unwrap(), (true, 1));
        assert!(load_store(&conn, ME).unwrap().liked_comments.contains("comment-1"));
    }

    /// The count is everybody's, so a second liker makes it 2 for both of them —
    /// where it used to read 1 to each, being per-viewer.
    #[test]
    fn a_like_total_counts_everybody() {
        let conn = db();
        let other = "account-2002";

        assert_eq!(toggle_review_like(&conn, ME, "r").unwrap(), (true, 1));
        assert_eq!(toggle_review_like(&conn, other, "r").unwrap(), (true, 2));
        assert_eq!(review_like_count(&conn, "r").unwrap(), 2);

        // One of them unliking leaves the other's like standing.
        assert_eq!(toggle_review_like(&conn, ME, "r").unwrap(), (false, 1));
        assert!(load_store(&conn, other).unwrap().liked_reviews.contains("r"));
        assert!(!load_store(&conn, ME).unwrap().liked_reviews.contains("r"));

        // A review nobody has liked counts zero rather than failing to find a row.
        assert_eq!(review_like_count(&conn, "never-liked").unwrap(), 0);

        assert_eq!(toggle_comment_like(&conn, ME, "comment-1").unwrap(), (true, 1));
        assert_eq!(toggle_comment_like(&conn, other, "comment-1").unwrap(), (true, 2));
    }

    /// A thread reads back with the ids the frontend exchanges, rebuilt from rowids,
    /// and scoped to its own review.
    #[test]
    fn a_thread_reads_back_with_its_ids_and_stays_on_its_review() {
        let conn = graph();
        let first = add_comment(&conn, ME, "review-a", "Mine").unwrap();
        let second = add_comment(&conn, ME, "review-a", "Also mine").unwrap();
        let elsewhere = add_comment(&conn, ME, "review-b", "Different review").unwrap();
        assert_eq!(first, "comment-1");
        assert_eq!(second, "comment-2");
        assert_eq!(elsewhere, "comment-3");

        let reply = add_reply(&conn, ME, "review-a", &first, "A follow-up").unwrap();
        assert_eq!(reply, "reply-1");

        let on_a = thread(&conn, "review-a").unwrap();
        assert_eq!(on_a.len(), 2);
        assert_eq!(on_a[0].id, "comment-1");
        assert_eq!(on_a[0].body, "Mine");
        assert_eq!(on_a[1].id, "comment-2");
        assert_eq!(thread(&conn, "review-b").unwrap().len(), 1);
        assert!(thread(&conn, "review-nobody-commented-on").unwrap().is_empty());

        // The reply hangs off its own comment and nowhere else.
        assert_eq!(on_a[0].replies.len(), 1);
        assert_eq!(on_a[0].replies[0].id, "reply-1");
        assert_eq!(on_a[0].replies[0].body, "A follow-up");
        assert!(on_a[1].replies.is_empty());
        assert!(thread(&conn, "review-b").unwrap()[0].replies.is_empty());
    }

    /// The point of a shared thread: two accounts talking to each other, each row
    /// credited to whoever wrote it.
    #[test]
    fn a_thread_is_shared_and_credits_every_author() {
        let conn = graph();
        let other = sign_in(&conn, "2002", "ada").id;
        let review = "user-elenarostova-le-souffle";

        let mine = add_comment(&conn, ME, review, "Loved the café scene.").unwrap();
        add_comment(&conn, &other, review, "Overrated, sorry.").unwrap();
        // Anybody can reply to anybody: this is the other account under my comment.
        add_reply(&conn, &other, review, &mine, "You would.").unwrap();

        let rows = thread(&conn, review).unwrap();
        assert_eq!(rows.len(), 2, "one account could not see the other's comment");
        // Oldest first, as it was written.
        assert_eq!(rows[0].body, "Loved the café scene.");
        assert_eq!(rows[0].author_id, ME);
        assert_eq!(rows[0].author_handle, "@testviewer");
        assert!(rows[0].author_avatar.src.starts_with('/'));
        assert_eq!(rows[1].author_id, other);
        assert_eq!(rows[1].author_handle, "@ada");

        // And the reply under mine is theirs, not mine.
        assert_eq!(rows[0].replies.len(), 1);
        assert_eq!(rows[0].replies[0].author_id, other);
        assert_eq!(rows[0].replies[0].body, "You would.");

        // Every row carries a real timestamp now — a comment used to have none worth
        // printing, so the thread said "Just now" on all of them.
        assert!(!rows[0].created_at.is_empty());
        assert!(!rows[0].replies[0].created_at.is_empty());

        // The same thread whoever asks, including nobody.
        assert_eq!(thread(&conn, review).unwrap().len(), 2);
    }

    /// A comment's like total is everybody's, and it is the one thing `thread`
    /// reports about likes — whose heart is filled comes from the reader's `Store`.
    #[test]
    fn a_comments_like_total_counts_every_liker() {
        let conn = graph();
        let other = sign_in(&conn, "2002", "ada").id;
        let review = "user-elenarostova-le-souffle";
        let id = add_comment(&conn, ME, review, "Worth it.").unwrap();

        assert_eq!(thread(&conn, review).unwrap()[0].like_count, 0);

        toggle_comment_like(&conn, ME, &id).unwrap();
        toggle_comment_like(&conn, &other, &id).unwrap();
        assert_eq!(thread(&conn, review).unwrap()[0].like_count, 2);

        // Each of them sees their own heart, out of their own store.
        assert!(load_store(&conn, ME).unwrap().liked_comments.contains(&id));
        assert!(load_store(&conn, &other).unwrap().liked_comments.contains(&id));
        assert!(!load_store(&conn, "account-3003").unwrap().liked_comments.contains(&id));
    }

    /// The bug the rowid scheme exists to prevent: the old in-memory counter
    /// restarted at 1 every boot, so a fresh comment would reuse a stored id.
    #[test]
    fn comment_ids_do_not_restart_after_a_reload() {
        let conn = db();
        add_comment(&conn, ME, "r", "one").unwrap();
        add_comment(&conn, ME, "r", "two").unwrap();

        // Reopening the same connection is as close as an in-memory database gets
        // to a restart; AUTOINCREMENT keeps its high-water mark in the file.
        let third = add_comment(&conn, ME, "r", "three").unwrap();
        assert_eq!(third, "comment-3");

        // Even after a delete, the number is not reused — that's AUTOINCREMENT
        // rather than a bare rowid.
        conn.execute("DELETE FROM comments WHERE id = 3", []).unwrap();
        assert_eq!(add_comment(&conn, ME, "r", "four").unwrap(), "comment-4");
    }

    #[test]
    fn comment_existence_is_scoped_to_the_review_and_not_to_the_asker() {
        let conn = db();
        let id = add_comment(&conn, ME, "review-a", "Mine").unwrap();
        assert!(comment_exists(&conn, "review-a", &id).unwrap());
        assert!(!comment_exists(&conn, "review-b", &id).unwrap());
        // A malformed or upstream id is simply not ours.
        assert!(!comment_exists(&conn, "review-a", "comment-marcus").unwrap());
        assert!(!comment_exists(&conn, "review-a", "nonsense").unwrap());
    }

    #[test]
    fn an_empty_database_loads_an_empty_store() {
        let store = load_store(&db(), ME).unwrap();
        assert!(store.watchlist.is_empty());
        assert!(store.ratings.is_empty());
        assert!(store.liked_reviews.is_empty());
        assert!(store.liked_comments.is_empty());
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
            set_watchlist(&conn, ME, id, Some(true)).unwrap();
        }
        assert_eq!(watchlist_recent_first(&conn, ME).unwrap(), ["c-film", "b-film", "a-film"]);
        // `load_store`'s order is unchanged, which is what keeps `hydrate` honest.
        let store: Vec<String> = load_store(&conn, ME).unwrap().watchlist.into_iter().collect();
        assert_eq!(store, ["a-film", "b-film", "c-film"]);
    }

    /// Re-rating moves a film to the front of the journal: the rating is the event.
    #[test]
    fn re_rating_a_film_moves_it_to_the_front_of_the_journal() {
        let conn = db();
        // Same second for all three, so the id tiebreak decides the initial order —
        // deterministic rather than whatever SQLite feels like.
        set_rating(&conn, ME, "middling", 6).unwrap();
        set_rating(&conn, ME, "great", 10).unwrap();
        set_rating(&conn, ME, "good", 8).unwrap();
        assert_eq!(
            journal_recent_first(&conn, ME).unwrap().iter().map(|r| r.movie_id.as_str()).collect::<Vec<_>>(),
            ["middling", "great", "good"]
        );

        set_rating(&conn, ME, "middling", 7).unwrap();
        let recent = journal_recent_first(&conn, ME).unwrap();
        assert_eq!(recent[0].movie_id, "middling");
        assert_eq!(recent[0].half_stars, Some(7));
        assert_eq!(recent[0].body, None, "a rating alone carries no prose");
    }

    /// The heart is its own act, independent of the rating and of the watchlist.
    #[test]
    fn favouriting_toggles_and_is_idempotent_when_told_the_target() {
        let conn = db();
        assert!(favorites_recent_first(&conn, ME).unwrap().is_empty());

        assert!(set_favorite(&conn, ME, "le-souffle", Some(true)).unwrap());
        // Twice is still favourited, and still one row — the PK sees to that.
        assert!(set_favorite(&conn, ME, "le-souffle", Some(true)).unwrap());
        assert_eq!(favorites_recent_first(&conn, ME).unwrap(), ["le-souffle"]);

        assert!(!set_favorite(&conn, ME, "le-souffle", Some(false)).unwrap());
        assert!(!set_favorite(&conn, ME, "le-souffle", Some(false)).unwrap());
        assert!(favorites_recent_first(&conn, ME).unwrap().is_empty());

        // No body toggles.
        assert!(set_favorite(&conn, ME, "le-souffle", None).unwrap());
        assert!(!set_favorite(&conn, ME, "le-souffle", None).unwrap());

        // And it left the neighbouring tables alone: a favourite is not a rating
        // and not a watchlist entry, which is the whole reason it has its own table.
        set_favorite(&conn, ME, "le-souffle", Some(true)).unwrap();
        let store = load_store(&conn, ME).unwrap();
        assert!(store.favorites.contains("le-souffle"));
        assert!(store.ratings.is_empty() && store.watchlist.is_empty());
    }

    #[test]
    fn favourites_read_back_newest_first() {
        let conn = db();
        for id in ["a-film", "b-film", "c-film"] {
            set_favorite(&conn, ME, id, Some(true)).unwrap();
        }
        assert_eq!(favorites_recent_first(&conn, ME).unwrap(), ["c-film", "b-film", "a-film"]);
    }

    /// Writing, rewriting and clearing. Blank deletes rather than storing an empty
    /// review, so a cleared composer leaves no trace on the profile.
    #[test]
    fn a_written_review_can_be_edited_and_cleared() {
        let conn = db();
        assert_eq!(set_user_review(&conn, ME, "le-souffle", "  First pass.  ").unwrap(),
                   Some("First pass.".into()), "the body is trimmed on the way in");

        let journal = journal_recent_first(&conn, ME).unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].body.as_deref(), Some("First pass."));
        assert_eq!(journal[0].half_stars, None, "prose without a score is allowed");

        assert_eq!(set_user_review(&conn, ME, "le-souffle", "Second thoughts.").unwrap(),
                   Some("Second thoughts.".into()));
        assert_eq!(journal_recent_first(&conn, ME).unwrap().len(), 1, "editing wrote a second row");

        assert_eq!(set_user_review(&conn, ME, "le-souffle", "   ").unwrap(), None);
        assert!(journal_recent_first(&conn, ME).unwrap().is_empty());
        assert!(load_store(&conn, ME).unwrap().written_reviews.is_empty());
    }

    /// A rating and a review of the same film are one journal entry, and clearing
    /// either one leaves the other standing.
    #[test]
    fn a_rating_and_a_review_of_one_film_are_one_entry() {
        let conn = db();
        set_rating(&conn, ME, "le-souffle", 9).unwrap();
        set_user_review(&conn, ME, "le-souffle", "Worth the hype.").unwrap();

        let journal = journal_recent_first(&conn, ME).unwrap();
        assert_eq!(journal.len(), 1, "the union double-counted the film");
        assert_eq!(journal[0].half_stars, Some(9));
        assert_eq!(journal[0].body.as_deref(), Some("Worth the hype."));

        // Clearing the rating must not delete what was written about it.
        set_rating(&conn, ME, "le-souffle", 0).unwrap();
        let after = journal_recent_first(&conn, ME).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].half_stars, None);
        assert_eq!(after[0].body.as_deref(), Some("Worth the hype."));

        // And the other way round.
        set_rating(&conn, ME, "le-souffle", 6).unwrap();
        set_user_review(&conn, ME, "le-souffle", "").unwrap();
        let last = journal_recent_first(&conn, ME).unwrap();
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
            "INSERT INTO ratings (user_id, movie_id, half_stars, rated_at)
                 VALUES ('account-1001', 'rated-today', 8, '2026-08-04 10:00:00');
             INSERT INTO visitor_reviews (user_id, movie_id, body, written_at)
                 VALUES ('account-1001', 'written-later', 'Still thinking about it.',
                         '2026-08-04 11:00:00');",
        )
        .unwrap();
        let ids: Vec<String> =
            journal_recent_first(&conn, ME).unwrap().into_iter().map(|r| r.movie_id).collect();
        assert_eq!(ids, ["written-later", "rated-today"]);

        // Now rewrite the older film's review: the film moves to the front even
        // though its rating is untouched and older than the other entry.
        set_user_review(&conn, ME, "rated-today", "Came back to it.").unwrap();
        let after: Vec<String> =
            journal_recent_first(&conn, ME).unwrap().into_iter().map(|r| r.movie_id).collect();
        assert_eq!(after, ["rated-today", "written-later"]);
    }

    /// The bio is the one identity field a user owns — Google supplies the rest.
    /// `None` means untouched, which is what lets `content` supply the default.
    #[test]
    fn the_bio_is_stored_and_clearing_it_restores_the_default() {
        let conn = db();
        let bio = |conn: &Connection| account(conn, ME).unwrap().unwrap().bio;
        sign_in(&conn, "1001", "testviewer");
        assert_eq!(bio(&conn), None, "an untouched bio is absent, not blank");

        assert_eq!(set_user_bio(&conn, ME, "  Watches too much. ").unwrap(),
                   Some("Watches too much.".into()));
        assert_eq!(bio(&conn), Some("Watches too much.".into()));

        // Editing replaces rather than accumulating: it's one column on one row.
        set_user_bio(&conn, ME, "Second draft.").unwrap();
        assert_eq!(bio(&conn), Some("Second draft.".into()));
        let rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM people WHERE id = ?1", [ME], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 1);

        assert_eq!(set_user_bio(&conn, ME, "  ").unwrap(), None);
        assert_eq!(bio(&conn), None);

        // And it is the *account's* bio: another sign-in must not see it.
        set_user_bio(&conn, ME, "Mine alone.").unwrap();
        let other = sign_in(&conn, "2002", "someoneelse");
        assert_eq!(other.bio, None);
    }

    // --- Accounts, sessions and CSRF ------------------------------------------

    /// Sign-in mints one account per Google subject, and signing in again finds the
    /// same row rather than making a second one.
    #[test]
    fn signing_in_twice_is_the_same_account() {
        let conn = graph();
        let first = sign_in(&conn, "1001", "testviewer");
        assert_eq!(first.id, ME);

        let again = sign_in(&conn, "1001", "adifferentnickname");
        assert_eq!(again.id, first.id);
        // The nickname is theirs, not Google's: people may be following it, so a
        // second sign-in must not rename them.
        assert_eq!(again.handle, first.handle);

        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM people WHERE is_account = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts, 1);
    }

    /// Two Google accounts wanting the same nickname both get one, because a
    /// nickname addresses a page and the column is unique.
    #[test]
    fn nicknames_are_made_unique_on_collision() {
        let conn = db();
        let first = sign_in(&conn, "1001", "sam");
        let second = sign_in(&conn, "2002", "sam");
        let third = sign_in(&conn, "3003", "sam");

        assert_eq!(first.handle, "@sam");
        assert_eq!(second.handle, "@sam2");
        assert_eq!(third.handle, "@sam3");
        // All three are reachable, which is the point of making them differ.
        for handle in [&first.handle, &second.handle, &third.handle] {
            assert!(person_by_handle(&conn, ME, handle).unwrap().is_some(), "{handle}");
        }
    }

    /// A new account opens on the seed's friends rather than an empty feed.
    #[test]
    fn a_new_account_starts_out_following_the_starter_set() {
        let conn = graph();
        let expected = GRAPH.iter().filter(|user| user.followed).count();
        assert_eq!(follow_count(&conn, ME).unwrap() as usize, expected);

        // The same set for the next person to sign in — it comes off `people`, not
        // off somebody else's follow rows.
        let other = sign_in(&conn, "2002", "someoneelse");
        assert_eq!(follow_count(&conn, &other.id).unwrap() as usize, expected);

        // Granting twice writes nothing new.
        assert_eq!(grant_starter_follows(&conn, ME).unwrap(), 0);
    }

    /// The whole point of a session row: what one account has is not what another
    /// has, and neither is what an anonymous reader has.
    #[test]
    fn two_accounts_do_not_share_a_watchlist() {
        let conn = graph();
        let other = sign_in(&conn, "2002", "someoneelse").id;

        set_watchlist(&conn, ME, "le-souffle", Some(true)).unwrap();
        set_rating(&conn, ME, "le-souffle", 9).unwrap();
        set_favorite(&conn, ME, "le-souffle", Some(true)).unwrap();
        set_user_review(&conn, ME, "le-souffle", "Mine.").unwrap();
        toggle_review_like(&conn, ME, "user-elenarostova-dune-part-two").unwrap();
        add_comment(&conn, ME, "user-elenarostova-dune-part-two", "Mine too.").unwrap();

        set_watchlist(&conn, &other, "red-shift", Some(true)).unwrap();

        let mine = load_store(&conn, ME).unwrap();
        let theirs = load_store(&conn, &other).unwrap();
        let nobodys = load_store(&conn, ANONYMOUS).unwrap();

        assert_eq!(mine.watchlist.iter().map(String::as_str).collect::<Vec<_>>(), ["le-souffle"]);
        assert_eq!(theirs.watchlist.iter().map(String::as_str).collect::<Vec<_>>(), ["red-shift"]);
        assert!(nobodys.watchlist.is_empty());

        // Every other delta is scoped the same way, which is six chances to leak.
        assert!(theirs.ratings.is_empty() && nobodys.ratings.is_empty());
        assert!(theirs.favorites.is_empty() && nobodys.favorites.is_empty());
        assert!(theirs.written_reviews.is_empty() && nobodys.written_reviews.is_empty());
        assert!(theirs.liked_reviews.is_empty() && nobodys.liked_reviews.is_empty());
        // Comments are shared content, so both accounts see the one that was posted —
        // but only its author is credited with it.
        let posted = thread(&conn, "user-elenarostova-dune-part-two").unwrap();
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].author_id, ME);

        // Both can watchlist the same film, which the old single-column primary key
        // made impossible.
        assert!(set_watchlist(&conn, &other, "le-souffle", Some(true)).unwrap());
        assert_eq!(watchlist_recent_first(&conn, ME).unwrap(), ["le-souffle"]);
        assert_eq!(watchlist_recent_first(&conn, &other).unwrap().len(), 2);

        // And their follows are their own, so one unfollowing changes nothing for the
        // other.
        set_follow(&conn, ME, "user-elenarostova", Some(false)).unwrap();
        assert!(person_by_handle(&conn, &other, "elenarostova").unwrap().unwrap().following);
        assert!(!person_by_handle(&conn, ME, "elenarostova").unwrap().unwrap().following);
    }

    /// Two accounts can follow each other, and each sees the other as a follower —
    /// which the seeded `follows_visitor` flag cannot express.
    #[test]
    fn accounts_can_follow_each_other() {
        let conn = graph();
        let other = sign_in(&conn, "2002", "someoneelse");

        assert_eq!(set_follow(&conn, ME, &other.id, Some(true)).unwrap(), Some(true));
        let seen_by_them = person_by_handle(&conn, &other.id, "testviewer").unwrap().unwrap();
        assert!(seen_by_them.follows_you, "their follower list misses a real follow");
        assert!(!seen_by_them.following, "they have not followed back");

        let handles: Vec<String> =
            followers(&conn, &other.id).unwrap().into_iter().map(|row| row.handle).collect();
        assert!(handles.contains(&"@testviewer".to_string()));
    }

    /// A session is a row, so logging out really revokes it — the token stops working
    /// rather than merely being forgotten by one browser.
    #[test]
    fn logging_out_revokes_the_session() {
        let conn = graph();
        let token = "a-token";
        create_session(&conn, token, ME).unwrap();

        assert_eq!(session_account(&conn, token).unwrap().map(|a| a.id).as_deref(), Some(ME));

        assert!(delete_session(&conn, token).unwrap());
        assert!(session_account(&conn, token).unwrap().is_none(), "the token still works");
        // Twice is not an error, so a repeated logout is safe.
        assert!(!delete_session(&conn, token).unwrap());

        // An unknown token was never a session.
        assert!(session_account(&conn, "never-issued").unwrap().is_none());
    }

    /// An expired session reads as no session at all rather than as an error, so the
    /// request is simply anonymous.
    #[test]
    fn an_expired_session_is_no_session() {
        let conn = graph();
        conn.execute(
            "INSERT INTO sessions (token, user_id, expires_at)
             VALUES ('stale', ?1, datetime('now', '-1 day'))",
            [ME],
        )
        .unwrap();
        assert!(session_account(&conn, "stale").unwrap().is_none());

        // And it is swept the next time somebody signs in.
        create_session(&conn, "fresh", ME).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE token = 'stale'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    /// The CSRF check. A callback may only be completed with a `state` this server
    /// issued, and only once.
    #[test]
    fn an_auth_state_is_accepted_once_and_never_again() {
        let conn = db();
        remember_auth_state(&conn, "issued").unwrap();

        // A value nobody issued is refused, which is the mismatch case.
        assert!(!consume_auth_state(&conn, "forged").unwrap());
        assert!(!consume_auth_state(&conn, "").unwrap());

        assert!(consume_auth_state(&conn, "issued").unwrap());
        // Replaying the same callback finds nothing left to spend.
        assert!(!consume_auth_state(&conn, "issued").unwrap());
    }

    /// And a state left in a tab overnight is refused too, rather than waiting
    /// forever for its callback.
    #[test]
    fn a_stale_auth_state_is_refused_and_swept() {
        let conn = db();
        conn.execute(
            "INSERT INTO auth_states (state, created_at) VALUES ('old', datetime('now', '-1 day'))",
            [],
        )
        .unwrap();
        assert!(!consume_auth_state(&conn, "old").unwrap());

        // Starting a new sign-in clears it, so the table cannot grow without bound.
        remember_auth_state(&conn, "new").unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM auth_states WHERE state = 'old'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    /// A rating written before `rated_at` existed must still come back — it sorts
    /// last rather than disappearing from the profile.
    #[test]
    fn ratings_predating_the_timestamp_column_still_load() {
        let conn = db();
        conn.execute(
            "INSERT INTO ratings (user_id, movie_id, half_stars) VALUES (?1, 'legacy', 9)",
            [ME],
        )
        .unwrap();
        set_rating(&conn, ME, "current", 4).unwrap();

        let recent: Vec<String> =
            journal_recent_first(&conn, ME).unwrap().into_iter().map(|row| row.movie_id).collect();
        assert_eq!(recent, ["current", "legacy"], "NULL rated_at sorts last, not out");
        assert_eq!(load_store(&conn, ME).unwrap().ratings.get("legacy"), Some(&9));
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
        assert_eq!(set_rating(&conn, ME, "m", 8).unwrap(), Some(8));
    }

    /// The follow list is only real follows, and it is now the *only* list of people
    /// any screen draws — the export's rails, which listed people you had never
    /// chosen, are gone.
    #[test]
    fn following_is_only_who_you_really_follow() {
        let conn = db();
        assert!(following(&conn, ME).unwrap().is_empty(), "no follows, no rows");

        let conn = graph();
        let rows = following(&conn, ME).unwrap();
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids.len(), follow_count(&conn, ME).unwrap() as usize);
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
        // Signed in *after* the seed, because `grant_starter_follows` reads the
        // `starter_follow` flags the seed writes.
        assert_eq!(sign_in(&conn, "1001", "testviewer").id, ME);
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
        assert_eq!(search_people(&conn, ME, "").unwrap().len(), GRAPH.len());

        // Everyone the seed wrote is a user with a page. Nothing else is in the table
        // — the eleven decorative rows the old `seed` added are gone.
        let non_users: i64 =
            conn.query_row("SELECT COUNT(*) FROM people WHERE is_user = 0", [], |r| r.get(0))
                .unwrap();
        assert_eq!(non_users, 0);
        // And the seed wrote no follow rows: there was nobody to own one.
        let follows: i64 =
            conn.query_row("SELECT COUNT(*) FROM follows", [], |r| r.get(0)).unwrap();
        assert_eq!(follows, 0, "the seed followed somebody on nobody's behalf");
    }

    /// A half-finished harvest leaves a usable graph, and a later run leaves it
    /// alone rather than talking over the visitor's follows.
    #[test]
    fn a_partial_seed_stands_and_is_not_topped_up() {
        let conn = db();
        let all = demo_graph();
        seed_graph(&conn, &all[..2]).unwrap();
        assert_eq!(search_people(&conn, ME, "").unwrap().len(), 2);

        assert_eq!(seed_graph(&conn, &all).unwrap(), 0, "the graph was re-seeded");
        assert_eq!(search_people(&conn, ME, "").unwrap().len(), 2);
        // The two who did land are complete — followable, with their reviews.
        assert!(!reviews_by_person(&conn, ME, &all[0].id).unwrap().is_empty());
        assert_eq!(set_follow(&conn, ME, &all[1].id, Some(true)).unwrap(), Some(true));
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
        let stored = search_people(&conn, ME, "").unwrap();
        assert_eq!(stored.len(), users.len() - 1);
        assert!(stored.iter().any(|u| u.handle == "@priyanaidu"));
    }

    #[test]
    fn handles_are_stored_with_an_at_sign_however_they_arrive() {
        let conn = db();
        let mut users = demo_graph();
        users[0].handle = "@alreadyprefixed".into();
        seed_graph(&conn, &users[..1]).unwrap();

        let stored = search_people(&conn, ME, "").unwrap();
        assert_eq!(stored[0].handle, "@alreadyprefixed", "the `@` was doubled");
        // And it's findable either way, because `person_by_handle` normalizes too.
        assert!(person_by_handle(&conn, ME, "alreadyprefixed").unwrap().is_some());
        assert!(person_by_handle(&conn, ME, "@alreadyprefixed").unwrap().is_some());
    }

    #[test]
    fn search_matches_nickname_and_name() {
        let conn = graph();

        let by_handle: Vec<String> =
            search_people(&conn, ME, "kline").unwrap().into_iter().map(|u| u.handle).collect();
        assert_eq!(by_handle, ["@sarahkline"]);

        // The display name works too, including the space the handle doesn't have.
        let by_name: Vec<String> =
            search_people(&conn, ME, "Sarah K").unwrap().into_iter().map(|u| u.name).collect();
        assert_eq!(by_name, ["Sarah Kline"]);

        // Case-insensitive, and a partial prefix is enough.
        assert_eq!(search_people(&conn, ME, "ELENA").unwrap().len(), 1);
        assert!(search_people(&conn, ME, "nobodyatall").unwrap().is_empty());
        // Empty lists everyone but the asker: that's what the screen shows before you
        // type, and a row offering to follow yourself would do nothing.
        assert_eq!(search_people(&conn, ME, "").unwrap().len(), GRAPH.len());
        assert!(search_people(&conn, ME, "testviewer").unwrap().is_empty());
    }

    /// The wildcards are escaped, so a search for them finds nothing rather than
    /// everyone.
    #[test]
    fn search_treats_wildcards_as_text() {
        let conn = graph();
        for pattern in ["%", "_", "\\", "%%", "a%"] {
            assert!(
                search_people(&conn, ME, pattern).unwrap().is_empty(),
                "'{pattern}' matched somebody"
            );
        }
    }

    /// People the visitor follows sort above strangers, so the friends you have are
    /// the first thing the directory shows.
    #[test]
    fn search_puts_followed_people_first() {
        let conn = graph();
        let results = search_people(&conn, ME, "").unwrap();
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
            followed_users(&conn, ME).unwrap().into_iter().map(|u| u.handle).collect();
        let followers: Vec<String> =
            followers(&conn, ME).unwrap().into_iter().map(|u| u.handle).collect();

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
        let before = follow_count(&conn, ME).unwrap();

        assert_eq!(set_follow(&conn, ME, id, Some(true)).unwrap(), Some(true));
        assert_eq!(follow_count(&conn, ME).unwrap(), before + 1);
        // Twice is still followed, and still one row.
        assert_eq!(set_follow(&conn, ME, id, Some(true)).unwrap(), Some(true));
        assert_eq!(follow_count(&conn, ME).unwrap(), before + 1);

        assert_eq!(set_follow(&conn, ME, id, Some(false)).unwrap(), Some(false));
        assert_eq!(set_follow(&conn, ME, id, Some(false)).unwrap(), Some(false));
        assert_eq!(follow_count(&conn, ME).unwrap(), before);

        // No body toggles.
        assert_eq!(set_follow(&conn, ME, id, None).unwrap(), Some(true));
        assert_eq!(set_follow(&conn, ME, id, None).unwrap(), Some(false));

        // And nobody follows themselves: that edge would put you in your own feed.
        assert_eq!(set_follow(&conn, ME, ME, Some(true)).unwrap(), None);
    }

    /// Unfollowing must not touch `follows_visitor` — one is the visitor's action,
    /// the other is about them, and conflating them would rewrite history.
    #[test]
    fn unfollowing_leaves_their_side_of_the_graph_alone() {
        let conn = graph();
        let handle = "elenarostova";
        assert!(person_by_handle(&conn, ME, handle).unwrap().unwrap().follows_you);

        set_follow(&conn, ME, "user-elenarostova", Some(false)).unwrap();
        let after = person_by_handle(&conn, ME, handle).unwrap().unwrap();
        assert!(!after.following);
        assert!(after.follows_you, "they stopped following the visitor too");
    }

    /// Only real users are followable. The export's decorative cast has no page and
    /// no follow button, and a stray id must not create a dangling row.
    #[test]
    fn the_export_cast_is_not_followable_or_findable() {
        let conn = graph();
        let before = follow_count(&conn, ME).unwrap();
        assert_eq!(set_follow(&conn, ME, "elena", Some(true)).unwrap(), None);
        assert_eq!(set_follow(&conn, ME, "no-such-person", Some(true)).unwrap(), None);
        assert_eq!(follow_count(&conn, ME).unwrap(), before, "a dangling follow row was written");

        assert!(person_by_id(&conn, ME, "elena").unwrap().is_none());
        assert!(person_by_handle(&conn, ME, "elena").unwrap().is_none());
        // And "Marcus" finds the user, not the export's story-rail Marcus.
        let found: Vec<String> =
            search_people(&conn, ME, "Marcus").unwrap().into_iter().map(|u| u.id).collect();
        assert_eq!(found, ["user-marcusdrey"]);
    }

    // --- An account's own reviews ----------------------------------------------

    /// The gap this closes: an account wrote reviews and nobody could see them, so
    /// following somebody bought you nothing.
    ///
    /// One review, and every screen that lists reviews has to find it — the author's
    /// public page, the film's page, the whole graph's list, and the feed of somebody
    /// who follows them.
    #[test]
    fn an_accounts_review_reaches_every_screen_a_seeded_one_does() {
        let conn = graph();
        let follower = sign_in(&conn, "2002", "ada").id;
        set_follow(&conn, &follower, ME, Some(true)).unwrap();

        set_rating(&conn, ME, "le-souffle", 9).unwrap();
        set_user_review(&conn, ME, "le-souffle", "Still the best hour of the New Wave.").unwrap();

        let id = review_id(ME, "le-souffle");

        // Their own page.
        let theirs = reviews_by_person(&conn, &follower, ME).unwrap();
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].body, "Still the best hour of the New Wave.");
        assert_eq!(theirs[0].half_stars, Some(9), "the score came from `ratings`");
        // Attributed to them, with the name, nickname and face their profile shows.
        assert_eq!(theirs[0].name, "testviewer the viewer");
        assert_eq!(theirs[0].handle, "@testviewer");
        assert!(theirs[0].followed, "the asker follows them");

        // The film's page, beside the seeded people's reviews of it.
        let on_film: Vec<String> = reviews_for_movie(&conn, &follower, "le-souffle")
            .unwrap()
            .into_iter()
            .map(|row| row.person_id)
            .collect();
        assert!(on_film.contains(&ME.to_string()));
        assert!(on_film.len() > 1, "the seeded reviews of this film went missing");

        // The follower's feed, which is the whole reason any of this matters.
        let followed: Vec<String> = reviews_from_followed(&conn, &follower, 50)
            .unwrap()
            .into_iter()
            .map(|row| review_id(&row.person_id, &row.movie_id))
            .collect();
        assert!(followed.contains(&id), "a follower's feed misses the review");

        // And not the feed of somebody who does not follow them.
        let stranger = sign_in(&conn, "3003", "stranger").id;
        let theirs_too: Vec<String> = reviews_from_followed(&conn, &stranger, 50)
            .unwrap()
            .into_iter()
            .map(|row| row.person_id)
            .collect();
        assert!(!theirs_too.contains(&ME.to_string()));

        // The graph-wide list, and the id resolves from either direction.
        assert!(recent_reviews(&conn, &follower, 50)
            .unwrap()
            .iter()
            .any(|row| row.person_id == ME));
        assert_eq!(review_by_id(&conn, &follower, &id).unwrap().map(|row| row.movie_id),
                   Some("le-souffle".to_string()));

        // A seeded person's reviews still work exactly as before.
        assert_eq!(reviews_by_person(&conn, &follower, "user-elenarostova").unwrap().len(), 5);
    }

    /// Prose without a score is a real state here — the two are separate acts — so it
    /// publishes with no rating rather than with zero stars.
    #[test]
    fn a_review_written_without_a_score_has_no_rating() {
        let conn = graph();
        set_user_review(&conn, ME, "endless", "No stars from me, just words.").unwrap();

        let rows = reviews_by_person(&conn, ME, ME).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].half_stars, None);

        // Scoring it later fills the same review in rather than making a second one.
        set_rating(&conn, ME, "endless", 6).unwrap();
        let rows = reviews_by_person(&conn, ME, ME).unwrap();
        assert_eq!(rows.len(), 1, "the rating became a second review");
        assert_eq!(rows[0].half_stars, Some(6));

        // Clearing the score leaves the prose standing, as it always has.
        set_rating(&conn, ME, "endless", 0).unwrap();
        assert_eq!(reviews_by_person(&conn, ME, ME).unwrap()[0].half_stars, None);

        // A rating with nothing written is not a review at all: there would be no
        // prose for the page to render.
        set_rating(&conn, ME, "red-shift", 8).unwrap();
        let films: Vec<String> =
            reviews_by_person(&conn, ME, ME).unwrap().into_iter().map(|r| r.movie_id).collect();
        assert_eq!(films, ["endless"]);
    }

    /// The count and the list have to be the same rows, whichever table they came out
    /// of, or a page prints a number its own contents contradict.
    #[test]
    fn review_counts_include_an_accounts_own_reviews() {
        let conn = graph();
        let other = sign_in(&conn, "2002", "ada").id;
        let counted = |conn: &Connection, id: &str| {
            person_by_id(conn, other.as_str(), id).unwrap().unwrap().review_count
        };

        assert_eq!(counted(&conn, ME), 0);

        set_user_review(&conn, ME, "le-souffle", "One.").unwrap();
        set_user_review(&conn, ME, "endless", "Two.").unwrap();
        assert_eq!(counted(&conn, ME), 2);
        assert_eq!(reviews_by_person(&conn, &other, ME).unwrap().len(), 2);

        // Editing is not a second review, and clearing one takes the count back down.
        set_user_review(&conn, ME, "le-souffle", "One, rewritten.").unwrap();
        assert_eq!(counted(&conn, ME), 2);
        set_user_review(&conn, ME, "endless", "").unwrap();
        assert_eq!(counted(&conn, ME), 1);

        // A seeded person's count is untouched by any of it.
        assert_eq!(counted(&conn, "user-elenarostova"), 5);

        // The profile's "Following" list counts the same way its own page does.
        set_follow(&conn, &other, ME, Some(true)).unwrap();
        let row = following(&conn, &other).unwrap().into_iter().find(|r| r.id == ME).unwrap();
        assert_eq!(row.review_count, 1);
    }

    /// The stories rail opens on somebody's newest review, and an account's counts as
    /// one — otherwise following a real person leaves a dimmed, unopenable circle.
    #[test]
    fn a_story_circle_opens_an_accounts_newest_review() {
        let conn = graph();
        let follower = sign_in(&conn, "2002", "ada").id;
        set_follow(&conn, &follower, ME, Some(true)).unwrap();

        let circle = |conn: &Connection| {
            followed_with_newest_review(conn, &follower, 50)
                .unwrap()
                .into_iter()
                .find(|row| row.id == ME)
                .expect("the followed account has a circle")
                .newest_review
        };
        assert_eq!(circle(&conn), None, "nothing written yet, so nothing to open");

        set_user_review(&conn, ME, "le-souffle", "Words.").unwrap();
        let newest = circle(&conn).expect("a review to open");
        assert_eq!(newest, review_id(ME, "le-souffle"));
        // And it is an id that really resolves, which is what the circle relies on.
        assert!(review_by_id(&conn, &follower, &newest).unwrap().is_some());
    }

    /// The film page's ordering: friends first, then the best-rated stranger.
    #[test]
    fn a_films_reviews_put_friends_first_then_the_highest_rated() {
        let conn = graph();
        let reviews = reviews_for_movie(&conn, ME, "dune-part-two").unwrap();
        let handles: Vec<&str> = reviews.iter().map(|r| r.handle.as_str()).collect();

        // Elena (9, followed) and Marcus (7, followed) outrank Priya (8, stranger)
        // even though she rated it higher than Marcus did.
        assert_eq!(handles, ["@elenarostova", "@marcusdrey", "@priyanaidu"]);
        assert!(reviews[0].followed && reviews[1].followed && !reviews[2].followed);

        // Unfollow the top one and she drops behind her own friend, then below the
        // stranger who rated it higher than she did — this is the fallback the
        // film page relies on when you have no friends who reviewed it.
        set_follow(&conn, ME, "user-elenarostova", Some(false)).unwrap();
        let after: Vec<String> =
            reviews_for_movie(&conn, ME, "dune-part-two").unwrap().into_iter().map(|r| r.handle).collect();
        assert_eq!(after, ["@marcusdrey", "@elenarostova", "@priyanaidu"]);
    }

    /// With no friends at all it's purely best-rated first — "random people's
    /// reviews or high-star reviews", which is the whole point of the fallback.
    #[test]
    fn with_no_friends_a_films_reviews_are_best_rated_first() {
        let conn = graph();
        conn.execute("DELETE FROM follows", []).unwrap();

        let stars: Vec<Option<u8>> =
            reviews_for_movie(&conn, ME, "dune-part-two").unwrap().iter().map(|r| r.half_stars).collect();
        assert_eq!(stars, [Some(9), Some(8), Some(7)]);
    }

    #[test]
    fn a_films_reviews_are_empty_for_a_film_nobody_reviewed() {
        let conn = graph();
        assert!(reviews_for_movie(&conn, ME, "no-such-film").unwrap().is_empty());
    }

    #[test]
    fn a_persons_reviews_come_back_newest_first() {
        let conn = graph();
        let reviews = reviews_by_person(&conn, ME, "user-elenarostova").unwrap();
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
            let stored = person_by_handle(&conn, ME, &user.handle).unwrap().unwrap();
            assert_eq!(stored.review_count as usize, user.reviews.len());
            assert_eq!(reviews_by_person(&conn, ME, &stored.id).unwrap().len(), user.reviews.len());
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
        assert!(search_people(&conn, ME, "").unwrap().is_empty());
        assert!(followed_with_newest_review(&conn, ME, 10).unwrap().is_empty());

        // And twice is still fine, on the DB and on the unique index.
        prepare(&conn).unwrap();
        assert_eq!(seed_graph(&conn, &demo_graph()).unwrap(), GRAPH.len());
    }

    /// A database written before sign-in existed, with one visitor's rows in it.
    ///
    /// The tables are declared the way the pre-accounts schema declared them —
    /// `movie_id` alone as the primary key, no `user_id` — which is the shape the
    /// migration has to recognise.
    fn pre_accounts_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL,
                 avatar_src TEXT NOT NULL, avatar_alt TEXT NOT NULL,
                 unseen INTEGER NOT NULL DEFAULT 0,
                 in_stories INTEGER NOT NULL DEFAULT 0,
                 position INTEGER NOT NULL DEFAULT 0,
                 handle TEXT UNIQUE, bio TEXT,
                 is_user INTEGER NOT NULL DEFAULT 0,
                 follows_visitor INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE follows (
                 person_id TEXT PRIMARY KEY REFERENCES people(id),
                 followed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE watchlist (
                 movie_id TEXT PRIMARY KEY,
                 added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE favorites (
                 movie_id TEXT PRIMARY KEY,
                 added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE ratings (
                 movie_id TEXT PRIMARY KEY, half_stars INTEGER NOT NULL, rated_at TEXT);
             CREATE TABLE visitor_reviews (
                 movie_id TEXT PRIMARY KEY, body TEXT NOT NULL,
                 written_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE liked_reviews (review_id TEXT PRIMARY KEY);
             CREATE TABLE liked_comments (comment_id TEXT PRIMARY KEY);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE comments (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, review_id TEXT NOT NULL,
                 body TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE replies (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, review_id TEXT NOT NULL,
                 comment_id TEXT NOT NULL, body TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);

             INSERT INTO people (id, name, avatar_src, avatar_alt, handle, is_user,
                                 follows_visitor)
                 VALUES ('user-elenarostova', 'Elena Rostova', 'img/e.jpg', 'Elena.',
                         '@elenarostova', 1, 1);
             INSERT INTO follows (person_id) VALUES ('user-elenarostova');
             INSERT INTO watchlist (movie_id) VALUES ('le-souffle'), ('red-shift');
             INSERT INTO favorites (movie_id) VALUES ('neon-reverie');
             INSERT INTO ratings (movie_id, half_stars, rated_at)
                 VALUES ('the-drop', 7, '2026-01-01 10:00:00');
             INSERT INTO visitor_reviews (movie_id, body) VALUES ('endless', 'Held up.');
             INSERT INTO liked_reviews (review_id) VALUES ('user-elenarostova-le-souffle');
             INSERT INTO liked_comments (comment_id) VALUES ('comment-1');
             INSERT INTO settings (key, value) VALUES ('visitor_bio', 'Watches too much.');
             INSERT INTO comments (review_id, body)
                 VALUES ('user-elenarostova-le-souffle', 'Agreed.');
             INSERT INTO replies (review_id, comment_id, body)
                 VALUES ('user-elenarostova-le-souffle', 'comment-1', 'Still agreed.');",
        )
        .unwrap();
        conn
    }

    /// The migration that matters most: the one visitor's rows get an owner rather
    /// than being dropped for a tidier schema.
    #[test]
    fn the_pre_account_rows_move_to_a_legacy_account() {
        let conn = pre_accounts_db();
        for table in ["watchlist", "favorites", "ratings", "visitor_reviews", "follows"] {
            assert!(!has_column(&conn, table, "user_id").unwrap(), "{table}");
        }

        prepare(&conn).unwrap();

        // The account exists, wears the export's identity, and cannot be signed in as.
        let legacy = account(&conn, LEGACY_USER_ID).unwrap().expect("a legacy account");
        assert_eq!(legacy.name, crate::hydrate::VISITOR_NAME);
        assert_eq!(legacy.handle, crate::hydrate::VISITOR_HANDLE);
        let sub: Option<String> = conn
            .query_row("SELECT google_sub FROM people WHERE id = ?1", [LEGACY_USER_ID], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sub, None, "the legacy account can be signed in as");

        // Every delta came across, and it is theirs.
        let store = load_store(&conn, LEGACY_USER_ID).unwrap();
        assert_eq!(
            store.watchlist.iter().map(String::as_str).collect::<Vec<_>>(),
            ["le-souffle", "red-shift"]
        );
        assert!(store.favorites.contains("neon-reverie"));
        assert_eq!(store.ratings.get("the-drop"), Some(&7));
        assert_eq!(store.written_reviews.get("endless").map(String::as_str), Some("Held up."));
        assert!(store.liked_reviews.contains("user-elenarostova-le-souffle"));
        assert!(store.liked_comments.contains("comment-1"));
        let migrated = thread(&conn, "user-elenarostova-le-souffle").unwrap();
        assert_eq!(migrated.len(), 1, "the adopted comment stopped rendering");
        assert_eq!(migrated[0].author_id, LEGACY_USER_ID);
        assert_eq!(migrated[0].body, "Agreed.");
        assert_eq!(migrated[0].replies.len(), 1);
        assert_eq!(migrated[0].replies[0].body, "Still agreed.");
        assert_eq!(follow_count(&conn, LEGACY_USER_ID).unwrap(), 1);

        // Their bio moved onto their own row, where a bio lives now.
        assert_eq!(legacy.bio.as_deref(), Some("Watches too much."));
        // And nobody else inherited any of it.
        assert!(load_store(&conn, ANONYMOUS).unwrap().watchlist.is_empty());

        // Whoever they followed becomes the starter set, so the next account to sign
        // in still opens on a feed.
        let signed_in = sign_in(&conn, "1001", "testviewer");
        assert_eq!(follow_count(&conn, &signed_in.id).unwrap(), 1);

        // The graph is not re-seeded over the top of theirs, and running the migration
        // twice changes nothing.
        assert!(!needs_graph_seed(&conn).unwrap());
        prepare(&conn).unwrap();
        assert_eq!(load_store(&conn, LEGACY_USER_ID).unwrap().watchlist.len(), 2);
    }

    /// The comment ids the frontend holds have to survive the migration: those two
    /// tables gain a column rather than being rebuilt.
    #[test]
    fn migrating_keeps_the_ids_a_client_already_has() {
        let conn = pre_accounts_db();
        prepare(&conn).unwrap();

        assert_eq!(thread(&conn, "user-elenarostova-le-souffle").unwrap()[0].id, "comment-1");
        // And the next one continues the sequence rather than colliding with it.
        assert_eq!(
            add_comment(&conn, LEGACY_USER_ID, "user-elenarostova-le-souffle", "More.").unwrap(),
            "comment-2"
        );
    }

    /// A pre-accounts database with the old schema but nothing in it gains no
    /// account: the migration adopts rows, it does not invent people.
    #[test]
    fn an_empty_older_database_gains_no_legacy_account() {
        let conn = pre_accounts_db();
        conn.execute_batch(
            "DELETE FROM watchlist; DELETE FROM favorites; DELETE FROM ratings;
             DELETE FROM visitor_reviews; DELETE FROM liked_reviews;
             DELETE FROM liked_comments; DELETE FROM settings;
             DELETE FROM replies; DELETE FROM comments; DELETE FROM follows;",
        )
        .unwrap();

        prepare(&conn).unwrap();
        assert!(account(&conn, LEGACY_USER_ID).unwrap().is_none());
        // The schema is still brought forward, so writes work.
        assert!(has_column(&conn, "watchlist", "user_id").unwrap());
        assert!(set_watchlist(&conn, ME, "le-souffle", Some(true)).unwrap());
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
        let stored = person_by_handle(&conn, ME, &seeded.handle).unwrap().unwrap();

        assert_eq!(favorites_by_person(&conn, &stored.id).unwrap(), seeded.favorites);
        assert_eq!(watchlist_by_person(&conn, &stored.id).unwrap(), seeded.watchlist);
        assert!(!seeded.favorites.is_empty() && !seeded.watchlist.is_empty());

        // Nobody's two strips name the same film, and everyone has something.
        for user in demo_graph() {
            let id = person_by_handle(&conn, ME, &user.handle).unwrap().unwrap().id;
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
        let id = person_by_handle(&conn, ME, &demo_graph()[0].handle).unwrap().unwrap().id;
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
