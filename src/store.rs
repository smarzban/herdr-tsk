//! Task Store: durable JSON under the plugin state dir.
//!
//! Atomic write: unique temp file in the same directory, then rename over the target.
//! Concurrent writers take an exclusive lock on `tasks.json.lock` and merge by
//! task/attempt id plus each record's `updated_at`, so writers do not drop sibling records.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

use crate::domain::DomainState;

/// On-disk document name under the state directory.
const STATE_FILE: &str = "tasks.json";
/// Inter-process exclusive lock file (sibling of the state document).
const LOCK_FILE: &str = "tasks.json.lock";

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

/// Persistence boundary used by dispatch recovery operations.
///
/// `TaskStore` is the production implementation. The trait permits deterministic failure
/// injection at the receipt-save boundary without weakening the production store's atomic write.
pub trait TaskStateStore {
    fn load(&self) -> Result<DomainState, StoreError>;
    fn save(&self, state: &DomainState) -> Result<(), StoreError>;

    /// Hold the store's record lock across load, one state transition, and save.
    ///
    /// This is the boundary for transitions that must not race another process.
    fn locked_transition<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<T, String>,
    ) -> Result<T, String>;
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
        let _guard = self.lock_exclusive()?;
        let mut durable = state.clone();
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
        let _guard = self.lock_exclusive()?;
        let disk = self.load_unlocked()?;
        local
            .merge_for_save(&disk)
            .map_err(|message| StoreError::Io(io::Error::other(message)))?;
        let mut durable = local.clone();
        durable.clear_merge_bases();
        self.save_unlocked_with(&durable, filesystem)?;
        local.clear_merge_bases();
        Ok(())
    }

    fn load_unlocked(&self) -> Result<DomainState, StoreError> {
        let file = self.state_file();
        if !file.exists() {
            return Ok(DomainState::new());
        }
        let data = fs::read_to_string(&file)?;
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
        fs::create_dir_all(&self.path)?;
        let file = self.state_file();
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
        file.lock_exclusive()?;
        Ok(StoreLockGuard { file })
    }

    fn state_file(&self) -> PathBuf {
        self.path.join(STATE_FILE)
    }

    /// Unique temp path so concurrent writers never share `.tasks.json.tmp`.
    fn unique_tmp_path(&self) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.path.join(format!(".{STATE_FILE}.tmp.{pid}.{nanos}"))
    }
}

impl TaskStateStore for TaskStore {
    fn load(&self) -> Result<DomainState, StoreError> {
        Self::load(self)
    }

    fn save(&self, state: &DomainState) -> Result<(), StoreError> {
        Self::save(self, state)
    }

    fn locked_transition<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<T, String>,
    ) -> Result<T, String> {
        Self::locked_transition(self, transition)
    }
}

/// RAII exclusive lock on the store lock file (released on drop via fs2 unlock).
struct StoreLockGuard {
    file: File,
}

impl Drop for StoreLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// State dir from `HERDR_PLUGIN_STATE_DIR`, else a per-user data directory.
///
/// Production herdr injects `HERDR_PLUGIN_STATE_DIR`. When unset (manual runs / tests
/// without the host), use `$XDG_DATA_HOME/herdr-tasks` or `$HOME/.local/share/herdr-tasks`.
/// Never falls back to a shared world path under `std::env::temp_dir()`.
pub fn default_state_dir() -> PathBuf {
    if let Some(dir) = env::var_os("HERDR_PLUGIN_STATE_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("herdr-tasks");
        }
    }
    if let Some(home) = env::var_os("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".local/share/herdr-tasks");
        }
    }
    // Last resort: relative per-process dir (still not shared /tmp/herdr-tasks-state).
    PathBuf::from(".herdr-tasks-state")
}

/// Store I/O and JSON failures.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "store I/O error: {e}"),
            StoreError::Json(e) => write!(f, "store JSON error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(e) => Some(e),
            StoreError::Json(e) => Some(e),
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
        env::temp_dir().join(format!("herdr-tasks-store-{label}-{nanos}-{seq}"))
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
    fn reload_merge_save_preserves_first_legacy_task_mutation() {
        let dir = temp_dir("legacy-first-mutation");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);
        let mut current = DomainState::new();
        let id = current
            .create(
                "original",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create");

        let mut legacy_json = serde_json::to_value(current).expect("serialize task");
        legacy_json["tasks"][0]
            .as_object_mut()
            .expect("task object")
            .remove("revision");
        let legacy: DomainState = serde_json::from_value(legacy_json).expect("legacy task");
        store.save(&legacy).expect("seed legacy task");

        let mut local = store.load().expect("load legacy task");
        local
            .edit(id, "first mutation", None, TaskScope::Global, None)
            .expect("edit legacy task");
        store
            .reload_merge_save(&mut local)
            .expect("merge-save legacy edit");

        assert_eq!(local.get(id).expect("local task").title, "first mutation");
        assert_eq!(
            store
                .load()
                .expect("reloaded task")
                .get(id)
                .expect("task")
                .title,
            "first mutation"
        );
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
                None,
                None,
                ProvenanceOrigin::Manual,
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
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .unwrap();
        let second = seed
            .create(
                "second",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
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

        // When set, env wins.
        // SAFETY: single-threaded under ENV_LOCK for the duration of this test.
        env::set_var("HERDR_PLUGIN_STATE_DIR", &marker);
        assert_eq!(default_state_dir(), marker);
        env::remove_var("HERDR_PLUGIN_STATE_DIR");

        let fallback = default_state_dir();
        let shared_tmp = env::temp_dir().join("herdr-tasks-state");
        assert_ne!(
            fallback, shared_tmp,
            "fallback must not be shared /tmp/herdr-tasks-state"
        );
        // Prefer XDG/HOME style paths when available.
        let fallback_s = fallback.to_string_lossy();
        assert!(
            fallback_s.contains("herdr-tasks") || fallback_s == ".herdr-tasks-state",
            "unexpected fallback: {fallback_s}"
        );
    }
}
