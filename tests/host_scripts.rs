//! Host Adapter launcher contract.
//! Scripts are bash open/focus helpers; tests are path + mode + text contracts
//! (no live herdr). Manual: second open-board focuses the same Tasks board.

use std::fs;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
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
