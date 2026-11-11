//! Replacing a file safely, for project files, exports, settings and the
//! copy of unsaved work.

use std::fs::{File, Metadata};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Replaces the file at `path` with `bytes`: written beside it and onto the
/// disk first (as `name.1a2b3c4d.tmp`, a name of its own each time), then
/// moved over it, so a crash, a power cut or a full disk leaves the earlier
/// file (and no half one). A symlink is followed, so the file it points to
/// is the one replaced (but not one another user left in a shared folder
/// such as `/tmp`), and on Unix the replaced file's permissions are kept.
pub fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::hash::{BuildHasher, Hasher, RandomState};
    let path = &follow_links(path)?;
    let old = std::fs::metadata(path).ok();
    // Random: every `RandomState` has keys of its own.
    let (tmp, file) = create_tmp(path, old.as_ref(), || RandomState::new().build_hasher().finish() as u32)?;
    let write = |mut file: File| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        // Not on Windows, where the only permission is read-only: a
        // read-only copy couldn't replace the file or be removed. Where
        // permissions can't be set (some network and FUSE folders), it's
        // saved with the ones it was created with, never more.
        #[cfg(unix)]
        if let Some(old) = &old {
            let _ = std::fs::set_permissions(&tmp, old.permissions());
        }
        std::fs::rename(&tmp, path)
    };
    let written = write(file);
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

const TMP_NAME_TRIES: usize = 16;

/// The name a replacement of `path` is written under: `name.1a2b3c4d.tmp`.
fn tmp_path(path: &Path, id: u32) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{id:08x}.tmp"));
    PathBuf::from(tmp)
}

/// Creates the file a replacement of `path` is written to, beside it, under
/// a name from `next_id` (see [`tmp_path`]), always as a new file. A file
/// already at that name (a leftover, someone's own file, another save of
/// the same file going on, or a symlink someone left to send the write
/// elsewhere) is left as it is, never written through or removed: another
/// name is tried. On Unix the new file starts with no more permissions than
/// the replaced file has.
fn create_tmp(
    path: &Path,
    old: Option<&Metadata>,
    mut next_id: impl FnMut() -> u32,
) -> std::io::Result<(PathBuf, File)> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(old.map_or(0o666, |m| m.permissions().mode() & 0o777));
    }
    #[cfg(not(unix))]
    let _ = old;
    for _ in 0..TMP_NAME_TRIES {
        let tmp = tmp_path(path, next_id());
        match options.open(&tmp) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            opened => return opened.map(|file| (tmp, file)),
        }
    }
    Err(Error::new(
        ErrorKind::AlreadyExists,
        format!("{}: no free name for a temporary file beside it", path.display()),
    ))
}

/// The file a chain of symlinks at `path` ends at (`path` itself when it
/// isn't one). Unlike `canonicalize`, works for a file that doesn't exist
/// yet and gives plain paths on Windows.
fn follow_links(path: &Path) -> std::io::Result<PathBuf> {
    let mut path = path.to_path_buf();
    // A limit, for links that point at each other.
    for _ in 0..40 {
        let Ok(link) = std::fs::symlink_metadata(&path) else {
            return Ok(path);
        };
        if !link.file_type().is_symlink() {
            return Ok(path);
        }
        may_follow(&path, &link)?;
        let target = std::fs::read_link(&path)?;
        path = match path.parent() {
            Some(dir) if target.is_relative() => dir.join(target),
            _ => target,
        };
    }
    Err(Error::other(format!(
        "{}: too many levels of symbolic links",
        path.display()
    )))
}

/// Refuses a link another user made in a shared folder (one anyone can
/// write to but only owners delete from, like `/tmp`), which could send the
/// write to a file of ours: the rule Linux applies itself when opening
/// files (`fs.protected_symlinks`), here on every Unix.
#[cfg(unix)]
fn may_follow(link: &Path, link_meta: &Metadata) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let dir = link
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let dir = std::fs::metadata(dir)?;
    // SAFETY: geteuid takes nothing, can't fail and touches no memory.
    let me = unsafe { libc::geteuid() };
    if followable(link_meta.uid(), dir.uid(), dir.mode(), me) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            format!(
                "{}: not following a link another user made in a shared folder",
                link.display()
            ),
        ))
    }
}

#[cfg(not(unix))]
fn may_follow(_link: &Path, _link_meta: &Metadata) -> std::io::Result<()> {
    Ok(())
}

/// Whether a link owned by `link_owner`, in a folder owned by `dir_owner`
/// with `dir_mode`, may be followed by the user `me`.
#[cfg(unix)]
fn followable(link_owner: u32, dir_owner: u32, dir_mode: u32, me: u32) -> bool {
    // Sticky, and writable by others.
    let shared = dir_mode & 0o1002 == 0o1002;
    !shared || link_owner == me || link_owner == dir_owner
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

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn replaces_whole_or_not_at_all() {
        let dir = temp_dir("replace");
        let path = dir.join("a.txt");
        replace_file(&path, b"first").unwrap();
        replace_file(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(names(&dir), ["a.txt"], "no temporary file is left");
        // A write that fails leaves nothing behind.
        assert!(replace_file(&dir.join("missing/a.txt"), b"x").is_err());
        assert!(!dir.join("missing").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_named_like_the_old_temporary_file_is_kept() {
        let dir = temp_dir("own-tmp");
        let path = dir.join("a.badge");
        std::fs::write(dir.join("a.badge.tmp"), b"someone's own").unwrap();
        replace_file(&path, b"saved").unwrap();
        assert_eq!(std::fs::read(dir.join("a.badge.tmp")).unwrap(), b"someone's own");
        assert_eq!(names(&dir), ["a.badge", "a.badge.tmp"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_taken_temporary_name_is_left_alone_and_another_tried() {
        let dir = temp_dir("taken");
        let path = dir.join("a.txt");
        std::fs::write(tmp_path(&path, 1), b"another save's").unwrap();
        let mut ids = [1, 1, 2].into_iter();
        let (tmp, _file) = create_tmp(&path, None, || ids.next().unwrap()).unwrap();
        assert_eq!(tmp, tmp_path(&path, 2));
        assert_eq!(std::fs::read(tmp_path(&path, 1)).unwrap(), b"another save's");
        let e = create_tmp(&path, None, || 1).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(tmp_path(&path, 1)).unwrap(), b"another save's");
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

    #[cfg(unix)]
    #[test]
    fn never_writes_through_a_file_already_at_the_temporary_name() {
        let dir = temp_dir("planted");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"keep").unwrap();
        let path = dir.join("out.gif");
        let planted = tmp_path(&path, 7);
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        let mut ids = [7, 8].into_iter();
        let (tmp, mut file) = create_tmp(&path, None, || ids.next().unwrap()).unwrap();
        file.write_all(b"gif").unwrap();
        assert_eq!(tmp, tmp_path(&path, 8));
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        assert!(std::fs::symlink_metadata(&planted).unwrap().file_type().is_symlink());
        replace_file(&path, b"gif").unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        assert_eq!(std::fs::read(&path).unwrap(), b"gif");
        assert!(!std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn links_in_shared_folders_are_followed_only_when_ours_or_the_folders() {
        let (me, other, root) = (501, 502, 0);
        // /tmp: sticky and writable by everyone.
        assert!(!followable(other, root, 0o41777, me));
        assert!(followable(me, root, 0o41777, me));
        assert!(followable(root, root, 0o41777, me));
        // An ordinary folder, or a sticky one only its owner writes to.
        assert!(followable(other, other, 0o40755, me));
        assert!(followable(other, other, 0o41755, me));
    }

    #[cfg(unix)]
    #[test]
    fn links_that_loop_are_refused() {
        let dir = temp_dir("loop");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();
        assert!(replace_file(&a, b"x").is_err());
        assert!(std::fs::symlink_metadata(&a).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(dir);
    }
}
