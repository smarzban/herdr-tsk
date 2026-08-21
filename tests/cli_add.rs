use std::io::{Cursor, Read};
use std::path::PathBuf;
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

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("herdr-tasks-cli-add-{label}-{nanos}-{seq}"));
    std::fs::create_dir_all(&dir).expect("create state directory");
    dir
}

fn add(args: &[String], stdin_is_tty: bool) -> herdr_tasks::cli::CliOutput {
    run_with(args, Cursor::new(Vec::<u8>::new()), stdin_is_tty)
}

fn state_dir_arg(dir: &std::path::Path) -> String {
    dir.to_string_lossy().into_owned()
}

struct PanicOnRead;

impl Read for PanicOnRead {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        panic!("flag add must not read stdin")
    }
}

fn task_store(dir: &std::path::Path) -> TaskStore {
    TaskStore::new(dir)
}

#[test]
fn flag_add_creates_ready_task_and_prints_added_title() {
    let _env = env_lock();
    let dir = temp_state_dir("flag");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "  hello  world  ".into(),
        ],
        true,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added hello  world\n");
    assert!(output.stderr.is_empty());
    let state = task_store(&dir).load().expect("load state");
    assert_eq!(state.tasks().len(), 1);
    let task = &state.tasks()[0];
    assert_eq!(task.title, "hello  world");
    assert_eq!(task.status, HumanStatus::Ready);
    assert!(task.notes.is_none());
    assert!(task.capsule.is_none());
    assert!(task.agent_meta.is_none());
    assert_eq!(task.provenance, ProvenanceOrigin::Capture);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_does_not_read_piped_stdin() {
    let _env = env_lock();
    let dir = temp_state_dir("piped-stdin");
    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "-t",
            "pipe safe",
        ],
        PanicOnRead,
        false,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added pipe safe\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_keeps_non_whitespace_notes_and_drops_whitespace_notes() {
    let _env = env_lock();
    let notes_dir = temp_state_dir("notes");
    let notes_output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&notes_dir),
            "-t".into(),
            "notes".into(),
            "--notes".into(),
            "line one\nline two".into(),
        ],
        true,
    );
    assert_eq!(notes_output.code, 0);
    assert_eq!(
        task_store(&notes_dir).load().expect("load notes").tasks()[0]
            .notes
            .as_deref(),
        Some("line one\nline two")
    );

    let blank_dir = temp_state_dir("blank-notes");
    let blank_output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&blank_dir),
            "-t".into(),
            "blank notes".into(),
            "-n".into(),
            " \t\n ".into(),
        ],
        true,
    );
    assert_eq!(blank_output.code, 0);
    assert!(task_store(&blank_dir)
        .load()
        .expect("load blank notes")
        .tasks()[0]
        .notes
        .is_none());

    let _ = std::fs::remove_dir_all(notes_dir);
    let _ = std::fs::remove_dir_all(blank_dir);
}

#[test]
fn flag_add_resolves_global_and_project_basename_scopes() {
    let _env = env_lock();
    let global_dir = temp_state_dir("global");
    let global_output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&global_dir),
            "-t".into(),
            "global task".into(),
            "--global".into(),
        ],
        true,
    );
    assert_eq!(global_output.code, 0);
    assert_eq!(
        task_store(&global_dir).load().expect("load global").tasks()[0].scope,
        TaskScope::Global
    );

    let project_dir = temp_state_dir("project");
    let store = task_store(&project_dir);
    let mut seeded = DomainState::new();
    seeded
        .create(
            "existing project",
            None,
            TaskScope::Project {
                path: "/projects/Widget".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed project");
    store.save(&seeded).expect("save seed");
    let project_output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&project_dir),
            "-t".into(),
            "project task".into(),
            "--project".into(),
            "widget".into(),
        ],
        true,
    );
    assert_eq!(project_output.code, 0);
    assert_eq!(
        task_store(&project_dir)
            .load()
            .expect("load project")
            .tasks()[1]
            .scope,
        TaskScope::Project {
            path: "/projects/Widget".into()
        }
    );

    let _ = std::fs::remove_dir_all(global_dir);
    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn flag_add_uses_the_invocation_default_scope() {
    let _env = env_lock();
    let repo = temp_state_dir("invocation-repo");
    std::fs::create_dir(repo.join(".git")).expect("create git marker");
    let dir = temp_state_dir("invocation-state");
    let prior = std::env::var_os("HERDR_PLUGIN_CONTEXT_JSON");
    let context = format!(
        r#"{{"focused_pane_cwd":{}}}"#,
        serde_json::to_string(&repo).unwrap()
    );
    // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
    unsafe { std::env::set_var("HERDR_PLUGIN_CONTEXT_JSON", context) };

    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "-t".into(),
            "invocation scope".into(),
        ],
        true,
    );

    match prior {
        Some(value) => {
            // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
            unsafe { std::env::set_var("HERDR_PLUGIN_CONTEXT_JSON", value) };
        }
        None => {
            // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
            unsafe { std::env::remove_var("HERDR_PLUGIN_CONTEXT_JSON") };
        }
    }

    assert_eq!(output.code, 0);
    assert_eq!(
        task_store(&dir)
            .load()
            .expect("load invocation state")
            .tasks()[0]
            .scope,
        TaskScope::Project {
            path: repo.to_string_lossy().into_owned()
        }
    );

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_requires_a_title() {
    let _env = env_lock();
    let dir = temp_state_dir("missing-title");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        true,
    );

    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(task_store(&dir)
        .load()
        .expect("load state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_refuses_empty_and_control_character_titles() {
    let _env = env_lock();
    let empty_dir = temp_state_dir("empty-title");
    let empty = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&empty_dir),
            "-t".into(),
            "".into(),
        ],
        true,
    );
    assert_eq!(empty.code, 1);
    assert!(empty.stdout.is_empty());
    assert!(task_store(&empty_dir)
        .load()
        .expect("load empty state")
        .tasks()
        .is_empty());

    let invalid_dir = temp_state_dir("invalid-title");
    let invalid = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&invalid_dir),
            "-t".into(),
            "hello\nworld".into(),
        ],
        true,
    );
    assert_eq!(invalid.code, 1);
    assert!(invalid.stdout.is_empty());
    assert!(invalid.stderr.contains("invalid-title"));
    assert!(task_store(&invalid_dir)
        .load()
        .expect("load invalid state")
        .tasks()
        .is_empty());

    let end_control_dir = temp_state_dir("end-control-title");
    let end_control = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&end_control_dir),
            "-t".into(),
            "hello\n".into(),
        ],
        true,
    );
    assert_eq!(end_control.code, 1);
    assert!(end_control.stdout.is_empty());
    assert!(end_control.stderr.contains("invalid-title"));
    assert!(task_store(&end_control_dir)
        .load()
        .expect("load end-control state")
        .tasks()
        .is_empty());

    let whitespace_dir = temp_state_dir("whitespace-title");
    let whitespace = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&whitespace_dir),
            "-t".into(),
            "   ".into(),
        ],
        true,
    );
    assert_eq!(whitespace.code, 1);
    assert!(whitespace.stdout.is_empty());
    assert!(task_store(&whitespace_dir)
        .load()
        .expect("load whitespace state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(empty_dir);
    let _ = std::fs::remove_dir_all(invalid_dir);
    let _ = std::fs::remove_dir_all(end_control_dir);
    let _ = std::fs::remove_dir_all(whitespace_dir);
}

#[test]
fn flag_add_rejects_global_with_project() {
    let _env = env_lock();
    let dir = temp_state_dir("global-project");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "-t".into(),
            "conflict".into(),
            "--global".into(),
            "-p".into(),
            "/projects/a".into(),
        ],
        true,
    );

    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(task_store(&dir)
        .load()
        .expect("load state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn add_without_item_flags_on_a_tty_is_usage() {
    let _env = env_lock();
    let dir = temp_state_dir("tty");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ],
        true,
    );

    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(task_store(&dir)
        .load()
        .expect("load state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn state_dir_flag_wins_over_environment() {
    let _env = env_lock();
    let environment_dir = temp_state_dir("environment");
    let argument_dir = temp_state_dir("argument");
    let prior = std::env::var_os("HERDR_PLUGIN_STATE_DIR");
    // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
    unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", &environment_dir) };

    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&argument_dir),
            "-t".into(),
            "argument state".into(),
        ],
        true,
    );

    match prior {
        Some(value) => {
            // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
            unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", value) };
        }
        None => {
            // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
            unsafe { std::env::remove_var("HERDR_PLUGIN_STATE_DIR") };
        }
    }

    assert_eq!(output.code, 0);
    assert_eq!(
        task_store(&argument_dir)
            .load()
            .expect("load argument state")
            .tasks()
            .len(),
        1
    );
    assert!(task_store(&environment_dir)
        .load()
        .expect("load environment state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(environment_dir);
    let _ = std::fs::remove_dir_all(argument_dir);
}
