//! Reads `herdr-plugin.toml` and asserts the host entrypoints.

use std::fs;
use std::path::PathBuf;

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("herdr-plugin.toml")
}

fn read_manifest() -> String {
    fs::read_to_string(manifest_path()).expect("herdr-plugin.toml must exist at crate root")
}

/// Slice from `header` through the line before the next `[[...]]` table (or EOF).
fn table_section<'a>(text: &'a str, header: &str) -> &'a str {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("manifest must contain {header}"));
    let after = &text[start + header.len()..];
    let end = after.find("\n[[").unwrap_or(after.len());
    &text[start..start + header.len() + end]
}

#[test]
fn id_contains_tasks_product_id() {
    let text = read_manifest();
    assert!(
        text.contains(r#"id = "herdr-tasks""#),
        "manifest id must be the tasks product id herdr-tasks"
    );
}

#[test]
fn min_herdr_version_is_0_7_5() {
    let text = read_manifest();
    assert!(
        text.contains(r#"min_herdr_version = "0.7.5""#),
        "min_herdr_version must be exactly 0.7.5"
    );
}

#[test]
fn platforms_include_linux_and_macos() {
    let text = read_manifest();
    assert!(
        text.contains(r#"platforms = ["linux", "macos"]"#),
        "top-level platforms must include linux and macos"
    );
}

#[test]
fn panes_entry_has_tasks_title_and_split_placement() {
    let text = read_manifest();
    assert!(text.contains("[[panes]]"), "must declare a [[panes]] entry");
    assert!(text.contains("Tasks"), "pane title must contain Tasks");
    assert!(
        text.contains(r#"placement = "split""#),
        "pane placement must be split"
    );
}

#[test]
fn actions_include_open_board() {
    let text = read_manifest();
    assert!(text.contains("[[actions]]"), "must declare [[actions]]");
    assert!(
        text.contains("open-board"),
        "must declare an [[actions]] entry for open-board"
    );
}

#[test]
fn actions_include_quick_capture() {
    let text = read_manifest();
    assert!(text.contains("[[actions]]"), "must declare [[actions]]");
    assert!(
        text.contains("quick-capture"),
        "must declare an [[actions]] entry for quick-capture"
    );
}

#[test]
fn pane_command_references_herdr_tasks_binary() {
    let text = read_manifest();
    let panes = table_section(&text, "[[panes]]");
    assert!(
        panes.contains(r#"command = ["./target/release/herdr-tasks"]"#),
        "pane command must reference the release binary ./target/release/herdr-tasks \
         (not merely the product id elsewhere in the file)"
    );
}

#[test]
fn no_events_table() {
    let text = read_manifest();
    assert!(
        !text.contains("[[events]]"),
        "must not declare [[events]] (explicit over ambient)"
    );
}

#[test]
fn no_host_park_or_resume_action() {
    // Park and resume are not host actions.
    let text = read_manifest();
    let mut action_ids = Vec::new();
    let mut rest = text.as_str();
    while let Some(pos) = rest.find("[[actions]]") {
        rest = &rest[pos + "[[actions]]".len()..];
        let end = rest.find("\n[[").unwrap_or(rest.len());
        let section = &rest[..end];
        for line in section.lines() {
            let line = line.trim();
            if let Some(raw) = line.strip_prefix("id = ") {
                let id = raw.trim().trim_matches('"');
                action_ids.push(id.to_string());
            }
        }
        rest = &rest[end..];
    }
    assert_eq!(
        action_ids,
        vec!["open-board".to_string(), "quick-capture".to_string()],
        "host actions must stay open-board and quick-capture; got {action_ids:?}"
    );
    for forbidden in [
        "park",
        "resume",
        "park-here",
        "start-agent",
        "create-worktree",
    ] {
        assert!(
            !action_ids.iter().any(|id| id.contains(forbidden)),
            "forbidden host action id containing {forbidden:?}"
        );
    }
}
