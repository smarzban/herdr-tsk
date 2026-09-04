//! Task Store: durable JSON under the plugin state dir.
//!
//! Atomic write: unique temp file in the same directory, then rename over the target.
//! Concurrent writers take an exclusive lock on `tsk.json.lock` and merge by
//! task id plus each record's revision, so writers do not drop sibling records.
//! A store whose `format_version` differs from this binary is refused rather than rewritten.
//! Each successful replace retains the previous document as `tsk.json.1`.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{DomainState, STORE_FORMAT_VERSION};

/// On-disk document name under the state directory.
const STATE_FILE: &str = "tsk.json";
/// Previous successful document, retained by hard-link before replace.
const BACKUP_FILE: &str = "tsk.json.1";
/// Inter-process exclusive lock file (sibling of the state document).
const LOCK_FILE: &str = "tsk.json.lock";

/// One document-format migration step.
type MigrationStep = fn(serde_json::Value) -> Result<serde_json::Value, StoreError>;

/// Document-format migrations: the index `i` step converts version `i + 1` to `i + 2`.
///
/// Empty while v1 is current; a later format lands its steps here and rides the
/// load/save seam already wired below.
const MIGRATIONS: &[MigrationStep] = &[];

/// Walk the shipped migration chain from `from` up to the current format.
fn migrate(document: serde_json::Value, from: u32) -> Result<serde_json::Value, StoreError> {
    migrate_with(document, from, MIGRATIONS)
}

/// Walk `steps` from `from`, applying the step at index `i` to a version `i + 1`
/// document. `migrate` fixes the chain to [`MIGRATIONS`]; tests inject fake steps.
/// Each applied step stamps the version it produces.
fn migrate_with(
    mut document: serde_json::Value,
    from: u32,
    steps: &[MigrationStep],
) -> Result<serde_json::Value, StoreError> {
    let start = usize::try_from(from.saturating_sub(1)).expect("u32 fits usize");
    let Some(steps) = steps.get(start..) else {
        return Err(StoreError::Io(io::Error::other(
            "migration chain is shorter than the document version",
        )));
    };
    for (offset, step) in steps.iter().enumerate() {
        document = step(document)?;
        let produced = from + offset as u32 + 1;
        document["format_version"] = serde_json::json!(produced);
    }
    Ok(document)
}

/// Filesystem operations that make atomic replacement durable.
///
/// Keeping the stages separate lets tests verify their ordering and failure propagation.
trait AtomicFilesystem {
    type File;

    fn create_file(&self, path: &Path) -> io::Result<Self::File>;
    fn write_all(&self, file: &mut Self::File, data: &[u8]) -> io::Result<()>;
    fn sync_file(&self, file: &Self::File) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

struct StdFilesystem;

impl AtomicFilesystem for StdFilesystem {
    type File = File;

    fn create_file(&self, path: &Path) -> io::Result<Self::File> {
        File::create(path)
    }

    fn write_all(&self, file: &mut Self::File, data: &[u8]) -> io::Result<()> {
        file.write_all(data)
    }

    fn sync_file(&self, file: &Self::File) -> io::Result<()> {
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        File::open(path)?.sync_all()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn sync_directory(&self, _path: &Path) -> io::Result<()> {
        // Platform fallback: directory synchronization is not claimed outside Linux/macOS.
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// Identity of the live document on disk, cheap to stat.
///
/// Every save renames a fresh temp file over the live one, so the inode changes on
/// every replace: two saves inside one mtime tick with equal length are still
/// distinguishable. On non-Unix platforms the inode is not claimed, so only the
/// mtime and length are carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(unix)]
pub struct StoreSignature {
    pub dev: u64,
    pub ino: u64,
    pub modified: SystemTime,
    pub len: u64,
}

/// Identity of the live document on disk, cheap to stat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(not(unix))]
pub struct StoreSignature {
    pub modified: SystemTime,
    pub len: u64,
}

/// Load/save `DomainState` as JSON under a state directory.
#[derive(Debug, Clone)]
pub struct TaskStore {
    path: PathBuf,
}

impl TaskStore {
    /// `path` is the plugin state directory (not the JSON file itself).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Live document path (`tsk.json` under the state directory).
    pub fn state_file(&self) -> PathBuf {
        self.path.join(STATE_FILE)
    }

    /// The live document's change signature. `None` when the file does not exist
    /// yet (a fresh, never-saved store) or its metadata could not be read.
    pub fn state_signature(&self) -> Option<StoreSignature> {
        let metadata = fs::metadata(self.state_file()).ok()?;
        let modified = metadata.modified().ok()?;
        let len = metadata.len();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(StoreSignature {
                dev: metadata.dev(),
                ino: metadata.ino(),
                modified,
                len,
            })
        }
        #[cfg(not(unix))]
        {
            Some(StoreSignature { modified, len })
        }
    }

    /// Load domain state. Missing file yields an empty state (first run).
    pub fn load(&self) -> Result<DomainState, StoreError> {
        let _guard = self.lock_exclusive()?;
        self.load_unlocked()
    }

    /// Persist domain state with atomic write (unique temp in same dir + rename).
    ///
    /// Takes the exclusive lock for the write. Prefer [`Self::reload_merge_save`] when
    /// another process may have written since this state was loaded.
    pub fn save(&self, state: &DomainState) -> Result<(), StoreError> {
        check_format_version(state.format_version())?;
        let _guard = self.lock_exclusive()?;
        let mut durable = state.clone();
        durable.assign_numbers_for_persistence();
        durable.clear_merge_bases();
        self.save_unlocked(&durable)
    }

    /// Hold the exclusive store lock across a state transition and its durable replacement.
    pub fn locked_transition<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<T, String>,
    ) -> Result<T, String> {
        let _guard = self.lock_exclusive().map_err(|error| error.to_string())?;
        let mut state = self.load_unlocked().map_err(|error| error.to_string())?;
        let result = transition(&mut state)?;
        state.assign_numbers_for_persistence();
        state.clear_merge_bases();
        self.save_unlocked(&state)
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    /// Hold the exclusive store lock across a transition that may not need a durable write.
    ///
    /// The transition returns its result and whether the loaded state changed. This lets
    /// idempotent callers check and create under one lock without rewriting an existing state.
    pub fn locked_transition_if_changed<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<(T, bool), String>,
    ) -> Result<T, String> {
        let _guard = self.lock_exclusive().map_err(|error| error.to_string())?;
        let mut state = self.load_unlocked().map_err(|error| error.to_string())?;
        let (result, changed) = transition(&mut state)?;
        if changed {
            state.assign_numbers_for_persistence();
            state.clear_merge_bases();
            self.save_unlocked(&state)
                .map_err(|error| error.to_string())?;
        }
        Ok(result)
    }

    /// Under exclusive lock: load disk, validate each local mutation against the revision it
    /// changed from, merge sibling records, then durably write. Divergent same-task writes are
    /// rejected rather than ordered by wall clock or silently overwritten.
    pub fn reload_merge_save(&self, local: &mut DomainState) -> Result<(), StoreError> {
        self.reload_merge_save_with(local, &StdFilesystem)
    }

    /// Internal filesystem seam for the merge-save durability boundary.
    ///
    /// The caller's state keeps its merge bases until the replacement succeeds, so Save
    /// Recovery can retry the same intended mutation after any write-stage failure.
    fn reload_merge_save_with<F: AtomicFilesystem>(
        &self,
        local: &mut DomainState,
        filesystem: &F,
    ) -> Result<(), StoreError> {
        check_format_version(local.format_version())?;
        let _guard = self.lock_exclusive()?;
        let disk = self.load_unlocked()?;
        check_format_version(disk.format_version())?;
        local
            .merge_for_save(&disk)
            .map_err(|message| StoreError::Io(io::Error::other(message)))?;
        let mut durable = local.clone();
        durable.assign_numbers_for_persistence();
        durable.clear_merge_bases();
        self.save_unlocked_with(&durable, filesystem)?;
        local.sync_numbers_from_persisted(&durable);
        local.clear_merge_bases();
        Ok(())
    }

    fn load_unlocked(&self) -> Result<DomainState, StoreError> {
        self.load_unlocked_supported(STORE_FORMAT_VERSION, migrate)
    }

    /// Load path parameterized for tests: `supported` is the target format and
    /// `migrations` walks an older document up to it. The live file is never
    /// rewritten by load; the migrated state exists in memory until the first save.
    fn load_unlocked_supported(
        &self,
        supported: u32,
        migrations: fn(serde_json::Value, u32) -> Result<serde_json::Value, StoreError>,
    ) -> Result<DomainState, StoreError> {
        self.sweep_orphan_temps();
        let file = self.state_file();
        if !file.exists() {
            return Ok(DomainState::new());
        }
        let data = fs::read_to_string(&file)?;
        let found = peek_format_version(&data)?;
        // Pre-versioned (0) and newer-than-supported documents are both refused.
        if found == 0 || found > supported {
            return Err(StoreError::UnsupportedFormat { found, supported });
        }
        if found < supported {
            let document: serde_json::Value = serde_json::from_str(&data)?;
            let migrated = migrations(document, found)?;
            return Ok(serde_json::from_value(migrated)?);
        }
        let state = serde_json::from_str(&data)?;
        Ok(state)
    }

    fn save_unlocked(&self, state: &DomainState) -> Result<(), StoreError> {
        self.save_unlocked_with(state, &StdFilesystem)
    }

    fn save_unlocked_with<F: AtomicFilesystem>(
        &self,
        state: &DomainState,
        filesystem: &F,
    ) -> Result<(), StoreError> {
        self.save_unlocked_supported(state, filesystem, STORE_FORMAT_VERSION)
    }

    /// Save path parameterized for tests: refuse an in-memory state or a live file
    /// above `supported`, but accept a live file below it after backing it up.
    fn save_unlocked_supported<F: AtomicFilesystem>(
        &self,
        state: &DomainState,
        filesystem: &F,
        supported: u32,
    ) -> Result<(), StoreError> {
        check_supported(state.format_version(), supported)?;
        fs::create_dir_all(&self.path)?;
        self.sweep_orphan_temps();
        let file = self.state_file();
        if file.exists() {
            match peek_format_version(&fs::read_to_string(&file)?) {
                Ok(0) => {
                    return Err(StoreError::UnsupportedFormat {
                        found: 0,
                        supported,
                    });
                }
                Ok(version) if version > supported => {
                    return Err(StoreError::UnsupportedFormat {
                        found: version,
                        supported,
                    });
                }
                Ok(version) => {
                    if version < supported {
                        retain_version_backup(&file, version)?;
                    }
                    retain_last_good(&file)?;
                }
                // A corrupt live document is replaced. Leave any last-good copy alone.
                Err(StoreError::Json(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let tmp = self.unique_tmp_path();
        let data = serde_json::to_string_pretty(state)?;
        // A save is successful only after the replacement and containing directory are synced.
        let write_result = (|| -> Result<(), StoreError> {
            let mut temp_file = filesystem.create_file(&tmp)?;
            filesystem.write_all(&mut temp_file, data.as_bytes())?;
            filesystem.sync_file(&temp_file)?;
            drop(temp_file);
            filesystem.rename(&tmp, &file)?;
            filesystem.sync_directory(&self.path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = filesystem.remove_file(&tmp);
        }
        write_result
    }

    /// Exclusive lock held for the duration of load-modify-save critical sections.
    fn lock_exclusive(&self) -> Result<StoreLockGuard, StoreError> {
        fs::create_dir_all(&self.path)?;
        let lock_path = self.path.join(LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        file.lock()?;
        Ok(StoreLockGuard { file })
    }

    /// Remove leftover `.tsk.json.tmp.*` files. Safe only under the exclusive lock.
    fn sweep_orphan_temps(&self) {
        let Ok(entries) = fs::read_dir(&self.path) else {
            return;
        };
        let prefix = format!(".{STATE_FILE}.tmp.");
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Unique temp path so concurrent writers never share `.tsk.json.tmp`.
    fn unique_tmp_path(&self) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.path.join(format!(".{STATE_FILE}.tmp.{pid}.{nanos}"))
    }
}

/// RAII exclusive lock on the store lock file (released on drop via `File::unlock`).
struct StoreLockGuard {
    file: File,
}

impl Drop for StoreLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// State dir from `TSK_STATE_DIR`, else `~/.tsk`.
///
/// One store everywhere: the herdr plugin pane and a bare terminal run resolve to the same
/// files, so there is exactly one board regardless of host. Herdr's injected
/// `HERDR_PLUGIN_STATE_DIR` is deliberately ignored — herdr documents plugin state as
/// plugin-owned ("it does not validate, sync, or delete their contents") and only recommends
/// the injected location. Empty values fall through. Never falls back to a shared world path
/// under `std::env::temp_dir()`.
pub fn default_state_dir() -> PathBuf {
    if let Some(dir) = env::var_os("TSK_STATE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Some(home) = env::var_os("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".tsk");
        }
    }
    // Last resort: relative per-process dir (still not shared /tmp/tsk-state).
    PathBuf::from(".tsk-state")
}

fn peek_format_version(data: &str) -> Result<u32, StoreError> {
    let value: serde_json::Value = serde_json::from_str(data)?;
    match value.get("format_version") {
        None => Ok(0),
        Some(version) => version
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| StoreError::Io(io::Error::other("format_version must be a u32"))),
    }
}

fn check_format_version(found: u32) -> Result<(), StoreError> {
    check_supported(found, STORE_FORMAT_VERSION)
}

fn check_supported(found: u32, supported: u32) -> Result<(), StoreError> {
    if found != supported {
        Err(StoreError::UnsupportedFormat { found, supported })
    } else {
        Ok(())
    }
}

/// Copy the live document to `tsk.json.v<version>` before the first save that
/// replaces a lower format version. Never overwrites an existing copy: the first
/// backup of a given version is the one that matters.
fn retain_version_backup(live: &Path, version: u32) -> Result<(), StoreError> {
    let backup = live.with_file_name(format!("{STATE_FILE}.v{version}"));
    if backup.exists() {
        return Ok(());
    }
    fs::copy(live, &backup)?;
    Ok(())
}

/// Keep the previous live document under `tsk.json.1` via hard-link so a crash
/// between link and rename leaves the backup identical to the still-live file.
fn retain_last_good(live: &Path) -> Result<(), StoreError> {
    let backup = live.with_file_name(BACKUP_FILE);
    match fs::remove_file(&backup) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::hard_link(live, &backup)?;
    Ok(())
}

/// Store I/O and JSON failures.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedFormat { found: u32, supported: u32 },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "store I/O error: {e}"),
            StoreError::Json(e) => write!(f, "store JSON error: {e}"),
            StoreError::UnsupportedFormat { found, supported } => write!(
                f,
                "store format {found} is unsupported by this tsk (expected {supported})"
            ),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(e) => Some(e),
            StoreError::Json(e) => Some(e),
            StoreError::UnsupportedFormat { .. } => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        StoreError::Io(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        StoreError::Json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ProvenanceOrigin, TaskScope};
    use std::sync::{Mutex, OnceLock};

    static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    /// Serialize env mutation across tests in this process.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        env::temp_dir().join(format!("tsk-store-{label}-{nanos}-{seq}"))
    }

    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SaveStage {
        Write,
        FileSync,
        Rename,
        DirectorySync,
    }

    struct RecordingFilesystem {
        events: std::cell::RefCell<Vec<SaveStage>>,
        fail_at: Option<SaveStage>,
    }

    impl RecordingFilesystem {
        fn new(fail_at: Option<SaveStage>) -> Self {
            Self {
                events: std::cell::RefCell::new(Vec::new()),
                fail_at,
            }
        }

        fn events(&self) -> Vec<SaveStage> {
            self.events.borrow().clone()
        }

        fn record(&self, stage: SaveStage) -> io::Result<()> {
            self.events.borrow_mut().push(stage);
            if self.fail_at == Some(stage) {
                return Err(io::Error::other("injected failure"));
            }
            Ok(())
        }
    }

    impl AtomicFilesystem for RecordingFilesystem {
        type File = ();

        fn create_file(&self, _path: &Path) -> io::Result<Self::File> {
            Ok(())
        }

        fn write_all(&self, _file: &mut Self::File, _data: &[u8]) -> io::Result<()> {
            self.record(SaveStage::Write)
        }

        fn sync_file(&self, _file: &Self::File) -> io::Result<()> {
            self.record(SaveStage::FileSync)
        }

        fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
            self.record(SaveStage::Rename)
        }

        fn sync_directory(&self, _path: &Path) -> io::Result<()> {
            self.record(SaveStage::DirectorySync)
        }

        fn remove_file(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn save_orders_durable_stages_and_propagates_every_stage_failure() {
        let dir = temp_dir("durability-order");
        fs::create_dir_all(&dir).expect("mkdir");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let expected = [
            SaveStage::Write,
            SaveStage::FileSync,
            SaveStage::Rename,
            SaveStage::DirectorySync,
        ];

        let filesystem = RecordingFilesystem::new(None);
        store
            .save_unlocked_with(&DomainState::new(), &filesystem)
            .expect("all durable stages succeed");
        assert_eq!(filesystem.events(), expected);

        for (index, stage) in expected.iter().copied().enumerate() {
            let filesystem = RecordingFilesystem::new(Some(stage));
            let error = store.save_unlocked_with(&DomainState::new(), &filesystem);
            assert!(
                matches!(error, Err(StoreError::Io(_))),
                "{stage:?} failure must be reported as a store I/O error"
            );
            assert_eq!(filesystem.events(), expected[..=index]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn save_reloads_from_the_real_filesystem() {
        let dir = temp_dir("durability-reload");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut state = DomainState::new();
        let id = state
            .create(
                "Durable task",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create task");

        store.save(&state).expect("save state");
        let reloaded = store.load().expect("reload state");
        assert_eq!(reloaded.get(id).expect("saved task").title, "Durable task");
    }

    #[test]
    fn reload_merge_save_retains_merge_bases_after_write_failure_for_retry() {
        let dir = temp_dir("merge-retry-after-write-failure");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut seed = DomainState::new();
        let id = seed
            .create(
                "original",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        store.save(&seed).expect("seed");

        let mut local = store.load().expect("load local");
        local.complete(id).expect("stage completion");
        assert!(
            local.get(id).expect("task").merge_base_revision.is_some(),
            "the staged mutation must retain its merge precondition"
        );

        let filesystem = RecordingFilesystem::new(Some(SaveStage::FileSync));
        assert!(
            store
                .reload_merge_save_with(&mut local, &filesystem)
                .is_err(),
            "the injected durable write failure must reach Save Recovery"
        );
        assert!(
            local.get(id).expect("task").merge_base_revision.is_some(),
            "a failed write must not discard the retry merge precondition"
        );

        store
            .reload_merge_save(&mut local)
            .expect("retry must persist the originally staged completion");
        assert_eq!(
            local.get(id).expect("local task").status,
            crate::domain::HumanStatus::Done
        );
        assert_eq!(
            store
                .load()
                .expect("reload")
                .get(id)
                .expect("saved task")
                .status,
            crate::domain::HumanStatus::Done
        );
    }

    #[test]
    fn reload_merge_save_keeps_disk_and_local_tasks() {
        let dir = temp_dir("merge");
        fs::create_dir_all(&dir).expect("mkdir");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);

        // Disk has task A.
        let mut disk_state = DomainState::new();
        let id_a = disk_state
            .create(
                "Task A",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create A");
        store.save(&disk_state).expect("seed disk");

        // Local writer only knows about task B (simulates concurrent create).
        let mut local = DomainState::new();
        let id_b = local
            .create(
                "Task B",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create B");

        store.reload_merge_save(&mut local).expect("merge-save");

        assert!(local.get(id_a).is_some(), "disk task A must survive merge");
        assert!(local.get(id_b).is_some(), "local task B must survive merge");
        assert_eq!(local.tasks().len(), 2);

        let reloaded = store.load().expect("reload");
        assert!(reloaded.get(id_a).is_some());
        assert!(reloaded.get(id_b).is_some());
        assert_eq!(reloaded.tasks().len(), 2);
    }

    #[test]
    fn reload_merge_save_accepts_mutation_when_disk_still_has_its_base_revision() {
        let dir = temp_dir("newer");
        fs::create_dir_all(&dir).expect("mkdir");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);

        let mut state = DomainState::new();
        let id = state
            .create(
                "Original",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        store.save(&state).expect("seed");

        // Disk side creates the revision the stale local writer will base its mutation on.
        let mut disk_side = store.load().expect("load");
        disk_side
            .edit(id, "From disk", None, TaskScope::Global, None)
            .expect("edit disk");
        store.save(&disk_side).expect("save disk edit");

        // Local side edits the loaded disk revision, recording that exact revision as its
        // save precondition instead of relying on wall-clock ordering.
        let mut local = store.load().expect("load local base");
        local
            .edit(id, "From local", None, TaskScope::Global, None)
            .expect("edit local");

        // The disk still has the base revision local changed from.
        store.reload_merge_save(&mut local).expect("merge");

        assert_eq!(local.get(id).expect("task").title, "From local");
    }

    #[test]
    fn reload_merge_save_rejects_divergent_same_task_without_wall_clock_arbitration() {
        let dir = temp_dir("revision-conflict");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut seed = DomainState::new();
        let id = seed
            .create(
                "original",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .unwrap();
        store.save(&seed).unwrap();
        let mut local = store.load().unwrap();
        let mut concurrent = store.load().unwrap();
        local
            .edit(id, "local", None, TaskScope::Global, None)
            .unwrap();
        concurrent
            .edit(id, "concurrent", None, TaskScope::Global, None)
            .unwrap();
        store.save(&concurrent).unwrap();

        let error = store.reload_merge_save(&mut local).unwrap_err();

        assert!(error.to_string().contains("changed during save"));
        assert_eq!(store.load().unwrap().get(id).unwrap().title, "concurrent");
    }

    #[test]
    fn reload_merge_save_keeps_distinct_concurrent_undo_entries() {
        let dir = temp_dir("undo-merge");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut seed = DomainState::new();
        let first = seed
            .create(
                "first",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .unwrap();
        let second = seed
            .create(
                "second",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .unwrap();
        store.save(&seed).unwrap();

        let mut disk_writer = store.load().unwrap();
        disk_writer.complete(first).unwrap();
        store.save(&disk_writer).unwrap();

        let mut stale_writer = seed.clone();
        stale_writer.soft_delete(second).unwrap();
        store.reload_merge_save(&mut stale_writer).unwrap();

        let mut merged = store.load().unwrap();
        merged.undo().unwrap();
        assert_eq!(
            merged.get(first).unwrap().status,
            crate::domain::HumanStatus::Ready
        );
        merged.undo().unwrap();
        assert!(!merged.get(second).unwrap().soft_deleted);
    }

    #[test]
    fn default_state_dir_respects_env_and_avoids_shared_tmp() {
        let _lock = env_lock();
        let marker = temp_dir("state-env");
        fs::create_dir_all(&marker).expect("mkdir");
        let _guard = TempDirGuard(marker.clone());

        // When set, the override wins.
        // SAFETY: single-threaded under ENV_LOCK for the duration of this test.
        env::set_var("TSK_STATE_DIR", &marker);
        assert_eq!(default_state_dir(), marker);
        env::remove_var("TSK_STATE_DIR");

        // Host injection is deliberately ignored: herdr documents plugin state as
        // plugin-owned, and one store must stay one store under every host.
        env::set_var("HERDR_PLUGIN_STATE_DIR", "/herdr/injected");
        let ignored = default_state_dir();
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        assert_ne!(
            ignored,
            PathBuf::from("/herdr/injected"),
            "injected host state dirs must not fragment the single store"
        );
        env::set_var("TSK_STATE_DIR", "");
        assert_ne!(
            default_state_dir(),
            PathBuf::from(""),
            "empty falls through"
        );
        env::remove_var("TSK_STATE_DIR");

        let fallback = default_state_dir();
        let shared_tmp = env::temp_dir().join("tsk-state");
        assert_ne!(
            fallback, shared_tmp,
            "fallback must not be shared /tmp/tsk-state"
        );
        // Prefer XDG/HOME style paths when available.
        let fallback_s = fallback.to_string_lossy();
        assert!(
            fallback_s.contains("tsk") || fallback_s == ".tsk-state",
            "unexpected fallback: {fallback_s}"
        );
    }

    fn write_state_json(dir: &Path, value: serde_json::Value) {
        fs::create_dir_all(dir).expect("mkdir");
        fs::write(
            dir.join(STATE_FILE),
            serde_json::to_vec_pretty(&value).expect("json"),
        )
        .expect("write store");
    }

    #[test]
    fn save_emits_format_version_one() {
        let dir = temp_dir("format-stamp");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).expect("save");

        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(STATE_FILE)).expect("read"))
                .expect("json");
        assert_eq!(value["format_version"], 1);
    }

    #[test]
    fn load_refuses_missing_format_without_rewriting() {
        let dir = temp_dir("format-missing");
        let _guard = TempDirGuard(dir.clone());
        let legacy = serde_json::json!({ "tasks": [], "undo_stack": [] });
        write_state_json(&dir, legacy.clone());

        let error = TaskStore::new(&dir)
            .load()
            .expect_err("missing format must refuse");
        assert!(matches!(
            error,
            StoreError::UnsupportedFormat {
                found: 0,
                supported: 1
            }
        ));
        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(STATE_FILE)).expect("read"))
                .expect("json");
        assert_eq!(on_disk, legacy);
    }

    #[test]
    fn load_refuses_noncurrent_format_without_rewriting() {
        for format_version in [0, 2] {
            let dir = temp_dir("format-noncurrent");
            let _guard = TempDirGuard(dir.clone());
            let document = serde_json::json!({
                "format_version": format_version,
                "tasks": [],
                "undo_stack": []
            });
            write_state_json(&dir, document.clone());

            let error = TaskStore::new(&dir)
                .load()
                .expect_err("noncurrent format must refuse");
            assert!(
                matches!(
                    error,
                    StoreError::UnsupportedFormat {
                        found,
                        supported: 1
                    } if found == format_version
                ),
                "{error}"
            );
            let on_disk: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(dir.join(STATE_FILE)).expect("read"))
                    .expect("json");
            assert_eq!(on_disk, document);
        }
    }

    #[test]
    fn save_refuses_noncurrent_in_memory_state_without_writing() {
        let dir = temp_dir("format-save-state");
        let _guard = TempDirGuard(dir.clone());
        let state: DomainState = serde_json::from_value(serde_json::json!({
            "format_version": 2,
            "next_task_number": 1,
            "tasks": [],
            "undo_stack": []
        }))
        .expect("shape is otherwise valid");

        let error = TaskStore::new(&dir)
            .save(&state)
            .expect_err("noncurrent state must refuse");
        assert!(matches!(
            error,
            StoreError::UnsupportedFormat {
                found: 2,
                supported: 1
            }
        ));
        assert!(!dir.join(STATE_FILE).exists());
    }

    #[test]
    fn reload_merge_save_refuses_noncurrent_local_state_without_rewriting() {
        let dir = temp_dir("format-merge-local");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).expect("seed v1 store");
        let before = fs::read(dir.join(STATE_FILE)).expect("read v1 store");
        let mut local: DomainState = serde_json::from_value(serde_json::json!({
            "format_version": 2,
            "next_task_number": 1,
            "tasks": [],
            "undo_stack": []
        }))
        .expect("shape is otherwise valid");

        let error = store
            .reload_merge_save(&mut local)
            .expect_err("noncurrent local state must refuse");
        assert!(matches!(
            error,
            StoreError::UnsupportedFormat {
                found: 2,
                supported: 1
            }
        ));
        assert_eq!(
            fs::read(dir.join(STATE_FILE)).expect("read unchanged"),
            before
        );
    }

    #[test]
    fn save_refuses_newer_format_and_leaves_the_document() {
        let dir = temp_dir("format-save-newer");
        let _guard = TempDirGuard(dir.clone());
        let newer = serde_json::json!({
            "format_version": 3,
            "tasks": [],
            "undo_stack": []
        });
        write_state_json(&dir, newer.clone());

        let error = TaskStore::new(&dir)
            .save(&DomainState::new())
            .expect_err("must not overwrite a newer store");
        assert!(
            matches!(
                error,
                StoreError::UnsupportedFormat {
                    found: 3,
                    supported: 1
                }
            ),
            "{error}"
        );
        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(STATE_FILE)).expect("read"))
                .expect("json");
        assert_eq!(on_disk, newer);
    }

    #[test]
    fn second_save_hard_links_the_previous_document() {
        let dir = temp_dir("last-good");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut first = DomainState::new();
        first
            .create(
                "first",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        store.save(&first).expect("first save");
        assert!(
            !dir.join("tsk.json.1").exists(),
            "the first save has no previous document to retain"
        );

        let second = DomainState::new();
        store.save(&second).expect("second save");

        let backup: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("tsk.json.1")).expect("read backup"))
                .expect("json");
        assert_eq!(backup["tasks"][0]["title"], "first");
        assert!(
            !dir.join("tasks.json.1").exists(),
            "the pre-rebrand last-good name must not be written"
        );
        let live = store.load().expect("load live");
        assert!(live.tasks().is_empty());
    }

    #[test]
    fn corrupt_live_document_does_not_replace_the_last_good_copy() {
        let dir = temp_dir("corrupt-keeps-backup");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut first = DomainState::new();
        first
            .create(
                "keep me",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        store.save(&first).expect("first save");
        store.save(&DomainState::new()).expect("second save");

        fs::write(dir.join(STATE_FILE), b"not valid json").expect("corrupt live");
        store
            .save(&DomainState::new())
            .expect("replace the corrupt live document");

        let backup: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("tsk.json.1")).expect("read backup"))
                .expect("backup must stay valid json");
        assert_eq!(backup["tasks"][0]["title"], "keep me");
    }

    #[test]
    fn save_sweeps_orphan_temp_files_under_the_lock() {
        let dir = temp_dir("orphan-sweep");
        let _guard = TempDirGuard(dir.clone());
        fs::create_dir_all(&dir).expect("mkdir");
        let orphan = dir.join(".tsk.json.tmp.1.1");
        fs::write(&orphan, b"stale").expect("seed orphan");

        TaskStore::new(&dir)
            .save(&DomainState::new())
            .expect("save");

        assert!(
            !orphan.exists(),
            "a locked save must remove leftover temp files"
        );
    }

    #[test]
    fn save_writes_tsk_json_and_not_tasks_json() {
        let dir = temp_dir("live-name");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        store.save(&DomainState::new()).expect("save");
        assert!(
            dir.join("tsk.json").exists(),
            "the live document is tsk.json"
        );
        assert!(
            !dir.join("tasks.json").exists(),
            "the pre-rebrand name must not be written"
        );
    }

    #[test]
    fn save_creates_literal_tsk_json_lock() {
        let dir = temp_dir("lock-name");
        let _guard = TempDirGuard(dir.clone());
        TaskStore::new(&dir)
            .save(&DomainState::new())
            .expect("save");
        assert!(
            dir.join("tsk.json.lock").exists(),
            "the lock file is tsk.json.lock"
        );
        assert!(
            !dir.join("tasks.json.lock").exists(),
            "the pre-rebrand lock name must not be written"
        );
    }

    #[cfg(unix)]
    #[test]
    fn signature_distinguishes_same_mtime_same_length_replaces() {
        let dir = temp_dir("signature-inode");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let state = round_tripped_v1_state("same bytes");
        store.save(&state).expect("first save");
        let first = store.state_signature().expect("first signature");

        store
            .save(&state)
            .expect("second save, byte-identical document");
        // Force the replaced file's mtime back to the first save's tick: the old
        // (mtime, len) signature can no longer tell these two saves apart.
        let file = File::options()
            .write(true)
            .open(store.state_file())
            .expect("open live document");
        file.set_modified(first.modified)
            .expect("force equal mtime");
        drop(file);

        let second = store.state_signature().expect("second signature");
        assert_eq!(
            first.len, second.len,
            "identical documents, identical length"
        );
        assert_eq!(first.modified, second.modified, "mtime forced equal");
        assert_ne!(
            first.ino, second.ino,
            "rename replace allocates a new inode"
        );
        assert_ne!(
            first, second,
            "the signature must distinguish the two saves"
        );
    }

    #[test]
    fn signature_missing_file_is_none_and_unchanged_file_is_stable() {
        let dir = temp_dir("signature-stable");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        assert_eq!(
            store.state_signature(),
            None,
            "a store that never saved has no live document"
        );

        store.save(&DomainState::new()).expect("save");
        let first = store.state_signature().expect("signature after save");
        let second = store.state_signature().expect("signature again");
        assert_eq!(first, second, "an unchanged file reads equal twice");
    }

    fn round_tripped_v1_state(title: &str) -> DomainState {
        let mut state = DomainState::new();
        state
            .create(
                title,
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create task");
        state
    }

    fn dir_listing_with_bytes(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir).expect("read dir").flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                fs::read(entry.path()).expect("read entry")
            } else {
                Vec::new()
            };
            entries.push((name, bytes));
        }
        entries.sort();
        entries
    }

    #[test]
    fn v1_document_round_trips_byte_identical_for_an_unchanged_state() {
        let dir = temp_dir("round-trip-bytes");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let state = round_tripped_v1_state("unchanged");
        store.save(&state).expect("first save");
        let first_bytes = fs::read(dir.join(STATE_FILE)).expect("read first");

        let loaded = store.load().expect("load");
        store.save(&loaded).expect("save loaded state");

        assert_eq!(
            fs::read(dir.join(STATE_FILE)).expect("read second"),
            first_bytes,
            "an unchanged v1 state must serialize byte-identically"
        );
    }

    /// Injected v1 -> v2 step for the migration seam tests.
    fn identity_v1_to_v2(document: serde_json::Value) -> Result<serde_json::Value, StoreError> {
        Ok(document)
    }

    #[test]
    fn migrate_with_walks_the_chain_from_the_given_version() {
        fn add_marker_one(document: serde_json::Value) -> Result<serde_json::Value, StoreError> {
            let mut document = document;
            document["marker_one"] = serde_json::json!(true);
            Ok(document)
        }
        fn add_marker_two(document: serde_json::Value) -> Result<serde_json::Value, StoreError> {
            let mut document = document;
            document["marker_two"] = serde_json::json!(true);
            Ok(document)
        }
        let steps: &[MigrationStep] = &[add_marker_one, add_marker_two];
        let base = serde_json::json!({ "format_version": 1 });

        let from_one = migrate_with(base.clone(), 1, steps).expect("migrate from v1");
        assert_eq!(from_one["format_version"], 3);
        assert_eq!(from_one["marker_one"], true);
        assert_eq!(from_one["marker_two"], true);

        let from_two = migrate_with(base.clone(), 2, steps).expect("migrate from v2");
        assert_eq!(from_two["format_version"], 3);
        assert!(from_two.get("marker_one").is_none(), "v1 step must not run");
        assert_eq!(from_two["marker_two"], true);

        let at_three = serde_json::json!({ "format_version": 3 });
        let from_three = migrate_with(at_three, 3, steps).expect("migrate from v3");
        assert_eq!(from_three["format_version"], 3);
        assert!(
            from_three.get("marker_one").is_none() && from_three.get("marker_two").is_none(),
            "no step runs when the document is already past the chain"
        );

        let failing: &[MigrationStep] = &[|_| Err(StoreError::Io(io::Error::other("boom")))];
        assert!(
            migrate_with(serde_json::json!({"format_version": 1}), 1, failing).is_err(),
            "a failing step must surface its error"
        );
    }

    #[test]
    fn migrated_load_backs_up_the_original_before_the_first_higher_version_save() {
        let dir = temp_dir("migrated-load-save");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let original = round_tripped_v1_state("migrate me");
        let original_bytes = serde_json::to_vec_pretty(&original).expect("encode v1");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(STATE_FILE), &original_bytes).expect("seed v1 file");

        let mut state = store
            .load_unlocked_supported(2, |document, from| {
                migrate_with(document, from, &[identity_v1_to_v2])
            })
            .expect("v1 file loads through the injected v1 -> v2 step");
        assert_eq!(state.format_version(), 2);
        assert_eq!(
            state.tasks()[0].title,
            "migrate me",
            "migration must preserve the task content"
        );

        store
            .save_unlocked_supported(&state, &StdFilesystem, 2)
            .expect("first save at v2");
        assert_eq!(
            fs::read(dir.join("tsk.json.v1")).expect("read version backup"),
            original_bytes,
            "the first backup of the pre-migration version is byte-identical"
        );
        let live: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(STATE_FILE)).expect("read live"))
                .expect("json");
        assert_eq!(live["format_version"], 2);
        assert_eq!(live["tasks"][0]["title"], "migrate me");
        assert_eq!(
            fs::read(dir.join("tsk.json.1")).expect("last-good holds the v1 original"),
            original_bytes,
            "tsk.json.1 semantics are unchanged"
        );

        state
            .create(
                "second task",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        store
            .save_unlocked_supported(&state, &StdFilesystem, 2)
            .expect("second save at v2");
        assert_eq!(
            fs::read(dir.join("tsk.json.v1")).expect("read version backup"),
            original_bytes,
            "a second save must not touch tsk.json.v1"
        );
    }

    #[test]
    fn save_never_overwrites_an_existing_version_backup() {
        let dir = temp_dir("version-backup-kept");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let original = round_tripped_v1_state("first backup wins");
        let original_bytes = serde_json::to_vec_pretty(&original).expect("encode v1");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(STATE_FILE), &original_bytes).expect("seed v1 file");
        fs::write(dir.join("tsk.json.v1"), b"existing backup").expect("seed existing backup");

        let state = store
            .load_unlocked_supported(2, |document, from| {
                migrate_with(document, from, &[identity_v1_to_v2])
            })
            .expect("load");
        store
            .save_unlocked_supported(&state, &StdFilesystem, 2)
            .expect("save");
        assert_eq!(
            fs::read(dir.join("tsk.json.v1")).expect("read backup"),
            b"existing backup",
            "the first backup of a given version is the one that matters"
        );
    }

    #[test]
    fn load_refuses_a_higher_version_and_changes_nothing_in_the_state_dir() {
        for (format_version, supported) in [(3u32, 1u32), (3, 2)] {
            let dir = temp_dir("format-higher");
            let _guard = TempDirGuard(dir.clone());
            let document = serde_json::json!({
                "format_version": format_version,
                "tasks": [],
                "undo_stack": []
            });
            write_state_json(&dir, document.clone());
            fs::write(dir.join("tsk.json.1"), b"bystander").expect("bystander file");
            // Pre-create the lock plumbing so the listing comparison only sees store content:
            // lock_exclusive creates tsk.json.lock on the first locked load.
            fs::write(dir.join("tsk.json.lock"), []).expect("pre-create lock file");
            let before = dir_listing_with_bytes(&dir);

            let error = TaskStore::new(&dir)
                .load()
                .expect_err("a newer store must be refused");
            assert!(
                matches!(
                    error,
                    StoreError::UnsupportedFormat { found, supported: 1 } if found == format_version
                ),
                "{error}"
            );

            let error = TaskStore::new(&dir)
                .load_unlocked_supported(supported, |document, from| {
                    migrate_with(document, from, &[])
                })
                .expect_err("a version above the seam target must also be refused");
            assert!(
                matches!(
                    error,
                    StoreError::UnsupportedFormat { found, supported: seam } if found == format_version && seam == supported
                ),
                "{error}"
            );

            assert_eq!(
                dir_listing_with_bytes(&dir),
                before,
                "no file in the state dir may change on a refused load"
            );
        }
    }

    #[test]
    fn failed_migration_step_surfaces_from_load_without_file_changes() {
        fn failing_step(_: serde_json::Value) -> Result<serde_json::Value, StoreError> {
            Err(StoreError::Io(io::Error::other("migration exploded")))
        }
        let dir = temp_dir("migration-fails");
        let _guard = TempDirGuard(dir.clone());
        let original = round_tripped_v1_state("fragile");
        let original_bytes = serde_json::to_vec_pretty(&original).expect("encode v1");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(STATE_FILE), &original_bytes).expect("seed v1 file");
        let before = dir_listing_with_bytes(&dir);

        let error = TaskStore::new(&dir)
            .load_unlocked_supported(2, |document, from| {
                migrate_with(document, from, &[failing_step])
            })
            .expect_err("a failing migration step must fail the load");
        assert!(error.to_string().contains("migration exploded"), "{error}");
        assert_eq!(
            dir_listing_with_bytes(&dir),
            before,
            "a failed migration must not change any file"
        );
    }

    #[test]
    fn load_returns_empty_when_only_legacy_tasks_json_exists() {
        let dir = temp_dir("legacy-filename");
        let _guard = TempDirGuard(dir.clone());
        fs::create_dir_all(&dir).expect("mkdir");
        let mut leftover = DomainState::new();
        leftover
            .create(
                "only in leftover tasks.json",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create");
        let payload = serde_json::to_vec_pretty(&leftover).expect("json");
        let legacy = dir.join("tasks.json");
        fs::write(&legacy, &payload).expect("write leftover");

        let loaded = TaskStore::new(&dir)
            .load()
            .expect("missing tsk.json is a first run");
        assert!(
            loaded.tasks().is_empty(),
            "leftover tasks.json must not be read"
        );
        assert!(
            !dir.join("tsk.json").exists(),
            "load must not promote the leftover file"
        );
        assert_eq!(
            fs::read(&legacy).expect("read leftover"),
            payload,
            "leftover tasks.json must be left untouched"
        );
    }
}
