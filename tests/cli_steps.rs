use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::cli::run_with;
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::store::TaskStore;
use uuid::Uuid;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-cli-steps-{label}-{nanos}-{seq}"));
    std::fs::create_dir_all(&dir).expect("create state directory");
    dir
}

fn state_dir_arg(dir: &std::path::Path) -> String {
    dir.to_string_lossy().into_owned()
}

fn steps(args: &[String]) -> tsk_tui::cli::CliOutput {
    run_with(args, Cursor::new(Vec::<u8>::new()), true)
}

fn seed_task(dir: &std::path::Path, title: &str) -> Uuid {
    let mut state = DomainState::new();
    let id = state
        .create(
            title,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    TaskStore::new(dir).save(&state).expect("seed store");
    id
}

fn steps_args(dir: &std::path::Path, task: Uuid) -> Vec<String> {
    vec![
        "tsk".into(),
        "steps".into(),
        task.to_string(),
        "--state-dir".into(),
        state_dir_arg(dir),
    ]
}

fn task_number(dir: &std::path::Path, task: Uuid) -> u64 {
    TaskStore::new(dir)
        .load()
        .expect("load state")
        .get(task)
        .expect("seed task")
        .number
        .expect("persisted task number")
}

/// One `[state] short-id text` line from single-task list output.
struct StepLine {
    done: bool,
    short_id: String,
    text: String,
}

fn step_lines(output: &str) -> Vec<StepLine> {
    output
        .lines()
        .filter(|line| line.contains("[x]") || line.contains("[ ]"))
        .map(|line| {
            let trimmed = line.trim_start();
            let done = trimmed.starts_with("[x]");
            let rest = trimmed[3..].trim_start();
            let (short_id, text) = rest.split_once(' ').unwrap_or((rest, ""));
            StepLine {
                done,
                short_id: short_id.to_owned(),
                text: text.to_owned(),
            }
        })
        .collect()
}

#[test]
fn steps_add_then_toggle_round_trips_step_state() {
    let dir = temp_state_dir("round-trip");
    let task = seed_task(&dir, "round trip target");

    let mut add_first = steps_args(&dir, task);
    add_first.extend(["add".into(), "  first step  ".into()]);
    let add_first = steps(&add_first);
    assert_eq!(add_first.code, 0, "add first step: {}", add_first.stderr);
    assert!(add_first.stderr.is_empty());

    let mut add_second = steps_args(&dir, task);
    add_second.extend(["add".into(), "second step".into()]);
    let add_second = steps(&add_second);
    assert_eq!(add_second.code, 0, "add second step: {}", add_second.stderr);

    let listed = steps(&[
        "tsk".into(),
        "list".into(),
        task.to_string(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    assert_eq!(listed.code, 0, "list task: {}", listed.stderr);
    let lines = step_lines(&listed.stdout);
    assert_eq!(lines.len(), 2, "one line per step: {}", listed.stdout);
    assert!(!lines[0].done);
    assert!(!lines[1].done);
    assert_eq!(
        lines[0].text, "first step",
        "text is trimmed: {}",
        listed.stdout
    );
    assert_eq!(lines[1].text, "second step");

    let mut toggle = steps_args(&dir, task);
    toggle.extend(["toggle".into(), lines[0].short_id.clone()]);
    let toggle = steps(&toggle);
    assert_eq!(toggle.code, 0, "toggle by short id: {}", toggle.stderr);

    let after_toggle = steps(&[
        "tsk".into(),
        "list".into(),
        task.to_string(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);
    let lines = step_lines(&after_toggle.stdout);
    assert!(lines[0].done, "first step checked: {}", after_toggle.stdout);
    assert!(
        !lines[1].done,
        "second step untouched: {}",
        after_toggle.stdout
    );
    assert_eq!(lines[0].short_id, step_lines(&listed.stdout)[0].short_id);

    let mut toggle_back = steps_args(&dir, task);
    toggle_back.extend(["toggle".into(), lines[0].short_id.clone()]);
    let toggle_back = steps(&toggle_back);
    assert_eq!(toggle_back.code, 0, "toggle back: {}", toggle_back.stderr);

    let state = TaskStore::new(&dir).load().expect("reload store");
    let steps_on_task = &state.tasks()[0].steps;
    assert_eq!(steps_on_task.len(), 2);
    assert!(
        !steps_on_task[0].done,
        "first step round-trips back to open"
    );
    assert!(!steps_on_task[1].done);
    assert_eq!(steps_on_task[0].text, "first step");
    assert_eq!(steps_on_task[1].text, "second step");

    let _ = std::fs::remove_dir_all(dir);
}

/// Two steps whose ids share the prefix `abcdef01`, shaped through the
/// store document because step ids are otherwise minted by the domain.
fn state_with_shared_prefix_steps() -> (DomainState, Uuid) {
    let mut state = DomainState::new();
    let task = state
        .create(
            "prefix target",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    state.add_step(task, "shared one").expect("seed first step");
    state
        .add_step(task, "shared two")
        .expect("seed second step");
    let mut document = serde_json::to_value(&state).expect("serialize seed state");
    for (index, id) in [
        "abcdef01-0000-4000-8000-000000000001",
        "abcdef01-0000-4000-8000-000000000002",
    ]
    .into_iter()
    .enumerate()
    {
        document["tasks"][0]["steps"][index]["id"] =
            serde_json::json!(Uuid::parse_str(id).expect("shaped step id"));
    }
    let shaped = serde_json::from_value(document).expect("state with shaped step ids");
    (shaped, task)
}

#[test]
fn steps_toggle_unknown_or_ambiguous_prefix_refuses_without_mutation() {
    let dir = temp_state_dir("prefix-refusal");
    let (state, task) = state_with_shared_prefix_steps();
    TaskStore::new(&dir).save(&state).expect("seed store");
    let state_file = dir.join("tsk.json");
    let before = std::fs::read(&state_file).expect("read seeded state");

    let mut ambiguous = steps_args(&dir, task);
    ambiguous.extend(["toggle".into(), "abcdef01".into()]);
    let ambiguous = steps(&ambiguous);
    assert_eq!(ambiguous.code, 1);
    assert!(ambiguous.stdout.is_empty());
    assert!(
        ambiguous.stderr.to_lowercase().contains("ambiguous"),
        "ambiguous refusal names ambiguity: {}",
        ambiguous.stderr
    );
    assert_eq!(
        std::fs::read(&state_file).expect("read store after ambiguous"),
        before,
        "ambiguous prefix must persist nothing"
    );

    let mut unknown = steps_args(&dir, task);
    unknown.extend(["toggle".into(), "12341234".into()]);
    let unknown = steps(&unknown);
    assert_eq!(unknown.code, 1);
    assert!(unknown.stdout.is_empty());
    assert!(unknown.stderr.contains("unknown-step"));
    assert_eq!(
        std::fs::read(&state_file).expect("read store after unknown"),
        before,
        "unknown prefix must persist nothing"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_toggle_empty_operand_refuses_without_mutation() {
    let dir = temp_state_dir("empty-toggle");
    let mut state = DomainState::new();
    let task = state
        .create(
            "empty operand target",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    state.add_step(task, "only step").expect("seed one step");
    TaskStore::new(&dir).save(&state).expect("seed store");
    let state_file = dir.join("tsk.json");
    let before = std::fs::read(&state_file).expect("read seeded state");

    let mut empty = steps_args(&dir, task);
    empty.extend(["toggle".into(), String::new()]);
    let empty = steps(&empty);
    assert_eq!(empty.code, 1);
    assert!(empty.stdout.is_empty());
    assert!(
        empty.stderr.contains("unknown-step"),
        "empty operand must refuse with a stable token: {}",
        empty.stderr
    );
    assert_eq!(
        std::fs::read(&state_file).expect("read store after empty operand"),
        before,
        "the empty operand must address no step, not the sole one"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_add_refuses_empty_and_control_char_text_without_mutation() {
    let dir = temp_state_dir("text-refusal");
    let task = seed_task(&dir, "text refusal target");
    let state_file = dir.join("tsk.json");
    let before = std::fs::read(&state_file).expect("read seeded state");

    for (text, token) in [
        ("", "empty-step-text"),
        ("   ", "empty-step-text"),
        // Controls refuse before trimming, same as task titles: a tab is C0.
        ("   \t ", "invalid-step-text"),
        ("line\nbreak", "invalid-step-text"),
    ] {
        let mut args = steps_args(&dir, task);
        args.extend(["add".into(), text.into()]);
        let output = steps(&args);
        assert_eq!(output.code, 1, "refuse {text:?}");
        assert!(output.stdout.is_empty(), "refuse {text:?}");
        assert!(
            output.stderr.contains(token),
            "refuse {text:?} with {token}: {}",
            output.stderr
        );
        assert!(
            !output.stderr.contains("invalid-item\n") && !output.stderr.contains("invalid-item "),
            "step refusals must not reuse the bulk-add invalid-item token: {}",
            output.stderr
        );
    }

    assert_eq!(
        std::fs::read(&state_file).expect("read store after refusals"),
        before,
        "refused step text must persist nothing"
    );
    let state = TaskStore::new(&dir).load().expect("reload store");
    assert!(
        state.tasks()[0].steps.is_empty(),
        "no step may be created by a refused add"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_on_soft_deleted_task_refuses_without_mutation() {
    let dir = temp_state_dir("soft-deleted");
    let mut state = DomainState::new();
    let task = state
        .create(
            "deleted target",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    state.soft_delete(task).expect("soft delete seed");
    TaskStore::new(&dir).save(&state).expect("seed store");
    let state_file = dir.join("tsk.json");
    let before = std::fs::read(&state_file).expect("read seeded state");

    let mut add = steps_args(&dir, task);
    add.extend(["add".into(), "no scripting deleted tasks".into()]);
    let add = steps(&add);
    assert_eq!(add.code, 1);
    assert!(add.stdout.is_empty());
    assert!(add.stderr.contains("soft-deleted-task"));
    assert_eq!(
        std::fs::read(&state_file).expect("read store after refusal"),
        before,
        "a deleted task's steps are not scriptable"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_bare_digits_act_on_the_task() {
    let dir = temp_state_dir("number-address");
    let task = seed_task(&dir, "number target");
    let number = task_number(&dir, task);

    let output = steps(&[
        "tsk".into(),
        "steps".into(),
        number.to_string(),
        "add".into(),
        "addressed by number".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(
        TaskStore::new(&dir)
            .load()
            .expect("reload state")
            .get(task)
            .expect("task")
            .steps[0]
            .text,
        "addressed by number"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_uuid_still_acts_on_the_task() {
    let dir = temp_state_dir("uuid-address");
    let task = seed_task(&dir, "uuid target");

    let mut args = steps_args(&dir, task);
    args.extend(["add".into(), "addressed by uuid".into()]);
    let output = steps(&args);

    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(
        TaskStore::new(&dir)
            .load()
            .expect("reload state")
            .get(task)
            .expect("task")
            .steps[0]
            .text,
        "addressed by uuid"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_unknown_number_is_unknown_task() {
    let dir = temp_state_dir("unknown-number");
    let output = steps(&[
        "tsk".into(),
        "steps".into(),
        "999".into(),
        "add".into(),
        "missing target".into(),
        "--state-dir".into(),
        state_dir_arg(&dir),
    ]);

    assert_eq!(output.code, 1);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.contains("unknown-task"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn steps_help_documents_toggle_flip_and_verify_guidance() {
    let output = steps(&["tsk".into(), "steps".into(), "--help".into()]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stderr.is_empty());
    for term in [
        "usage: tsk steps",
        "toggle",
        "flips",
        "verify",
        "list",
        "exit 0",
        "exit 1",
        "exit 2",
        "exit 3",
        "nothing persisted",
        "indeterminate",
    ] {
        assert!(
            output.stdout.contains(term),
            "steps help should contain {term:?}"
        );
    }
}
