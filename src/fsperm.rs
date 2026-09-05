//! User-private file modes for on-disk state.
//!
//! State files hold the user's tasks; they are nobody else's business. On Unix
//! the state/config directories are `0700` and the files `0600` (`tsk.json`,
//! `trash.jsonl`, backups, the lock file, temp files, `walkthrough.json`), both
//! for fresh creation and tightened after the fact for paths an older version
//! or a looser umask left readable. Tightening only ever strips bits: an
//! existing stricter mode (a file an admin locked to `0400`, a read-only
//! directory) is left as it is. No permission is claimed on other platforms:
//! there these helpers compile down to plain create/copy.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Create `path` (recursively) as a user-private directory (`0700` on Unix),
/// stripping any group/other access from one that already exists. Stricter
/// existing modes are kept.
pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    tighten_private_dir(path)?;
    Ok(())
}

/// Tighten an existing state/config directory without creating it. This is
/// best effort for read paths, which must remain usable when config is absent
/// or permissions cannot be repaired.
pub fn tighten_dir(path: &Path) {
    #[cfg(unix)]
    {
        let _ = tighten_private_dir(path);
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(unix)]
fn tighten_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "state/config directory must be a directory, not a symlink",
        ));
    }

    // Apply chmod through a verified open handle. This prevents a final-path
    // symlink, or a directory swapped between metadata and open, from redirecting
    // the permission change to another target.
    let directory = File::open(path)?;
    let opened_metadata = directory.metadata()?;
    if path_metadata.dev() != opened_metadata.dev() || path_metadata.ino() != opened_metadata.ino()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "state/config directory changed while permissions were checked",
        ));
    }
    let mode = opened_metadata.permissions().mode();
    directory.set_permissions(fs::Permissions::from_mode(mode & 0o700))
}

/// Create a file holding owner-only content (`0600` on Unix when created),
/// truncating an existing one. Temp files and their renamed targets go through
/// here so a document is never briefly world-readable.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Open (or create) a lock file without truncating it, `0600` on Unix when
/// created. Existing files are tightened by the caller after open.
pub fn open_lock_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Tighten an existing file to owner-only (`0600`) on Unix by stripping
/// group/other bits, never granting owner bits the file did not have. Missing
/// paths and other platforms are no-ops; only regular files are touched, never
/// symlinks. Best effort: a failed tighten must not fail the surrounding
/// operation.
pub fn tighten_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let Ok(path_metadata) = fs::symlink_metadata(path) else {
            return;
        };
        if !path_metadata.file_type().is_file() {
            return;
        }
        let Ok(file) = File::open(path) else {
            return;
        };
        let Ok(opened_metadata) = file.metadata() else {
            return;
        };
        if path_metadata.dev() != opened_metadata.dev()
            || path_metadata.ino() != opened_metadata.ino()
        {
            return;
        }
        let mode = opened_metadata.permissions().mode();
        let _ = file.set_permissions(fs::Permissions::from_mode(mode & 0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDirGuard(std::path::PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::set_permissions(self.0.join("target"), fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn ensure_private_dir_rejects_a_symlink_without_chmodding_its_target() {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("tsk-fsperm-symlink-{}-{seq}", std::process::id()));
        fs::create_dir_all(&root).expect("mkdir root");
        let _guard = TempDirGuard(root.clone());
        let target = root.join("target");
        let link = root.join("state");
        fs::create_dir(&target).expect("mkdir target");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o500)).expect("chmod target");
        symlink(&target, &link).expect("symlink state dir");

        ensure_private_dir(&link).expect_err("a state/config directory symlink must be rejected");

        assert_eq!(
            fs::metadata(&target)
                .expect("target metadata")
                .permissions()
                .mode()
                & 0o7777,
            0o500,
            "rejecting the symlink must not grant permissions on its target"
        );
    }
}
