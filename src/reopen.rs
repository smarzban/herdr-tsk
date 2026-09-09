//! One-shot context handoff from the board launcher to an existing board pane.
//!
//! Herdr injects invocation context when a pane is created, but focusing an existing
//! plugin pane cannot change that process environment. The launcher therefore writes a
//! short-lived, private request in the normal tsk state directory before focusing the
//! pane. The board consumes it during its idle tick.

use std::fs::{self, File};
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
    _file: File,
}

impl RequestLock {
    /// Acquire the persistent lock inode with a bounded wait. Ownership is the OS
    /// advisory lock, not the inode, so a crash releases it without unlinking anything.
    fn acquire(state_dir: &Path) -> io::Result<Self> {
        let path = state_dir.join(LOCK_FILE);
        let file = crate::fsperm::open_lock_file(&path)?;
        crate::fsperm::tighten_file(&path);
        for _ in 0..100 {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "reopen request is busy",
        ))
    }

    /// One nonblocking claim for the board's idle path. A competing writer must
    /// never stall the event loop.
    fn try_acquire(state_dir: &Path) -> io::Result<Self> {
        let path = state_dir.join(LOCK_FILE);
        let file = crate::fsperm::open_lock_file(&path)?;
        crate::fsperm::tighten_file(&path);
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "reopen request is busy",
            )),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }
}

/// Idle watcher for the one-shot reopen request.
#[derive(Debug, Default)]
pub struct ReopenWatch {
    state_dir: PathBuf,
    last_seen: Option<RequestSignature>,
    pending: Option<ReopenRequest>,
    pending_signature: Option<RequestSignature>,
    deferred_notice_sent: bool,
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
        self.poll_with_pre_lock(|| {})
    }

    /// Read the pending request without taking the writer lock. Writers publish with an
    /// atomic rename, so any read sees a whole request, and the signature hashes the bytes
    /// that were read, so a stale answer cannot be paired with a fresh signature. Locking
    /// here only added a failure mode: a busy lock made the idle path re-deliver the previous
    /// request (the macOS CI flake in `reopen_watch_keeps_the_latest_request`).
    fn poll_with_pre_lock(&mut self, before_read: impl FnOnce()) -> Option<ReopenRequest> {
        let path = self.request_path();
        before_read();
        let Some((signature, bytes)) = request_snapshot(&path) else {
            // A removed or unreadable pending request was withdrawn, so do not keep
            // delivering a stale context switch or its one-shot deferred notice.
            self.pending = None;
            self.pending_signature = None;
            self.deferred_notice_sent = false;
            return None;
        };
        if self.pending_signature == Some(signature) {
            return self.pending.clone();
        }
        if self.pending.is_none() && self.last_seen == Some(signature) {
            return None;
        }
        self.last_seen = Some(signature);
        self.pending = match bytes {
            Some(bytes) => parse_request(&bytes),
            None => None,
        };
        if self.pending.is_none() {
            // Oversized or malformed: drop it, but only while nobody is replacing it and
            // it is still the file we judged.
            if let Ok(_lock) = RequestLock::try_acquire(&self.state_dir) {
                if request_snapshot(&path).map(|(sig, _)| sig) == Some(signature) {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        self.pending_signature = self.pending.as_ref().map(|_| signature);
        self.deferred_notice_sent = false;
        self.pending.clone()
    }

    /// Return true once per pending request when a dirty editor defers it.
    pub fn mark_deferred_notice(&mut self) -> bool {
        if self.deferred_notice_sent {
            false
        } else {
            self.deferred_notice_sent = true;
            true
        }
    }

    /// Mark the currently applied request handled and remove its file. If another
    /// invocation replaced it meanwhile, leave that newer request for the next poll.
    pub fn acknowledge(&mut self) {
        let Some(signature) = self.pending_signature else {
            return;
        };
        // Application already happened. Consume it before opportunistic cleanup so a
        // busy lock cannot cause the same request to be applied on every idle tick.
        self.pending = None;
        self.pending_signature = None;
        self.last_seen = Some(signature);
        self.deferred_notice_sent = false;

        let Ok(_lock) = RequestLock::try_acquire(&self.state_dir) else {
            return;
        };
        let path = self.request_path();
        if request_snapshot(&path).map(|(sig, _)| sig) == Some(signature) {
            let _ = fs::remove_file(path);
            self.last_seen = None;
        }
    }

    fn request_path(&self) -> PathBuf {
        self.state_dir.join(REQUEST_FILE)
    }
}

/// Identity of the pending request file. The stat signature alone is not enough: APFS reuses
/// the inode of a just-unlinked file, and two requests of equal length written inside one
/// mtime tick then look identical, so the watch kept delivering the first. The file is capped
/// at `MAX_REQUEST_BYTES`, so hashing its bytes on every preflight is cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestSignature {
    stat: StoreSignature,
    digest: u64,
}

/// Stat plus bytes of the pending request. `None` when there is no regular file. The bytes
/// are `None` when the file is larger than `MAX_REQUEST_BYTES` (it is still identified, so
/// the caller can decide to remove it) and its digest is fixed at zero.
fn request_snapshot(path: &Path) -> Option<(RequestSignature, Option<Vec<u8>>)> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let modified = metadata.modified().ok()?;
    #[cfg(unix)]
    let stat = {
        use std::os::unix::fs::MetadataExt;
        StoreSignature {
            dev: metadata.dev(),
            ino: metadata.ino(),
            modified,
            len: metadata.len(),
        }
    };
    #[cfg(not(unix))]
    let stat = StoreSignature {
        modified,
        len: metadata.len(),
    };
    if metadata.len() as usize > MAX_REQUEST_BYTES {
        return Some((RequestSignature { stat, digest: 0 }, None));
    }
    let mut file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Some((RequestSignature { stat, digest: 0 }, None));
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&bytes, &mut hasher);
    let digest = std::hash::Hasher::finish(&hasher);
    Some((RequestSignature { stat, digest }, Some(bytes)))
}

fn parse_request(bytes: &[u8]) -> Option<ReopenRequest> {
    let wire = serde_json::from_slice::<WireRequest>(bytes).ok()?;
    ReopenRequest::from_wire(wire).ok()
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

    #[test]
    fn request_lock_is_released_when_owner_process_exits() {
        if std::env::var_os("TSK_REOPEN_LOCK_CHILD").is_some() {
            let dir = PathBuf::from(std::env::var_os("TSK_REOPEN_LOCK_DIR").expect("lock dir"));
            let _lock = RequestLock::acquire(&dir).expect("child lock");
            std::process::exit(0);
        }
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-crash-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "reopen::tests::request_lock_is_released_when_owner_process_exits",
                "--nocapture",
            ])
            .env("TSK_REOPEN_LOCK_CHILD", "1")
            .env("TSK_REOPEN_LOCK_DIR", &dir)
            .output()
            .expect("child");
        assert!(
            output.status.success(),
            "child lock owner failed: {output:?}"
        );
        assert!(
            dir.join(LOCK_FILE).exists(),
            "the persistent lock inode must outlive a crashing owner"
        );
        assert!(
            RequestLock::acquire(&dir).is_ok(),
            "crash-released lock remains held"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn idle_poll_without_a_request_does_not_create_or_touch_a_lock() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-idle-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let store = TaskStore::new(&dir);
        let mut watch = ReopenWatch::seeded(&store);
        fs::remove_file(dir.join(LOCK_FILE)).expect("remove setup lock");
        assert_eq!(watch.poll(), None);
        assert!(
            !dir.join(LOCK_FILE).exists(),
            "idle poll created a lock file"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn idle_poll_reads_the_request_without_waiting_for_a_competing_lock() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-nonblocking-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let store = TaskStore::new(&dir);
        let mut watch = ReopenWatch::seeded(&store);
        ReopenRequest::new(Some(PathBuf::from("/repo/pending")))
            .write(&dir)
            .expect("request");
        let lock = RequestLock::acquire(&dir).expect("competing lock");
        let started = std::time::Instant::now();
        // The request on disk is whole (atomic rename), so a held writer lock neither
        // hides it nor delays it.
        let request = watch.poll().expect("request is readable under a held lock");
        assert_eq!(request.project.as_deref(), Some(Path::new("/repo/pending")));
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "idle poll must not wait for the writer's 500ms bounded acquisition"
        );
        drop(lock);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unchanged_pending_request_does_not_reopen_or_tighten_the_lock() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-unchanged-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let store = TaskStore::new(&dir);
        let mut watch = ReopenWatch::seeded(&store);
        ReopenRequest::new(Some(PathBuf::from("/repo/pending")))
            .write(&dir)
            .expect("request");
        let request = watch.poll().expect("pending request");
        fs::remove_file(dir.join(LOCK_FILE)).expect("remove lock");
        assert_eq!(watch.poll(), Some(request));
        assert!(
            !dir.join(LOCK_FILE).exists(),
            "unchanged pending request reopened the lock file"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn poll_pairs_the_post_lock_signature_with_the_request_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-signature-race-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let store = TaskStore::new(&dir);
        let mut watch = ReopenWatch::seeded(&store);
        ReopenRequest::new(Some(PathBuf::from("/repo/a")))
            .write(&dir)
            .expect("request a");
        let request = watch
            .poll_with_pre_lock(|| {
                ReopenRequest::new(Some(PathBuf::from("/repo/b")))
                    .write(&dir)
                    .expect("replace with b");
            })
            .expect("replacement request");
        assert_eq!(request.project.as_deref(), Some(Path::new("/repo/b")));
        watch.acknowledge();
        assert_eq!(watch.poll(), None, "acknowledged b must not replay");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn acknowledge_is_nonblocking_and_consumes_a_request_when_cleanup_is_busy() {
        let dir = std::env::temp_dir().join(format!(
            "tsk-reopen-ack-nonblocking-{}-{}",
            std::process::id(),
            REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("state dir");
        let store = TaskStore::new(&dir);
        let mut watch = ReopenWatch::seeded(&store);
        ReopenRequest::new(Some(PathBuf::from("/repo/a")))
            .write(&dir)
            .expect("request a");
        assert!(watch.poll().is_some(), "request applied");
        let lock = RequestLock::acquire(&dir).expect("competing cleanup lock");
        let started = std::time::Instant::now();
        watch.acknowledge();
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "acknowledgement must not wait for the writer's 500ms bounded acquisition"
        );
        drop(lock);
        assert_eq!(watch.poll(), None, "handled request must not replay");
        ReopenRequest::new(Some(PathBuf::from("/repo/b")))
            .write(&dir)
            .expect("newer request");
        assert_eq!(
            watch.poll().expect("newer request").project.as_deref(),
            Some(Path::new("/repo/b"))
        );
        let _ = fs::remove_dir_all(dir);
    }
}
