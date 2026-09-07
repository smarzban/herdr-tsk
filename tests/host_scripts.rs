//! Host Adapter launcher contract.
//! Scripts are bash open/focus helpers; tests are path + mode + text contracts
//! (no live herdr). Manual: second open-board focuses the same Tasks board.

use std::fs;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use tsk_tui::app::MODE_ENV;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn open_board_path() -> PathBuf {
    repo_root().join("scripts/open-board.sh")
}

fn open_capture_path() -> PathBuf {
    repo_root().join("scripts/open-capture.sh")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn open_board_exercises_context_handoff_before_focus_and_new_pane_paths() {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsk-host-launcher-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).expect("temp root");
    let stub = root.join("herdr");
    let log = root.join("calls.log");
    fs::write(
        &stub,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$STUB_LOG"
if [ "$1" = pane ] && [ "$2" = list ]; then
  if [ "${STUB_MODE:-existing}" = existing ]; then
    printf '%s' '{"result":{"panes":[{"pane_id":"w0:p1","label":"tsk"}]}}'
  else
    printf '%s' '{"result":{"panes":[]}}'
  fi
fi
if [ "$1" = plugin ] && [ "$2" = pane ] && [ "$3" = focus ] && [ ! -f "$STUB_STATE/reopen.json" ]; then
  exit 1
fi
"#,
    )
    .expect("stub");
    let mut permissions = fs::metadata(&stub).expect("stub metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&stub, permissions).expect("stub executable");
    let state = root.join("state");
    fs::create_dir_all(&state).expect("state");
    let run = |mode: &str| {
        Command::new("bash")
            .arg(open_board_path())
            .env("HERDR_BIN_PATH", &stub)
            .env("TSK_BIN", env!("CARGO_BIN_EXE_tsk"))
            .env("TSK_STATE_DIR", &state)
            .env("STUB_LOG", &log)
            .env("STUB_STATE", &state)
            .env("STUB_MODE", mode)
            .env(
                "HERDR_PLUGIN_CONTEXT_JSON",
                r#"{"focused_pane_cwd":"/tmp"}"#,
            )
            .output()
            .expect("launcher")
    };
    let existing = run("existing");
    assert!(
        existing.status.success(),
        "existing launcher: {:?}",
        existing
    );
    let calls = read(&log);
    assert!(calls.contains("pane list"));
    assert!(calls.contains("plugin pane focus w0:p1"));
    assert!(
        !calls.lines().any(|line| line == "plugin pane open"),
        "existing pane must not be reopened"
    );
    assert!(calls.find("pane list").unwrap() < calls.find("plugin pane focus").unwrap());
    assert!(state.join("reopen.json").exists(), "context was published");

    let absent = run("absent");
    assert!(absent.status.success(), "new-pane launcher: {:?}", absent);
    let calls = read(&log);
    assert!(calls.contains("plugin pane open"));

    let before = calls.lines().count();
    let failed = Command::new("bash")
        .arg(open_board_path())
        .env("HERDR_BIN_PATH", &stub)
        .env("TSK_BIN", env!("CARGO_BIN_EXE_tsk"))
        .env("TSK_STATE_DIR", &state)
        .env("STUB_LOG", &log)
        .env("STUB_STATE", &state)
        .env("STUB_MODE", "existing")
        .env("HERDR_PLUGIN_CONTEXT_JSON", "not json")
        .output()
        .expect("failing launcher");
    assert!(!failed.status.success());
    let after = read(&log);
    assert_eq!(
        after.lines().count(),
        before + 1,
        "failed handoff must not focus"
    );
    assert!(!after
        .lines()
        .skip(before)
        .any(|line| line == "plugin pane focus w0:p1"));
    let _ = fs::remove_dir_all(root);
}

/// Non-comment, non-empty lines of a shell script (strip `# ...` full-line comments).
fn active_script_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Slice the `[[actions]]` table whose `id = "..."` matches `action_id`.
/// Bounds: from that table header through the line before the next `[[...]]` (or EOF).
/// Comments above the header are excluded so path mentions in prose cannot green the assert.
fn action_section<'a>(text: &'a str, action_id: &str) -> &'a str {
    let id_line = format!(r#"id = "{action_id}""#);
    let id_pos = text
        .find(&id_line)
        .unwrap_or_else(|| panic!("manifest must declare action id {action_id}"));
    let table_start = text[..id_pos]
        .rfind("[[actions]]")
        .unwrap_or_else(|| panic!("action id {action_id} must sit under an [[actions]] table"));
    let after_header = &text[table_start + "[[actions]]".len()..];
    let end = after_header.find("\n[[").unwrap_or(after_header.len());
    &text[table_start..table_start + "[[actions]]".len() + end]
}

fn assert_executable(path: &Path) {
    assert!(
        path.is_file(),
        "expected launcher script at {}",
        path.display()
    );
    let mode = fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "{} must be executable (mode {mode:#o}); chmod +x and keep git filemode 100755",
        path.display()
    );

    // Prefer git's recorded mode when the tree is tracked (CI / fresh clone).
    let rel = path
        .strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let out = Command::new("git")
        .args(["ls-files", "-s", "--", &rel])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files");
    if out.status.success() {
        let line = String::from_utf8_lossy(&out.stdout);
        let line = line.trim();
        if !line.is_empty() {
            assert!(
                line.starts_with("100755 "),
                "git mode for {rel} must be 100755 (executable), got: {line}"
            );
        }
    }
}

#[test]
fn open_board_script_exists_and_is_executable() {
    assert_executable(&open_board_path());
}

#[test]
fn open_capture_script_exists_and_is_executable() {
    assert_executable(&open_capture_path());
}

#[test]
fn open_board_uses_herdr_cli_and_board_entrypoint() {
    let text = read(&open_board_path());
    assert!(
        text.contains("HERDR_BIN_PATH"),
        "open-board must use HERDR_BIN_PATH"
    );
    assert!(
        text.contains("plugin pane open"),
        "open-board must call herdr plugin pane open"
    );
    assert!(
        text.contains("board"),
        "open-board must reference the board entrypoint id"
    );
    assert!(
        text.contains("tsk"),
        "open-board must reference the tsk board title/label"
    );
    assert!(
        text.contains("split"),
        "open-board must open with split placement"
    );
}

#[test]
fn open_board_has_idempotent_focus_list_logic() {
    let text = read(&open_board_path());
    let active = active_script_lines(&text).join("\n");

    // Contract: list existing panes, then focus if board already present.
    // Require these tokens on non-comment lines so a comment-only script cannot green.
    assert!(
        active.contains("pane list") || active.contains("plugin pane list"),
        "open-board active lines must list panes to find an existing board"
    );
    assert!(
        active.contains("focus") || active.contains("plugin pane focus"),
        "open-board active lines must focus an existing board pane when found"
    );
    assert!(
        active.contains("--find-board-pane"),
        "open-board must pipe pane list through tsk --find-board-pane (no python3)"
    );
    assert!(
        active.contains("target/release/tsk") || active.contains("TSK_BIN"),
        "open-board must locate plugin binary relative to script (or TSK_BIN)"
    );

    // No bare python3 dependency for focus logic.
    for line in active_script_lines(&text) {
        assert!(
            !line.contains("python3"),
            "open-board must not depend on python3 for focus logic; found: {line}"
        );
    }
}

#[test]
fn open_capture_uses_herdr_cli_and_capture_path() {
    let text = read(&open_capture_path());
    assert!(
        text.contains("HERDR_BIN_PATH"),
        "open-capture must use HERDR_BIN_PATH"
    );
    assert!(
        text.contains("plugin pane open"),
        "open-capture must call herdr plugin pane open"
    );
    assert!(
        text.contains("capture") || text.contains("overlay"),
        "open-capture must open capture mode or a popup/overlay surface"
    );
    assert!(
        text.contains("tsk") || text.contains("board"),
        "open-capture must target this plugin / board entrypoint"
    );
}

#[test]
fn open_capture_opens_a_sized_popup_for_the_capture_session() {
    let text = read(&open_capture_path());
    let active = active_script_lines(&text).join("\n");
    assert!(
        active.contains("--placement popup"),
        "open-capture must open a popup pane; active lines: {active}"
    );
    assert!(
        !text.contains("overlay"),
        "open-capture must not use the retired overlay placement: {text}"
    );
    assert!(
        active.contains("--width") && active.contains("--height"),
        "a popup needs explicit --width/--height sized for the task page: {active}"
    );
    assert!(
        active.contains("--width 80") && active.contains("--height 15"),
        "the popup is 80x15 (operable for the task page): {active}"
    );
    assert!(
        active.contains("--focus"),
        "the popup must take focus when quick capture is invoked: {active}"
    );
    assert!(
        active.contains(&format!("--env {}=capture", MODE_ENV)),
        "the popup must still launch the binary in the capture session mode"
    );
}

#[test]
fn quick_capture_injects_the_mode_env_var_the_binary_reads() {
    let text = read(&open_capture_path());
    assert!(
        text.contains(&format!("--env {}=capture", MODE_ENV)),
        "open-capture must inject --env {MODE_ENV}=capture: the binary reads \
         app::MODE_ENV, and any other variable name silently opens the full board instead"
    );
    assert!(
        !text.contains("HERDR_TASKS_"),
        "open-capture must not carry legacy HERDR_TASKS_* variable names"
    );
}

#[test]
fn manifest_actions_point_at_launcher_scripts() {
    let manifest = read(&repo_root().join("herdr-plugin.toml"));
    let open_board = action_section(&manifest, "open-board");
    assert!(
        open_board.contains(r#"command = ["bash", "scripts/open-board.sh"]"#),
        "open-board action command must be [\"bash\", \"scripts/open-board.sh\"] \
         (not merely a path mention in a comment or elsewhere in the file)"
    );
    let quick_capture = action_section(&manifest, "quick-capture");
    assert!(
        quick_capture.contains(r#"command = ["bash", "scripts/open-capture.sh"]"#),
        "quick-capture action command must be [\"bash\", \"scripts/open-capture.sh\"] \
         (not merely a path mention in a comment or elsewhere in the file)"
    );
    assert!(
        !manifest.contains("[[events]]"),
        "must not declare [[events]] (explicit over ambient)"
    );
}
