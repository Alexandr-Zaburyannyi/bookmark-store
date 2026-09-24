use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Writability {
    Writable,
    BrowserRunning,
    Encrypted,
    Missing,
}

impl Writability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Writability::Writable => "writable",
            Writability::BrowserRunning => "browser_running",
            Writability::Encrypted => "encrypted",
            Writability::Missing => "missing",
        }
    }
}

// Lock files can be dangling symlinks (Chromium's SingletonLock points at "host-pid"),
// so `exists()` would follow the link and report false.
fn lock_present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn has_encrypted_bookmarks(profile_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(profile_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with("EncryptedBookmarks")
    })
}

pub fn check(family: &str, path: &Path) -> Writability {
    if !path.is_file() {
        return Writability::Missing;
    }
    let Some(profile_dir) = path.parent() else {
        return Writability::Missing;
    };

    match family {
        "chromium" => {
            if has_encrypted_bookmarks(profile_dir) {
                return Writability::Encrypted;
            }
            let running = profile_dir
                .parent()
                .is_some_and(|user_data| lock_present(&user_data.join("SingletonLock")));
            if running {
                Writability::BrowserRunning
            } else {
                Writability::Writable
            }
        }
        "firefox" => {
            if lock_present(&profile_dir.join("lock")) {
                Writability::BrowserRunning
            } else {
                Writability::Writable
            }
        }
        _ => Writability::Writable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("bookmark-store-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn chromium_profile(name: &str) -> (PathBuf, PathBuf) {
        let root = temp_root(name);
        let profile = root.join("Default");
        fs::create_dir_all(&profile).unwrap();
        let bookmarks = profile.join("Bookmarks");
        fs::write(&bookmarks, "{}").unwrap();
        (root, bookmarks)
    }

    #[test]
    fn missing_file_is_missing() {
        let root = temp_root("missing");
        assert_eq!(
            check("chromium", &root.join("Default/Bookmarks")),
            Writability::Missing
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chromium_is_writable_until_singleton_lock_appears() {
        let (root, bookmarks) = chromium_profile("chromium-lock");
        assert_eq!(check("chromium", &bookmarks), Writability::Writable);

        // dangling symlink, as a real browser leaves it
        #[cfg(unix)]
        std::os::unix::fs::symlink("host-1234", root.join("SingletonLock")).unwrap();
        #[cfg(not(unix))]
        fs::write(root.join("SingletonLock"), "").unwrap();
        assert_eq!(check("chromium", &bookmarks), Writability::BrowserRunning);

        fs::remove_file(root.join("SingletonLock")).unwrap();
        assert_eq!(check("chromium", &bookmarks), Writability::Writable);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chromium_with_encrypted_bookmarks_is_encrypted() {
        let (root, bookmarks) = chromium_profile("chromium-encrypted");
        fs::write(root.join("Default/EncryptedBookmarks2"), "").unwrap();
        assert_eq!(check("chromium", &bookmarks), Writability::Encrypted);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn firefox_is_writable_until_lock_appears() {
        let root = temp_root("firefox-lock");
        let places = root.join("places.sqlite");
        fs::write(&places, "").unwrap();
        assert_eq!(check("firefox", &places), Writability::Writable);

        fs::write(root.join("lock"), "").unwrap();
        assert_eq!(check("firefox", &places), Writability::BrowserRunning);
        fs::remove_dir_all(root).unwrap();
    }
}
