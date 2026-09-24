use rusqlite::params;
use serde::Serialize;
use std::collections::HashMap;
mod db;
mod sync;
mod url_key;

use sync::writeback::{is_out_of_date, subtree_updated_at};

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BookmarkSourceDto {
    source_id: i64,
    browser: String,
    profile: String,
    folder_path: String,
    present: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BookmarkDto {
    id: i64,
    name: String,
    link: String,
    description: String,
    created_at: i64,
    folder_id: Option<i64>,
    sources: Vec<BookmarkSourceDto>,
}

#[tauri::command]
fn add_bookmark(name: String, link: String, description: String) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();
    let key = url_key::url_key(&link);

    conn.execute(
        "INSERT INTO bookmarks (name, link, description, url_key, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5);",
        params![name, link, description, key, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_bookmarks() -> Result<Vec<BookmarkDto>, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    let mut sources_by_bookmark: HashMap<i64, Vec<BookmarkSourceDto>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT bs.bookmark_id, bs.source_id, s.browser, s.profile, bs.folder_path, bs.present
                 FROM bookmark_sources bs
                 JOIN sources s ON s.id = bs.source_id;",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let bookmark_id: i64 = row.get(0)?;
                Ok((
                    bookmark_id,
                    BookmarkSourceDto {
                        source_id: row.get(1)?,
                        browser: row.get(2)?,
                        profile: row.get(3)?,
                        folder_path: row.get(4)?,
                        present: row.get::<_, i64>(5)? != 0,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            let (bookmark_id, source) = row.map_err(|e| e.to_string())?;
            sources_by_bookmark
                .entry(bookmark_id)
                .or_default()
                .push(source);
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, name, link, description, created_at, folder_id
             FROM bookmarks
             WHERE deleted_at IS NULL
             ORDER BY created_at DESC, id DESC;",
        )
        .map_err(|e| e.to_string())?;

    let bookmarks = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            Ok(BookmarkDto {
                id,
                name: row.get(1)?,
                link: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                folder_id: row.get(5)?,
                sources: sources_by_bookmark.remove(&id).unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<BookmarkDto>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(bookmarks)
}

#[tauri::command]
fn update_bookmark(
    name: Option<String>,
    link: Option<String>,
    description: Option<String>,
    bookmark_id: i64,
) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();
    let key = link.as_deref().map(url_key::url_key);

    conn.execute(
        "UPDATE bookmarks
         SET name = COALESCE(?1, name),
             link = COALESCE(?2, link),
             description = COALESCE(?3, description),
             url_key = COALESCE(?4, url_key),
             updated_at = ?5
         WHERE id = ?6;",
        params![name, link, description, key, now, bookmark_id],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE folders SET updated_at = ?1
         WHERE id = (SELECT folder_id FROM bookmarks WHERE id = ?2);",
        params![now, bookmark_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn delete_bookmark(bookmark_id: i64) -> Result<i64, String> {
    let mut conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let rows_affected = tx
        .execute(
            "UPDATE bookmarks SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL;",
            params![now, bookmark_id],
        )
        .map_err(|e| e.to_string())? as i64;

    if rows_affected > 0 {
        // Browsers still holding this bookmark get it removed on the next write-back.
        tx.execute(
            "UPDATE bookmark_sources SET pending_delete = 1
             WHERE bookmark_id = ?1 AND present = 1;",
            params![bookmark_id],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "UPDATE folders SET updated_at = ?1
             WHERE id = (SELECT folder_id FROM bookmarks WHERE id = ?2);",
            params![now, bookmark_id],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(rows_affected)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderTargetDto {
    source_id: i64,
    state: String,
    last_pushed_at: Option<i64>,
    last_error: Option<String>,
    out_of_date: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderDto {
    id: i64,
    name: String,
    parent_id: Option<i64>,
    position: i64,
    bookmark_count: i64,
    targets: Vec<FolderTargetDto>,
}

fn load_folders(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<FolderDto>> {
    let subtree = subtree_updated_at(conn)?;

    let mut targets_by_folder: HashMap<i64, Vec<FolderTargetDto>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT folder_id, source_id, state, last_pushed_at, last_error
             FROM folder_targets WHERE folder_id IS NOT NULL ORDER BY source_id;",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (folder_id, source_id, state, last_pushed_at, last_error) = row?;
            let out_of_date = is_out_of_date(
                last_pushed_at,
                subtree.get(&folder_id).copied().unwrap_or(0),
            );
            targets_by_folder
                .entry(folder_id)
                .or_default()
                .push(FolderTargetDto {
                    source_id,
                    state,
                    last_pushed_at,
                    last_error,
                    out_of_date,
                });
        }
    }

    let mut stmt = conn.prepare(
        "SELECT f.id, f.name, f.parent_id, f.position,
                (SELECT COUNT(*) FROM bookmarks b
                 WHERE b.folder_id = f.id AND b.deleted_at IS NULL)
         FROM folders f
         ORDER BY f.position, f.id;",
    )?;

    let folders = stmt
        .query_map([], |row| {
            Ok(FolderDto {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                position: row.get(3)?,
                bookmark_count: row.get(4)?,
                targets: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<FolderDto>>>()?;

    Ok(folders
        .into_iter()
        .map(|mut folder| {
            folder.targets = targets_by_folder.remove(&folder.id).unwrap_or_default();
            folder
        })
        .collect())
}

fn touch_folder(
    conn: &rusqlite::Connection,
    folder_id: Option<i64>,
    now: i64,
) -> rusqlite::Result<()> {
    if let Some(id) = folder_id {
        conn.execute(
            "UPDATE folders SET updated_at = ?1 WHERE id = ?2;",
            params![now, id],
        )?;
    }
    Ok(())
}

#[tauri::command]
fn list_folders() -> Result<Vec<FolderDto>, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    load_folders(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_folder(name: String, parent_id: Option<i64>) -> Result<FolderDto, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();

    let position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM folders WHERE parent_id IS ?1;",
            params![parent_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO folders (name, parent_id, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4);",
        params![name, parent_id, position, now],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();

    touch_folder(&conn, parent_id, now).map_err(|e| e.to_string())?;
    load_folders(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|folder| folder.id == id)
        .ok_or_else(|| "folder disappeared after insert".to_string())
}

#[tauri::command]
fn set_folder_targets(folder_id: i64, source_ids: Vec<i64>) -> Result<(), String> {
    let mut conn = db::connect_to_db().map_err(|e| e.to_string())?;

    let parent_id: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM folders WHERE id = ?1;",
            params![folder_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if parent_id.is_some() {
        return Err("Only top-level folders can be shown in browsers".to_string());
    }

    for source_id in &source_ids {
        let (family, path): (String, String) = conn
            .query_row(
                "SELECT family, path FROM sources WHERE id = ?1;",
                params![source_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        if sync::writable::check(&family, std::path::Path::new(&path))
            == sync::writable::Writability::Encrypted
        {
            return Err("That browser profile uses encrypted bookmark storage".to_string());
        }
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    for source_id in &source_ids {
        tx.execute(
            "INSERT INTO folder_targets (folder_id, source_id, state)
             VALUES (?1, ?2, 'active')
             ON CONFLICT(folder_id, source_id) DO UPDATE SET state = 'active';",
            params![folder_id, source_id],
        )
        .map_err(|e| e.to_string())?;
    }

    let existing: Vec<(i64, i64, Option<String>)> = {
        let mut stmt = tx
            .prepare("SELECT id, source_id, external_id FROM folder_targets WHERE folder_id = ?1;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![folder_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    for (target_id, source_id, external_id) in existing {
        if source_ids.contains(&source_id) {
            continue;
        }
        if external_id.is_none() {
            // Never pushed, so there is nothing to remove from the browser.
            tx.execute(
                "DELETE FROM folder_targets WHERE id = ?1;",
                params![target_id],
            )
        } else {
            tx.execute(
                "UPDATE folder_targets SET state = 'remove' WHERE id = ?1;",
                params![target_id],
            )
        }
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_folder(folder_id: i64, name: String) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE folders SET name = ?1, updated_at = ?2 WHERE id = ?3;",
        params![name, url_key::now_ms(), folder_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn delete_folder(folder_id: i64) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();

    let parent_id: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM folders WHERE id = ?1;",
            params![folder_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // A target that was never pushed has nothing to remove from the browser.
    conn.execute(
        "DELETE FROM folder_targets WHERE folder_id = ?1 AND external_id IS NULL;",
        params![folder_id],
    )
    .map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM folders WHERE id = ?1;", params![folder_id])
        .map_err(|e| e.to_string())?;

    touch_folder(&conn, parent_id, now).map_err(|e| e.to_string())
}

#[tauri::command]
fn move_bookmark(bookmark_id: i64, folder_id: Option<i64>) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;
    let now = url_key::now_ms();

    let old_folder_id: Option<i64> = conn
        .query_row(
            "SELECT folder_id FROM bookmarks WHERE id = ?1;",
            params![bookmark_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE bookmarks SET folder_id = ?1, updated_at = ?2 WHERE id = ?3;",
        params![folder_id, now, bookmark_id],
    )
    .map_err(|e| e.to_string())?;

    touch_folder(&conn, old_folder_id, now).map_err(|e| e.to_string())?;
    touch_folder(&conn, folder_id, now).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    id: i64,
    browser: String,
    profile: String,
    family: String,
    path: String,
    last_synced_at: Option<i64>,
    last_error: Option<String>,
    present_count: i64,
    removed_count: i64,
    writability: String,
    pending_count: i64,
}

/// Changes waiting to be written to this source's browser: bookmark deletes plus
/// folder targets that are being removed, orphaned, or out of date.
fn pending_count(
    conn: &rusqlite::Connection,
    source_id: i64,
    subtree: &HashMap<i64, i64>,
) -> rusqlite::Result<i64> {
    let mut count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM bookmark_sources
         WHERE source_id = ?1 AND pending_delete = 1 AND present = 1;",
        params![source_id],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT folder_id, state, last_pushed_at FROM folder_targets WHERE source_id = ?1;",
    )?;
    let rows = stmt.query_map(params![source_id], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    for row in rows {
        let (folder_id, state, last_pushed_at) = row?;
        let pending = match folder_id {
            None => true,
            Some(_) if state == "remove" => true,
            Some(id) => is_out_of_date(last_pushed_at, subtree.get(&id).copied().unwrap_or(0)),
        };
        if pending {
            count += 1;
        }
    }
    Ok(count)
}

#[tauri::command]
fn list_sources() -> Result<Vec<SourceDto>, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    let detected = sync::discover::detect();
    sync::upsert_sources(&conn, &detected).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT
               s.id, s.browser, s.profile, s.family, s.path, s.last_synced_at, s.last_error,
               COALESCE(SUM(CASE WHEN bs.present = 1 THEN 1 ELSE 0 END), 0) AS present_count,
               COALESCE(SUM(CASE WHEN bs.present = 0 THEN 1 ELSE 0 END), 0) AS removed_count
             FROM sources s
             LEFT JOIN bookmark_sources bs ON bs.source_id = s.id
             GROUP BY s.id
             ORDER BY s.browser, s.profile;",
        )
        .map_err(|e| e.to_string())?;

    let sources = stmt
        .query_map([], |row| {
            Ok(SourceDto {
                id: row.get(0)?,
                browser: row.get(1)?,
                profile: row.get(2)?,
                family: row.get(3)?,
                path: row.get(4)?,
                last_synced_at: row.get(5)?,
                last_error: row.get(6)?,
                present_count: row.get(7)?,
                removed_count: row.get(8)?,
                writability: String::new(),
                pending_count: 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<SourceDto>, _>>()
        .map_err(|e| e.to_string())?;

    let subtree = subtree_updated_at(&conn).map_err(|e| e.to_string())?;

    let mut sources = sources;
    for source in &mut sources {
        source.writability =
            sync::writable::check(&source.family, std::path::Path::new(&source.path))
                .as_str()
                .to_string();
        source.pending_count =
            pending_count(&conn, source.id, &subtree).map_err(|e| e.to_string())?;
    }

    Ok(sources)
}

#[tauri::command]
async fn sync_bookmarks() -> Result<sync::SyncSummary, String> {
    let mut conn = db::connect_to_db().map_err(|e| e.to_string())?;
    Ok(sync::sync_all(&mut conn))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            greet,
            add_bookmark,
            get_bookmarks,
            update_bookmark,
            delete_bookmark,
            list_folders,
            create_folder,
            rename_folder,
            delete_folder,
            move_bookmark,
            set_folder_targets,
            list_sources,
            sync_bookmarks
        ])
        .setup(|app| {
            let app_data_dir = app
                .path_resolver()
                .app_data_dir()
                .expect("could not resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            db::set_db_path(app_data_dir.join("database.db"));
            sync::writeback::set_backup_dir(app_data_dir.join("browser-backups"));

            let mut conn = db::connect_to_db().expect("Database connection failed");
            db::initialize_db(&conn).expect("Database initialization failed");
            db::migrate(&mut conn).expect("Database migration failed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
