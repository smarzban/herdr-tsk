#![cfg(unix)]
#[path = "support/pty.rs"]
mod pty;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use tsk_tui::announcements;
use tsk_tui::app::{load_board_for_quick_capture, load_board_model};
use tsk_tui::delivery;
use tsk_tui::domain::{DomainState, HumanStatus, ProvenanceOrigin, Task, TaskScope};
use tsk_tui::guides::CATALOG;
use tsk_tui::store::TaskStore;
use tsk_tui::update::suppress_background_fetch;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

struct StateDirEnv {
    previous: Option<std::ffi::OsString>,
    dir: PathBuf,
}

impl StateDirEnv {
    fn set(label: &str) -> Self {
        let dir = pty::scratch_root(label).join("state");
        let previous = std::env::var_os("TSK_STATE_DIR");
        std::env::set_var("TSK_STATE_DIR", &dir);
        Self { previous, dir }
    }
}

impl Drop for StateDirEnv {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("TSK_STATE_DIR", value),
            None => std::env::remove_var("TSK_STATE_DIR"),
        }
        let _ = fs::remove_dir_all(self.dir.parent().unwrap_or(&self.dir));
    }
}

fn notices(state: &DomainState) -> Vec<&Task> {
    state
        .tasks()
        .iter()
        .filter(|task| task.is_notice())
        .collect()
}

fn catalog_ids() -> BTreeSet<String> {
    CATALOG
        .iter()
        .map(|guide| guide.catalog_id.to_string())
        .collect()
}

#[test]
fn full_board_open_seeds_the_guides_once_beside_existing_tasks() {
    let _lock = env_lock();
    suppress_background_fetch();
    let env = StateDirEnv::set("board-open");
    let store = TaskStore::new(&env.dir);
    let mut seeded = DomainState::new();
    seeded
        .create(
            "existing work",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("task");
    store.save(&seeded).expect("save");

    let _ = load_board_model().expect("first open");
    let state = store.load().expect("load");
    assert_eq!(notices(&state).len(), 5);
    assert_eq!(
        state
            .tasks()
            .iter()
            .filter(|task| !task.is_notice())
            .map(|task| (task.title.as_str(), task.number))
            .collect::<Vec<_>>(),
        vec![("existing work", Some(1))]
    );
    assert_eq!(delivery::load(&env.dir).guides, catalog_ids());

    let _ = load_board_model().expect("second open");
    assert_eq!(notices(&store.load().expect("reload")).len(), 5);
}

#[test]
fn a_fresh_full_board_open_records_the_bundled_announcements_without_seeding_them() {
    let _lock = env_lock();
    suppress_background_fetch();
    let env = StateDirEnv::set("announcements");
    let bundled = announcements::catalog().expect("bundled catalog");
    let newest = bundled.last().expect("at least one entry").id;

    let _ = load_board_model().expect("first open");
    let state = TaskStore::new(&env.dir).load().expect("load");
    assert_eq!(notices(&state).len(), 5, "guides only");
    assert!(notices(&state)
        .iter()
        .all(|task| task.title != announcements::TITLE));
    let record = delivery::load(&env.dir);
    assert_eq!(record.announcement_watermark, newest);
    assert_eq!(record.guides, catalog_ids());
}

#[test]
fn quick_capture_open_seeds_nothing() {
    let _lock = env_lock();
    suppress_background_fetch();
    let env = StateDirEnv::set("capture-open");
    let (store, state, _model) = load_board_for_quick_capture().expect("capture open");
    assert!(notices(&state).is_empty());
    assert!(notices(&store.load().expect("load")).is_empty());
    assert!(!env.dir.join(delivery::DELIVERY_FILE).exists());
}

fn drop_delivery_record(state_dir: &std::path::Path) {
    fs::remove_file(state_dir.join(delivery::DELIVERY_FILE)).unwrap();
}

#[test]
fn completing_a_guide_on_the_real_board_records_its_dismissal() {
    let root = pty::scratch_root("dismiss");
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let state_dir = root.join("state");
    let mut session = pty::Session::spawn(root, &outside, &[], &[], 24, 78);
    // Painted words are split by cursor moves, so wait on the last guide's `N` prefix.
    let painted = session.output_until("N5");
    for prefix in ["N1", "N2", "N3", "N4"] {
        assert!(painted.contains(prefix), "{prefix} painted: {painted}");
    }
    let store = TaskStore::new(&state_dir);
    assert_eq!(delivery::load(&state_dir).guides, catalog_ids());
    drop_delivery_record(&state_dir);

    session.send(b"\x04");
    session.send(b"\x11");
    assert!(session.wait_exit(Duration::from_secs(5)).success());

    let state = store.load().unwrap();
    let done: Vec<&Task> = notices(&state)
        .into_iter()
        .filter(|task| task.status == HumanStatus::Done)
        .collect();
    assert_eq!(done.len(), 1, "ctrl+d completed the selected guide");
    assert_eq!(notices(&state).len(), 5, "the other guides stay");
    let dismissed = done[0].notice.as_ref().unwrap().catalog_id.clone();
    assert_eq!(
        delivery::load(&state_dir).guides,
        [dismissed].into_iter().collect::<BTreeSet<_>>()
    );
}
