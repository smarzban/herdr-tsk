//! The product docs under `site/` describe what ships. These checks read the shipped
//! Markdown and compare it with the code, so a table row can no longer promise an action
//! the board does not offer (the palette listed a `Set done` that never existed).

use std::path::{Path, PathBuf};

use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::BoardModel;

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("site/src/content/docs/docs")
}

fn read_doc(name: &str) -> String {
    let path = docs_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The rows of the first Markdown table after `heading`, as trimmed cell vectors, header
/// and separator excluded.
fn table_after(doc: &str, heading: &str) -> Vec<Vec<String>> {
    let start = doc
        .find(heading)
        .unwrap_or_else(|| panic!("heading {heading:?} missing"));
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in doc[start + heading.len()..].lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') {
            in_table = true;
            let cells: Vec<String> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect();
            if cells
                .iter()
                .all(|cell| cell.chars().all(|c| c == '-' || c == ':'))
            {
                continue;
            }
            rows.push(cells);
        } else if in_table {
            break;
        }
    }
    assert!(rows.len() > 1, "table after {heading:?} has no rows");
    rows.remove(0);
    rows
}

/// Expand one "Available actions" cell into the palette labels it promises.
fn promised_labels(cell: &str) -> Vec<String> {
    let mut labels = Vec::new();
    for part in cell.split(',') {
        let part = part.trim().to_lowercase();
        if let Some(rest) = part.strip_prefix("set ") {
            // "Set open/ready/started/blocked/review" is five status commands.
            for status in rest.split('/') {
                labels.push(format!("set status: {}", status.trim()));
            }
        } else {
            labels.push(part);
        }
    }
    labels
}

fn selected_model() -> BoardModel {
    let mut domain = DomainState::new();
    domain
        .create(
            "palette witness",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("seed task");
    let model = BoardModel::from_tasks(domain.tasks().to_vec(), None);
    assert!(model.selected_id().is_some(), "a task must be selected");
    model
}

#[test]
fn board_md_palette_table_matches_the_palette_catalog() {
    let doc = read_doc("board.md");
    let rows = table_after(&doc, "## Palette");

    let mut model = selected_model();
    let with_selection: Vec<String> = model
        .available_commands()
        .iter()
        .map(|command| command.label.to_lowercase())
        .collect();

    model.begin_save_recovery("disk full");
    let recovery: Vec<String> = model
        .available_commands()
        .iter()
        .map(|command| command.label.to_lowercase())
        .collect();

    let mut documented = Vec::new();
    for row in &rows {
        let (actions, when) = (&row[0], &row[1]);
        let available = match when.as_str() {
            "Always" | "A task is selected" => &with_selection,
            "A save has failed" => &recovery,
            other => panic!("unknown palette condition {other:?} in board.md"),
        };
        for label in promised_labels(actions) {
            assert!(
                available.contains(&label),
                "board.md promises palette action {label:?} ({when}), the catalog offers {available:?}"
            );
            documented.push(label);
        }
    }

    // And the other way: every command the palette offers is documented.
    for label in with_selection.iter().chain(recovery.iter()) {
        assert!(
            documented.contains(label),
            "palette offers {label:?} but board.md's table does not list it"
        );
    }
}

#[test]
fn keys_md_board_table_matches_the_normal_mode_keymap() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tsk_tui::ui::board::BoardInputMode;
    use tsk_tui::ui::input::map_key;

    let doc = read_doc("keys.md");
    let rows = table_after(&doc, "## Board");
    let mut checked = 0;
    for row in rows {
        let keys = &row[1];
        // Only single ctrl chords are mechanically checkable here; the rest of the table
        // is exercised by tests/v1_keymap_guard.rs.
        let Some(letter) = keys
            .strip_prefix("`ctrl+")
            .and_then(|rest| rest.strip_suffix('`'))
            .filter(|letter| letter.len() == 1)
        else {
            continue;
        };
        let intent = map_key(
            BoardInputMode::Normal,
            KeyEvent::new(
                KeyCode::Char(letter.chars().next().unwrap()),
                KeyModifiers::CONTROL,
            ),
        );
        assert!(
            intent.is_some(),
            "keys.md lists {keys} ({}) but normal mode maps nothing to it",
            row[0]
        );
        checked += 1;
    }
    assert!(
        checked >= 6,
        "expected the ctrl verbs in keys.md, checked {checked}"
    );
}
