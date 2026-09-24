use super::Entry;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn read(path: &Path) -> Result<Vec<Entry>, String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&contents)
}

fn parse(contents: &str) -> Result<Vec<Entry>, String> {
    let json: Value = serde_json::from_str(contents).map_err(|e| e.to_string())?;
    let roots = json
        .get("roots")
        .ok_or_else(|| "missing \"roots\" key".to_string())?;

    let mut entries = Vec::new();
    for key in ["bookmark_bar", "other", "synced"] {
        if let Some(node) = roots.get(key) {
            walk(node, "", &mut entries);
        }
    }
    Ok(entries)
}

fn walk(node: &Value, path: &str, out: &mut Vec<Entry>) {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");

    match node_type {
        "url" => {
            let Some(url) = node.get("url").and_then(|v| v.as_str()) else {
                return;
            };
            let external_id = node
                .get("guid")
                .and_then(|v| v.as_str())
                .or_else(|| node.get("id").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            let title = if name.is_empty() {
                url.to_string()
            } else {
                name.to_string()
            };
            let added_at = node
                .get("date_added")
                .and_then(|v| v.as_str())
                .and_then(chrome_time_to_unix_ms);

            out.push(Entry {
                external_id,
                title,
                url: url.to_string(),
                folder_path: path.to_string(),
                added_at,
            });
        }
        "folder" => {
            let new_path = if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}/{name}")
            };
            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                for child in children {
                    walk(child, &new_path, out);
                }
            }
        }
        _ => {}
    }
}

fn chrome_time_to_unix_ms(raw: &str) -> Option<i64> {
    let webkit_us: i64 = raw.parse().ok()?;
    if webkit_us == 0 {
        return None;
    }
    Some(webkit_us / 1000 - 11_644_473_600_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
    {
      "roots": {
        "bookmark_bar": {
          "type": "folder",
          "name": "Bookmarks bar",
          "children": [
            {
              "type": "folder",
              "name": "Dev",
              "children": [
                {
                  "type": "url",
                  "guid": "11111111-1111-1111-1111-111111111111",
                  "id": "5",
                  "name": "Rust",
                  "url": "https://rust-lang.org",
                  "date_added": "13350000000000000"
                },
                {
                  "type": "url",
                  "guid": "22222222-2222-2222-2222-222222222222",
                  "id": "6",
                  "name": "",
                  "url": "https://example.com",
                  "date_added": "0"
                }
              ]
            }
          ]
        },
        "other": { "type": "folder", "name": "Other bookmarks", "children": [] },
        "synced": { "type": "folder", "name": "Mobile bookmarks", "children": [] }
      }
    }
    "#;

    #[test]
    fn walks_nested_folders_and_builds_the_path() {
        let entries = parse(FIXTURE).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].folder_path, "Bookmarks bar/Dev");
        assert_eq!(entries[0].title, "Rust");
        assert_eq!(
            entries[0].external_id,
            "11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn falls_back_to_the_url_when_the_title_is_empty() {
        let entries = parse(FIXTURE).unwrap();
        assert_eq!(entries[1].title, "https://example.com");
    }

    #[test]
    fn converts_a_known_webkit_timestamp_to_unix_ms() {
        let entries = parse(FIXTURE).unwrap();
        assert_eq!(entries[0].added_at, Some(1_705_526_400_000));
    }

    #[test]
    fn treats_a_zero_date_added_as_not_recorded() {
        let entries = parse(FIXTURE).unwrap();
        assert_eq!(entries[1].added_at, None);
    }
}
