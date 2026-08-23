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

fn create_task_with_thread(
    state: &mut DomainState,
    title: &str,
    scope: TaskScope,
    status: HumanStatus,
    thread: Option<&str>,
) -> uuid::Uuid {
    let id = state
        .create_with_thread(
            title,
            None,
            scope,
            None,
            None,
            ProvenanceOrigin::Manual,
            thread.map(str::to_owned),
        )
        .expect("create task");
    state.set_status(id, status).expect("set status");
    id
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
        "STARTED\n - started target\n\nREADY\n - ready target\n\nBLOCKED\n - blocked target\n\nREVIEW\n - review target\n"
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
    assert_eq!(done.stdout, "DONE\n - done visible\n");

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
        "DELETED\n - deleted ready\n - deleted done\n"
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_groups_each_status_by_concise_scope_for_every_filter() {
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
        "project started",
        project.clone(),
        HumanStatus::Started,
    );
    create_task(
        &mut state,
        "other started",
        TaskScope::Project {
            path: "/projects/other".into(),
        },
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
    let project_name = repo
        .file_name()
        .and_then(|name| name.to_str())
        .expect("project basename");

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
        format!(
            "STARTED\n  global\n    - global started\n  {project_name}\n    - project started\n  other\n    - other started\n\nREADY\n  {project_name}\n    - project ready\n\nBLOCKED\n  other\n    - other blocked\n\nREVIEW\n  global\n    - global review\n"
        )
    );

    let open_json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    let open_rows: Vec<serde_json::Value> =
        serde_json::from_str(&open_json.stdout).expect("open JSON rows");
    assert_eq!(
        open_rows
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec![
            "global started",
            "project started",
            "other started",
            "project ready",
            "other blocked",
            "global review",
        ]
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
    for row in &done_rows {
        assert_eq!(
            row.as_object()
                .expect("JSON row")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["id", "project", "status", "thread", "title"]
        );
    }
    let done_human = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--done".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(
        done_human.stdout,
        format!("DONE\n  {project_name}\n    - project done\n  global\n    - global done\n")
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
    let deleted_human = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--deleted".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(
        deleted_human.stdout,
        "DELETED\n  global\n    - global deleted\n  other\n    - other deleted\n"
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_distinguishes_global_from_project_global_and_uses_visible_scope_labels() {
    let _env = env_lock();
    let dir = temp_state_dir("all-colliding-scopes");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "global task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "project global token",
        TaskScope::Project {
            path: "global".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "project global path",
        TaskScope::Project {
            path: "/work/global".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "work api",
        TaskScope::Project {
            path: "/work/api".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "personal api",
        TaskScope::Project {
            path: "/personal/api".into(),
        },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "whitespace scope",
        TaskScope::Project {
            path: " \t ".into(),
        },
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    assert_eq!(
        output.stdout,
        "READY\n  global\n    - global task\n  project: global\n    - project global token\n  work/global\n    - project global path\n  work/api\n    - work api\n  personal/api\n    - personal api\n  project: <empty project 1>\n    - whitespace scope\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_uses_shortest_unique_trailing_scope_labels_across_statuses() {
    let _env = env_lock();
    let dir = temp_state_dir("all-cross-status-scopes");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "project global",
        TaskScope::Project {
            path: "global".into(),
        },
        HumanStatus::Started,
    );
    create_task(
        &mut state,
        "work api",
        TaskScope::Project {
            path: "/work/api".into(),
        },
        HumanStatus::Started,
    );
    create_task(&mut state, "global", TaskScope::Global, HumanStatus::Ready);
    create_task(
        &mut state,
        "blank one",
        TaskScope::Project { path: " ".into() },
        HumanStatus::Ready,
    );
    create_task(
        &mut state,
        "personal api",
        TaskScope::Project {
            path: "/personal/api".into(),
        },
        HumanStatus::Blocked,
    );
    create_task(
        &mut state,
        "blank two",
        TaskScope::Project { path: "\t".into() },
        HumanStatus::Review,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    assert_eq!(
        output.stdout,
        "STARTED\n  project: global\n    - project global\n  work/api\n    - work api\n\nREADY\n  global\n    - global\n  project: <empty project 1>\n    - blank one\n\nBLOCKED\n  personal/api\n    - personal api\n\nREVIEW\n  project: <empty project 2>\n    - blank two\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_preserves_raw_scope_syntax_after_trailing_segments_are_exhausted() {
    let _env = env_lock();
    let dir = temp_state_dir("all-raw-scope-syntax");
    let mut state = DomainState::new();
    for (title, path) in [
        ("absolute api", "/work/api"),
        ("relative api", "work/api"),
        ("trailing api", "/work/api/"),
        ("repeated api", "//work//api"),
    ] {
        create_task(
            &mut state,
            title,
            TaskScope::Project { path: path.into() },
            HumanStatus::Ready,
        );
    }
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    assert_eq!(
        output.stdout,
        "READY\n  project: \"/work/api\"\n    - absolute api\n  project: \"work/api\"\n    - relative api\n  project: \"/work/api/\"\n    - trailing api\n  project: \"//work//api\"\n    - repeated api\n"
    );
    assert!(
        !output.stdout.contains("(scope "),
        "syntactically distinct scopes must remain distinguishable without synthetic suffixes"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_all_visibly_escapes_and_disambiguates_control_scope_labels() {
    let _env = env_lock();
    let dir = temp_state_dir("all-escaped-scope-label");
    let mut state = DomainState::new();
    for (title, path) in [
        ("control scope task", "\u{001b}[2Japi"),
        ("literal escape scope task", "\\u{001b}[2Japi"),
    ] {
        create_task(
            &mut state,
            title,
            TaskScope::Project { path: path.into() },
            HumanStatus::Ready,
        );
    }
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    assert!(!output.stdout.contains('\u{001b}'));
    let labels = output
        .stdout
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("    "))
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 2);
    assert_ne!(labels[0], labels[1]);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn human_list_escapes_terminal_control_titles_without_changing_json() {
    let _env = env_lock();
    let dir = temp_state_dir("terminal-control-title");
    let title = "control\u{001b}]52;c;clipboard\u{0007}";
    let escaped = "control\\u{001b}]52;c;clipboard\\u{0007}";
    let mut state = DomainState::new();
    create_task(&mut state, title, TaskScope::Global, HumanStatus::Ready);
    create_task(&mut state, title, TaskScope::Global, HumanStatus::Done);
    let deleted = state
        .create(
            title,
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create deleted task");
    state.soft_delete(deleted).expect("soft delete task");
    TaskStore::new(&dir).save(&state).expect("seed store");

    for view in [vec![], vec!["--done"], vec!["--deleted"]] {
        let mut args = vec![
            "herdr-tasks".into(),
            "list".into(),
            "--global".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ];
        args.extend(view.into_iter().map(String::from));
        let human = list(&args);
        assert_eq!(human.code, 0);
        assert!(human.stderr.is_empty());
        assert!(human.stdout.contains(escaped));
        assert!(!human.stdout.contains('\u{001b}'));
        assert!(!human.stdout.contains('\u{0007}'));
    }

    let json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(json.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("JSON rows");
    assert_eq!(rows[0]["title"], title);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn human_list_escapes_terminal_control_thread_markers_without_changing_json() {
    let _env = env_lock();
    let dir = temp_state_dir("terminal-control-thread");
    let thread = "release\u{001b}]52;c;clipboard\u{0007}";
    let escaped = "release\\u{001b}]52;c;clipboard\\u{0007}";
    let mut state = DomainState::new();
    create_task_with_thread(
        &mut state,
        "threaded task",
        TaskScope::Global,
        HumanStatus::Ready,
        Some(thread),
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let human = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(human.code, 0);
    assert!(human.stdout.contains(escaped));
    assert!(!human.stdout.contains('\u{001b}'));
    assert!(!human.stdout.contains('\u{0007}'));

    let json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(json.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("JSON rows");
    assert_eq!(rows[0]["thread"], thread);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_equals_state_dir_form_accepts_dash_leading_value() {
    let cwd = temp_state_dir("equals-dash-state");
    let state_dir = cwd.join("-state");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "equals state task",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&state_dir).save(&state).expect("seed state");
    let binary = std::env::var("CARGO_BIN_EXE_herdr-tasks")
        .expect("Cargo must provide the herdr-tasks binary path");

    let output = std::process::Command::new(binary)
        .current_dir(&cwd)
        .args(["list", "--global", "--json", "--state-dir=-state"])
        .output()
        .expect("run equals state directory");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("JSON rows");
    assert_eq!(rows[0]["title"], "equals state task");

    let _ = std::fs::remove_dir_all(cwd);
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
        vec![
            "herdr-tasks".into(),
            "list".into(),
            "-p".into(),
            "-maintenance".into(),
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
fn list_thread_parse_rejects_invalid_space_and_equals_forms() {
    let _env = env_lock();
    let dir = temp_state_dir("invalid-thread");

    for thread in ["--thread", "--thread=bad_name"] {
        let mut args = vec!["herdr-tasks".into(), "list".into(), thread.into()];
        if thread == "--thread" {
            args.push("bad_name".into());
        }
        args.extend(["--state-dir".into(), state_dir_arg(&dir)]);
        let output = list(&args);
        assert_eq!(output.code, 2, "{thread}");
        assert!(output.stdout.is_empty(), "{thread}");
        assert!(output.stderr.contains("invalid thread name"), "{thread}");
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_equals_project_form_accepts_dash_leading_scope() {
    let _env = env_lock();
    let dir = temp_state_dir("equals-project");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "maintenance task",
        TaskScope::Project {
            path: "-maintenance".into(),
        },
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--project=-maintenance".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&output.stdout).expect("JSON rows"),
        vec![serde_json::json!({
            "id": state.tasks()[0].id,
            "title": "maintenance task",
            "status": "ready",
            "project": "-maintenance",
            "thread": null,
        })]
    );

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

/// One task whose steps carry chosen ids `aaa1…`/`aaa2…`, shaped through
/// the store document because step ids are otherwise minted by the domain.
fn state_with_steps(done_first: bool) -> (DomainState, herdr_tasks::domain::Step) {
    let mut state = DomainState::new();
    let id = state
        .create(
            "steps target",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed steps task");
    state.add_step(id, "First step").expect("seed first step");
    state.add_step(id, "Second step").expect("seed second step");
    if done_first {
        let first = state.tasks()[0].steps[0].id;
        state.toggle_step(id, first).expect("seed first step done");
    }
    let mut document = serde_json::to_value(&state).expect("serialize seed state");
    for (index, id) in [
        "aaa11111-0000-4000-8000-000000000001",
        "aaa22222-0000-4000-8000-000000000002",
    ]
    .into_iter()
    .enumerate()
    {
        document["tasks"][0]["steps"][index]["id"] =
            serde_json::json!(uuid::Uuid::parse_str(id).expect("shaped step id"));
    }
    let shaped: DomainState = serde_json::from_value(document).expect("state with shaped step ids");
    let first_step = shaped.tasks()[0].steps[0].clone();
    (shaped, first_step)
}

#[test]
fn list_task_prints_step_lines_with_state_and_short_id() {
    let dir = temp_state_dir("step-lines");
    let (state, first_step) = state_with_steps(true);
    let task = state.tasks()[0].id;
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        task.to_string(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        format!("READY\n - steps target\n   [x] aaa1 First step\n   [ ] aaa2 Second step\n"),
        "one line per step with state and unambiguous short id"
    );
    assert!(
        first_step.id.to_string().starts_with("aaa1"),
        "printed short id must prefix the step identity"
    );

    let json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        task.to_string(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(json.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    for key in ["id", "project", "status", "thread", "title"] {
        assert!(rows[0].get(key).is_some(), "single-task row keeps {key}");
    }
    assert_eq!(
        rows[0]["steps"],
        serde_json::json!([
            {
                "id": first_step.id.to_string(),
                "done": true,
                "short_id": "aaa1",
                "text": "First step",
            },
            {
                "id": "aaa22222-0000-4000-8000-000000000002",
                "done": false,
                "short_id": "aaa2",
                "text": "Second step",
            },
        ]),
        "single-task JSON carries the steps with ids, state, and short ids"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_task_without_steps_keeps_task_rows_and_rejects_conflicting_flags() {
    let dir = temp_state_dir("task-without-steps");
    let mut state = DomainState::new();
    create_task(
        &mut state,
        "plain target",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");
    let task = state.tasks()[0].id;

    let plain = list(&[
        "herdr-tasks".into(),
        "list".into(),
        task.to_string(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(plain.code, 0);
    assert_eq!(plain.stdout, "READY\n - plain target\n");

    let plain_json = list(&[
        "herdr-tasks".into(),
        "list".into(),
        task.to_string(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(plain_json.code, 0);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&plain_json.stdout).expect("JSON rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .as_object()
            .expect("JSON row")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["id", "project", "status", "thread", "title"],
        "a task without steps keeps today's exact JSON row shape"
    );

    for extra in ["--global", "--all", "--done", "--deleted"] {
        let output = list(&[
            "herdr-tasks".into(),
            "list".into(),
            task.to_string(),
            extra.into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ]);
        assert_eq!(output.code, 2, "task id with {extra} is usage");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("usage: herdr-tasks list"));
    }
    let threaded = list(&[
        "herdr-tasks".into(),
        "list".into(),
        task.to_string(),
        "--thread".into(),
        "release".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(threaded.code, 2, "task id with --thread is usage");
    assert!(threaded.stdout.is_empty());
    assert!(threaded.stderr.contains("usage: herdr-tasks list"));

    let invalid = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "not-a-uuid".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(invalid.code, 2);
    assert!(invalid.stdout.is_empty());
    assert!(invalid.stderr.contains("invalid task id"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_thread_filters_after_scope_selection() {
    let _env = env_lock();
    let dir = temp_state_dir("thread-filter");
    let mut state = DomainState::new();
    create_task_with_thread(
        &mut state,
        "selected thread",
        TaskScope::Project {
            path: "/projects/selected".into(),
        },
        HumanStatus::Ready,
        Some("release"),
    );
    create_task_with_thread(
        &mut state,
        "selected other thread",
        TaskScope::Project {
            path: "/projects/selected".into(),
        },
        HumanStatus::Ready,
        Some("ops"),
    );
    create_task_with_thread(
        &mut state,
        "other scope same thread",
        TaskScope::Project {
            path: "/projects/other".into(),
        },
        HumanStatus::Ready,
        Some("release"),
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let scoped = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--project".into(),
        "selected".into(),
        "--thread".into(),
        "Release".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(scoped.code, 0, "{}", scoped.stderr);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&scoped.stdout)
            .expect("scoped JSON rows")
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec!["selected thread"]
    );

    let all = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--thread=RELEASE".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(all.code, 0, "{}", all.stderr);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&all.stdout)
            .expect("all JSON rows")
            .iter()
            .map(|row| row["title"].as_str().expect("title"))
            .collect::<Vec<_>>(),
        vec!["selected thread", "other scope same thread"]
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn json_rows_always_carry_thread_field() {
    let _env = env_lock();
    let dir = temp_state_dir("thread-json");
    let mut state = DomainState::new();
    create_task_with_thread(
        &mut state,
        "threaded",
        TaskScope::Global,
        HumanStatus::Ready,
        Some("release"),
    );
    create_task(
        &mut state,
        "unthreaded",
        TaskScope::Global,
        HumanStatus::Ready,
    );
    TaskStore::new(&dir).save(&state).expect("seed store");

    let output = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--global".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0, "{}", output.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout).expect("JSON rows");
    assert_eq!(rows[0]["thread"], "release");
    assert!(rows[1].get("thread").expect("unthreaded field").is_null());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn human_output_appends_thread_marker_iff_row_threaded_snapshots() {
    let _env = env_lock();
    let repo = project_repo("thread-human");
    let _context = EnvironmentGuard::context_for(&repo);
    let dir = temp_state_dir("thread-human");
    let project = TaskScope::Project {
        path: repo.to_string_lossy().into_owned(),
    };
    let mut state = DomainState::new();
    create_task_with_thread(
        &mut state,
        "scoped threaded",
        project.clone(),
        HumanStatus::Ready,
        Some("alpha"),
    );
    create_task(
        &mut state,
        "scoped unthreaded",
        project.clone(),
        HumanStatus::Ready,
    );
    create_task_with_thread(
        &mut state,
        "global threaded",
        TaskScope::Global,
        HumanStatus::Ready,
        Some("ops"),
    );
    let step = create_task_with_thread(
        &mut state,
        "step threaded",
        TaskScope::Global,
        HumanStatus::Done,
        Some("steps"),
    );
    state.add_step(step, "Keep this line").expect("add step");
    let step_short_id = state.get(step).expect("step task").steps[0].id.to_string()[..1].to_owned();
    TaskStore::new(&dir).save(&state).expect("seed store");
    let project_name = repo
        .file_name()
        .and_then(|name| name.to_str())
        .expect("project basename");

    let scoped = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(
        scoped.stdout,
        "READY\n - scoped threaded #alpha\n - scoped unthreaded\n"
    );

    let all = list(&[
        "herdr-tasks".into(),
        "list".into(),
        "--all".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(
        all.stdout,
        format!(
            "READY\n  {project_name}\n    - scoped threaded #alpha\n    - scoped unthreaded\n  global\n    - global threaded #ops\n"
        )
    );

    let single = list(&[
        "herdr-tasks".into(),
        "list".into(),
        step.to_string(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(
        single.stdout,
        format!("DONE\n - step threaded #steps\n   [ ] {step_short_id} Keep this line\n")
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}
