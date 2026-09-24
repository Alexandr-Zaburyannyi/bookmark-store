use super::Entry;
use rusqlite::{Connection, Result as SqliteResult};
use std::fs;
use std::path::{Path, PathBuf};

const TREE_QUERY: &str = "
WITH RECURSIVE tree(id, path) AS (
  SELECT id, CASE guid WHEN 'menu________'  THEN 'Bookmarks Menu'
                       WHEN 'toolbar_____'  THEN 'Bookmarks Toolbar'
                       WHEN 'unfiled_____'  THEN 'Other Bookmarks'
                       WHEN 'mobile______'  THEN 'Mobile Bookmarks' END
  FROM moz_bookmarks WHERE parent = 1 AND guid <> 'tags________'
  UNION ALL
  SELECT b.id, tree.path || '/' || b.title
  FROM moz_bookmarks b JOIN tree ON b.parent = tree.id WHERE b.type = 2
)
SELECT b.guid, COALESCE(NULLIF(b.title, ''), p.title, p.url), p.url, tree.path, b.dateAdded
FROM moz_bookmarks b
JOIN moz_places p ON p.id = b.fk
JOIN tree ON b.parent = tree.id
WHERE b.type = 1 AND p.url NOT LIKE 'place:%';
";

pub fn read(path: &Path, source_id: i64) -> Result<Vec<Entry>, String> {
    let temp = copy_to_temp(path, source_id).map_err(|e| e.to_string())?;
    let result = read_copy(&temp.db_path);
    temp.cleanup();
    result.map_err(|e| e.to_string())
}

struct TempCopy {
    db_path: PathBuf,
    wal_path: PathBuf,
}

impl TempCopy {
    fn cleanup(&self) {
        let _ = fs::remove_file(&self.db_path);
        let _ = fs::remove_file(&self.wal_path);
    }
}

fn copy_to_temp(path: &Path, source_id: i64) -> std::io::Result<TempCopy> {
    let dir = std::env::temp_dir().join("bookmark-store");
    fs::create_dir_all(&dir)?;

    let db_path = dir.join(format!("places-{source_id}.sqlite"));
    let wal_path = dir.join(format!("places-{source_id}.sqlite-wal"));

    fs::copy(path, &db_path)?;

    let source_wal = path.with_extension("sqlite-wal");
    if source_wal.is_file() {
        fs::copy(&source_wal, &wal_path)?;
    } else {
        let _ = fs::remove_file(&wal_path);
    }

    Ok(TempCopy { db_path, wal_path })
}

fn read_copy(db_path: &Path) -> SqliteResult<Vec<Entry>> {
    let conn = Connection::open(db_path)?;
    query_entries(&conn)
}

pub(super) fn query_entries(conn: &Connection) -> SqliteResult<Vec<Entry>> {
    let mut stmt = conn.prepare(TREE_QUERY)?;

    let entries = stmt
        .query_map([], |row| {
            let date_added: Option<i64> = row.get(4)?;
            Ok(Entry {
                external_id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                folder_path: row.get(3)?,
                added_at: date_added.and_then(firefox_time_to_unix_ms),
            })
        })?
        .collect::<SqliteResult<Vec<_>>>()?;

    Ok(entries)
}

fn firefox_time_to_unix_ms(raw: i64) -> Option<i64> {
    if raw <= 0 {
        None
    } else {
        Some(raw / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_bookmarks (
                id INTEGER PRIMARY KEY,
                type INTEGER,
                fk INTEGER,
                parent INTEGER,
                title TEXT,
                guid TEXT,
                dateAdded INTEGER
            );
            CREATE TABLE moz_places (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT
            );",
        )
        .unwrap();

        conn.execute_batch(
            "INSERT INTO moz_bookmarks (id, type, fk, parent, title, guid, dateAdded) VALUES
                (1, 2, NULL, 0, NULL, 'root________', NULL),
                (2, 2, NULL, 1, 'menu', 'menu________', NULL),
                (3, 2, NULL, 1, 'toolbar', 'toolbar_____', NULL),
                (4, 2, NULL, 1, 'tags', 'tags________', NULL),
                (5, 2, NULL, 1, 'unfiled', 'unfiled_____', NULL),
                (6, 2, NULL, 1, 'mobile', 'mobile______', NULL),
                (7, 2, NULL, 3, 'Dev', 'folder1_____', NULL),
                (8, 1, 100, 7, 'Rust', 'bm1_________', 1700000000000000),
                (9, 1, 100, 4, 'rust', 'tagbm_______', 1700000000000000);

            INSERT INTO moz_places (id, url, title) VALUES
                (100, 'https://rust-lang.org', 'Rust Lang');",
        )
        .unwrap();

        conn
    }

    #[test]
    fn excludes_the_tag_row_and_builds_the_nested_folder_path() {
        let conn = build_fixture();
        let entries = query_entries(&conn).unwrap();

        assert_eq!(entries.len(), 1, "the tags________ row must be excluded");
        assert_eq!(entries[0].external_id, "bm1_________");
        assert_eq!(entries[0].title, "Rust");
        assert_eq!(entries[0].url, "https://rust-lang.org");
        assert_eq!(entries[0].folder_path, "Bookmarks Toolbar/Dev");
    }

    #[test]
    fn converts_a_known_firefox_timestamp_to_unix_ms() {
        let conn = build_fixture();
        let entries = query_entries(&conn).unwrap();
        assert_eq!(entries[0].added_at, Some(1_700_000_000_000));
    }
}
