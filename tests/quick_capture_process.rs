//! Real capture entrypoint, isolated PTY and store. Never uses a live Herdr session.
#![cfg(unix)]
#[path = "support/pty.rs"]
mod pty;

use std::ffi::OsString;
use std::fs;
use std::time::Duration;

use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::reopen::ReopenRequest;
use tsk_tui::store::TaskStore;

#[test]
fn capture_entrypoint_skips_launch_card_preserves_reopen_and_exits_on_escape() {
    let root = pty::scratch_root("capture");
    fs::create_dir_all(root.join("repo/.git")).unwrap();
    let repo = root.join("repo");
    let store = TaskStore::new(root.join("state"));
    let mut state = DomainState::new();
    state
        .create(
            "archived seed",
            None,
            TaskScope::Project {
                path: repo.display().to_string(),
            },
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    state.archive_project(repo.to_str().unwrap()).unwrap();
    store.save(&state).unwrap();
    ReopenRequest::new(None).write(store.path()).unwrap();
    let request = fs::read(store.path().join("reopen.json")).unwrap();
    let context = OsString::from(
        serde_json::json!({"focused_pane_cwd":repo,"selected_text":"PTY capture"}).to_string(),
    );
    let mut session = pty::Session::spawn(
        root.clone(),
        &repo,
        &["capture"],
        &[("HERDR_PLUGIN_CONTEXT_JSON", context.as_os_str())],
        13,
        78,
    );
    let output = session.output_until("cancel");
    assert!(
        output.contains("+ step"),
        "capture must open the expanded page: {output}"
    );
    assert!(
        output.contains("desk"),
        "archived launch falls back to desk: {output}"
    );
    assert!(!output.contains("would you like to unarchive"));
    assert_eq!(
        fs::read(store.path().join("reopen.json")).unwrap(),
        request,
        "popup must not retire board requests"
    );
    ReopenRequest::new(None).write(store.path()).unwrap();
    let fresh = fs::read(store.path().join("reopen.json")).unwrap();
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(fs::read(store.path().join("reopen.json")).unwrap(), fresh);
    session.send(b"\x1b[27u");
    assert!(
        session.wait_exit(Duration::from_secs(5)).success(),
        "one Escape must exit real capture loop"
    );
    let tasks = store.load().unwrap();
    assert_eq!(tasks.tasks().len(), 1, "cancel saves no task");
    assert!(
        tasks.tasks().iter().all(|task| !task.is_notice()),
        "quick capture never seeds the starter guides"
    );
    assert!(
        !store.path().join("delivery.json").exists(),
        "quick capture writes no delivery record"
    );
}
