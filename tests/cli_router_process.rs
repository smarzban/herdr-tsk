use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::store::TaskStore;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn binary() -> String {
    std::env::var("CARGO_BIN_EXE_tsk").expect("Cargo must provide the tsk binary path")
}

fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-process-{label}-{nanos}-{seq}"));
    std::fs::create_dir_all(&dir).expect("create state directory");
    dir
}

fn wait_with_output_before_deadline(mut child: Child, description: &str) -> Output {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child.try_wait().expect("poll child process").is_some() {
            return child
                .wait_with_output()
                .expect("collect child process output");
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop process after timeout");
            let _ = child.wait();
            panic!("{description} kept a TUI open");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn top_level_help_names_subcommands_and_their_help() {
    let output = wait_with_output_before_deadline(
        Command::new(binary())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn global help"),
        "top-level help",
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert!(stdout.contains("add"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("edit"));
    assert!(stdout.contains("archive") && stdout.contains("unarchive"));
    assert!(stdout.contains("tsk add --help"));
    assert!(stdout.contains("tsk list --help"));
    assert!(stdout.contains("tsk status --help"));
    assert!(stdout.contains("tsk edit --help"));
    assert!(stdout.contains("tsk archive --help"));
}

#[test]
fn top_level_help_lists_guide_and_ends_with_agent_footer() {
    let output = wait_with_output_before_deadline(
        Command::new(binary())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn global help"),
        "top-level help",
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert!(
        stdout.contains("guide"),
        "top-level help must list the guide command"
    );
    let trimmed = stdout.trim_end();
    assert!(
        trimmed.ends_with("Agents: run `tsk guide`, or read https://gettsk.sh/docs/agents.md"),
        "help must end with the agent footer, got {trimmed:?}"
    );
}

#[test]
fn executable_accepts_piped_json_plan_and_persists_it() {
    let dir = temp_state_dir("piped-plan");
    let mut child = Command::new(binary())
        .args(["add", "--state-dir", dir.to_str().expect("UTF-8 state dir")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn plan add");
    let mut stdin = child.stdin.take().expect("plan stdin");
    let writer = thread::spawn(move || {
        stdin.write_all(br#"[{"title":"from executable pipe","project":null}]"#)
    });
    let output = wait_with_output_before_deadline(child, "piped plan add");
    writer
        .join()
        .expect("join plan writer")
        .expect("write plan");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(result["created"][0]["title"], "from executable pipe");
    assert!(result["existing"].as_array().expect("existing").is_empty());
    assert!(result["failed"].as_array().expect("failed").is_empty());
    let state = TaskStore::new(&dir).load().expect("load persisted plan");
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].title, "from executable pipe");
    assert_eq!(state.tasks()[0].scope, TaskScope::Global);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn executable_list_json_reads_state_without_opening_the_board() {
    let dir = temp_state_dir("list-json");
    let mut state = DomainState::new();
    state
        .create(
            "listed by executable",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    TaskStore::new(&dir).save(&state).expect("seed state");

    let output = wait_with_output_before_deadline(
        Command::new(binary())
            .args([
                "list",
                "--desk",
                "--json",
                "--state-dir",
                dir.to_str().expect("UTF-8 state dir"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn list"),
        "JSON list",
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("list JSON");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "listed by executable");
    assert_eq!(rows[0]["project"], serde_json::Value::Null);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unknown_positional_exits_2_without_opening_the_board() {
    let mut child = Command::new(binary())
        .arg("foo")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tsk foo");
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll child process") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop TUI process after timeout");
            let _ = child.wait();
            panic!("unknown positional kept a TUI open");
        }
        thread::sleep(Duration::from_millis(10));
    };

    assert_eq!(status.code(), Some(2));
}
