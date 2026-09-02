//! Store persistence smoke independent of retired board chrome.

use std::sync::atomic::{AtomicUsize, Ordering};

use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::store::TaskStore;

fn temp_state_dir() -> std::path::PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("tsk-e2e-persist-{nanos}-{seq}"));
    std::fs::create_dir_all(&dir).expect("create state directory");
    dir
}

#[test]
fn task_persists_and_reloads_without_board_session_state() {
    let dir = temp_state_dir();
    let store = TaskStore::new(&dir);
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Persist across reopen",
            Some("notes survive".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    store.save(&domain).expect("save state");

    let reloaded = store.load().expect("reload state");
    assert_eq!(
        reloaded.get(id).expect("task").title,
        "Persist across reopen"
    );
    assert_eq!(
        reloaded.get(id).expect("task").notes.as_deref(),
        Some("notes survive")
    );

    let _ = std::fs::remove_dir_all(dir);
}
