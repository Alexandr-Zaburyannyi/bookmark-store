use super::writeback::{Applied, DesiredNode, Plan};
use crate::url_key::{now_ms, url_key};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::time::Duration;

type Result<T> = rusqlite::Result<T>;

const GOLDEN_RATIO: u32 = 0x9E37_79B9;
const URL_HASH_MAX_BYTES: usize = 1500;

/// Mozilla's `HashString`: golden-ratio hash over the UTF-8 bytes.
fn hash_string(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |hash, &byte| {
        GOLDEN_RATIO.wrapping_mul(hash.rotate_left(5) ^ u32::from(byte))
    })
}

/// `moz_places.url_hash`: the scheme's hash in the high 16 bits of a 48-bit value, plus the
/// hash of the first 1500 bytes of the URL.
pub fn url_hash(url: &str) -> i64 {
    let bytes = url.as_bytes();
    let scheme = url.split(':').next().unwrap_or("");
    let prefix = u64::from(hash_string(scheme.as_bytes()) & 0xFFFF);
    let body = u64::from(hash_string(&bytes[..bytes.len().min(URL_HASH_MAX_BYTES)]));
    ((prefix << 32) + body) as i64
}

fn rev_host(url: &str) -> String {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_lowercase))
        .unwrap_or_default();
    if host.is_empty() {
        return String::new();
    }
    let mut reversed: String = host.chars().rev().collect();
    reversed.push('.');
    reversed
}

/// (prefix, host) as stored in `moz_origins`, e.g. ("https://", "example.com:8080").
fn origin_of(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_lowercase();
    let host = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };
    Some((format!("{}://", parsed.scheme()), host))
}

/// A 12-character base64url guid from 9 random bytes.
fn new_guid() -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let uuid = uuid::Uuid::new_v4();
    let mut out = String::with_capacity(12);
    for chunk in uuid.as_bytes()[..9].chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        for shift in [18, 12, 6, 0] {
            out.push(ALPHABET[((n >> shift) & 0x3F) as usize] as char);
        }
    }
    out
}

/// Which optional columns this Firefox version has; the schema changes between releases.
struct Columns {
    place_origin_id: bool,
    place_recalc_frecency: bool,
    origin_recalc_frecency: bool,
}

fn has_column(tx: &Transaction, table: &str, column: &str) -> Result<bool> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table});"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;
    Ok(names.iter().any(|name| name == column))
}

impl Columns {
    fn detect(tx: &Transaction) -> Result<Self> {
        Ok(Columns {
            place_origin_id: has_column(tx, "moz_places", "origin_id")?,
            place_recalc_frecency: has_column(tx, "moz_places", "recalc_frecency")?,
            origin_recalc_frecency: has_column(tx, "moz_origins", "recalc_frecency")?,
        })
    }
}

fn ensure_origin(tx: &Transaction, cols: &Columns, url: &str) -> Result<Option<i64>> {
    if !cols.place_origin_id {
        return Ok(None);
    }
    let Some((prefix, host)) = origin_of(url) else {
        return Ok(None);
    };

    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM moz_origins WHERE prefix = ?1 AND host = ?2;",
            params![prefix, host],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(existing);
    }

    let sql = if cols.origin_recalc_frecency {
        "INSERT INTO moz_origins (prefix, host, frecency, recalc_frecency) VALUES (?1, ?2, 1, 1);"
    } else {
        "INSERT INTO moz_origins (prefix, host, frecency) VALUES (?1, ?2, 1);"
    };
    tx.execute(sql, params![prefix, host])?;
    Ok(Some(tx.last_insert_rowid()))
}

/// Finds or creates the `moz_places` row for `url` and counts one more reference to it.
fn ensure_place(tx: &Transaction, cols: &Columns, url: &str, title: &str) -> Result<i64> {
    let hash = url_hash(url);
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM moz_places WHERE url_hash = ?1 AND url = ?2;",
            params![hash, url],
            |row| row.get(0),
        )
        .optional()?;

    let place_id = match existing {
        Some(id) => id,
        None => {
            let origin_id = ensure_origin(tx, cols, url)?;
            let mut columns = vec![
                "url",
                "title",
                "rev_host",
                "frecency",
                "guid",
                "foreign_count",
                "url_hash",
            ];
            if cols.place_recalc_frecency {
                columns.push("recalc_frecency");
            }
            if cols.place_origin_id {
                columns.push("origin_id");
            }
            let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT INTO moz_places ({}) VALUES ({});",
                columns.join(", "),
                placeholders.join(", ")
            );

            let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(url.to_string()),
                Box::new(title.to_string()),
                Box::new(rev_host(url)),
                Box::new(1i64),
                Box::new(new_guid()),
                Box::new(0i64),
                Box::new(hash),
            ];
            if cols.place_recalc_frecency {
                values.push(Box::new(1i64));
            }
            if cols.place_origin_id {
                values.push(Box::new(origin_id));
            }
            let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
            tx.execute(&sql, refs.as_slice())?;
            tx.last_insert_rowid()
        }
    };

    tx.execute(
        "UPDATE moz_places SET foreign_count = foreign_count + 1 WHERE id = ?1;",
        params![place_id],
    )?;
    Ok(place_id)
}

fn touch(tx: &Transaction, id: i64, ts: i64) -> Result<()> {
    tx.execute(
        "UPDATE moz_bookmarks
         SET lastModified = ?1, syncChangeCounter = syncChangeCounter + 1
         WHERE id = ?2;",
        params![ts, id],
    )?;
    Ok(())
}

fn insert_bookmark(
    tx: &Transaction,
    parent: i64,
    position: i64,
    node_type: i64,
    fk: Option<i64>,
    title: &str,
    ts: i64,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO moz_bookmarks
           (type, fk, parent, position, title, dateAdded, lastModified, guid, syncStatus, syncChangeCounter)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, 1, 1);",
        params![node_type, fk, parent, position, title, ts, new_guid()],
    )?;
    Ok(tx.last_insert_rowid())
}

fn delete_node(tx: &Transaction, id: i64, ts: i64) -> Result<()> {
    let (guid, node_type, fk, sync_status): (String, i64, Option<i64>, i64) = tx.query_row(
        "SELECT guid, type, fk, syncStatus FROM moz_bookmarks WHERE id = ?1;",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    if node_type == 2 {
        let mut stmt = tx.prepare("SELECT id FROM moz_bookmarks WHERE parent = ?1;")?;
        let children = stmt
            .query_map(params![id], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>>>()?;
        drop(stmt);
        for child in children {
            delete_node(tx, child, ts)?;
        }
    }

    if let Some(place_id) = fk {
        tx.execute(
            "UPDATE moz_places SET foreign_count = foreign_count - 1
             WHERE id = ?1 AND foreign_count > 0;",
            params![place_id],
        )?;
    }
    tx.execute("DELETE FROM moz_bookmarks WHERE id = ?1;", params![id])?;

    // Firefox Sync only learns about a deletion from a tombstone, and only for rows it has seen.
    if sync_status == 2 {
        tx.execute(
            "INSERT OR REPLACE INTO moz_bookmarks_deleted (guid, dateRemoved) VALUES (?1, ?2);",
            params![guid, ts],
        )?;
    }
    Ok(())
}

fn remove_by_guid(tx: &Transaction, guid: &str, ts: i64) -> Result<()> {
    let found: Option<(i64, i64, i64)> = tx
        .query_row(
            "SELECT id, parent, position FROM moz_bookmarks WHERE guid = ?1;",
            params![guid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((id, parent, position)) = found else {
        return Ok(());
    };
    // parent 0 is the root, and its children are Firefox's built-in folders.
    if parent <= 1 {
        return Ok(());
    }

    delete_node(tx, id, ts)?;
    tx.execute(
        "UPDATE moz_bookmarks
         SET position = position - 1, lastModified = ?3, syncChangeCounter = syncChangeCounter + 1
         WHERE parent = ?1 AND position > ?2;",
        params![parent, position, ts],
    )?;
    touch(tx, parent, ts)
}

struct Child {
    id: i64,
    node_type: i64,
    title: String,
    url: Option<String>,
}

fn load_children(tx: &Transaction, parent: i64) -> Result<Vec<Child>> {
    let mut stmt = tx.prepare(
        "SELECT b.id, b.type, COALESCE(b.title, ''), p.url
         FROM moz_bookmarks b LEFT JOIN moz_places p ON p.id = b.fk
         WHERE b.parent = ?1 ORDER BY b.position, b.id;",
    )?;
    let children = stmt
        .query_map(params![parent], |row| {
            Ok(Child {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                url: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(children)
}

fn reconcile(
    tx: &Transaction,
    cols: &Columns,
    parent: i64,
    desired: &[DesiredNode],
    ts: i64,
) -> Result<()> {
    let mut pool = load_children(tx, parent)?;
    let mut ordered: Vec<i64> = Vec::with_capacity(desired.len());
    let mut changed = false;

    for wanted in desired {
        match wanted {
            DesiredNode::Url { title, url } => {
                let key = url_key(url);
                let position = pool.iter().position(|child| {
                    child.node_type == 1
                        && child.url.as_deref().map(url_key).as_deref() == Some(key.as_str())
                });
                let id = match position {
                    Some(index) => {
                        let child = pool.remove(index);
                        if child.title != *title {
                            tx.execute(
                                "UPDATE moz_bookmarks SET title = ?1 WHERE id = ?2;",
                                params![title, child.id],
                            )?;
                            touch(tx, child.id, ts)?;
                            changed = true;
                        }
                        child.id
                    }
                    None => {
                        let place_id = ensure_place(tx, cols, &key, title)?;
                        changed = true;
                        insert_bookmark(
                            tx,
                            parent,
                            ordered.len() as i64,
                            1,
                            Some(place_id),
                            title,
                            ts,
                        )?
                    }
                };
                ordered.push(id);
            }
            DesiredNode::Folder { name, children } => {
                let position = pool
                    .iter()
                    .position(|child| child.node_type == 2 && child.title == *name);
                let id = match position {
                    Some(index) => pool.remove(index).id,
                    None => {
                        changed = true;
                        insert_bookmark(tx, parent, ordered.len() as i64, 2, None, name, ts)?
                    }
                };
                reconcile(tx, cols, id, children, ts)?;
                ordered.push(id);
            }
        }
    }

    for leftover in pool {
        delete_node(tx, leftover.id, ts)?;
        changed = true;
    }

    for (position, id) in ordered.iter().enumerate() {
        let moved = tx.execute(
            "UPDATE moz_bookmarks SET position = ?1 WHERE id = ?2 AND position <> ?1;",
            params![position as i64, id],
        )?;
        if moved > 0 {
            touch(tx, *id, ts)?;
            changed = true;
        }
    }

    if changed {
        touch(tx, parent, ts)?;
    }
    Ok(())
}

fn apply_in_tx(tx: &Transaction, plan: &Plan, now: i64) -> Result<Applied> {
    let cols = Columns::detect(tx)?;
    // Firefox stores microseconds.
    let ts = now * 1000;

    for guid in &plan.remove_ids {
        remove_by_guid(tx, guid, ts)?;
    }

    let mut applied = Applied::default();
    for op in &plan.publish {
        let existing: Option<i64> = match &op.external_id {
            Some(guid) => tx
                .query_row(
                    "SELECT id FROM moz_bookmarks WHERE guid = ?1 AND type = 2 AND parent > 1;",
                    params![guid],
                    |row| row.get(0),
                )
                .optional()?,
            None => None,
        };

        let folder_id = match existing {
            Some(id) => {
                let title: String = tx.query_row(
                    "SELECT COALESCE(title, '') FROM moz_bookmarks WHERE id = ?1;",
                    params![id],
                    |row| row.get(0),
                )?;
                if title != op.name {
                    tx.execute(
                        "UPDATE moz_bookmarks SET title = ?1 WHERE id = ?2;",
                        params![op.name, id],
                    )?;
                    touch(tx, id, ts)?;
                }
                id
            }
            None => {
                let toolbar: i64 = tx.query_row(
                    "SELECT id FROM moz_bookmarks WHERE guid = 'toolbar_____';",
                    [],
                    |row| row.get(0),
                )?;
                let position: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(position), -1) + 1 FROM moz_bookmarks WHERE parent = ?1;",
                    params![toolbar],
                    |row| row.get(0),
                )?;
                let id = insert_bookmark(tx, toolbar, position, 2, None, &op.name, ts)?;
                touch(tx, toolbar, ts)?;
                id
            }
        };

        reconcile(tx, &cols, folder_id, &op.children, ts)?;

        let guid: String = tx.query_row(
            "SELECT guid FROM moz_bookmarks WHERE id = ?1;",
            params![folder_id],
            |row| row.get(0),
        )?;
        applied.folder_ids.push((op.target_id, guid));
    }

    Ok(applied)
}

pub fn apply_to_conn(
    conn: &mut Connection,
    plan: &Plan,
    now: i64,
) -> std::result::Result<Applied, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let applied = apply_in_tx(&tx, plan, now).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(applied)
}

/// Writes to the real `places.sqlite`. Callers must have confirmed the browser is closed.
pub fn apply(path: &Path, plan: &Plan) -> std::result::Result<Applied, String> {
    let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    apply_to_conn(&mut conn, plan, now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::firefox::query_entries;
    use crate::sync::writeback::PublishOp;

    #[test]
    fn url_hash_matches_values_from_a_real_profile() {
        assert_eq!(
            url_hash("https://www.mozilla.org/en-GB/privacy/firefox/"),
            47356951092694
        );
        assert_eq!(
            url_hash("place:type=6&sort=14&maxResults=10"),
            268505606444332
        );
    }

    #[test]
    fn guids_are_twelve_url_safe_characters() {
        let guid = new_guid();
        assert_eq!(guid.len(), 12);
        assert!(guid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn rev_host_reverses_the_lowercase_host() {
        assert_eq!(rev_host("https://WWW.Mozilla.org/x"), "gro.allizom.www.");
        assert_eq!(rev_host("data:text/plain,hi"), "");
    }

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_origins (
               id INTEGER PRIMARY KEY, prefix TEXT NOT NULL, host TEXT NOT NULL,
               frecency INTEGER NOT NULL DEFAULT 0, recalc_frecency INTEGER NOT NULL DEFAULT 0,
               UNIQUE (prefix, host)
             );
             CREATE TABLE moz_places (
               id INTEGER PRIMARY KEY, url TEXT, title TEXT, rev_host TEXT,
               visit_count INTEGER DEFAULT 0, hidden INTEGER DEFAULT 0, typed INTEGER DEFAULT 0,
               frecency INTEGER DEFAULT -1, last_visit_date INTEGER, guid TEXT,
               foreign_count INTEGER DEFAULT 0, url_hash INTEGER DEFAULT 0,
               origin_id INTEGER, recalc_frecency INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE moz_bookmarks (
               id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, position INTEGER,
               title TEXT, dateAdded INTEGER, lastModified INTEGER, guid TEXT,
               syncStatus INTEGER NOT NULL DEFAULT 0, syncChangeCounter INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE moz_bookmarks_deleted (guid TEXT PRIMARY KEY, dateRemoved INTEGER);

             INSERT INTO moz_bookmarks (id, type, parent, position, title, guid) VALUES
               (1, 2, 0, 0, '', 'root________'),
               (2, 2, 1, 0, 'menu', 'menu________'),
               (3, 2, 1, 1, 'toolbar', 'toolbar_____'),
               (4, 2, 1, 2, 'tags', 'tags________'),
               (5, 2, 1, 3, 'unfiled', 'unfiled_____'),
               (6, 2, 1, 4, 'mobile', 'mobile______');
             INSERT INTO moz_places (id, url, title, guid, foreign_count, url_hash)
               VALUES (100, 'https://rust-lang.org/', 'Rust', 'place-guid-1', 2, 0);
             INSERT INTO moz_bookmarks (id, type, fk, parent, position, title, guid, syncStatus) VALUES
               (10, 1, 100, 3, 0, 'Existing synced', 'synced______', 2),
               (11, 1, 100, 3, 1, 'Existing new', 'newrow_______', 1);",
        )
        .unwrap();
        conn.execute(
            "UPDATE moz_places SET url_hash = ?1 WHERE id = 100",
            params![url_hash("https://rust-lang.org/")],
        )
        .unwrap();
        conn
    }

    fn url(title: &str, url: &str) -> DesiredNode {
        DesiredNode::Url {
            title: title.to_string(),
            url: url.to_string(),
        }
    }

    fn publish(external_id: Option<&str>, children: Vec<DesiredNode>) -> Plan {
        Plan {
            publish: vec![PublishOp {
                target_id: 1,
                external_id: external_id.map(str::to_string),
                name: "Work".to_string(),
                children,
            }],
            remove_ids: vec![],
        }
    }

    fn foreign_count(conn: &Connection, place_id: i64) -> i64 {
        conn.query_row(
            "SELECT foreign_count FROM moz_places WHERE id = ?1",
            params![place_id],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn publish_creates_the_folder_under_the_toolbar_with_contiguous_positions() {
        let mut conn = database();
        let plan = publish(
            None,
            vec![
                DesiredNode::Folder {
                    name: "Sub".into(),
                    children: vec![url("Inner", "https://inner.example/")],
                },
                url("A", "https://a.example/"),
                url("B", "https://b.example/"),
            ],
        );

        let applied = apply_to_conn(&mut conn, &plan, 1_700_000_000_000).unwrap();
        let folder_guid = &applied.folder_ids[0].1;

        let (folder_id, parent, position): (i64, i64, i64) = conn
            .query_row(
                "SELECT id, parent, position FROM moz_bookmarks WHERE guid = ?1",
                params![folder_guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(parent, 3);
        assert_eq!(position, 2, "appended after the two existing toolbar rows");

        let positions: Vec<i64> = conn
            .prepare("SELECT position FROM moz_bookmarks WHERE parent = ?1 ORDER BY position")
            .unwrap()
            .query_map(params![folder_id], |row| row.get(0))
            .unwrap()
            .map(|p| p.unwrap())
            .collect();
        assert_eq!(positions, vec![0, 1, 2]);

        // microsecond timestamps, and rows Firefox Sync will treat as new
        let (added, status, counter): (i64, i64, i64) = conn
            .query_row(
                "SELECT dateAdded, syncStatus, syncChangeCounter FROM moz_bookmarks WHERE id = ?1",
                params![folder_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((added, status), (1_700_000_000_000_000, 1));
        assert!(counter >= 1);
    }

    #[test]
    fn new_places_get_the_computed_hash_and_origin() {
        let mut conn = database();
        apply_to_conn(
            &mut conn,
            &publish(None, vec![url("A", "https://a.example/page")]),
            1_700_000_000_000,
        )
        .unwrap();

        let (hash, rev, origin_id, foreign): (i64, String, i64, i64) = conn
            .query_row(
                "SELECT url_hash, rev_host, origin_id, foreign_count FROM moz_places WHERE url = 'https://a.example/page'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(hash, url_hash("https://a.example/page"));
        assert_eq!(rev, "elpmaxe.a.");
        assert_eq!(foreign, 1);
        let host: String = conn
            .query_row(
                "SELECT prefix || host FROM moz_origins WHERE id = ?1",
                params![origin_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(host, "https://a.example");
    }

    #[test]
    fn foreign_count_returns_to_its_original_value_after_unpublish() {
        let mut conn = database();
        assert_eq!(foreign_count(&conn, 100), 2);

        let applied = apply_to_conn(
            &mut conn,
            &publish(None, vec![url("Rust", "https://rust-lang.org")]),
            1_700_000_000_000,
        )
        .unwrap();
        assert_eq!(foreign_count(&conn, 100), 3, "the existing place is reused");

        let remove = Plan {
            publish: vec![],
            remove_ids: vec![applied.folder_ids[0].1.clone()],
        };
        apply_to_conn(&mut conn, &remove, 1_700_000_001_000).unwrap();
        assert_eq!(foreign_count(&conn, 100), 2);

        let leftover: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM moz_bookmarks WHERE title = 'Work'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0);
    }

    #[test]
    fn deleting_writes_a_tombstone_only_for_rows_firefox_sync_has_seen() {
        let mut conn = database();
        let remove = Plan {
            publish: vec![],
            remove_ids: vec!["synced______".into(), "newrow_______".into()],
        };
        apply_to_conn(&mut conn, &remove, 1_700_000_000_000).unwrap();

        let tombstones: Vec<String> = conn
            .prepare("SELECT guid FROM moz_bookmarks_deleted")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|g| g.unwrap())
            .collect();
        assert_eq!(tombstones, vec!["synced______"]);
    }

    #[test]
    fn built_in_folders_cannot_be_removed() {
        let mut conn = database();
        let remove = Plan {
            publish: vec![],
            remove_ids: vec!["toolbar_____".into(), "root________".into()],
        };
        apply_to_conn(&mut conn, &remove, 1_700_000_000_000).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM moz_bookmarks WHERE parent <= 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn republishing_keeps_retained_rows_and_renumbers_positions() {
        let mut conn = database();
        let first = apply_to_conn(
            &mut conn,
            &publish(
                None,
                vec![
                    url("A", "https://a.example/"),
                    url("B", "https://b.example/"),
                ],
            ),
            1_700_000_000_000,
        )
        .unwrap();
        let folder_guid = first.folder_ids[0].1.clone();
        let b_guid: String = conn
            .query_row(
                "SELECT guid FROM moz_bookmarks WHERE title = 'B'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        apply_to_conn(
            &mut conn,
            &publish(Some(&folder_guid), vec![url("B", "https://b.example/")]),
            1_700_000_100_000,
        )
        .unwrap();

        let rows: Vec<(String, i64)> = conn
            .prepare("SELECT guid, position FROM moz_bookmarks WHERE parent = (SELECT id FROM moz_bookmarks WHERE guid = ?1)")
            .unwrap()
            .query_map(params![folder_guid], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows, vec![(b_guid, 0)]);
    }

    #[test]
    fn round_trip_reads_back_exactly_the_published_tree() {
        let mut conn = database();
        let plan = publish(
            None,
            vec![
                DesiredNode::Folder {
                    name: "Sub".into(),
                    children: vec![url("Inner", "https://inner.example/")],
                },
                url("A", "https://a.example/"),
            ],
        );
        apply_to_conn(&mut conn, &plan, 1_700_000_000_000).unwrap();

        let entries = query_entries(&conn).unwrap();
        let mut published: Vec<(String, String, String)> = entries
            .into_iter()
            .filter(|entry| entry.folder_path.starts_with("Bookmarks Toolbar/Work"))
            .map(|entry| (entry.folder_path, entry.title, entry.url))
            .collect();
        published.sort();
        assert_eq!(
            published,
            vec![
                (
                    "Bookmarks Toolbar/Work".to_string(),
                    "A".to_string(),
                    "https://a.example/".to_string()
                ),
                (
                    "Bookmarks Toolbar/Work/Sub".to_string(),
                    "Inner".to_string(),
                    "https://inner.example/".to_string()
                ),
            ]
        );
    }
}
