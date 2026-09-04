//! Trash file: cross-process merge rules (AC-14).
//! Uses temp dirs only; never writes real plugin state.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-trash-store-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create(state: &mut DomainState, title: &str) -> uuid::Uuid {
    state
        .create(
            title,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task")
}

fn live_tasks(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("tsk.json")).expect("read live"))
        .expect("live document is JSON")
}

#[test]
fn reload_merge_save_does_not_resurrect_a_task_another_process_trashed() {
    let dir = temp_state_dir("resurrection");
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    // Seed the disk: task A soft-deleted with an empty undo stack, so it is
    // trash-eligible for any writer.
    let mut seed = DomainState::new();
    let a = create(&mut seed, "trashed elsewhere");
    let b = create(&mut seed, "witness");
    store.save(&seed).expect("seed");
    let mut soft = store.load().expect("load");
    soft.soft_delete(a).expect("soft delete");
    let mut document = serde_json::to_value(&soft).expect("serialize");
    document["undo_stack"] = serde_json::json!([]);
    for task in document["tasks"].as_array_mut().expect("tasks") {
        task.as_object_mut()
            .expect("task object")
            .remove("merge_base_revision");
    }
    fs::write(
        dir.join("tsk.json"),
        serde_json::to_vec_pretty(&document).expect("encode"),
    )
    .expect("install soft-deleted document");

    // Process A holds the soft-deleted task in memory.
    let mut process_a = store.load().expect("A loads");
    assert!(process_a.get(a).expect("A sees the task").soft_deleted);

    // Process B (a second TaskStore on the same dir) trashes it on save.
    let store_b = TaskStore::new(&dir);
    let mut process_b = store_b.load().expect("B loads");
    store_b.reload_merge_save(&mut process_b).expect("B saves");

    let trash_after_b = fs::read_to_string(dir.join("trash.jsonl")).expect("trash exists");
    assert_eq!(
        trash_after_b.lines().count(),
        1,
        "B appended exactly one trash line"
    );
    let live = live_tasks(&dir);
    assert!(
        !live["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .any(|task| task["id"] == a.to_string()),
        "B removed the trashed task from the live document"
    );

    // A's next reload_merge_save must not resurrect it or append a second trash line.
    store.reload_merge_save(&mut process_a).expect("A saves");

    assert_eq!(
        fs::read_to_string(dir.join("trash.jsonl")).expect("read trash"),
        trash_after_b,
        "A must not append a second trash line"
    );
    let live = live_tasks(&dir);
    assert!(
        !live["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .any(|task| task["id"] == a.to_string()),
        "A must not resurrect the trashed task"
    );
    assert!(
        live["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .any(|task| task["id"] == b.to_string()),
        "the witness task survives"
    );
    assert!(
        process_a.get(a).is_none(),
        "A dropped the trashed task from its local state"
    );
    assert!(process_a.get(b).is_some(), "A keeps the witness task");
}

#[test]
fn merge_tasks_from_disk_drops_a_locally_held_task_that_was_trashed_elsewhere() {
    let dir = temp_state_dir("idle-merge");
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut seed = DomainState::new();
    let a = create(&mut seed, "held locally");
    store.save(&seed).expect("seed");
    let mut soft = store.load().expect("load");
    soft.soft_delete(a).expect("soft delete");
    let mut document = serde_json::to_value(&soft).expect("serialize");
    document["undo_stack"] = serde_json::json!([]);
    for task in document["tasks"].as_array_mut().expect("tasks") {
        task.as_object_mut()
            .expect("task object")
            .remove("merge_base_revision");
    }
    fs::write(
        dir.join("tsk.json"),
        serde_json::to_vec_pretty(&document).expect("encode"),
    )
    .expect("install soft-deleted document");

    // The board-side presentation merge sees a disk snapshot that lacks the task
    // (another process trashed it).
    let mut local = store.load().expect("load local");
    assert!(local.get(a).is_some());
    let disk_state: DomainState = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "next_task_number": 2,
        "tasks": [],
        "undo_stack": []
    }))
    .expect("empty disk snapshot");

    local.merge_tasks_from_disk(&disk_state);
    assert!(
        local.get(a).is_none(),
        "a soft-deleted local task with no merge base disappears with the disk"
    );
}
