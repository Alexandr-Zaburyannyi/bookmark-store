use rusqlite::{Connection, Result};

pub fn connect_to_db() -> Result<Connection> {
    Connection::open("database.db")
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
