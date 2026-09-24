use rusqlite::{Connection, Result};
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

#[cfg(test)]
mod tests {
    use super::*;

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
