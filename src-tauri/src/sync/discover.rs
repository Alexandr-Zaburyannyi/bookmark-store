use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
}

impl Family {
    pub fn as_str(&self) -> &'static str {
        match self {
            Family::Chromium => "chromium",
            Family::Firefox => "firefox",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetectedSource {
    pub path: PathBuf,
    pub family: Family,
    pub browser: String,
    pub profile: String,
}

struct Root {
    config: PathBuf,
    home: PathBuf,
    suffix: &'static str,
}

const CHROMIUM_DIRS: &[(&str, &str)] = &[
    ("google-chrome", "Google Chrome"),
    ("google-chrome-beta", "Chrome Beta"),
    ("google-chrome-unstable", "Chrome Dev"),
    ("chromium", "Chromium"),
    ("BraveSoftware/Brave-Browser", "Brave"),
    ("microsoft-edge", "Microsoft Edge"),
    ("vivaldi", "Vivaldi"),
];

const MOZILLA_DIRS: &[(&str, &str)] = &[
    (".mozilla/firefox", "Firefox"),
    (".zen", "Zen"),
    (".floorp", "Floorp"),
    (".librewolf", "LibreWolf"),
    (".waterfox", "Waterfox"),
];

pub fn detect() -> Vec<DetectedSource> {
    let mut found = Vec::new();
    for root in roots() {
        found.extend(detect_chromium(&root));
        found.extend(detect_mozilla(&root));
    }
    dedupe_by_inode(found)
}

fn roots() -> Vec<Root> {
    let mut roots = Vec::new();

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return roots;
    };

    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    roots.push(Root {
        config,
        home: home.clone(),
        suffix: "",
    });

    let flatpak_apps = home.join(".var/app");
    if let Ok(entries) = fs::read_dir(&flatpak_apps) {
        for entry in entries.flatten() {
            let app_dir = entry.path();
            if app_dir.is_dir() {
                roots.push(Root {
                    config: app_dir.join("config"),
                    home: app_dir,
                    suffix: " (Flatpak)",
                });
            }
        }
    }

    roots
}

fn detect_chromium(root: &Root) -> Vec<DetectedSource> {
    let mut out = Vec::new();

    for (dir_name, label) in CHROMIUM_DIRS {
        let base_dir = root.config.join(dir_name);
        if !base_dir.is_dir() {
            continue;
        }
        let browser_label = format!("{}{}", label, root.suffix);

        let mut emitted_any = false;
        if let Some(profiles) = read_chromium_profiles(&base_dir) {
            for (profile_dir, display_name) in profiles {
                let bookmarks_path = base_dir.join(&profile_dir).join("Bookmarks");
                if bookmarks_path.is_file() {
                    out.push(DetectedSource {
                        path: bookmarks_path,
                        family: Family::Chromium,
                        browser: browser_label.clone(),
                        profile: display_name,
                    });
                    emitted_any = true;
                }
            }
        }

        if !emitted_any {
            if let Ok(entries) = fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name == "Default" || name.starts_with("Profile ") {
                        let bookmarks_path = entry.path().join("Bookmarks");
                        if bookmarks_path.is_file() {
                            out.push(DetectedSource {
                                path: bookmarks_path,
                                family: Family::Chromium,
                                browser: browser_label.clone(),
                                profile: name,
                            });
                        }
                    }
                }
            }
        }
    }

    out
}

fn read_chromium_profiles(base_dir: &std::path::Path) -> Option<Vec<(String, String)>> {
    let contents = fs::read_to_string(base_dir.join("Local State")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let info_cache = json.get("profile")?.get("info_cache")?.as_object()?;

    Some(
        info_cache
            .iter()
            .map(|(profile_dir, info)| {
                let display_name = info
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(profile_dir)
                    .to_string();
                (profile_dir.clone(), display_name)
            })
            .collect(),
    )
}

fn detect_mozilla(root: &Root) -> Vec<DetectedSource> {
    let mut out = Vec::new();

    for (dir_name, label) in MOZILLA_DIRS {
        let base_dir = root.home.join(dir_name);
        let ini_path = base_dir.join("profiles.ini");
        let Ok(contents) = fs::read_to_string(&ini_path) else {
            continue;
        };
        let browser_label = format!("{}{}", label, root.suffix);

        for section in parse_ini_sections(&contents) {
            if !section.name.starts_with("Profile") {
                continue;
            }
            let Some(profile_name) = section.get("Name") else {
                continue;
            };
            let Some(path_value) = section.get("Path") else {
                continue;
            };
            let is_relative = section.get("IsRelative").map(|v| v != "0").unwrap_or(true);

            let profile_dir = if is_relative {
                base_dir.join(path_value)
            } else {
                PathBuf::from(path_value)
            };

            let places_path = profile_dir.join("places.sqlite");
            if places_path.is_file() {
                out.push(DetectedSource {
                    path: places_path,
                    family: Family::Firefox,
                    browser: browser_label.clone(),
                    profile: profile_name.clone(),
                });
            }
        }
    }

    out
}

struct IniSection {
    name: String,
    entries: Vec<(String, String)>,
}

impl IniSection {
    fn get(&self, key: &str) -> Option<&String> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

fn parse_ini_sections(contents: &str) -> Vec<IniSection> {
    let mut sections = Vec::new();
    let mut current: Option<IniSection> = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            current = Some(IniSection {
                name: line[1..line.len() - 1].to_string(),
                entries: Vec::new(),
            });
        } else if let Some(section) = current.as_mut() {
            if let Some((key, value)) = line.split_once('=') {
                section
                    .entries
                    .push((key.trim().to_string(), value.trim().to_string()));
            }
        }
    }
    if let Some(section) = current.take() {
        sections.push(section);
    }

    sections
}

fn dedupe_by_inode(sources: Vec<DetectedSource>) -> Vec<DetectedSource> {
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut out = Vec::new();

    for source in sources {
        match fs::metadata(&source.path) {
            Ok(meta) => {
                if seen.insert((meta.dev(), meta.ino())) {
                    out.push(source);
                }
            }
            Err(_) => out.push(source),
        }
    }

    out
}
