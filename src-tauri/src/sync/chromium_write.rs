use super::writeback::{Applied, DesiredNode, Plan};
use crate::url_key::{now_ms, url_key};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;

const ROOT_KEYS: [&str; 3] = ["bookmark_bar", "other", "synced"];
const WEBKIT_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;

fn str_field<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get(key).and_then(|v| v.as_str())
}

fn is_type(node: &Value, node_type: &str) -> bool {
    str_field(node, "type") == Some(node_type)
}

fn max_id(node: &Value) -> u64 {
    let own = str_field(node, "id")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    node.get("children")
        .and_then(|c| c.as_array())
        .map_or(own, |kids| kids.iter().map(max_id).fold(own, u64::max))
}

struct Ctx {
    next_id: u64,
    stamp: String,
}

impl Ctx {
    fn take_id(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        id.to_string()
    }

    fn new_url(&mut self, title: &str, url: &str) -> Value {
        json!({
            "date_added": self.stamp,
            "date_last_used": "0",
            "guid": uuid::Uuid::new_v4().to_string(),
            "id": self.take_id(),
            "name": title,
            "type": "url",
            "url": url,
        })
    }

    fn new_folder(&mut self, name: &str) -> Value {
        json!({
            "children": [],
            "date_added": self.stamp,
            "date_last_used": "0",
            "date_modified": self.stamp,
            "guid": uuid::Uuid::new_v4().to_string(),
            "id": self.take_id(),
            "name": name,
            "type": "folder",
        })
    }
}

fn remove_guid(node: &mut Value, guid: &str) -> bool {
    let Some(children) = node.get_mut("children").and_then(|c| c.as_array_mut()) else {
        return false;
    };
    let before = children.len();
    children.retain(|child| str_field(child, "guid") != Some(guid));
    let mut removed = children.len() != before;
    for child in children.iter_mut() {
        removed |= remove_guid(child, guid);
    }
    removed
}

/// Index path from `node` down to the folder with this guid.
fn find_folder_path(node: &Value, guid: &str) -> Option<Vec<usize>> {
    let children = node.get("children")?.as_array()?;
    for (index, child) in children.iter().enumerate() {
        if is_type(child, "folder") && str_field(child, "guid") == Some(guid) {
            return Some(vec![index]);
        }
        if let Some(mut rest) = find_folder_path(child, guid) {
            rest.insert(0, index);
            return Some(rest);
        }
    }
    None
}

fn node_at_mut<'a>(root: &'a mut Value, path: &[usize]) -> &'a mut Value {
    let mut current = root;
    for &index in path {
        current = &mut current["children"][index];
    }
    current
}

fn reconcile(children: &mut Vec<Value>, desired: &[DesiredNode], ctx: &mut Ctx) {
    let mut pool = std::mem::take(children);

    for wanted in desired {
        match wanted {
            DesiredNode::Url { title, url } => {
                let key = url_key(url);
                let position = pool.iter().position(|node| {
                    is_type(node, "url")
                        && str_field(node, "url").map(url_key).as_deref() == Some(key.as_str())
                });
                let mut node = match position {
                    Some(index) => pool.remove(index),
                    None => ctx.new_url(title, url),
                };
                node["name"] = json!(title);
                children.push(node);
            }
            DesiredNode::Folder {
                name,
                children: kids,
            } => {
                let position = pool.iter().position(|node| {
                    is_type(node, "folder") && str_field(node, "name") == Some(name)
                });
                let mut node = match position {
                    Some(index) => pool.remove(index),
                    None => ctx.new_folder(name),
                };
                fill_folder(&mut node, name, kids, ctx);
                children.push(node);
            }
        }
    }
    // anything left in the pool is no longer wanted in this folder
}

fn fill_folder(node: &mut Value, name: &str, desired: &[DesiredNode], ctx: &mut Ctx) {
    node["name"] = json!(name);
    let mut children = match node["children"].take() {
        Value::Array(children) => children,
        _ => Vec::new(),
    };
    reconcile(&mut children, desired, ctx);
    node["children"] = Value::Array(children);
}

fn feed_checksum(node: &Value, hasher: &mut Md5) {
    hasher.update(str_field(node, "id").unwrap_or("").as_bytes());
    let name: Vec<u8> = str_field(node, "name")
        .unwrap_or("")
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    hasher.update(&name);

    if is_type(node, "url") {
        hasher.update(b"url");
        hasher.update(str_field(node, "url").unwrap_or("").as_bytes());
    } else {
        hasher.update(b"folder");
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                feed_checksum(child, hasher);
            }
        }
    }
}

pub fn compute_checksum(json: &Value) -> Option<String> {
    let roots = json.get("roots")?;
    let mut hasher = Md5::new();
    for key in ROOT_KEYS {
        feed_checksum(roots.get(key)?, &mut hasher);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

pub fn apply_to_value(json: &mut Value, plan: &Plan, now: i64) -> Result<Applied, String> {
    if json.get("roots").is_none() {
        return Err("missing \"roots\" key".to_string());
    }

    // Only trust our checksum algorithm if it reproduces the file's own checksum.
    let checksum_verified =
        str_field(json, "checksum").map(|stored| compute_checksum(json).as_deref() == Some(stored));

    let next_id = ROOT_KEYS
        .iter()
        .filter_map(|key| json["roots"].get(*key))
        .map(max_id)
        .max()
        .unwrap_or(0)
        + 1;
    let mut ctx = Ctx {
        next_id,
        stamp: ((now + WEBKIT_EPOCH_OFFSET_MS) * 1000).to_string(),
    };

    let roots = &mut json["roots"];

    for guid in &plan.remove_ids {
        for key in ROOT_KEYS {
            if let Some(root) = roots.get_mut(key) {
                remove_guid(root, guid);
            }
        }
    }

    let mut applied = Applied::default();

    for op in &plan.publish {
        let mut found = None;
        if let Some(guid) = &op.external_id {
            for key in ROOT_KEYS {
                if let Some(path) = roots.get(key).and_then(|root| find_folder_path(root, guid)) {
                    found = Some((key, path));
                    break;
                }
            }
        }

        let guid = match found {
            Some((key, path)) => {
                let node = node_at_mut(&mut roots[key], &path);
                fill_folder(node, &op.name, &op.children, &mut ctx);
                node["date_modified"] = json!(ctx.stamp);
                str_field(node, "guid").unwrap_or_default().to_string()
            }
            None => {
                let mut node = ctx.new_folder(&op.name);
                fill_folder(&mut node, &op.name, &op.children, &mut ctx);
                let guid = str_field(&node, "guid").unwrap_or_default().to_string();
                roots
                    .get_mut("bookmark_bar")
                    .and_then(|bar| bar.get_mut("children"))
                    .and_then(|children| children.as_array_mut())
                    .ok_or_else(|| "missing bookmark bar".to_string())?
                    .push(node);
                guid
            }
        };
        applied.folder_ids.push((op.target_id, guid));
    }

    match checksum_verified {
        Some(true) => {
            if let Some(checksum) = compute_checksum(json) {
                json["checksum"] = json!(checksum);
            }
        }
        Some(false) => {
            // Chromium skips validation when the key is absent.
            if let Some(object) = json.as_object_mut() {
                object.remove("checksum");
            }
        }
        None => {}
    }

    Ok(applied)
}

pub fn apply(path: &Path, plan: &Plan) -> Result<Applied, String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let applied = apply_to_value(&mut json, plan, now_ms())?;

    let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
    let tmp = path.with_file_name("Bookmarks.bookmark-store.tmp");

    let write = || -> Result<(), String> {
        let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if let Ok(metadata) = fs::metadata(path) {
            let _ = fs::set_permissions(&tmp, metadata.permissions());
        }
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    };

    if let Err(error) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::writeback::PublishOp;

    fn fixture() -> Value {
        json!({
            "roots": {
                "bookmark_bar": { "type": "folder", "name": "Bookmarks bar", "id": "1", "guid": "bar", "children": [] },
                "other": { "type": "folder", "name": "Other bookmarks", "id": "2", "guid": "oth", "children": [
                    { "type": "url", "id": "7", "guid": "old-guid", "name": "Old", "url": "https://old.example/" }
                ] },
                "synced": { "type": "folder", "name": "Mobile bookmarks", "id": "3", "guid": "syn", "children": [] }
            },
            "sync_metadata": "keep-me",
            "version": 1
        })
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
                target_id: 9,
                external_id: external_id.map(str::to_string),
                name: "Work".to_string(),
                children,
            }],
            remove_ids: vec![],
        }
    }

    fn bar_children(json: &Value) -> &Vec<Value> {
        json["roots"]["bookmark_bar"]["children"]
            .as_array()
            .unwrap()
    }

    #[test]
    fn publish_into_empty_bar_creates_folder_with_nested_children() {
        let mut json = fixture();
        let plan = publish(
            None,
            vec![
                DesiredNode::Folder {
                    name: "Rust".to_string(),
                    children: vec![url("Rust", "https://rust-lang.org")],
                },
                url("Docs", "https://docs.rs"),
            ],
        );

        let applied = apply_to_value(&mut json, &plan, 1_700_000_000_000).unwrap();

        let folder = &bar_children(&json)[0];
        assert_eq!(folder["name"], "Work");
        assert_eq!(folder["type"], "folder");
        assert_eq!(folder["children"][0]["name"], "Rust");
        assert_eq!(
            folder["children"][0]["children"][0]["url"],
            "https://rust-lang.org"
        );
        assert_eq!(folder["children"][1]["url"], "https://docs.rs");
        assert_eq!(
            applied.folder_ids,
            vec![(9, folder["guid"].as_str().unwrap().to_string())]
        );
        // ids continue after the highest existing id (7)
        assert_eq!(folder["id"], "8");
    }

    #[test]
    fn republishing_with_one_url_removed_keeps_other_guids() {
        let mut json = fixture();
        let first = publish(
            None,
            vec![url("A", "https://a.example"), url("B", "https://b.example")],
        );
        let applied = apply_to_value(&mut json, &first, 1_700_000_000_000).unwrap();
        let folder_guid = applied.folder_ids[0].1.clone();
        let b_guid = bar_children(&json)[0]["children"][1]["guid"].clone();

        let second = publish(Some(&folder_guid), vec![url("B", "https://b.example")]);
        apply_to_value(&mut json, &second, 1_700_000_100_000).unwrap();

        assert_eq!(bar_children(&json).len(), 1);
        let kids = bar_children(&json)[0]["children"].as_array().unwrap();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0]["guid"], b_guid);
    }

    #[test]
    fn remove_ids_delete_a_node_from_other() {
        let mut json = fixture();
        let plan = Plan {
            publish: vec![],
            remove_ids: vec!["old-guid".to_string()],
        };
        apply_to_value(&mut json, &plan, 1_700_000_000_000).unwrap();
        assert!(json["roots"]["other"]["children"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unknown_top_level_keys_survive() {
        let mut json = fixture();
        apply_to_value(&mut json, &publish(None, vec![]), 1_700_000_000_000).unwrap();
        assert_eq!(json["sync_metadata"], "keep-me");
        assert_eq!(json["version"], 1);
    }

    #[test]
    fn verified_checksum_is_recomputed() {
        let mut json = fixture();
        json["checksum"] = json!(compute_checksum(&json).unwrap());
        let before = json["checksum"].clone();

        apply_to_value(
            &mut json,
            &publish(None, vec![url("A", "https://a.example")]),
            1_700_000_000_000,
        )
        .unwrap();

        assert_ne!(json["checksum"], before);
        assert_eq!(json["checksum"], json!(compute_checksum(&json).unwrap()));
    }

    #[test]
    fn unverifiable_checksum_is_dropped() {
        let mut json = fixture();
        json["checksum"] = json!("00000000000000000000000000000000");
        apply_to_value(&mut json, &publish(None, vec![]), 1_700_000_000_000).unwrap();
        assert!(json.get("checksum").is_none());
    }

    #[test]
    fn missing_checksum_stays_missing() {
        let mut json = fixture();
        apply_to_value(&mut json, &publish(None, vec![]), 1_700_000_000_000).unwrap();
        assert!(json.get("checksum").is_none());
    }

    #[test]
    fn apply_writes_atomically_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("bookmark-store-write-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Bookmarks");
        fs::write(&path, serde_json::to_string(&fixture()).unwrap()).unwrap();

        apply(&path, &publish(None, vec![url("A", "https://a.example")])).unwrap();

        let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(bar_children(&written)[0]["name"], "Work");
        assert!(!dir.join("Bookmarks.bookmark-store.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Read-only check against a real browser file. Must pass before writing to a real browser:
    /// `BOOKMARKS_FILE=~/.config/BraveSoftware/Brave-Browser/Default/Bookmarks \
    ///  cargo test checksum_matches_real_file -- --ignored`
    #[test]
    #[ignore]
    fn checksum_matches_real_file() {
        let path = std::env::var("BOOKMARKS_FILE").expect("set BOOKMARKS_FILE");
        let json: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let stored = json["checksum"].as_str().expect("file has no checksum");
        assert_eq!(compute_checksum(&json).as_deref(), Some(stored));
    }
}
