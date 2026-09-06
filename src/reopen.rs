//! One-shot context handoff from the board launcher to an existing board pane.
//!
//! Herdr injects invocation context when a pane is created, but focusing an existing
//! plugin pane cannot change that process environment. The launcher therefore writes a
//! short-lived, private request in the normal tsk state directory before focusing the
//! pane. The board consumes it during its idle tick.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fsperm;
use crate::store::{StoreSignature, TaskStore};

const REQUEST_FILE: &str = "reopen.json";
const LOCK_FILE: &str = "reopen.json.lock";
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_PROJECT_BYTES: usize = 4096;
const FORMAT_VERSION: u32 = 1;
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(0);

/// A validated invocation context handoff. `project: None` means the invoking
/// directory was outside a repository and the board should select Desk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenRequest {
    pub project: Option<PathBuf>,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireRequest {
    version: u32,
    id: String,
    project: Option<String>,
}

impl ReopenRequest {
    /// Build a request with a process-unique id. Validation is repeated at write time,
    /// because callers may construct the value directly in tests or future adapters.
    pub fn new(project: Option<PathBuf>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let seq = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
        Self {
            project,
            id: format!("{}-{}-{}", std::process::id(), now, seq),
        }
    }

    /// Atomically publish this request under `state_dir`, using the same private-directory
    /// policy as the store. A target symlink is replaced, never followed.
    pub fn write(&self, state_dir: &Path) -> io::Result<()> {
        let wire = self.to_wire()?;
        let data = serde_json::to_vec(&wire)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if data.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "reopen request is too large",
            ));
        }
        fsperm::ensure_private_dir(state_dir)?;
        let _lock = RequestLock::acquire(state_dir)?;
        let sequence = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let temp = state_dir.join(format!(
            ".{REQUEST_FILE}.tmp.{}.{}",
            std::process::id(),
            sequence
        ));
        let target = state_dir.join(REQUEST_FILE);
        let result = (|| {
            let mut file = fsperm::create_private_file(&temp)?;
            file.write_all(&data)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &target)?;
            sync_directory(state_dir)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn to_wire(&self) -> io::Result<WireRequest> {
        if self.id.is_empty() || self.id.len() > 128 || !valid_id(&self.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid reopen request id",
            ));
        }
        let project = self
            .project
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        if project.as_deref().is_some_and(|path| {
            path.is_empty() || path.len() > MAX_PROJECT_BYTES || path.contains('\0')
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid reopen project path",
            ));
        }
        Ok(WireRequest {
            version: FORMAT_VERSION,
            id: self.id.clone(),
            project,
        })
    }

    fn from_wire(wire: WireRequest) -> io::Result<Self> {
        if wire.version != FORMAT_VERSION
            || wire.id.is_empty()
            || wire.id.len() > 128
            || !valid_id(&wire.id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid reopen request",
            ));
        }
        let project = wire.project.map(PathBuf::from);
        let request = Self {
            project,
            id: wire.id,
        };
        request.to_wire().map(|_| request)
    }
}

fn valid_id(id: &str) -> bool {
    id.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = path;
        Ok(())
    }
}

struct RequestLock {
    path: PathBuf,
}

impl RequestLock {
    fn acquire(state_dir: &Path) -> io::Result<Self> {
        let path = state_dir.join(LOCK_FILE);
        for _ in 0..100 {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    file.sync_all()?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // A lock may belong to a live launcher or board. Never infer staleness
                    // from its mtime: waiting is safe, deleting it can interleave two writers.
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "reopen request is busy",
        ))
    }
}

impl Drop for RequestLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Idle watcher for the one-shot reopen request.
#[derive(Debug, Default)]
pub struct ReopenWatch {
    state_dir: PathBuf,
    last_seen: Option<StoreSignature>,
    pending: Option<ReopenRequest>,
    pending_signature: Option<StoreSignature>,
}

impl ReopenWatch {
    /// Start a board watcher and retire any request left by a prior board process.
    /// A request present before startup is stale, not a new invocation.
    pub fn seeded(store: &TaskStore) -> Self {
        let path = store.path().join(REQUEST_FILE);
        if let Ok(_lock) = RequestLock::acquire(store.path()) {
            let _ = fs::remove_file(path);
        }
        Self {
            state_dir: store.path().to_path_buf(),
            ..Self::default()
        }
    }

    /// Poll without consuming. The same request remains pending until `acknowledge`,
    /// which lets a save-recovery or dirty editor defer the context switch safely.
    pub fn poll(&mut self) -> Option<ReopenRequest> {
        let Ok(_lock) = RequestLock::acquire(&self.state_dir) else {
            return self.pending.clone();
        };
        let path = self.request_path();
        let signature = request_signature(&path);
        if self.pending.is_some() {
            if signature != self.pending_signature {
                self.pending = self.read_request(&path);
                self.pending_signature = self.pending.as_ref().and(signature);
            }
            return self.pending.clone();
        }
        if signature == self.last_seen {
            return None;
        }
        self.last_seen = signature;
        self.pending = self.read_request(&path);
        self.pending_signature = self.pending.as_ref().and(signature);
        self.pending.clone()
    }

    /// Mark the currently applied request handled and remove its file. If another
    /// invocation replaced it meanwhile, leave that newer request for the next poll.
    pub fn acknowledge(&mut self) {
        let Ok(_lock) = RequestLock::acquire(&self.state_dir) else {
            return;
        };
        let path = self.request_path();
        if self
            .pending_signature
            .is_some_and(|signature| request_signature(&path) == Some(signature))
        {
            let _ = fs::remove_file(path);
            self.last_seen = None;
        }
        self.pending = None;
        self.pending_signature = None;
    }

    fn request_path(&self) -> PathBuf {
        self.state_dir.join(REQUEST_FILE)
    }

    fn read_request(&self, path: &Path) -> Option<ReopenRequest> {
        let metadata = fs::symlink_metadata(path).ok()?;
        if !metadata.file_type().is_file() || metadata.len() as usize > MAX_REQUEST_BYTES {
            let _ = fs::remove_file(path);
            return None;
        }
        let mut file = fs::File::open(path).ok()?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_REQUEST_BYTES {
            let _ = fs::remove_file(path);
            return None;
        }
        let wire = serde_json::from_slice::<WireRequest>(&bytes).ok();
        let request = wire.and_then(|wire| ReopenRequest::from_wire(wire).ok());
        if request.is_none() {
            let _ = fs::remove_file(path);
        }
        request
    }
}

fn request_signature(path: &Path) -> Option<StoreSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(StoreSignature {
            dev: metadata.dev(),
            ino: metadata.ino(),
            modified,
            len: metadata.len(),
        })
    }
    #[cfg(not(unix))]
    {
        Some(StoreSignature {
            modified,
            len: metadata.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_are_safe_and_unique() {
        let a = ReopenRequest::new(None);
        let b = ReopenRequest::new(None);
        assert_ne!(a.id, b.id);
        assert!(valid_id(&a.id));
    }

    #[test]
    fn request_lock_serializes_claim_and_acknowledge_operations() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-lock-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let lock = RequestLock::acquire(&dir).expect("first lock");
        let second = RequestLock::acquire(&dir);
        assert!(
            second.is_err(),
            "a concurrent writer cannot claim the request lock"
        );
        drop(lock);
        assert!(RequestLock::acquire(&dir).is_ok());
        let _ = fs::remove_dir_all(dir);
    }
}
