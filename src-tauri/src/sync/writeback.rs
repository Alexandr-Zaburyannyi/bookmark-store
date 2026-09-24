use super::{chromium_write, firefox_write, writable, FailedSource};
use crate::url_key::now_ms;
use rusqlite::{params, Connection, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static BACKUP_DIR: OnceLock<PathBuf> = OnceLock::new();
const BACKUPS_KEPT: usize = 10;

pub fn set_backup_dir(path: PathBuf) {
    BACKUP_DIR
        .set(path)
        .expect("set_backup_dir called more than once");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesiredNode {
    Folder {
        name: String,
        children: Vec<DesiredNode>,
    },
    Url {
        title: String,
        url: String,
    },
}

#[derive(Debug, Clone)]
pub struct PublishOp {
    pub target_id: i64,
    pub external_id: Option<String>,
    pub name: String,
    pub children: Vec<DesiredNode>,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub publish: Vec<PublishOp>,
    pub remove_ids: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.remove_ids.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct Applied {
    /// (folder_targets.id, the folder node's guid in the browser)
    pub folder_ids: Vec<(i64, String)>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSource {
    #[serde(skip)]
    pub source_id: i64,
    pub label: String,
    pub browser: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSource {
    pub label: String,
    pub browser: String,
    pub reason: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct WriteBackSummary {
    pub applied: Vec<AppliedSource>,
    pub skipped: Vec<SkippedSource>,
    pub failed: Vec<FailedSource>,
}

/// For every folder, the newest `updated_at` among itself and its descendants.
pub fn subtree_updated_at(conn: &Connection) -> Result<HashMap<i64, i64>> {
    let mut stmt = conn.prepare("SELECT id, parent_id, updated_at FROM folders;")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>>>()?;

    let parents: HashMap<i64, Option<i64>> = rows.iter().map(|(id, p, _)| (*id, *p)).collect();
    let mut newest: HashMap<i64, i64> = HashMap::new();
    for (id, _, updated_at) in &rows {
        let mut current = Some(*id);
        while let Some(folder_id) = current {
            let entry = newest.entry(folder_id).or_insert(*updated_at);
            *entry = (*entry).max(*updated_at);
            current = parents.get(&folder_id).copied().flatten();
        }
    }
    Ok(newest)
}

pub fn is_out_of_date(last_pushed_at: Option<i64>, subtree_updated_at: i64) -> bool {
    last_pushed_at.is_none_or(|pushed| pushed < subtree_updated_at)
}

fn folder_children(conn: &Connection, folder_id: i64) -> Result<Vec<DesiredNode>> {
    let mut nodes = Vec::new();

    let mut stmt =
        conn.prepare("SELECT id, name FROM folders WHERE parent_id = ?1 ORDER BY position, name;")?;
    let subfolders = stmt
        .query_map(params![folder_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>>>()?;
    for (id, name) in subfolders {
        nodes.push(DesiredNode::Folder {
            name,
            children: folder_children(conn, id)?,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT name, link FROM bookmarks
         WHERE folder_id = ?1 AND deleted_at IS NULL ORDER BY name, id;",
    )?;
    let urls = stmt
        .query_map(params![folder_id], |row| {
            Ok(DesiredNode::Url {
                title: row.get(0)?,
                url: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    nodes.extend(urls);

    Ok(nodes)
}

pub fn build_plan(conn: &Connection, source_id: i64) -> Result<Plan> {
    let subtree = subtree_updated_at(conn)?;
    let mut plan = Plan::default();

    let mut stmt = conn.prepare(
        "SELECT t.id, t.folder_id, t.state, t.external_id, t.last_pushed_at, f.name
         FROM folder_targets t
         LEFT JOIN folders f ON f.id = t.folder_id
         WHERE t.source_id = ?1
         ORDER BY t.id;",
    )?;
    let targets = stmt
        .query_map(params![source_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>>>()?;

    for (target_id, folder_id, state, external_id, last_pushed_at, name) in targets {
        match (folder_id, name) {
            (Some(folder_id), Some(name)) if state == "active" => {
                let newest = subtree.get(&folder_id).copied().unwrap_or(0);
                if is_out_of_date(last_pushed_at, newest) {
                    plan.publish.push(PublishOp {
                        target_id,
                        external_id,
                        name,
                        children: folder_children(conn, folder_id)?,
                    });
                }
            }
            _ => {
                if let Some(external_id) = external_id {
                    plan.remove_ids.push(external_id);
                }
            }
        }
    }

    let mut stmt = conn.prepare(
        "SELECT external_id FROM bookmark_sources
         WHERE source_id = ?1 AND pending_delete = 1 AND present = 1;",
    )?;
    let pending = stmt
        .query_map(params![source_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    plan.remove_ids.extend(pending);

    Ok(plan)
}

fn backup(source_id: i64, path: &Path) -> std::result::Result<(), String> {
    let root = BACKUP_DIR
        .get()
        .ok_or_else(|| "backup directory is not configured".to_string())?;
    let dir = root.join(source_id.to_string());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let file_name = path
        .file_name()
        .ok_or_else(|| "bookmarks path has no file name".to_string())?
        .to_string_lossy();
    let stamp = now_ms();
    fs::copy(path, dir.join(format!("{stamp}-{file_name}"))).map_err(|e| e.to_string())?;

    // Firefox keeps recent writes in a -wal file next to the database; without it the copy is stale.
    let wal = path.with_extension("sqlite-wal");
    if wal.is_file() {
        let wal_name = wal
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        fs::copy(&wal, dir.join(format!("{stamp}-{wal_name}"))).map_err(|e| e.to_string())?;
    }

    let files: Vec<(i64, PathBuf)> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|entry| {
            let stamp = entry
                .file_name()
                .to_string_lossy()
                .split('-')
                .next()?
                .parse::<i64>()
                .ok()?;
            Some((stamp, entry.path()))
        })
        .collect();
    let mut stamps: Vec<i64> = files.iter().map(|(stamp, _)| *stamp).collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    stamps.dedup();
    if let Some(&oldest_kept) = stamps.get(BACKUPS_KEPT - 1) {
        for (stamp, stale) in files {
            if stamp < oldest_kept {
                let _ = fs::remove_file(stale);
            }
        }
    }
    Ok(())
}

fn record_applied(
    conn: &mut Connection,
    source_id: i64,
    plan: &Plan,
    applied: &Applied,
    pushed_at: i64,
) -> Result<()> {
    let now = now_ms();
    let tx = conn.transaction()?;

    for (target_id, guid) in &applied.folder_ids {
        tx.execute(
            "UPDATE folder_targets
             SET external_id = ?1, last_pushed_at = ?2, last_error = NULL
             WHERE id = ?3;",
            params![guid, pushed_at, target_id],
        )?;
    }

    tx.execute(
        "DELETE FROM folder_targets
         WHERE source_id = ?1 AND (folder_id IS NULL OR state = 'remove');",
        params![source_id],
    )?;

    for external_id in &plan.remove_ids {
        tx.execute(
            "UPDATE bookmark_sources
             SET present = 0, removed_at = ?1, pending_delete = 0
             WHERE source_id = ?2 AND external_id = ?3 AND pending_delete = 1;",
            params![now, source_id, external_id],
        )?;
    }

    tx.commit()
}

fn record_error(conn: &Connection, source_id: i64, error: &str) {
    let _ = conn.execute(
        "UPDATE folder_targets SET last_error = ?1 WHERE source_id = ?2;",
        params![error, source_id],
    );
}

pub fn apply_all(conn: &mut Connection) -> WriteBackSummary {
    let mut summary = WriteBackSummary::default();

    let sources: Vec<(i64, String, String, String, String)> = match (|| {
        let mut stmt =
            conn.prepare("SELECT id, path, family, browser, profile FROM sources ORDER BY id;")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<Vec<_>>>();
        rows
    })() {
        Ok(sources) => sources,
        Err(e) => {
            summary.failed.push(FailedSource {
                label: "sources".to_string(),
                error: e.to_string(),
            });
            return summary;
        }
    };

    for (source_id, path, family, browser, profile) in sources {
        let label = format!("{browser} · {profile}");
        // Changes made after this point stay "out of date" and go out on the next sync.
        let started = now_ms();

        let plan = match build_plan(conn, source_id) {
            Ok(plan) => plan,
            Err(e) => {
                summary.failed.push(FailedSource {
                    label,
                    error: e.to_string(),
                });
                continue;
            }
        };
        if plan.is_empty() {
            continue;
        }

        let mut skip = |reason: &str| {
            summary.skipped.push(SkippedSource {
                label: label.clone(),
                browser: browser.clone(),
                reason: reason.to_string(),
            });
        };

        let writer: fn(&Path, &Plan) -> std::result::Result<Applied, String> = match family.as_str()
        {
            "chromium" => chromium_write::apply,
            "firefox" => firefox_write::apply,
            _ => {
                skip("unsupported");
                continue;
            }
        };
        let writability = writable::check(&family, Path::new(&path));
        if writability != writable::Writability::Writable {
            skip(writability.as_str());
            continue;
        }

        let result =
            backup(source_id, Path::new(&path)).and_then(|_| writer(Path::new(&path), &plan));

        match result {
            Ok(applied) => match record_applied(conn, source_id, &plan, &applied, started) {
                Ok(()) => summary.applied.push(AppliedSource {
                    source_id,
                    label,
                    browser,
                }),
                Err(e) => summary.failed.push(FailedSource {
                    label,
                    error: e.to_string(),
                }),
            },
            Err(error) => {
                record_error(conn, source_id, &error);
                summary.failed.push(FailedSource { label, error });
            }
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn database() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        db::initialize_db(&conn).unwrap();
        db::migrate(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sources (id, path, family, browser, profile) VALUES (1, '/p', 'chromium', 'Brave', 'Default')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn builds_publish_ops_for_out_of_date_active_targets() {
        let conn = database();
        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 100)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO folders (id, name, parent_id, position, created_at, updated_at) VALUES (2, 'Rust', 1, 0, 0, 200)", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description, folder_id) VALUES (1, 'B', 'https://b.example', '', 1)", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description, folder_id) VALUES (2, 'A', 'https://a.example', '', 1)", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description, folder_id, deleted_at) VALUES (3, 'Gone', 'https://gone.example', '', 1, 5)", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description, folder_id) VALUES (4, 'In Rust', 'https://rust-lang.org', '', 2)", []).unwrap();
        conn.execute("INSERT INTO folder_targets (id, folder_id, source_id, last_pushed_at) VALUES (1, 1, 1, 150)", []).unwrap();

        let plan = build_plan(&conn, 1).unwrap();

        assert_eq!(plan.publish.len(), 1);
        let op = &plan.publish[0];
        assert_eq!(op.name, "Work");
        assert_eq!(
            op.children,
            vec![
                DesiredNode::Folder {
                    name: "Rust".into(),
                    children: vec![DesiredNode::Url {
                        title: "In Rust".into(),
                        url: "https://rust-lang.org".into()
                    }],
                },
                DesiredNode::Url {
                    title: "A".into(),
                    url: "https://a.example".into()
                },
                DesiredNode::Url {
                    title: "B".into(),
                    url: "https://b.example".into()
                },
            ]
        );
    }

    #[test]
    fn up_to_date_targets_produce_no_ops() {
        let conn = database();
        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folder_targets (folder_id, source_id, last_pushed_at) VALUES (1, 1, 100)",
            [],
        )
        .unwrap();
        assert!(build_plan(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn removals_come_from_remove_targets_orphans_and_pending_deletes() {
        let conn = database();
        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO folder_targets (folder_id, source_id, state, external_id) VALUES (1, 1, 'remove', 'g-remove')", []).unwrap();
        conn.execute("INSERT INTO folder_targets (folder_id, source_id, state, external_id) VALUES (NULL, 1, 'active', 'g-orphan')", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description) VALUES (1, 'x', 'https://x.example', '')", []).unwrap();
        conn.execute("INSERT INTO bookmark_sources (bookmark_id, source_id, external_id, title, folder_path, present, last_seen_at, pending_delete) VALUES (1, 1, 'g-bookmark', 'x', '', 1, 0, 1)", []).unwrap();
        conn.execute("INSERT INTO bookmark_sources (bookmark_id, source_id, external_id, title, folder_path, present, last_seen_at, pending_delete) VALUES (1, 1, 'g-already-gone', 'x', '', 0, 0, 1)", []).unwrap();

        let mut ids = build_plan(&conn, 1).unwrap().remove_ids;
        ids.sort();
        assert_eq!(ids, vec!["g-bookmark", "g-orphan", "g-remove"]);
    }

    #[test]
    fn record_applied_updates_targets_and_clears_pending_deletes() {
        let mut conn = database();
        conn.execute(
            "INSERT INTO folders (id, name, created_at, updated_at) VALUES (1, 'Work', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folder_targets (id, folder_id, source_id) VALUES (1, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO folder_targets (id, folder_id, source_id, state, external_id) VALUES (2, NULL, 1, 'active', 'g-orphan')", []).unwrap();
        conn.execute("INSERT INTO bookmarks (id, name, link, description) VALUES (1, 'x', 'https://x.example', '')", []).unwrap();
        conn.execute("INSERT INTO bookmark_sources (bookmark_id, source_id, external_id, title, folder_path, present, last_seen_at, pending_delete) VALUES (1, 1, 'g-bookmark', 'x', '', 1, 0, 1)", []).unwrap();

        let plan = Plan {
            publish: vec![],
            remove_ids: vec!["g-bookmark".into(), "g-orphan".into()],
        };
        let applied = Applied {
            folder_ids: vec![(1, "new-guid".into())],
        };
        record_applied(&mut conn, 1, &plan, &applied, 500).unwrap();

        let (external_id, pushed): (String, i64) = conn
            .query_row(
                "SELECT external_id, last_pushed_at FROM folder_targets WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((external_id.as_str(), pushed), ("new-guid", 500));
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM folder_targets WHERE id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
        let (present, pending): (i64, i64) = conn
            .query_row("SELECT present, pending_delete FROM bookmark_sources WHERE external_id = 'g-bookmark'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((present, pending), (0, 0));
    }
}
