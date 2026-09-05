//! Human CLI output must not emit raw terminal controls from stored text.
//! Isolated state only; output is captured, never sent to a live terminal.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::cli::{run_with, CliOutput};
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

const TITLE: &str = "safe\u{001b}[2Jtitle";
const ESCAPED_TITLE: &str = "safe\\u{001b}[2Jtitle";
const STEP: &str = "step\u{001b}[2Jtext";
const ESCAPED_STEP: &str = "step\\u{001b}[2Jtext";
const PROJECT_PATH: &str = "/tmp/safe\u{001b}[2Jproj";
const ESCAPED_PROJECT: &str = "safe\\u{001b}[2Jproj";

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-cli-escape-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(args: Vec<String>) -> CliOutput {
    run_with(args, Cursor::new(Vec::<u8>::new()), true)
}

fn state_dir_arg(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

fn seed_task(dir: &Path, title: &str, scope: TaskScope) -> (u64, uuid::Uuid) {
    let mut state = DomainState::new();
    let id = state
        .create(title, None, scope, ProvenanceOrigin::Manual, None)
        .expect("seed task");
    TaskStore::new(dir).save(&state).expect("save seed");
    let loaded = TaskStore::new(dir).load().expect("reload");
    let task = loaded.get(id).expect("seeded task");
    (task.number.expect("number"), id)
}

fn assert_human_safe(output: &CliOutput, escaped: &str) {
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stderr.is_empty(), "{}", output.stderr);
    assert!(
        output.stdout.contains(escaped),
        "escaped form missing: {}",
        output.stdout
    );
    assert!(
        !output.stdout.contains('\u{001b}'),
        "raw ESC in stdout: {:?}",
        output.stdout.as_bytes()
    );
    assert!(
        output.stdout.contains("safe") || output.stdout.contains("step"),
        "ordinary text should stay readable: {}",
        output.stdout
    );
}

#[test]
fn archive_and_unarchive_escape_control_titles() {
    let dir = temp_state_dir("archive");
    let _guard = TempDirGuard(dir.clone());
    let (number, _) = seed_task(&dir, TITLE, TaskScope::Global);
    let address = format!("T{number}");

    let listed = cli(vec![
        "tsk".into(),
        "list".into(),
        address.clone(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&listed, ESCAPED_TITLE);

    let archived = cli(vec![
        "tsk".into(),
        "archive".into(),
        address.clone(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&archived, ESCAPED_TITLE);
    assert!(archived.stdout.starts_with(&format!("archived {address} ")));

    let unarchived = cli(vec![
        "tsk".into(),
        "unarchive".into(),
        address,
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&unarchived, ESCAPED_TITLE);
}

#[test]
fn steps_toggle_escapes_control_step_text() {
    let dir = temp_state_dir("steps");
    let _guard = TempDirGuard(dir.clone());
    let mut state = DomainState::new();
    let id = state
        .create(
            TITLE,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    state.add_step(id, STEP).expect("seed step");
    TaskStore::new(&dir).save(&state).expect("save seed");
    let loaded = TaskStore::new(&dir).load().expect("reload");
    let number = loaded.get(id).expect("task").number.expect("number");
    let listed = cli(vec![
        "tsk".into(),
        "list".into(),
        format!("T{number}"),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&listed.stdout).expect("json");
    assert_eq!(rows[0]["steps"][0]["text"], STEP);
    let short = rows[0]["steps"][0]["short_id"]
        .as_str()
        .expect("short id")
        .to_string();

    let toggled = cli(vec![
        "tsk".into(),
        "steps".into(),
        format!("T{number}"),
        "toggle".into(),
        short,
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&toggled, ESCAPED_STEP);
    assert!(toggled.stdout.contains("toggled"));
}

#[test]
fn project_archive_escapes_control_project_names() {
    let dir = temp_state_dir("project");
    let _guard = TempDirGuard(dir.clone());
    seed_task(
        &dir,
        "widget",
        TaskScope::Project {
            path: PROJECT_PATH.into(),
        },
    );

    let archived = cli(vec![
        "tsk".into(),
        "project".into(),
        "archive".into(),
        PROJECT_PATH.into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&archived, ESCAPED_PROJECT);

    let unarchived = cli(vec![
        "tsk".into(),
        "project".into(),
        "unarchive".into(),
        PROJECT_PATH.into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&unarchived, ESCAPED_PROJECT);

    let unknown = cli(vec![
        "tsk".into(),
        "project".into(),
        "archive".into(),
        format!("missing\u{001b}[2Jname"),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(unknown.code, 1);
    assert!(unknown.stdout.is_empty());
    assert!(
        unknown.stderr.contains("missing\\u{001b}[2Jname"),
        "{}",
        unknown.stderr
    );
    assert!(!unknown.stderr.contains('\u{001b}'));
}

#[test]
fn trash_restore_escapes_control_titles() {
    let dir = temp_state_dir("trash");
    let _guard = TempDirGuard(dir.clone());
    let mut state = DomainState::new();
    let deleted = state
        .create(
            TITLE,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("deleted");
    let other = state
        .create(
            "other",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("other");
    state.soft_delete(deleted).expect("soft delete");
    state.complete(other).expect("later undoable action");
    TaskStore::new(&dir).save(&state).expect("save");

    let listed = cli(vec![
        "tsk".into(),
        "list".into(),
        "--deleted".into(),
        "--desk".into(),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&listed.stdout).expect("json");
    let row = rows
        .iter()
        .find(|row| row["title"] == TITLE)
        .expect("trashed title in JSON");
    let number = row["number"].as_u64().expect("number");

    let restored = cli(vec![
        "tsk".into(),
        "trash".into(),
        "restore".into(),
        format!("T{number}"),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_human_safe(&restored, ESCAPED_TITLE);
}

#[test]
fn json_list_keeps_raw_control_values() {
    let dir = temp_state_dir("json");
    let _guard = TempDirGuard(dir.clone());
    let (number, _) = seed_task(&dir, TITLE, TaskScope::Global);
    let json = cli(vec![
        "tsk".into(),
        "list".into(),
        format!("T{number}"),
        "--json".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(json.code, 0, "{}", json.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).expect("json");
    assert_eq!(rows[0]["title"], TITLE);
}
