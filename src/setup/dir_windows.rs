//! Setup I/O for Windows. Uses `std::fs` with a lock file for concurrency.
//!
//! Unix `dir.rs` pins a parent directory descriptor and does all I/O relative to it
//! (openat/mkdirat/renameat/unlinkat/fstatat) to defeat symlink races. Windows has no
//! equivalent and no default world-writable config directory for a single user, so the
//! TOCTOU threat model does not apply the same way. The `File::try_lock` (Rust 1.96)
//! still serializes concurrent `tsk setup herdr` invocations, which is the real
//! concurrency concern.
// ponytail: no descriptor-relative I/O on Windows; user-local config dir has no
// symlink-attack threat. Upgrade to CreateFileW + NtCreateFile if a hardened model is needed.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

pub(super) struct Dir {
    pub path: PathBuf,
}

fn name(path: &Path) -> io::Result<&Path> {
    if path.components().count() != 1 || path.file_name() != Some(path.as_os_str()) {
        return Err(io::Error::other("expected one child filename"));
    }
    Ok(path)
}

impl Dir {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => Ok(Self {
                path: path.to_path_buf(),
            }),
            Ok(_) => Err(io::Error::other(format!(
                "not a directory: {}",
                path.display()
            ))),
            Err(e) if e.kind() == io::ErrorKind::NotFound && create => {
                let parent = path
                    .parent()
                    .ok_or_else(|| io::Error::other("missing config ancestor"))?;
                Self::open(parent, true)?.child(
                    Path::new(
                        path.file_name()
                            .ok_or_else(|| io::Error::other("missing directory name"))?,
                    ),
                    true,
                )
            }
            Err(e) => Err(e),
        }
    }

    pub fn child(&self, child: &Path, create: bool) -> io::Result<Self> {
        let n = name(child)?;
        let path = self.path.join(n);
        if create {
            fs::create_dir_all(&path)?;
        }
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => Ok(Self { path }),
            Ok(_) => Err(io::Error::other(format!(
                "not a directory: {}",
                path.display()
            ))),
            Err(e) => Err(e),
        }
    }

    pub fn read(&self, child: &Path) -> io::Result<Option<String>> {
        let n = name(child)?;
        match fs::read_to_string(self.path.join(n)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn write_new(&self, child: &Path, text: &str) -> io::Result<()> {
        let n = name(child)?;
        let path = self.path.join(n);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        use std::io::Write;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    }

    pub fn lock(&self) -> io::Result<File> {
        let path = self.path.join(".tsk-setup.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.try_lock()
            .map_err(|e| io::Error::other(format!("could not acquire setup lock: {e}")))?;
        Ok(file)
    }

    /// Whether a child name exists. Pure stat, never creates.
    pub fn exists(&self, child: &Path) -> io::Result<bool> {
        let n = name(child)?;
        match fs::metadata(self.path.join(n)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from = name(from)?;
        let to = name(to)?;
        let from_path = self.path.join(from);
        let to_path = self.path.join(to);
        // On Windows, rename fails if the destination exists. Remove it first.
        let _ = fs::remove_file(&to_path);
        fs::rename(&from_path, &to_path)
    }

    pub fn remove(&self, child: &Path, directory: bool) -> io::Result<()> {
        let n = name(child)?;
        let path = self.path.join(n);
        if directory {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        }
    }

    /// On Unix this checks dev/ino identity to detect a replaced parent directory.
    /// Windows has no equivalent without extra handles; the lock file serializes
    /// concurrent setup runs, and the config dir is user-local.
    pub fn validate(&self) -> io::Result<()> {
        let meta = fs::symlink_metadata(&self.path)?;
        if !meta.is_dir() {
            return Err(io::Error::other(format!(
                "config/asset directory is not a directory: {}",
                self.path.display()
            )));
        }
        Ok(())
    }
}

pub(super) struct TempFile<'a> {
    pub dir: &'a Dir,
    pub name: PathBuf,
}

impl TempFile<'_> {
    pub fn preserve(&mut self) {
        self.name = PathBuf::new();
    }
}

impl Drop for TempFile<'_> {
    fn drop(&mut self) {
        if !self.name.as_os_str().is_empty() {
            let _ = self.dir.remove(&self.name, false);
        }
    }
}
