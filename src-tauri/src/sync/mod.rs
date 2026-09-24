pub mod chromium;
pub mod chromium_write;
pub mod discover;
pub mod firefox;
pub mod firefox_write;
pub mod writable;
pub mod writeback;

use crate::url_key::{now_ms, url_key};
use discover::DetectedSource;
use rusqlite::{params, Connection, OptionalExtension, Result, ToSql};
use serde::Serialize;

pub fn upsert_sources(conn: &Connection, detected: &[DetectedSource]) -> Result<()> {
    let mut upsert_stmt = conn.prepare(
        "INSERT INTO sources (path, family, browser, profile)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET
           family = excluded.family,
           browser = excluded.browser,
           profile = excluded.profile,
           last_error = NULL;",
    )?;

    let mut seen_paths: Vec<String> = Vec::with_capacity(detected.len());
    for source in detected {
        let path = source.path.to_string_lossy().into_owned();
        upsert_stmt.execute(params![
            path,
            source.family.as_str(),
            source.browser,
            source.profile,
        ])?;
        seen_paths.push(path);
    }
    drop(upsert_stmt);

    if seen_paths.is_empty() {
        conn.execute("UPDATE sources SET last_error = 'Not found on disk';", [])?;
    } else {
        let placeholders = vec!["?"; seen_paths.len()].join(",");
        let sql = format!(
            "UPDATE sources SET last_error = 'Not found on disk' WHERE path NOT IN ({});",
            placeholders
        );
        let bound: Vec<&dyn ToSql> = seen_paths.iter().map(|p| p as &dyn ToSql).collect();
        conn.execute(&sql, bound.as_slice())?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub external_id: String,
    pub title: String,
    pub url: String,
    pub folder_path: String,
    pub added_at: Option<i64>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FailedSource {
    pub label: String,
    pub error: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub sources: i64,
    pub added: i64,
    pub marked_removed: i64,
    pub failed: Vec<FailedSource>,
    pub write_back: writeback::WriteBackSummary,
}

struct ActiveSource {
    id: i64,
    path: String,
    family: String,
    browser: String,
    profile: String,
}

fn list_active_sources(conn: &Connection) -> Result<Vec<ActiveSource>> {
    let mut stmt = conn.prepare("SELECT id, path, family, browser, profile FROM sources;")?;
    let sources = stmt
        .query_map([], |row| {
            Ok(ActiveSource {
                id: row.get(0)?,
                path: row.get(1)?,
                family: row.get(2)?,
                browser: row.get(3)?,
                profile: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(sources)
}

pub fn sync_all(conn: &mut Connection) -> SyncSummary {
    let run_at = now_ms();

    let detected = discover::detect();
    if let Err(e) = upsert_sources(conn, &detected) {
        return SyncSummary {
            sources: 0,
            added: 0,
            marked_removed: 0,
            failed: vec![FailedSource {
                label: "sources".to_string(),
                error: e.to_string(),
            }],
            write_back: writeback::WriteBackSummary::default(),
        };
    }

    let sources = match list_active_sources(conn) {
        Ok(sources) => sources,
        Err(e) => {
            return SyncSummary {
                sources: 0,
                added: 0,
                marked_removed: 0,
                failed: vec![FailedSource {
                    label: "sources".to_string(),
                    error: e.to_string(),
                }],
                write_back: writeback::WriteBackSummary::default(),
            }
        }
    };

    let mut added_total = 0;
    let mut removed_total = 0;
    let mut failed = Vec::new();

    for source in &sources {
        let label = format!("{} · {}", source.browser, source.profile);

        let entries = match read_source(source) {
            Ok(entries) => entries,
            Err(err) => {
                record_source_error(conn, source.id, &err);
                failed.push(FailedSource { label, error: err });
                continue;
            }
        };

        match sync_source(conn, source.id, &entries, run_at) {
            Ok((added, removed)) => {
                added_total += added;
                removed_total += removed;
            }
            Err(e) => {
                let error = e.to_string();
                record_source_error(conn, source.id, &error);
                failed.push(FailedSource { label, error });
            }
        }
    }

    let write_back = writeback::apply_all(conn);

    // Re-read every browser we just wrote to, so bookmark_sources picks up the new node guids.
    for applied in &write_back.applied {
        let Some(source) = sources.iter().find(|source| source.id == applied.source_id) else {
            continue;
        };
        let result = read_source(source).and_then(|entries| {
            sync_source(conn, source.id, &entries, now_ms()).map_err(|e| e.to_string())
        });
        if let Err(error) = result {
            record_source_error(conn, source.id, &error);
            failed.push(FailedSource {
                label: applied.label.clone(),
                error,
            });
        }
    }

    SyncSummary {
        sources: sources.len() as i64,
        added: added_total,
        marked_removed: removed_total,
        failed,
        write_back,
    }
}

fn read_source(source: &ActiveSource) -> Result<Vec<Entry>, String> {
    match source.family.as_str() {
        "chromium" => chromium::read(std::path::Path::new(&source.path)),
        "firefox" => firefox::read(std::path::Path::new(&source.path), source.id),
        other => Err(format!("unknown source family: {other}")),
    }
}

fn record_source_error(conn: &Connection, source_id: i64, error: &str) {
    let _ = conn.execute(
        "UPDATE sources SET last_error = ?1 WHERE id = ?2;",
        params![error, source_id],
    );
}

fn sync_source(
    conn: &mut Connection,
    source_id: i64,
    entries: &[Entry],
    run_at: i64,
) -> Result<(i64, i64)> {
    let tx = conn.transaction()?;
    let mut added = 0;

    {
        let mut find_bookmark_stmt =
            tx.prepare("SELECT id FROM bookmarks WHERE url_key = ?1 ORDER BY id LIMIT 1;")?;
        let mut insert_bookmark_stmt = tx.prepare(
            "INSERT INTO bookmarks (name, link, description, url_key, created_at, updated_at)
             VALUES (?1, ?2, '', ?3, ?4, ?5);",
        )?;
        let mut lower_created_at_stmt =
            tx.prepare("UPDATE bookmarks SET created_at = ?1 WHERE id = ?2 AND created_at > ?1;")?;
        let mut upsert_source_row_stmt = tx.prepare(
            "INSERT INTO bookmark_sources
               (bookmark_id, source_id, external_id, title, folder_path, added_at, present, last_seen_at, removed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, NULL)
             ON CONFLICT(source_id, external_id) DO UPDATE SET
               bookmark_id = excluded.bookmark_id,
               title = excluded.title,
               folder_path = excluded.folder_path,
               added_at = excluded.added_at,
               present = 1,
               last_seen_at = excluded.last_seen_at,
               removed_at = NULL;",
        )?;

        for entry in entries {
            let key = url_key(&entry.url);

            let existing_id: Option<i64> = find_bookmark_stmt
                .query_row(params![key], |row| row.get(0))
                .optional()?;

            let bookmark_id = match existing_id {
                Some(id) => {
                    if let Some(added_at) = entry.added_at {
                        lower_created_at_stmt.execute(params![added_at, id])?;
                    }
                    id
                }
                None => {
                    let created_at = entry.added_at.unwrap_or(run_at);
                    insert_bookmark_stmt.execute(params![
                        entry.title,
                        entry.url,
                        key,
                        created_at,
                        run_at
                    ])?;
                    added += 1;
                    tx.last_insert_rowid()
                }
            };

            upsert_source_row_stmt.execute(params![
                bookmark_id,
                source_id,
                entry.external_id,
                entry.title,
                entry.folder_path,
                entry.added_at,
                run_at,
            ])?;
        }
    }

    let removed = tx.execute(
        "UPDATE bookmark_sources SET present = 0, removed_at = ?1
         WHERE source_id = ?2 AND present = 1 AND last_seen_at < ?1;",
        params![run_at, source_id],
    )? as i64;

    tx.execute(
        "UPDATE sources SET last_synced_at = ?1, last_error = NULL WHERE id = ?2;",
        params![run_at, source_id],
    )?;

    tx.commit()?;

    Ok((added, removed))
}
