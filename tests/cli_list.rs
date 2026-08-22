use std::ffi::OsString;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use herdr_tasks::cli::run_with;
use herdr_tasks::domain::{DomainState, ProvenanceOrigin, TaskScope};
use herdr_tasks::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("lock environment")
}

struct StateDirEnvGuard {
    prior: Option<OsString>,
}

impl StateDirEnvGuard {
    fn set(path: &std::path::Path) -> Self {
        let prior = std::env::var_os("HERDR_PLUGIN_STATE_DIR");
        // SAFETY: ENV_LOCK serializes this process-wide environment mutation.
        unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", path) };
        Self { prior }
    }
}

impl Drop for StateDirEnvGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(value) => {
                // SAFETY: ENV_LOCK remains held until this guard is dropped.
                unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", value) };
            }
            None => {
                // SAFETY: ENV_LOCK remains held until this guard is dropped.
                unsafe { std::env::remove_var("HERDR_PLUGIN_STATE_DIR") };
            }
        }
    }
}

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("herdr-tasks-cli-list-{label}-{nanos}-{seq}"));
    std::fs::create_dir_all(&dir).expect("create state directory");
    dir
}

fn state_dir_arg(dir: &std::path::Path) -> String {
    dir.to_string_lossy().into_owned()
}

#[test]
fn list_json_includes_done_excludes_soft_deleted() {
    let dir = temp_state_dir("rows");
    let store = TaskStore::new(&dir);
    let mut state = DomainState::new();
    let ready = state
        .create(
            "ready task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create ready task");
    let done = state
        .create(
            "done task",
            None,
            TaskScope::Project {
                path: "/projects/widget".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create done task");
    state.complete(done).expect("complete task");
    let deleted = state
        .create(
            "deleted task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create deleted task");
    state.soft_delete(deleted).expect("soft delete task");
    store.save(&state).expect("seed store");

    let json = run_with(
        [
            "herdr-tasks",
            "list",
            "--json",
            "--state-dir",
            &state_dir_arg(&dir),
        ],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(json.code, 0);
    assert!(json.stderr.is_empty());
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 2);
    let ready_row = rows
        .iter()
        .find(|row| row["id"] == ready.to_string())
        .expect("ready row");
    assert_eq!(ready_row["title"], "ready task");
    assert_eq!(ready_row["status"], "ready");
    assert!(ready_row["project"].is_null());
    let done_row = rows
        .iter()
        .find(|row| row["id"] == done.to_string())
        .expect("done row");
    assert_eq!(done_row["title"], "done task");
    assert_eq!(done_row["status"], "done");
    assert_eq!(done_row["project"], "/projects/widget");
    for row in &rows {
        assert!(row.get("id").is_some());
        assert!(row.get("title").is_some());
        assert!(row.get("status").is_some());
        assert!(row.get("project").is_some());
    }

    let human = run_with(
        ["herdr-tasks", "list", "--state-dir", &state_dir_arg(&dir)],
        Cursor::new(Vec::<u8>::new()),
        true,
    );
    assert_eq!(human.code, 0);
    assert!(human.stderr.is_empty());
    assert_eq!(
        human.stdout.lines().collect::<Vec<_>>(),
        vec!["ready task", "done task"]
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_when_state_dir_is_a_file_exits_3() {
    let _env = env_lock();
    let parent = temp_state_dir("state-dir-file");
    let state_file = parent.join("not-a-directory");
    std::fs::write(&state_file, "not a directory").expect("create state-dir file");

    let output = run_with(
        [
            "herdr-tasks",
            "list",
            "--json",
            "--state-dir",
            &state_dir_arg(&state_file),
        ],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(output.code, 3);
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());

    let _ = std::fs::remove_dir_all(parent);
}

#[test]
fn list_state_dir_flag_wins_over_environment() {
    let _env = env_lock();
    let environment_dir = temp_state_dir("environment");
    let argument_dir = temp_state_dir("argument");
    let mut environment_state = DomainState::new();
    environment_state
        .create(
            "environment task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed environment task");
    TaskStore::new(&environment_dir)
        .save(&environment_state)
        .expect("save environment state");
    let mut argument_state = DomainState::new();
    argument_state
        .create(
            "argument task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed argument task");
    TaskStore::new(&argument_dir)
        .save(&argument_state)
        .expect("save argument state");
    let _state_dir = StateDirEnvGuard::set(&environment_dir);

    let output = run_with(
        [
            "herdr-tasks",
            "list",
            "--json",
            "--state-dir",
            &state_dir_arg(&argument_dir),
        ],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(output.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "argument task");

    let _ = std::fs::remove_dir_all(environment_dir);
    let _ = std::fs::remove_dir_all(argument_dir);
}

#[test]
fn list_uses_environment_state_dir_by_default() {
    let _env = env_lock();
    let dir = temp_state_dir("default");
    let mut state = DomainState::new();
    state
        .create(
            "environment default task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed default task");
    TaskStore::new(&dir)
        .save(&state)
        .expect("save default state");
    let _state_dir = StateDirEnvGuard::set(&dir);

    let output = run_with(
        ["herdr-tasks", "list", "--json"],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(output.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "environment default task");

    let _ = std::fs::remove_dir_all(dir);
}
