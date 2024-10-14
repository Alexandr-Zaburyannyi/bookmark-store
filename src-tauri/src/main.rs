use rusqlite::{params, Result as SqliteResult};
mod db;

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn add_bookmark(name: String, link: String, description: String) -> Result<(), String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO bookmarks (name, link, description) VALUES (?1, ?2, ?3);",
        &[&name, &link, &description],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_bookmarks() -> Result<Vec<(u64, String, String, String)>, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT * FROM bookmarks;")
        .map_err(|e| e.to_string())?;

    let bookmarks: Vec<(u64, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<SqliteResult<Vec<(u64, String, String, String)>>>()
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

    let name = name.as_deref();
    let link = link.as_deref();
    let description = description.as_deref();

    conn.execute(
        "UPDATE bookmarks SET name = ?1, link = ?2, description = ?3 WHERE id = ?4;",
        params![name, link, description, bookmark_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn delete_bookmark(bookmark_id: i64) -> Result<i64, String> {
    let conn = db::connect_to_db().map_err(|e| e.to_string())?;

    let rows_affected = conn
        .execute("DELETE FROM bookmarks WHERE id = ?1;", params![bookmark_id])
        .map_err(|e| e.to_string())? as i64;

    Ok(rows_affected)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            greet,
            add_bookmark,
            get_bookmarks,
            update_bookmark,
            delete_bookmark
        ])
        .setup(|_app| {
            let conn = db::connect_to_db().expect("Database connection failed");
            db::initialize_db(&conn).expect("Database initialization failed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
