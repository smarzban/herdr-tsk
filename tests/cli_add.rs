use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use herdr_tasks::cli::{parser, run_with};
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
        panic!("stdin must not be read")
    }
}

fn task_store(dir: &std::path::Path) -> TaskStore {
    TaskStore::new(dir)
}

#[test]
fn add_thread_flag_applies_to_every_item_and_round_trips() {
    let _env = env_lock();
    let dir = temp_state_dir("thread-flag");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "threaded task".into(),
            "--thread".into(),
            "Release-2026".into(),
        ],
        true,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added threaded task\n");
    let state = task_store(&dir).load().expect("reload threaded task");
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].thread.as_deref(), Some("release-2026"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn thread_flag_with_file_is_usage_error_exit_2() {
    let _env = env_lock();
    let dir = temp_state_dir("thread-file-plan");
    let plan = dir.join("plan.json");
    std::fs::write(&plan, r#"[{"title":"from file"}]"#).expect("write plan");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--thread".into(),
            "release".into(),
            "--file".into(),
            state_dir_arg(&plan),
        ],
        true,
    );
    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(output
        .stderr
        .contains("item flags cannot be used with --file"));
    assert!(task_store(&dir)
        .load()
        .expect("load file state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn thread_flag_ignores_piped_plan_and_creates_only_flag_task() {
    let _env = env_lock();
    let dir = temp_state_dir("thread-stdin-plan");
    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--thread",
            "release",
            "-t",
            "flag title",
        ],
        Cursor::new(r#"[{"title":"from stdin"}]"#),
        false,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added flag title\n");
    let state = task_store(&dir).load().expect("load stdin state");
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].title, "flag title");
    assert_eq!(state.tasks()[0].thread.as_deref(), Some("release"));
    assert!(state.tasks().iter().all(|task| task.title != "from stdin"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plan_item_thread_applies_per_item() {
    let _env = env_lock();
    let dir = temp_state_dir("plan-item-thread");
    let plan = dir.join("plan.json");
    std::fs::write(
        &plan,
        r#"[{"title":"threaded from file","thread":"Release-2026"},{"title":"unthreaded from file","thread":null}]"#,
    )
    .expect("write plan");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--file".into(),
            state_dir_arg(&plan),
        ],
        true,
    );

    assert_eq!(output.code, 0);
    let state = task_store(&dir).load().expect("load file plan state");
    assert_eq!(state.tasks().len(), 2);
    assert_eq!(state.tasks()[0].thread.as_deref(), Some("release-2026"));
    assert_eq!(state.tasks()[1].thread, None);

    let stdin_dir = temp_state_dir("plan-item-thread-stdin");
    let stdin = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&stdin_dir),
        ],
        Cursor::new(r#"[{"title":"threaded from stdin","thread":"ops"}]"#),
        false,
    );
    assert_eq!(stdin.code, 0);
    assert_eq!(
        task_store(&stdin_dir)
            .load()
            .expect("load stdin plan state")
            .tasks()[0]
            .thread
            .as_deref(),
        Some("ops")
    );

    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(stdin_dir);
}

#[test]
fn invalid_thread_flag_is_usage_error_exit_2_nothing_persisted() {
    let _env = env_lock();
    let dir = temp_state_dir("invalid-thread-flag");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "valid title".into(),
            "--thread".into(),
            "not_valid".into(),
        ],
        true,
    );

    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.contains("invalid thread"));
    assert!(task_store(&dir)
        .load()
        .expect("load state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_plan_item_thread_fails_item_exit_1_others_persist() {
    let _env = env_lock();
    let dir = temp_state_dir("invalid-plan-thread");
    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"valid thread","thread":"release"},{"title":"invalid thread","thread":"bad_name"},{"title":"valid unthreaded","thread":null}]"#,
        ),
        true,
    );

    assert_eq!(output.code, 1);
    let result: serde_json::Value = serde_json::from_str(&output.stdout).expect("plan result");
    assert_eq!(result["created"].as_array().expect("created").len(), 2);
    assert_eq!(result["failed"][0]["i"], 1);
    assert_eq!(result["failed"][0]["title"], "invalid thread");
    assert_eq!(result["failed"][0]["code"], "invalid-thread");
    let state = task_store(&dir).load().expect("load state");
    assert_eq!(state.tasks().len(), 2);
    assert_eq!(state.tasks()[0].thread.as_deref(), Some("release"));
    assert_eq!(state.tasks()[1].thread, None);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idempotency_key_includes_thread_both_directions() {
    let _env = env_lock();
    let threaded_first_dir = temp_state_dir("thread-idempotency-threaded-first");
    let threaded_first = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&threaded_first_dir),
            "--title".into(),
            "same title".into(),
            "--global".into(),
            "--thread".into(),
            "Release".into(),
        ],
        true,
    );
    assert_eq!(threaded_first.code, 0);
    let unthreaded_second = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&threaded_first_dir),
            "--title".into(),
            "same title".into(),
            "--global".into(),
        ],
        true,
    );
    assert_eq!(unthreaded_second.code, 0);
    assert_eq!(
        task_store(&threaded_first_dir)
            .load()
            .expect("load threaded-first state")
            .tasks()
            .len(),
        2
    );

    let unthreaded_first_dir = temp_state_dir("thread-idempotency-unthreaded-first");
    let unthreaded_first = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&unthreaded_first_dir),
            "--title".into(),
            "same title".into(),
            "--global".into(),
        ],
        true,
    );
    assert_eq!(unthreaded_first.code, 0);
    let threaded_second = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&unthreaded_first_dir),
            "--title".into(),
            "same title".into(),
            "--global".into(),
            "--thread".into(),
            "release".into(),
        ],
        true,
    );
    assert_eq!(threaded_second.code, 0);
    let normalized_duplicate = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&unthreaded_first_dir),
            "--title".into(),
            "same title".into(),
            "--global".into(),
            "--thread".into(),
            "RELEASE".into(),
        ],
        true,
    );
    assert_eq!(normalized_duplicate.code, 0);
    assert_eq!(normalized_duplicate.stdout, "task already exists\n");
    assert_eq!(
        task_store(&unthreaded_first_dir)
            .load()
            .expect("load unthreaded-first state")
            .tasks()
            .len(),
        2
    );

    let plan_dir = temp_state_dir("thread-plan-idempotency");
    let plan = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&plan_dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"plan title","project":null,"thread":null},{"title":"plan title","project":null,"thread":"Release"},{"title":"plan title","project":null,"thread":"release"}]"#,
        ),
        true,
    );
    assert_eq!(plan.code, 0);
    let result: serde_json::Value = serde_json::from_str(&plan.stdout).expect("plan result");
    assert_eq!(result["created"].as_array().expect("created").len(), 2);
    assert_eq!(result["existing"][0]["i"], 2);
    assert_eq!(
        task_store(&plan_dir)
            .load()
            .expect("load plan state")
            .tasks()
            .len(),
        2
    );

    let _ = std::fs::remove_dir_all(threaded_first_dir);
    let _ = std::fs::remove_dir_all(unthreaded_first_dir);
    let _ = std::fs::remove_dir_all(plan_dir);
}

#[cfg(unix)]
struct ReadOnlyDir {
    path: PathBuf,
    permissions: std::fs::Permissions,
}

#[cfg(unix)]
impl ReadOnlyDir {
    fn new(path: &std::path::Path) -> Self {
        let permissions = std::fs::metadata(path)
            .expect("stat state directory")
            .permissions();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555))
            .expect("make state directory read-only");
        Self {
            path: path.to_owned(),
            permissions,
        }
    }
}

#[cfg(unix)]
impl Drop for ReadOnlyDir {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.path, self.permissions.clone());
    }
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
fn flag_add_existing_trimmed_title_and_scope_is_a_successful_noop() {
    let _env = env_lock();
    let dir = temp_state_dir("flag-existing");
    let first = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "same task".into(),
            "--notes".into(),
            "original notes".into(),
            "--global".into(),
        ],
        true,
    );
    assert_eq!(first.code, 0);
    let id = task_store(&dir).load().expect("load task").tasks()[0].id;
    let mut state = task_store(&dir).load().expect("load task for status");
    state
        .set_status(id, HumanStatus::Done)
        .expect("set existing task status");
    task_store(&dir).save(&state).expect("save status");
    let state_file = dir.join("tasks.json");
    let before = std::fs::read(&state_file).expect("read seeded state");
    let before_mtime = std::fs::metadata(&state_file)
        .expect("stat seeded state")
        .modified()
        .expect("state mtime");
    #[cfg(unix)]
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
        .expect("make state dir unwritable");

    let duplicate = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "  same task  ".into(),
            "--notes".into(),
            "different notes".into(),
            "--global".into(),
        ],
        true,
    );

    #[cfg(unix)]
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore state dir permissions");

    assert_eq!(duplicate.code, 0);
    assert_eq!(duplicate.stdout, "task already exists\n");
    assert!(duplicate.stderr.is_empty());
    assert_eq!(
        std::fs::read(&state_file).expect("read unchanged state"),
        before
    );
    assert_eq!(
        std::fs::metadata(&state_file)
            .expect("stat unchanged state")
            .modified()
            .expect("state mtime"),
        before_mtime
    );
    let state = task_store(&dir).load().expect("reload state");
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].status, HumanStatus::Done);
    assert_eq!(state.tasks()[0].notes.as_deref(), Some("original notes"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_ignores_soft_deleted_title_and_scope_matches() {
    let _env = env_lock();
    let dir = temp_state_dir("flag-soft-deleted");
    let store = task_store(&dir);
    let mut seeded = DomainState::new();
    let id = seeded
        .create(
            "same task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed task");
    seeded.soft_delete(id).expect("soft delete seed");
    store.save(&seeded).expect("save seed");

    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title".into(),
            "same task".into(),
            "--global".into(),
        ],
        true,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added same task\n");
    assert_eq!(store.load().expect("reload").tasks().len(), 2);

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
fn mixed_plan_persists_only_valid_items_exits_1() {
    let _env = env_lock();
    let dir = temp_state_dir("mixed-plan");
    let fixture = format!(
        "{}/tests/fixtures/cli_add_mixed_plan.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--file".into(),
            fixture,
        ],
        true,
    );

    assert_eq!(output.code, 1);
    assert!(output.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_str(&output.stdout).expect("tiny result JSON");
    let created = result["created"].as_array().expect("created array");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["i"], 0);
    assert!(created[0]["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(created[0]["title"], "ok");
    assert!(created[0].get("notes").is_none());
    let failed = result["failed"].as_array().expect("failed array");
    assert_eq!(failed.len(), 2);
    assert_eq!(failed[0]["i"], 1);
    assert_eq!(failed[0]["title"], "");
    assert_eq!(failed[0]["code"], "empty-title");
    assert_eq!(failed[1]["i"], 2);
    assert!(failed[1]["title"].is_null());
    assert_eq!(failed[1]["code"], "invalid-item");
    assert_eq!(
        task_store(&dir).load().expect("load state").tasks()[0].title,
        "ok"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn plan_with_only_existing_tasks_is_a_successful_read_only_noop() {
    let _env = env_lock();
    let dir = temp_state_dir("plan-existing");
    let store = task_store(&dir);
    let mut seeded = DomainState::new();
    let id = seeded
        .create(
            "same task",
            Some("old notes".into()),
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed task");
    seeded
        .set_status(id, HumanStatus::Review)
        .expect("set status");
    store.save(&seeded).expect("save seed");
    let state_file = dir.join("tasks.json");
    let before = std::fs::read(&state_file).expect("read seeded state");
    let before_mtime = std::fs::metadata(&state_file)
        .expect("stat seeded state")
        .modified()
        .expect("state mtime");

    let output = {
        let _read_only = ReadOnlyDir::new(&dir);
        run_with(
            [
                "herdr-tasks",
                "add",
                "--state-dir",
                &state_dir_arg(&dir),
                "--file",
                "-",
            ],
            Cursor::new(r#"[{"title":"  same task  ","notes":"new notes","project":null}]"#),
            true,
        )
    };

    assert_eq!(output.code, 0);
    let result: serde_json::Value = serde_json::from_str(&output.stdout).expect("tiny result");
    assert!(result["created"].as_array().expect("created").is_empty());
    assert_eq!(
        result["existing"],
        serde_json::json!([{"i":0,"id":id,"title":"same task"}])
    );
    assert!(result["failed"].as_array().expect("failed").is_empty());
    assert_eq!(
        std::fs::read(&state_file).expect("read unchanged state"),
        before
    );
    assert_eq!(
        std::fs::metadata(&state_file)
            .expect("stat unchanged state")
            .modified()
            .expect("state mtime"),
        before_mtime
    );
    assert_eq!(store.load().expect("reload").tasks().len(), 1);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn mixed_plan_exit_1_preserves_created_and_existing_rows_for_failed_only_retry() {
    let _env = env_lock();
    let dir = temp_state_dir("mixed-existing-failed");
    let store = task_store(&dir);
    let mut seeded = DomainState::new();
    let existing_id = seeded
        .create(
            "already exists",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("seed existing task");
    store.save(&seeded).expect("save seed");

    let initial = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"created now","project":null},{"title":"already exists","project":null},{"title":"\u0000","project":null}]"#,
        ),
        true,
    );

    assert_eq!(initial.code, 1);
    let result: serde_json::Value = serde_json::from_str(&initial.stdout).expect("tiny result");
    assert_eq!(result["created"].as_array().expect("created").len(), 1);
    assert_eq!(result["created"][0]["i"], 0);
    assert_eq!(
        result["existing"],
        serde_json::json!([{"i":1,"id":existing_id,"title":"already exists"}])
    );
    assert_eq!(result["failed"].as_array().expect("failed").len(), 1);
    assert_eq!(result["failed"][0]["i"], 2);
    assert_eq!(result["failed"][0]["code"], "invalid-title");

    let retry = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(r#"[{"title":"recovered failed item","project":null}]"#),
        true,
    );

    assert_eq!(retry.code, 0);
    let retry_result: serde_json::Value =
        serde_json::from_str(&retry.stdout).expect("retry result");
    assert_eq!(
        retry_result["created"].as_array().expect("created").len(),
        1
    );
    assert!(retry_result["existing"]
        .as_array()
        .expect("existing")
        .is_empty());
    assert!(retry_result["failed"]
        .as_array()
        .expect("failed")
        .is_empty());
    let state = store.load().expect("load state after retry");
    assert_eq!(state.tasks().len(), 3);
    assert_eq!(
        state
            .tasks()
            .iter()
            .filter(|task| task.title == "already exists")
            .count(),
        1
    );
    assert_eq!(
        state
            .tasks()
            .iter()
            .filter(|task| task.title == "created now")
            .count(),
        1
    );
    assert_eq!(
        state
            .tasks()
            .iter()
            .filter(|task| task.title == "recovered failed item")
            .count(),
        1
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plan_marks_earlier_accepted_duplicate_as_existing_and_keeps_scopes_distinct() {
    let _env = env_lock();
    let dir = temp_state_dir("plan-duplicate");
    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"same task","project":null},{"title":" same task ","project":null},{"title":"same task","project":"/projects/widget"}]"#,
        ),
        true,
    );

    assert_eq!(output.code, 0);
    let result: serde_json::Value = serde_json::from_str(&output.stdout).expect("tiny result");
    let created = result["created"].as_array().expect("created");
    assert_eq!(created.len(), 2);
    assert_eq!(created[0]["i"], 0);
    assert_eq!(created[1]["i"], 2);
    assert_eq!(result["existing"][0]["i"], 1);
    assert_eq!(result["existing"][0]["id"], created[0]["id"]);
    assert!(result["failed"].as_array().expect("failed").is_empty());
    let state = task_store(&dir).load().expect("load state");
    assert_eq!(state.tasks().len(), 2);
    assert_eq!(state.tasks()[0].scope, TaskScope::Global);
    assert_eq!(
        state.tasks()[1].scope,
        TaskScope::Project {
            path: "/projects/widget".into()
        }
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plan_reads_dash_file_and_piped_stdin_and_allows_empty_array() {
    let _env = env_lock();
    let dash_dir = temp_state_dir("dash-plan");
    let dash = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dash_dir),
            "--file",
            "-",
        ],
        Cursor::new(r#"[{"title":"from dash"}]"#),
        true,
    );
    assert_eq!(dash.code, 0);
    assert_eq!(
        task_store(&dash_dir)
            .load()
            .expect("load dash state")
            .tasks()[0]
            .title,
        "from dash"
    );

    let piped_dir = temp_state_dir("piped-plan");
    let piped = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&piped_dir),
        ],
        Cursor::new(r#"[{"title":"from pipe"}]"#),
        false,
    );
    assert_eq!(piped.code, 0);
    assert_eq!(
        task_store(&piped_dir)
            .load()
            .expect("load piped state")
            .tasks()[0]
            .title,
        "from pipe"
    );

    let empty_dir = temp_state_dir("empty-plan");
    let empty = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&empty_dir),
            "--file",
            "-",
        ],
        Cursor::new("[]"),
        true,
    );
    assert_eq!(empty.code, 0);
    assert_eq!(
        empty.stdout,
        "{\"created\":[],\"existing\":[],\"failed\":[]}\n"
    );
    assert!(task_store(&empty_dir)
        .load()
        .expect("load empty state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(dash_dir);
    let _ = std::fs::remove_dir_all(piped_dir);
    let _ = std::fs::remove_dir_all(empty_dir);
}

#[test]
fn plan_usage_errors_persist_nothing_and_do_not_read_mixed_stdin() {
    let _env = env_lock();
    let object_dir = temp_state_dir("object-plan");
    let object = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&object_dir),
            "--file",
            "-",
        ],
        Cursor::new(r#"{"title":"not an array"}"#),
        true,
    );
    assert_eq!(object.code, 2);
    assert!(object.stdout.is_empty());
    let malformed = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&object_dir),
            "--file",
            "-",
        ],
        Cursor::new("["),
        true,
    );
    assert_eq!(malformed.code, 2);
    assert!(malformed.stdout.is_empty());
    assert!(task_store(&object_dir)
        .load()
        .expect("load object state")
        .tasks()
        .is_empty());

    let mixed_dir = temp_state_dir("mixed-plan-flags");
    let mixed = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&mixed_dir),
            "-t",
            "never read",
            "--file",
            "-",
        ],
        PanicOnRead,
        false,
    );
    assert_eq!(mixed.code, 2);
    assert!(mixed.stdout.is_empty());
    assert!(task_store(&mixed_dir)
        .load()
        .expect("load mixed state")
        .tasks()
        .is_empty());

    let _ = std::fs::remove_dir_all(object_dir);
    let _ = std::fs::remove_dir_all(mixed_dir);
}

#[test]
fn plan_projects_and_item_validation_follow_the_contract() {
    let _env = env_lock();
    let repo = temp_state_dir("plan-default-repo");
    std::fs::create_dir(repo.join(".git")).expect("create git marker");
    let dir = temp_state_dir("plan-validation");
    let prior = std::env::var_os("HERDR_PLUGIN_CONTEXT_JSON");
    let context = format!(
        r#"{{"focused_pane_cwd":{}}}"#,
        serde_json::to_string(&repo).expect("serialize repo")
    );
    // SAFETY: ENV_LOCK serializes this test's process-wide environment mutation.
    unsafe { std::env::set_var("HERDR_PLUGIN_CONTEXT_JSON", context) };

    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"global","project":null},{"title":"default"},{"title":"extra","unknown":true,"notes":null},{"title":"  bad notes  ","notes":42},{"title":"bad project","project":42},{"title":"bad\n"}]"#,
        ),
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

    assert_eq!(output.code, 1);
    let result: serde_json::Value = serde_json::from_str(&output.stdout).expect("tiny result JSON");
    assert_eq!(result["created"].as_array().expect("created").len(), 3);
    assert_eq!(result["failed"][0]["code"], "invalid-item");
    assert_eq!(result["failed"][0]["title"], "bad notes");
    assert_eq!(result["failed"][1]["code"], "invalid-item");
    assert_eq!(result["failed"][1]["title"], "bad project");
    assert_eq!(result["failed"][2]["code"], "invalid-title");
    let state = task_store(&dir).load().expect("load state");
    assert_eq!(state.tasks()[0].scope, TaskScope::Global);
    assert_eq!(
        state.tasks()[1].scope,
        TaskScope::Project {
            path: repo.to_string_lossy().into_owned()
        }
    );
    assert_eq!(state.tasks()[2].title, "extra");
    assert!(state.tasks()[2].notes.is_none());

    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plan_project_string_uses_the_shared_basename_resolver() {
    let _env = env_lock();
    let dir = temp_state_dir("plan-project");
    let store = task_store(&dir);
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

    let output = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&dir),
            "--file",
            "-",
        ],
        Cursor::new(r#"[{"title":"resolved","project":"widget"}]"#),
        true,
    );

    assert_eq!(output.code, 0);
    assert_eq!(
        task_store(&dir).load().expect("load state").tasks()[1].scope,
        TaskScope::Project {
            path: "/projects/Widget".into()
        }
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plan_project_resolution_is_independent_of_item_order() {
    let _env = env_lock();
    let forward_dir = temp_state_dir("plan-project-order-forward");
    let forward = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&forward_dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"explicit first","project":"/repos/Widget"},{"title":"bare second","project":"widget"}]"#,
        ),
        true,
    );
    assert_eq!(forward.code, 0);
    let forward_state = task_store(&forward_dir).load().expect("load forward state");
    assert_eq!(
        forward_state.tasks()[0].scope,
        TaskScope::Project {
            path: "/repos/Widget".into()
        }
    );
    assert_eq!(
        forward_state.tasks()[1].scope,
        TaskScope::Project {
            path: "widget".into()
        }
    );

    let reverse_dir = temp_state_dir("plan-project-order-reverse");
    let reverse = run_with(
        [
            "herdr-tasks",
            "add",
            "--state-dir",
            &state_dir_arg(&reverse_dir),
            "--file",
            "-",
        ],
        Cursor::new(
            r#"[{"title":"bare first","project":"widget"},{"title":"explicit second","project":"/repos/Widget"}]"#,
        ),
        true,
    );
    assert_eq!(reverse.code, 0);
    let reverse_state = task_store(&reverse_dir).load().expect("load reverse state");
    assert_eq!(
        reverse_state.tasks()[0].scope,
        TaskScope::Project {
            path: "widget".into()
        }
    );
    assert_eq!(
        reverse_state.tasks()[1].scope,
        TaskScope::Project {
            path: "/repos/Widget".into()
        }
    );

    let _ = std::fs::remove_dir_all(forward_dir);
    let _ = std::fs::remove_dir_all(reverse_dir);
}

#[test]
fn flag_add_rejects_flag_like_item_values_without_persisting() {
    let _env = env_lock();

    for (label, item_flags) in [
        ("title", vec!["--title", "--global"]),
        (
            "notes",
            vec!["--title", "ordinary title", "--notes", "--global"],
        ),
        (
            "project",
            vec!["--title", "ordinary title", "--project", "--global"],
        ),
    ] {
        let dir = temp_state_dir(label);
        let mut args = vec![
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
        ];
        args.extend(item_flags.into_iter().map(String::from));

        let output = add(&args, true);

        assert_eq!(output.code, 2, "{label}");
        assert!(output.stdout.is_empty(), "{label}");
        assert!(output.stderr.contains("missing value"), "{label}");
        assert!(task_store(&dir)
            .load()
            .expect("load state")
            .tasks()
            .is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn flag_add_equals_forms_allow_dash_leading_values() {
    let _env = env_lock();
    let dir = temp_state_dir("dash-leading-values");
    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "--title=-fix parser".into(),
            "--notes=-5 degrees".into(),
            "--project=-maintenance".into(),
        ],
        true,
    );

    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "added -fix parser\n");
    let state = task_store(&dir).load().expect("load state");
    let task = &state.tasks()[0];
    assert_eq!(task.title, "-fix parser");
    assert_eq!(task.notes.as_deref(), Some("-5 degrees"));
    assert_eq!(
        task.scope,
        TaskScope::Project {
            path: "-maintenance".into()
        }
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn flag_add_equals_state_dir_and_file_forms_accept_dash_leading_values() {
    let cwd = temp_state_dir("equals-dash-paths");
    std::fs::write(
        cwd.join("-plan.json"),
        r#"[{"title":"equals file task","project":null}]"#,
    )
    .expect("write dash-leading plan");
    let binary = std::env::var("CARGO_BIN_EXE_herdr-tasks")
        .expect("Cargo must provide the herdr-tasks binary path");

    let output = Command::new(binary)
        .current_dir(&cwd)
        .args(["add", "--state-dir=-state", "--file=-plan.json"])
        .output()
        .expect("run equals forms");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        task_store(&cwd.join("-state"))
            .load()
            .expect("load equals state")
            .tasks()[0]
            .title,
        "equals file task"
    );

    let stdin = parser::parse_flag_add(&["herdr-tasks".into(), "add".into(), "--file=-".into()])
        .expect("parse stdin file marker");
    assert_eq!(stdin.file, Some(PathBuf::from("-")));

    let _ = std::fs::remove_dir_all(cwd);
}

#[test]
fn flag_add_rejects_flag_like_state_dir_and_file_values_without_mutating() {
    let _env = env_lock();
    let cwd = temp_state_dir("flag-like-global-value");
    let state_dir = temp_state_dir("flag-like-global-state");
    let binary = std::env::var("CARGO_BIN_EXE_herdr-tasks")
        .expect("Cargo must provide the herdr-tasks binary path");

    let state_dir_output = Command::new(&binary)
        .current_dir(&cwd)
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .args(["add", "--state-dir", "--global", "--title", "junk"])
        .output()
        .expect("run flag-like state-dir value");
    assert_eq!(state_dir_output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&state_dir_output.stderr).contains("missing value for --state-dir")
    );
    assert!(!cwd.join("--global").exists());

    let file = Command::new(binary)
        .current_dir(&cwd)
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .args(["add", "--file", "--global"])
        .output()
        .expect("run flag-like file value");
    assert_eq!(file.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&file.stderr).contains("missing value for --file"));
    assert!(!cwd.join("--global").exists());

    let _ = std::fs::remove_dir_all(cwd);
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn flag_add_json_reports_created_and_existing_resolved_tasks() {
    let _env = env_lock();
    let dir = temp_state_dir("json");
    let args = [
        "herdr-tasks".into(),
        "add".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
        "-t".into(),
        "  json task  ".into(),
        "-p".into(),
        "/projects/json".into(),
    ];

    let created = add(&args, true);
    assert_eq!(created.code, 0);
    assert!(created.stderr.is_empty());
    let created: serde_json::Value = serde_json::from_str(&created.stdout).expect("created JSON");
    assert_eq!(created["outcome"], "created");
    assert_eq!(created["title"], "json task");
    assert_eq!(created["project"], "/projects/json");
    let id = created["id"].as_str().expect("created id").to_owned();
    assert_eq!(created.as_object().expect("created object").len(), 4);

    let existing = add(&args, true);
    assert_eq!(existing.code, 0);
    assert!(existing.stderr.is_empty());
    let existing: serde_json::Value =
        serde_json::from_str(&existing.stdout).expect("existing JSON");
    assert_eq!(existing["outcome"], "existing");
    assert_eq!(existing["id"], id);
    assert_eq!(existing["title"], "json task");
    assert_eq!(existing["project"], "/projects/json");
    assert_eq!(existing.as_object().expect("existing object").len(), 4);

    let global = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--json".into(),
            "--state-dir".into(),
            state_dir_arg(&dir),
            "-t".into(),
            "global JSON task".into(),
            "--global".into(),
        ],
        true,
    );
    assert_eq!(global.code, 0);
    let global: serde_json::Value = serde_json::from_str(&global.stdout).expect("global JSON");
    assert_eq!(global["outcome"], "created");
    assert!(global["project"].is_null());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn add_when_state_dir_is_a_file_exits_3() {
    let _env = env_lock();
    let parent = temp_state_dir("state-dir-file");
    let state_file = parent.join("not-a-directory");
    std::fs::write(&state_file, "not a directory").expect("create state-dir file");

    let output = add(
        &[
            "herdr-tasks".into(),
            "add".into(),
            "--state-dir".into(),
            state_dir_arg(&state_file),
            "--title".into(),
            "cannot persist".into(),
        ],
        true,
    );

    assert_eq!(output.code, 3);
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());

    let _ = std::fs::remove_dir_all(parent);
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
