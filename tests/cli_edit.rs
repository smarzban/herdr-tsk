//! Edit CLI: `tsk edit T<n> --title/--notes`. Uses temp dirs only.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::cli::{run_with, CliOutput};
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskEventKind, TaskScope};
use tsk_tui::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-cli-edit-{label}-{nanos}-{seq}"));
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
        "-n".into(),
        "original notes".into(),
        "--desk".into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ])
}

fn edit(dir: &Path, args: &[&str]) -> CliOutput {
    let mut command = vec![
        "tsk".into(),
        "edit".into(),
        "--state-dir".into(),
        dir.to_string_lossy().into_owned(),
    ];
    command.extend(args.iter().map(|argument| (*argument).to_string()));
    cli(command)
}

#[test]
fn edit_title_and_notes_and_repeat_is_idempotent() {
    let dir = temp_state_dir("round-trip");
    let _guard = TempDirGuard(dir.clone());
    assert_eq!(add_task(&dir, "old title").code, 0);

    let titled = edit(&dir, &["T1", "--title", "  new title  "]);
    assert_eq!(titled.code, 0, "{:?}", titled.stderr);
    assert_eq!(titled.stdout, "edited T1 new title\n");
    let state = TaskStore::new(&dir).load().expect("load");
    let task = &state.tasks()[0];
    assert_eq!(task.title, "new title");
    assert_eq!(task.notes.as_deref(), Some("original notes"));

    let noted = edit(&dir, &["T1", "--notes", "updated notes"]);
    assert_eq!(noted.code, 0, "{:?}", noted.stderr);
    let state = TaskStore::new(&dir).load().expect("load");
    let task = &state.tasks()[0];
    assert_eq!(task.title, "new title");
    assert_eq!(task.notes.as_deref(), Some("updated notes"));

    let events = task
        .history
        .iter()
        .filter(|event| event.kind == TaskEventKind::Edited)
        .count();
    let again = edit(
        &dir,
        &["T1", "--title", "new title", "--notes", "updated notes"],
    );
    assert_eq!(again.code, 0, "{:?}", again.stderr);
    assert_eq!(again.stdout, "edited T1 new title\n");
    let state = TaskStore::new(&dir).load().expect("load");
    let after = state.tasks()[0]
        .history
        .iter()
        .filter(|event| event.kind == TaskEventKind::Edited)
        .count();
    assert_eq!(after, events, "a repeat edit writes no second event");
}

#[test]
fn edit_clears_notes_and_keeps_scope_and_thread() {
    let dir = temp_state_dir("preserve");
    let _guard = TempDirGuard(dir.clone());
    let mut state = DomainState::new();
    let id = state
        .create(
            "keep scope",
            Some("drop these".into()),
            TaskScope::Project {
                path: "/tmp/edit-cli-project".into(),
            },
            ProvenanceOrigin::Manual,
            Some("v6".into()),
        )
        .expect("seed");
    TaskStore::new(&dir).save(&state).expect("save seed");
    let number = TaskStore::new(&dir)
        .load()
        .expect("reload")
        .get(id)
        .expect("task")
        .number
        .expect("number");

    let output = edit(&dir, &[&format!("T{number}"), "--notes="]);
    assert_eq!(output.code, 0, "{:?}", output.stderr);
    let task = TaskStore::new(&dir).load().expect("load").tasks()[0].clone();
    assert!(
        task.notes.is_none(),
        "whitespace-only notes clear the field"
    );
    assert_eq!(
        task.scope,
        TaskScope::Project {
            path: "/tmp/edit-cli-project".into(),
        }
    );
    assert_eq!(task.thread.as_deref(), Some("v6"));
    assert_eq!(task.title, "keep scope");
}

#[test]
fn edit_refuses_empty_title_and_control_chars_without_mutation() {
    let dir = temp_state_dir("refusals");
    let _guard = TempDirGuard(dir.clone());
    assert_eq!(add_task(&dir, "keep me").code, 0);
    let state_file = dir.join("tsk.json");
    let before = fs::read(&state_file).expect("read seeded state");

    for (args, token) in [
        (["T1", "--title", "   "].as_slice(), "empty-title"),
        (["T1", "--title", "line\nbreak"].as_slice(), "invalid-title"),
        (["T1", "--notes", "line\nbreak"].as_slice(), "invalid-notes"),
    ] {
        let output = edit(&dir, args);
        assert_eq!(output.code, 1, "{token}: {:?}", output.stderr);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains(token), "{token}: {}", output.stderr);
        assert_eq!(
            fs::read(&state_file).expect("read after refusal"),
            before,
            "{token} must persist nothing"
        );
    }
}

#[test]
fn edit_unknown_and_deleted_refuse() {
    let dir = temp_state_dir("missing");
    let _guard = TempDirGuard(dir.clone());
    let missing = edit(&dir, &["T9", "--title", "nope"]);
    assert_eq!(missing.code, 1);
    assert!(missing.stderr.contains("unknown-task"));

    assert_eq!(add_task(&dir, "delete me").code, 0);
    let mut state = TaskStore::new(&dir).load().expect("load");
    let id = state.tasks()[0].id;
    state.soft_delete(id).expect("soft delete");
    TaskStore::new(&dir).save(&state).expect("save deleted");
    let deleted = edit(&dir, &["T1", "--title", "nope"]);
    assert_eq!(deleted.code, 1);
    assert!(deleted.stderr.contains("soft-deleted-task"));
}

#[test]
fn edit_without_fields_is_usage() {
    let dir = temp_state_dir("usage");
    let _guard = TempDirGuard(dir.clone());
    assert_eq!(add_task(&dir, "keep me").code, 0);
    let output = edit(&dir, &["T1"]);
    assert_eq!(output.code, 2);
    assert!(output.stderr.contains("title or notes is required"));
}

#[test]
fn edit_help_names_title_notes_and_equals_forms() {
    let output = cli(vec!["tsk".into(), "edit".into(), "--help".into()]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stderr.is_empty());
    for term in [
        "usage: tsk edit",
        "--title",
        "--notes",
        "--title=<value>",
        "--notes=<value>",
        "idempotent",
        "empty-title",
        "invalid-title",
        "invalid-notes",
        "exit 0",
        "exit 1",
        "exit 2",
        "exit 3",
        "nothing persisted",
        "indeterminate",
    ] {
        assert!(
            output.stdout.contains(term),
            "edit help should contain {term:?}"
        );
    }
}
