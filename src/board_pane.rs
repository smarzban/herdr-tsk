//! Find an existing Tasks board pane from `herdr pane list` JSON.
//!
//! Used by `scripts/open-board.sh` via `tsk --find-board-pane` so the
//! launcher does not depend on python3.

use std::io::{self, Read};

use serde_json::Value;

/// Board pane label as declared in herdr-plugin.toml `[[panes]]`.
pub const BOARD_PANE_LABEL: &str = "tsk";

/// Read pane-list JSON from `stdin` and print the first flag-safe Tasks pane id.
///
/// Exit code semantics are handled by the binary: returns `Ok(Some(id))` when found,
/// `Ok(None)` when no matching pane, `Err` on I/O or unparseable JSON.
pub fn find_board_pane_from_stdin() -> io::Result<Option<String>> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(find_board_pane_id(&buf))
}

/// Parse herdr `pane list` JSON and return the first flag-safe Tasks pane_id.
///
/// Matches panes whose `label` equals `"tsk"`.
///
/// A terminal title is not a board identity: it can be the standalone `tsk` binary's
/// default title in an unrelated pane. Pane ids must be non-empty, not start with `-`,
/// and match `[A-Za-z0-9_.:-]+` so they are safe to pass as a CLI argument to
/// `plugin pane focus`.
pub fn find_board_pane_id(json: &str) -> Option<String> {
    let data: Value = serde_json::from_str(json).ok()?;
    let panes = data
        .get("result")
        .and_then(|r| r.get("panes"))
        .and_then(|p| p.as_array())?;

    for pane in panes {
        let label = pane.get("label").and_then(|v| v.as_str()).unwrap_or("");
        if label != BOARD_PANE_LABEL {
            continue;
        }
        let pid = pane.get("pane_id").and_then(|v| v.as_str()).unwrap_or("");
        if is_flag_safe_pane_id(pid) {
            return Some(pid.to_string());
        }
    }
    None
}

fn is_flag_safe_pane_id(pid: &str) -> bool {
    if pid.is_empty() || pid.starts_with('-') {
        return false;
    }
    pid.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tasks_pane_by_label() {
        let json = r#"{
            "result": {
                "panes": [
                    {"pane_id": "w0:p1", "label": "Editor", "terminal_title_stripped": "nvim"},
                    {"pane_id": "w0:p2", "label": "tsk", "terminal_title_stripped": "tsk"}
                ]
            }
        }"#;
        assert_eq!(find_board_pane_id(json).as_deref(), Some("w0:p2"));
    }

    #[test]
    fn ignores_non_board_pane_with_tsk_terminal_title() {
        let json = r#"{
            "result": {
                "panes": [
                    {"pane_id": "w1:p0", "label": "shell", "terminal_title_stripped": "tsk"}
                ]
            }
        }"#;
        assert_eq!(find_board_pane_id(json), None);
    }

    #[test]
    fn skips_unsafe_pane_ids() {
        let json = r#"{
            "result": {
                "panes": [
                    {"pane_id": "--evil", "label": "tsk"},
                    {"pane_id": "bad id", "label": "tsk"},
                    {"pane_id": "w0:p9", "label": "tsk"}
                ]
            }
        }"#;
        assert_eq!(find_board_pane_id(json).as_deref(), Some("w0:p9"));
    }

    #[test]
    fn returns_none_when_no_tasks_pane() {
        let json = r#"{
            "result": {
                "panes": [
                    {"pane_id": "w0:p1", "label": "shell", "terminal_title_stripped": "zsh"}
                ]
            }
        }"#;
        assert_eq!(find_board_pane_id(json), None);
    }

    #[test]
    fn returns_none_on_invalid_json() {
        assert_eq!(find_board_pane_id("not json"), None);
        assert_eq!(find_board_pane_id("{}"), None);
    }
}
