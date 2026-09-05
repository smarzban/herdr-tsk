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
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::symlink_metadata(path)?;
        let mode = metadata.permissions().mode();
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o700))?;
    }
    Ok(())
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
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.is_file() {
                let mode = metadata.permissions().mode();
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o600));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
