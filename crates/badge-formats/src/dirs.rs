//! Where the app keeps its per-user files.

use std::path::{Path, PathBuf};

/// Name of the app's per-user folders.
pub const APP_DIR: &str = "led-badge-studio";
/// The folder name used before the app was called LED Badge Studio.
const LEGACY_APP_DIR: &str = "led-badge";

/// A per-user base folder: `%APPDATA%` on Windows,
/// `~/Library/Application Support` on macOS, and elsewhere `$xdg_var` or
/// `~/fallback` (also when `$xdg_var` is empty or relative, which the XDG
/// spec says to ignore).
fn user_base(xdg_var: &str, fallback: &str) -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os(xdg_var)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(fallback)))
    }
}

/// Base of the per-user settings folder (`~/.config`, `%APPDATA%`, …).
pub fn config_base() -> Option<PathBuf> {
    user_base("XDG_CONFIG_HOME", ".config")
}

/// Base of the per-user data folder (`~/.local/share`, `%APPDATA%`, …).
pub fn data_base() -> Option<PathBuf> {
    user_base("XDG_DATA_HOME", ".local/share")
}

/// Base of the per-user cache folder (`~/.cache`, `%APPDATA%`, …).
pub fn cache_base() -> Option<PathBuf> {
    user_base("XDG_CACHE_HOME", ".cache")
}

/// `base/led-badge-studio/name`. If the file only exists in the folder that
/// earlier versions used (`base/led-badge/name`), it is moved over first.
pub fn user_file(base: &Path, name: &str) -> PathBuf {
    let path = base.join(APP_DIR).join(name);
    let old = base.join(LEGACY_APP_DIR).join(name);
    if !path.exists() && old.is_file() && std::fs::create_dir_all(base.join(APP_DIR)).is_ok() {
        let _ = std::fs::rename(&old, &path);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_file_moves_files_from_the_old_folder() {
        let base = std::env::temp_dir().join(format!("led-badge-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(LEGACY_APP_DIR)).unwrap();
        std::fs::write(base.join(LEGACY_APP_DIR).join("settings.json"), "{}").unwrap();
        std::fs::write(base.join(LEGACY_APP_DIR).join("other"), "kept").unwrap();

        let path = user_file(&base, "settings.json");
        assert_eq!(path, base.join(APP_DIR).join("settings.json"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        assert!(!base.join(LEGACY_APP_DIR).join("settings.json").exists());
        // Only the app's own file moves.
        assert!(base.join(LEGACY_APP_DIR).join("other").exists());
        // A file missing from both folders is just a path.
        assert!(!user_file(&base, "missing").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
