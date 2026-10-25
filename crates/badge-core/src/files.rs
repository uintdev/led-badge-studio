//! Replacing a file safely, for project files, exports, settings and the
//! copy of unsaved work.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Replaces the file at `path` with `bytes`: written beside it and onto the
/// disk first, then moved over it, so a crash, a power cut or a full disk
/// leaves the earlier file (and no half one). A symlink is followed, so the
/// file it points to is the one replaced, and on Unix the replaced file's
/// permissions are kept.
pub fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let path = &follow_links(path);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // Not on Windows, where the only permission is read-only: a
        // read-only copy couldn't replace the file or be removed.
        #[cfg(unix)]
        if let Ok(old) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, old.permissions())?;
        }
        std::fs::rename(&tmp, path)
    };
    let written = write();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// The file a chain of symlinks at `path` ends at (`path` itself when it
/// isn't one). Unlike `canonicalize`, works for a file that doesn't exist
/// yet and gives plain paths on Windows.
fn follow_links(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();
    // A limit, for links that point at each other.
    for _ in 0..40 {
        let is_link = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        let Some(target) = is_link.then(|| std::fs::read_link(&path).ok()).flatten() else {
            break;
        };
        path = match path.parent() {
            Some(dir) if target.is_relative() => dir.join(target),
            _ => target,
        };
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("badge-core-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_whole_or_not_at_all() {
        let dir = temp_dir("replace");
        let path = dir.join("a.txt");
        replace_file(&path, b"first").unwrap();
        replace_file(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!dir.join("a.txt.tmp").exists());
        // A write that fails leaves nothing behind.
        assert!(replace_file(&dir.join("missing/a.txt"), b"x").is_err());
        assert!(!dir.join("missing").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn follows_links_and_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("links");
        let target = dir.join("real.txt");
        replace_file(&target, b"first").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        // A relative link, as `ln -s real.txt link.txt` makes.
        let link = dir.join("link.txt");
        std::os::unix::fs::symlink("real.txt", &link).unwrap();
        replace_file(&link, b"second").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
        // A link to a file that doesn't exist yet creates that file.
        let dangling = dir.join("dangling.txt");
        std::os::unix::fs::symlink(dir.join("new.txt"), &dangling).unwrap();
        replace_file(&dangling, b"new").unwrap();
        assert_eq!(std::fs::read(dir.join("new.txt")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(dir);
    }
}
