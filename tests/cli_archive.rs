//! Archive CLI: `tsk archive T<n>` / `tsk unarchive T<n>` (AC-29, AC-31).
//! Uses temp dirs only; never writes real plugin state.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::cli::{run_with, CliOutput};
use tsk_tui::domain::TaskEventKind;
use tsk_tui::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-cli-archive-{label}-{nanos}-{seq}"));
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

fn add_task(dir: &Path, title: &str) -> CliOutput {
    cli(vec![
        "tsk".into(),
        "add".into(),
        "-t".into(),
        title.into(),
        "--desk".into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ])
}

fn archive(dir: &Path, verb: &str, task: &str) -> CliOutput {
    cli(vec![
        "tsk".into(),
        verb.into(),
        task.into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ])
}

fn archived_flag_and_events(dir: &Path) -> (bool, usize) {
    let state = TaskStore::new(dir).load().expect("load store");
    let task = state.tasks().first().expect("seeded task");
    (
        task.archived,
        task.history
            .iter()
            .filter(|event| event.kind == TaskEventKind::Archived)
            .count(),
    )
}

#[test]
fn archive_and_unarchive_by_number_exit_0_and_repeat_is_idempotent() {
    let dir = temp_state_dir("idempotent");
    let _guard = TempDirGuard(dir.clone());
    let added = add_task(&dir, "file me");
    assert_eq!(added.code, 0, "{:?}", added.stderr);

    let first = archive(&dir, "archive", "T1");
    assert_eq!(first.code, 0, "{:?}", first.stderr);
    assert_eq!(first.stdout, "archived T1 file me\n");
    let (flag, events) = archived_flag_and_events(&dir);
    assert!(flag);
    assert_eq!(events, 1, "exactly one archived event after one archive");

    // Repeat prints the same and stays exit 0 with no second event.
    let second = archive(&dir, "archive", "T1");
    assert_eq!(second.code, 0, "{:?}", second.stderr);
    assert_eq!(second.stdout, "archived T1 file me\n");
    let (flag, events) = archived_flag_and_events(&dir);
    assert!(flag);
    assert_eq!(events, 1, "a repeat archive writes no second event");

    let un = archive(&dir, "unarchive", "T1");
    assert_eq!(un.code, 0, "{:?}", un.stderr);
    assert_eq!(un.stdout, "unarchived T1 file me\n");
    let un2 = archive(&dir, "unarchive", "T1");
    assert_eq!(un2.code, 0);
    assert_eq!(un2.stdout, "unarchived T1 file me\n");
    let (flag, events) = archived_flag_and_events(&dir);
    assert!(!flag);
    assert_eq!(
        events, 1,
        "unarchive never touches the archived event count"
    );
}

#[test]
fn archive_of_an_unknown_number_or_a_soft_deleted_task_exits_1_with_a_message() {
    let dir = temp_state_dir("refusals");
    let _guard = TempDirGuard(dir.clone());
    let added = add_task(&dir, "delete me later");
    assert_eq!(added.code, 0);

    let unknown = archive(&dir, "archive", "T99");
    assert_eq!(unknown.code, 1, "{:?}", unknown.stdout);
    assert!(
        unknown.stderr.contains("T99"),
        "the refusal names the address: {:?}",
        unknown.stderr
    );

    // Soft-delete the task, then archive must refuse and unarchive must refuse too.
    let deleted = cli(vec![
        "tsk".into(),
        "add".into(),
        "-t".into(),
        "gone".into(),
        "--desk".into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ]);
    assert_eq!(deleted.code, 0);
    let state = TaskStore::new(&dir).load().expect("load");
    let gone_id = state
        .tasks()
        .iter()
        .find(|task| task.title == "gone")
        .expect("gone task")
        .id;
    let mut writable = TaskStore::new(&dir).load().expect("load");
    writable.soft_delete(gone_id).expect("soft delete");
    TaskStore::new(&dir)
        .save(&writable)
        .expect("persist the soft delete");

    let archived_deleted = archive(&dir, "archive", "T2");
    assert_eq!(archived_deleted.code, 1, "{:?}", archived_deleted.stdout);
    assert!(
        archived_deleted.stderr.contains("deleted"),
        "the refusal says the task is deleted: {:?}",
        archived_deleted.stderr
    );
    let unarchived_deleted = archive(&dir, "unarchive", "T2");
    assert_eq!(unarchived_deleted.code, 1);
}

#[test]
fn project_archive_and_unarchive_resolve_basename_and_path_exit_0_and_are_idempotent() {
    let dir = temp_state_dir("project-verbs");
    let _guard = TempDirGuard(dir.clone());
    let added = cli(vec![
        "tsk".into(),
        "add".into(),
        "-t".into(),
        "widget task".into(),
        "-p".into(),
        "/tmp/x/widget".into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ]);
    assert_eq!(added.code, 0, "{:?}", added.stderr);

    let store = || TaskStore::new(&dir).load().expect("load store");
    let project = |verb: &str, name: &str| {
        cli(vec![
            "tsk".into(),
            "project".into(),
            verb.into(),
            name.into(),
            "--state-dir".into(),
            dir.to_string_lossy().into_owned(),
        ])
    };

    // Basename, case-insensitive.
    let out = project("archive", "widget");
    assert_eq!(out.code, 0, "{:?}", out.stderr);
    assert_eq!(out.stdout, "archived project widget\n");
    assert!(store().is_project_archived("/tmp/x/widget"));

    // Idempotent repeat.
    let again = project("archive", "widget");
    assert_eq!(again.code, 0);
    assert_eq!(again.stdout, "archived project widget\n");
    assert_eq!(store().projects().len(), 1, "exactly one record");

    // Case-insensitive basename is the same project.
    let upper = project("archive", "WIDGET");
    assert_eq!(upper.code, 0, "{:?}", upper.stderr);
    assert_eq!(store().projects().len(), 1);

    // Verbatim path addresses the same project.
    let verbatim = project("archive", "/tmp/x/widget");
    assert_eq!(verbatim.code, 0, "{:?}", verbatim.stderr);
    assert_eq!(store().projects().len(), 1);

    // Unarchive removes the record.
    let un = project("unarchive", "widget");
    assert_eq!(un.code, 0, "{:?}", un.stderr);
    assert_eq!(un.stdout, "unarchived project widget\n");
    assert!(
        !store().is_project_archived("/tmp/x/widget"),
        "the record is gone"
    );
    assert_eq!(store().projects().len(), 0);

    // An unknown name exits 1.
    let unknown = project("archive", "nothing-here");
    assert_eq!(unknown.code, 1, "{:?}", unknown.stdout);
    assert!(
        unknown
            .stderr
            .contains("no project named nothing-here has tasks"),
        "{:?}",
        unknown.stderr
    );
}

#[test]
fn unarchive_refusals_name_the_unarchive_verb() {
    let dir = temp_state_dir("verb-refusal");
    let _guard = TempDirGuard(dir.clone());
    let added = add_task(&dir, "verb check");
    assert_eq!(added.code, 0);

    let unknown = archive(&dir, "unarchive", "T99");
    assert_eq!(unknown.code, 1);
    assert!(
        unknown
            .stderr
            .starts_with("tsk unarchive: T99 is not on the board"),
        "{:?}",
        unknown.stderr
    );
}
