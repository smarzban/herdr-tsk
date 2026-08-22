use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use herdr_tasks::cli::run_with;
use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use herdr_tasks::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("lock environment")
}

struct EnvironmentGuard {
    key: &'static str,
    prior: Option<OsString>,
}

impl EnvironmentGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let prior = std::env::var_os(key);
        // SAFETY: ENV_LOCK serializes this process-wide environment mutation.
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }

    fn context_for(cwd: &Path) -> Self {
        let context = format!(
            r#"{{"focused_pane_cwd":{}}}"#,
            serde_json::to_string(cwd).expect("serialize repo path")
        );
        Self::set("HERDR_PLUGIN_CONTEXT_JSON", context)
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(value) => {
                // SAFETY: ENV_LOCK remains held until this guard is dropped.
                unsafe { std::env::set_var(self.key, value) };
            }
            None => {
                // SAFETY: ENV_LOCK remains held until this guard is dropped.
                unsafe { std::env::remove_var(self.key) };
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

fn project_repo(label: &str) -> PathBuf {
    let repo = temp_state_dir(label);
    std::fs::create_dir(repo.join(".git")).expect("create git marker");
    repo
}

fn state_dir_arg(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

fn list(args: &[String]) -> herdr_tasks::cli::CliOutput {
    run_with(args, Cursor::new(Vec::<u8>::new()), true)
}

fn create_task(state: &mut DomainState, title: &str, scope: TaskScope, status: HumanStatus) {
    let id = state
        .create(title, None, scope, None, None, ProvenanceOrigin::Manual)
        .expect("create task");
    state.set_status(id, status).expect("set status");
}

#[test]
fn list_defaults_to_invocation_project_open_tasks_in_human_and_json_group_order() {
    let _env = env_lock();
    let repo = project_repo("default-repo");
    let _context = EnvironmentGuard::context_for(&repo);
    let dir = temp_state_dir("default-rows");
    let project = TaskScope::Project {
        path: repo.to_string_lossy().into_owned(),
    };
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "review target",
        project.clone(),
        HumanStatus::Review,
    );
    create_task(
        &mut state,
        "ready target",
        project.clone(),
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "started target",
        project.clone(),
        HumanStatus::Started,
    );
    create_task(
        &mut state,
        "blocked target",
        project.clone(),
        HumanStatus::Blocked,
    );
    create_task(
        &mut state,
        "global hidden",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "other hidden",
        TaskScope::Project {
            path: "/projects/other".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "done hidden",
        project.clone(),
        HumanStatus::Done,
    );
    let deleted = state
        .create(
            "deleted hidden",
            None,
            project.clone(),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create deleted task");
    state.soft_delete(deleted).expect("soft delete task");
    TaskStore::new(&dir).save(&state).expect("seed store");

    let human = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(human.code, 0);
    assert!(human.stderr.is_empty());
    assert_eq!(
        human.stdout,
        "STARTED\nstarted target\nREADY\nready target\nBLOCKED\nblocked target\nREVIEW\nreview target\n"
    );

    let json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stderr.is_empty());
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("JSON rows");
    assert_eq!(
        rows.iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec![
            "started target",
            "ready target",
            "blocked target",
            "review target"
        ]
    );
    for row in &rows {
        assert!(row["id"].as_str().is_some_and(|id| !id.is_empty()));
        assert_eq!(row["project"], repo.to_string_lossy().as_ref());
    }

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_resolves_named_and_global_scopes() {
    let _env = env_lock();
    let repo = project_repo("scope-repo");
    let _context = EnvironmentGuard::context_for(&repo);
    let dir = temp_state_dir("scopes");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "widget task",
        TaskScope::Project {
            path: "/projects/Widget".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "global task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let named = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "-p".into(),
        "widget".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(named.code, 0);
    let named_rows: Vec<serde_json::Value> =
        serde_json::from_str(&named.stdout).expect("named JSON rows");
    assert_eq!(named_rows.len(), 1);
    assert_eq!(named_rows[0]["title"], "widget task");
    assert_eq!(named_rows[0]["project"], "/projects/Widget");

    let global = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(global.code, 0);
    let global_rows: Vec<serde_json::Value> =
        serde_json::from_str(&global.stdout).expect("global JSON rows");
    assert_eq!(global_rows.len(), 1);
    assert_eq!(global_rows[0]["title"], "global task");
    assert!(global_rows[0]["project"].is_null());

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_done_and_deleted_filters_are_status_and_soft_delete_specific() {
    let _env = env_lock();
    let repo = project_repo("filters-repo");
    let _context = EnvironmentGuard::context_for(&repo);
    let dir = temp_state_dir("filters");
    let project = TaskScope::Project {
        path: repo.to_string_lossy().into_owned(),
    };
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "done visible",
        project.clone(),
        HumanStatus::Done,
    );
    let deleted_ready = state
        .create(
            "deleted ready",
            None,
            project.clone(),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create deleted ready");
    state.soft_delete(deleted_ready).expect("soft delete ready");
    let deleted_done = state
        .create(
            "deleted done",
            None,
            project.clone(),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create deleted done");
    state.complete(deleted_done).expect("complete deleted task");
    state.soft_delete(deleted_done).expect("soft delete done");
    create_task(
        &mut state,
        "other done",
        TaskScope::Global,
        HumanStatus::Done,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let done = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--done".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(done.code, 0);
    assert_eq!(done.stdout, "DONE\ndone visible\n");

    let deleted = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--deleted".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(deleted.code, 0);
    let deleted_rows: Vec<serde_json::Value> =
        serde_json::from_str(&deleted.stdout).expect("deleted JSON rows");
    assert_eq!(
        deleted_rows
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec!["deleted ready", "deleted done"]
    );
    assert_eq!(deleted_rows[0]["status"], "ready");
    assert_eq!(deleted_rows[1]["status"], "done");

    let deleted_human = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--deleted".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(deleted_human.code, 0);
    assert_eq!(
        deleted_human.stdout,
        "DELETED\ndeleted ready\ndeleted done\n"
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_includes_every_scope_in_displayed_order_for_each_filter() {
    let _env = env_lock();
    let repo = project_repo("all-repo");
    let _context = EnvironmentGuard::context_for(&repo);
    let dir = temp_state_dir("all");
    let project = TaskScope::Project {
        path: repo.to_string_lossy().into_owned(),
    };
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "global started",
        TaskScope::Global,
        HumanStatus::Started,
    );
    create_task(
        &mut state,
        "project ready",
        project.clone(),
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "other blocked",
        TaskScope::Project {
            path: "/projects/other".into(),
        },
        HumanStatus::Blocked,
    );
    create_task(
        &mut state,
        "global review",
        TaskScope::Global,
        HumanStatus::Review,
    );
    create_task(&mut state, "project done", project, HumanStatus::Done);
    create_task(
        &mut state,
        "global done",
        TaskScope::Global,
        HumanStatus::Done,
    );
    let deleted_global = state
        .create(
            "global deleted",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create global deleted task");
    state
        .soft_delete(deleted_global)
        .expect("soft delete global task");
    let deleted_other = state
        .create(
            "other deleted",
            None,
            TaskScope::Project {
                path: "/projects/other".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create other deleted task");
    state
        .soft_delete(deleted_other)
        .expect("soft delete other task");
    TaskStore::new(&dir).save(&state).expect("seed store");

    let open = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(open.code, 0);
    assert_eq!(
        open.stdout,
        "STARTED\nglobal started\nREADY\nproject ready\nBLOCKED\nother blocked\nREVIEW\nglobal review\n"
    );

    let done = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--done".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(done.code, 0);
    let done_rows: Vec<serde_json::Value> = serde_json::from_str(&done.stdout).expect("JSON rows");
    assert_eq!(
        done_rows
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec!["project done", "global done"]
    );

    let deleted = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--deleted".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(deleted.code, 0);
    let deleted_rows: Vec<serde_json::Value> =
        serde_json::from_str(&deleted.stdout).expect("JSON rows");
    assert_eq!(
        deleted_rows
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec!["global deleted", "other deleted"]
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bare_list_outside_a_repo_falls_back_to_global_scope() {
    let _env = env_lock();
    let outside = temp_state_dir("outside-repo");
    let _context = EnvironmentGuard::context_for(&outside);
    let dir = temp_state_dir("outside-rows");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "global task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "project hidden",
        TaskScope::Project {
            path: "/projects/hidden".into(),
        },
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "global task");
    assert!(rows[0]["project"].is_null());

    let _ = std::fs::remove_dir_all(outside);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_rejects_conflicting_scope_and_filter_flags_and_missing_project_values() {
    let _env = env_lock();
    let dir = temp_state_dir("conflicts");

    for args in [
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "--all".into(),
            "--project".into(),
            "/projects/a".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "--all".into(),
            "--global".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "--global".into(),
            "--project".into(),
            "/projects/a".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "--done".into(),
            "--deleted".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "-p".into(),
            "--json".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
    ] {
        let output = list(&args);
        assert_eq!(output.code, 2);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("usage: herdr-tasks list"));
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_when_state_dir_is_a_file_exits_3() {
    let _env = env_lock();
    let parent = temp_state_dir("state-dir-file");
    let state_file = parent.join("not-a-directory");
    std::fs::write(&state_file, "not a directory").expect("create state-dir file");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&state_file),
    ]);

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
    create_task(
        &mut environment_state,
        "environment task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&environment_dir)
        .save(&environment_state)
        .expect("save environment state");
    let mut argument_state = DomainState::new();
    create_task(
        &mut argument_state,
        "argument task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&argument_dir)
        .save(&argument_state)
        .expect("save argument state");
    let _state_dir = EnvironmentGuard::set("HERDR_PLUGIN_STATE_DIR", &environment_dir);

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&argument_dir),
    ]);

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
    create_task(
        &mut state,
        "environment default task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&dir)
        .save(&state)
        .expect("save default state");
    let _state_dir = EnvironmentGuard::set("HERDR_PLUGIN_STATE_DIR", &dir);

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
    ]);

    assert_eq!(output.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "environment default task");

    let _ = std::fs::remove_dir_all(dir);
}
