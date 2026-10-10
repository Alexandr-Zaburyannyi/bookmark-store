use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::PathBuf;
use std::sync::OnceLock;

static DB_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_db_path(path: PathBuf) {
    DB_PATH
        .set(path)
        .expect("set_db_path called more than once");
}

pub fn connect_to_db() -> Result<Connection> {
    let path = DB_PATH.get().expect("db path not set");
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub fn initialize_db(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            link TEXT NOT NULL,
            description TEXT NOT NULL,
            relevant BOOLEAN DEFAULT TRUE
        )",
        [],
    )?;
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current_version: i64 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

    let migrations: Vec<fn(&rusqlite::Transaction) -> Result<()>> = vec![
        migration_001_sync_schema,
        migration_002_folders,
        migration_003_write_back,
    ];

    for (i, migration) in migrations.iter().enumerate() {
        let version = (i + 1) as i64;
        if version <= current_version {
            continue;
        }

        let tx = conn.transaction()?;
        migration(&tx)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }

    Ok(())
}

fn migration_001_sync_schema(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE bookmarks ADD COLUMN url_key    TEXT;
         ALTER TABLE bookmarks ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE bookmarks ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE bookmarks ADD COLUMN deleted_at INTEGER;
         CREATE INDEX idx_bookmarks_url_key ON bookmarks(url_key);

         CREATE TABLE sources (
           id             INTEGER PRIMARY KEY,
           path           TEXT NOT NULL UNIQUE,
           family         TEXT NOT NULL,
           browser        TEXT NOT NULL,
           profile        TEXT NOT NULL,
           last_synced_at INTEGER,
           last_error     TEXT
         );

         CREATE TABLE bookmark_sources (
           id           INTEGER PRIMARY KEY,
           bookmark_id  INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
           source_id    INTEGER NOT NULL REFERENCES sources(id)   ON DELETE CASCADE,
           external_id  TEXT    NOT NULL,
           title        TEXT    NOT NULL,
           folder_path  TEXT    NOT NULL,
           added_at     INTEGER,
           present      INTEGER NOT NULL DEFAULT 1,
           last_seen_at INTEGER NOT NULL,
           removed_at   INTEGER,
           UNIQUE (source_id, external_id)
         );
         CREATE INDEX idx_bookmark_sources_bookmark ON bookmark_sources(bookmark_id);",
    )?;

    let now = crate::url_key::now_ms();
    let mut select_stmt = tx.prepare("SELECT id, link FROM bookmarks")?;
    let rows: Vec<(i64, String)> = select_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>>>()?;
    drop(select_stmt);

    let mut update_stmt = tx.prepare(
        "UPDATE bookmarks SET url_key = ?1, created_at = ?2, updated_at = ?2 WHERE id = ?3",
    )?;
    for (id, link) in rows {
        update_stmt.execute(rusqlite::params![crate::url_key::url_key(&link), now, id])?;
    }

    Ok(())
}

fn migration_002_folders(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE folders (
           id         INTEGER PRIMARY KEY,
           name       TEXT    NOT NULL,
           parent_id  INTEGER REFERENCES folders(id) ON DELETE CASCADE,
           position   INTEGER NOT NULL DEFAULT 0,
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL
         );
         ALTER TABLE bookmarks ADD COLUMN folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL;
         CREATE INDEX idx_bookmarks_folder ON bookmarks(folder_id);",
    )
}

fn migration_003_write_back(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE folder_targets (
           id             INTEGER PRIMARY KEY,
           folder_id      INTEGER REFERENCES folders(id) ON DELETE SET NULL,
           source_id      INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
           state          TEXT    NOT NULL DEFAULT 'active',
           external_id    TEXT,
           last_pushed_at INTEGER,
           last_error     TEXT,
           UNIQUE (folder_id, source_id)
         );
         ALTER TABLE bookmark_sources ADD COLUMN pending_delete INTEGER NOT NULL DEFAULT 0;",
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    Restored,
    Exists,
}

impl AddOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            AddOutcome::Added => "added",
            AddOutcome::Restored => "restored",
            AddOutcome::Exists => "exists",
        }
    }
}

fn touch_folder_updated_at(
    tx: &rusqlite::Transaction,
    folder_id: Option<i64>,
    now: i64,
) -> Result<()> {
    if let Some(id) = folder_id {
        tx.execute(
            "UPDATE folders SET updated_at = ?1 WHERE id = ?2;",
            params![now, id],
        )?;
    }
    Ok(())
}

/// Adds a bookmark unless its URL is already saved. A link that was deleted in the app is
/// restored in place, so sync keeps attaching browser copies to the same row.
/// Returns (id, name, outcome).
pub fn add_or_restore_bookmark(
    conn: &mut Connection,
    name: &str,
    link: &str,
    description: &str,
    folder_id: Option<i64>,
    now: i64,
) -> Result<(i64, String, AddOutcome)> {
    let key = crate::url_key::url_key(link);
    let tx = conn.transaction()?;

    let live: Option<(i64, String)> = tx
        .query_row(
            "SELECT id, name FROM bookmarks WHERE url_key = ?1 AND deleted_at IS NULL ORDER BY id LIMIT 1;",
            params![key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((id, existing_name)) = live {
        return Ok((id, existing_name, AddOutcome::Exists));
    }

    // Same lookup as sync (sync/mod.rs), so soft-deleted rows are found here too.
    let deleted: Option<i64> = tx
        .query_row(
            "SELECT id FROM bookmarks WHERE url_key = ?1 ORDER BY id LIMIT 1;",
            params![key],
            |row| row.get(0),
        )
        .optional()?;

    let (id, outcome) = match deleted {
        Some(id) => {
            tx.execute(
                "UPDATE bookmarks
                 SET deleted_at = NULL, name = ?1, link = ?2, description = ?3,
                     folder_id = ?4, created_at = ?5, updated_at = ?5
                 WHERE id = ?6;",
                params![name, link, description, folder_id, now, id],
            )?;
            tx.execute(
                "UPDATE bookmark_sources SET pending_delete = 0 WHERE bookmark_id = ?1;",
                params![id],
            )?;
            (id, AddOutcome::Restored)
        }
        None => {
            tx.execute(
                "INSERT INTO bookmarks (name, link, description, folder_id, url_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6);",
                params![name, link, description, folder_id, key, now],
            )?;
            (tx.last_insert_rowid(), AddOutcome::Added)
        }
    };

    touch_folder_updated_at(&tx, folder_id, now)?;
    tx.commit()?;
    Ok((id, name.to_string(), outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_database() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        initialize_db(&conn).unwrap();
        migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn adding_a_new_link_stores_its_key_and_folder() {
        let mut conn = migrated_database();
        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 0)",
            [],
        )
        .unwrap();

        let (id, name, outcome) =
            add_or_restore_bookmark(&mut conn, "Example", "https://a.com", "", Some(1), 100)
                .unwrap();
        assert_eq!(outcome, AddOutcome::Added);
        assert_eq!(name, "Example");

        let (url_key, folder_id): (String, Option<i64>) = conn
            .query_row(
                "SELECT url_key, folder_id FROM bookmarks WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(url_key, "https://a.com/");
        assert_eq!(folder_id, Some(1));
    }

    #[test]
    fn adding_a_link_that_differs_only_by_case_reports_the_existing_bookmark() {
        let mut conn = migrated_database();

        let (first_id, _, _) =
            add_or_restore_bookmark(&mut conn, "First", "https://a.com", "", None, 100).unwrap();
        let (second_id, name, outcome) =
            add_or_restore_bookmark(&mut conn, "Second", "https://A.com", "", None, 200).unwrap();

        assert_eq!(outcome, AddOutcome::Exists);
        assert_eq!(second_id, first_id);
        assert_eq!(name, "First");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn adding_a_link_deleted_in_the_app_restores_it_and_cancels_pending_deletes() {
        let mut conn = migrated_database();

        let (id, _, _) =
            add_or_restore_bookmark(&mut conn, "Old", "https://a.com", "", None, 100).unwrap();
        conn.execute(
            "UPDATE bookmarks SET deleted_at = 150 WHERE id = ?1",
            params![id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources (id, path, family, browser, profile) VALUES (1, '/p', 'chromium', 'Brave', 'Default')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmark_sources (bookmark_id, source_id, external_id, title, folder_path, present, last_seen_at, pending_delete)
             VALUES (?1, 1, 'g-1', 'Old', '', 1, 0, 1)",
            params![id],
        )
        .unwrap();

        let (restored_id, _, outcome) =
            add_or_restore_bookmark(&mut conn, "New name", "https://a.com", "d", None, 300)
                .unwrap();
        assert_eq!(outcome, AddOutcome::Restored);
        assert_eq!(restored_id, id);

        let (deleted_at, name, pending): (Option<i64>, String, i64) = conn
            .query_row(
                "SELECT b.deleted_at, b.name, bs.pending_delete
                 FROM bookmarks b JOIN bookmark_sources bs ON bs.bookmark_id = b.id
                 WHERE b.id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(deleted_at, None);
        assert_eq!(name, "New name");
        assert_eq!(pending, 0);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn deleting_parent_folder_cascades_and_unfiles_bookmarks() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        initialize_db(&conn).unwrap();
        migrate(&mut conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);

        conn.execute(
            "INSERT INTO folders (id, name, parent_id, created_at, updated_at) VALUES (1, 'parent', NULL, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (id, name, parent_id, created_at, updated_at) VALUES (2, 'child', 1, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmarks (id, name, link, description, folder_id) VALUES (1, 'b', 'https://a.com', '', 2)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM folders WHERE id = 1", [])
            .unwrap();

        let folders: i64 = conn
            .query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(folders, 0);

        let folder_id: Option<i64> = conn
            .query_row("SELECT folder_id FROM bookmarks WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(folder_id, None);
    }

    #[test]
    fn deleting_a_folder_orphans_its_targets_instead_of_deleting_them() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        initialize_db(&conn).unwrap();
        migrate(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources (id, path, family, browser, profile) VALUES (1, '/p', 'chromium', 'Brave', 'Default')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folder_targets (folder_id, source_id, external_id) VALUES (1, 1, 'guid')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM folders WHERE id = 1", [])
            .unwrap();

        let (folder_id, external_id): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT folder_id, external_id FROM folder_targets",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(folder_id, None);
        assert_eq!(external_id.as_deref(), Some("guid"));
    }
}
