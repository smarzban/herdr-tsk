//! Undo stack for soft-delete and complete.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainError, DomainState};

/// Maximum undo entries retained in a saved document.
pub const UNDO_CAP: usize = 50;

/// One reversible user action on the undo stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoEntry {
    SoftDelete { id: Uuid, expected_revision: Uuid },
    Complete { id: Uuid, expected_revision: Uuid },
}

impl UndoEntry {
    pub(crate) fn target(&self) -> (Uuid, Uuid) {
        match *self {
            UndoEntry::SoftDelete {
                id,
                expected_revision,
            }
            | UndoEntry::Complete {
                id,
                expected_revision,
            } => (id, expected_revision),
        }
    }
}

impl DomainState {
    /// Apply the inverse of the last undo entry when its target revision still matches.
    ///
    /// Empty stack is a documented no-op. Stale entries are retained so a refused Undo never
    /// changes durable state or exposes an older entry accidentally.
    pub fn undo(&mut self) -> Result<(), DomainError> {
        let Some(entry) = self.last_undo().cloned() else {
            return Ok(());
        };
        let (id, expected_revision) = entry.target();
        let current_revision = self.get(id).ok_or(DomainError::UnknownId(id))?.revision;
        if current_revision != expected_revision {
            return Err(DomainError::StaleUndo(id));
        }

        let entry = self
            .pop_undo()
            .expect("undo entry remains present after revision check");
        match entry {
            UndoEntry::SoftDelete { id, .. } => self.restore(id),
            UndoEntry::Complete { id, .. } => self.reopen(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{HumanStatus, ProvenanceOrigin, TaskScope};

    static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_state_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let seq = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tsk-undo-cap-{label}-{nanos}-{seq}"));
        std::fs::create_dir_all(&dir).expect("create temp state dir");
        dir
    }

    struct TempDirGuard(std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn create_sample(state: &mut DomainState) -> Uuid {
        state
            .create(
                "Fix flake",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("valid title creates a task")
    }

    fn persisted_undo_len(state: &DomainState) -> usize {
        serde_json::to_value(state).expect("state serializes")["undo_stack"]
            .as_array()
            .expect("undo_stack is an array")
            .len()
    }

    #[test]
    fn undo_cap_is_fifty() {
        assert_eq!(UNDO_CAP, 50);
    }

    #[test]
    fn save_keeps_exactly_fifty_undo_entries_and_evicts_the_oldest() {
        let dir = temp_state_dir("cap");
        let _guard = TempDirGuard(dir.clone());
        let store = crate::store::TaskStore::new(&dir);
        let mut state = DomainState::new();
        let mut ids = Vec::new();
        for index in 0..=UNDO_CAP {
            let id = state
                .create(
                    format!("task {index}"),
                    None,
                    TaskScope::Global,
                    ProvenanceOrigin::Manual,
                    None,
                )
                .expect("create");
            state.complete(id).expect("complete");
            ids.push(id);
        }
        assert_eq!(persisted_undo_len(&state), UNDO_CAP + 1);

        store.save(&state).expect("save");
        let mut loaded = store.load().expect("reload");
        assert_eq!(
            persisted_undo_len(&loaded),
            UNDO_CAP,
            "the saved document carries exactly UNDO_CAP entries"
        );

        for _ in 0..UNDO_CAP {
            loaded.undo().expect("undo a kept entry");
        }
        assert_eq!(
            loaded.get(ids[0]).expect("task 0").status,
            HumanStatus::Done,
            "the evicted oldest entry can no longer undo its task"
        );
        assert_eq!(
            loaded.get(ids[1]).expect("task 1").status,
            HumanStatus::Ready,
            "the oldest kept entry still undoes its task"
        );
    }

    #[test]
    fn save_drops_stale_undo_entries_and_keeps_live_ones_beneath_them() {
        let dir = temp_state_dir("stale");
        let _guard = TempDirGuard(dir.clone());
        let store = crate::store::TaskStore::new(&dir);
        let mut state = DomainState::new();
        let stale_target = create_sample(&mut state);
        let live_target = create_sample(&mut state);
        state.complete(stale_target).expect("complete");
        state
            .edit(
                stale_target,
                "edited after the undoable action",
                None,
                TaskScope::Global,
                None,
            )
            .expect("edit moves the revision on");
        state.complete(live_target).expect("complete");
        assert_eq!(persisted_undo_len(&state), 2);

        store.save(&state).expect("save");
        let mut loaded = store.load().expect("reload");
        assert_eq!(
            persisted_undo_len(&loaded),
            1,
            "the stale entry is absent from the saved document"
        );
        loaded.undo().expect("undo the kept entry");
        assert_eq!(
            loaded.get(live_target).expect("live task").status,
            HumanStatus::Ready,
            "the live entry beneath the stale one is kept"
        );
        assert_eq!(
            loaded.get(stale_target).expect("stale task").status,
            HumanStatus::Done,
            "the stale entry's task is untouched"
        );
    }

    #[test]
    fn stale_top_entry_still_refuses_in_memory_because_pruning_runs_only_at_save() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.complete(id).expect("complete");
        state
            .edit(id, "changed", None, TaskScope::Global, None)
            .expect("edit");

        let error = state.undo().expect_err("a stale top entry refuses");
        assert_eq!(error, DomainError::StaleUndo(id));
        assert_eq!(
            persisted_undo_len(&state),
            1,
            "a refused undo retains the entry in memory"
        );
    }

    #[test]
    fn prune_runs_before_the_cap_so_a_dead_entry_never_evicts_a_live_one() {
        let dir = temp_state_dir("order");
        let _guard = TempDirGuard(dir.clone());
        let store = crate::store::TaskStore::new(&dir);
        let mut state = DomainState::new();
        let mut ids = Vec::new();
        for index in 0..=UNDO_CAP {
            let id = state
                .create(
                    format!("task {index}"),
                    None,
                    TaskScope::Global,
                    ProvenanceOrigin::Manual,
                    None,
                )
                .expect("create");
            state.complete(id).expect("complete");
            ids.push(id);
        }
        // Task 25's entry goes stale: 51 entries, one stale in the middle.
        state
            .edit(ids[25], "moved on", None, TaskScope::Global, None)
            .expect("edit");

        store.save(&state).expect("save");
        let mut loaded = store.load().expect("reload");
        assert_eq!(
            persisted_undo_len(&loaded),
            UNDO_CAP,
            "prune-first drops only the stale entry; the cap then does nothing"
        );
        for _ in 0..UNDO_CAP {
            loaded.undo().expect("every kept entry undoes");
        }
        assert_eq!(
            loaded.get(ids[0]).expect("task 0").status,
            HumanStatus::Ready,
            "a cap-first prune would have evicted this live oldest entry"
        );
    }

    #[test]
    fn cap_runs_after_merge_undo_entries_union() {
        let dir = temp_state_dir("merge-then-cap");
        let _guard = TempDirGuard(dir.clone());
        let store = crate::store::TaskStore::new(&dir);
        // Disk side: UNDO_CAP/2 completions saved by one writer.
        let mut disk_side = DomainState::new();
        for index in 0..UNDO_CAP / 2 {
            let id = disk_side
                .create(
                    format!("disk {index}"),
                    None,
                    TaskScope::Global,
                    ProvenanceOrigin::Manual,
                    None,
                )
                .expect("create");
            disk_side.complete(id).expect("complete");
        }
        store.save(&disk_side).expect("seed disk");

        // Local side: UNDO_CAP completions of its own, merged with the disk entries on save.
        let mut local = store.load().expect("load");
        for index in 0..UNDO_CAP {
            let id = local
                .create(
                    format!("local {index}"),
                    None,
                    TaskScope::Global,
                    ProvenanceOrigin::Manual,
                    None,
                )
                .expect("create");
            local.complete(id).expect("complete");
        }
        store.reload_merge_save(&mut local).expect("merge-save");

        let loaded = store.load().expect("reload");
        assert_eq!(
            persisted_undo_len(&loaded),
            UNDO_CAP,
            "the union is capped after merging, not before"
        );
    }

    #[test]
    fn soft_delete_then_undo_clears_soft_deleted() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.soft_delete(id).expect("soft_delete");
        assert!(state.get(id).expect("task exists").soft_deleted);

        state.undo().expect("undo after soft_delete");

        let task = state.get(id).expect("task still in store");
        assert!(!task.soft_deleted);
        assert_eq!(task.status, HumanStatus::Ready);
        assert_eq!(task.title, "Fix flake");
    }

    #[test]
    fn complete_then_undo_returns_status_ready() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.complete(id).expect("complete");
        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Done
        );

        state.undo().expect("undo after complete");

        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Ready
        );
    }

    #[test]
    fn undo_with_empty_stack_is_noop() {
        let mut state = DomainState::new();
        // Documented no-op: empty stack must not panic and must succeed.
        state.undo().expect("empty undo is Ok");
        assert!(state.tasks().is_empty());
    }
}
