//! Board task-page title and notes editing regressions.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tsk_tui::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::{
    apply_intent, board_hit_map, draw_board, BoardInputMode, BoardModel, IntentOutcome,
};
use tsk_tui::ui::capture::CaptureField;
use tsk_tui::ui::input::{map_board_form_key, BoardIntent};
use tsk_tui::ui::render::QueueHitTarget;

const THIS_REPO: &str = "/repos/app";

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

/// Title edit opened via `e` must obey EditBuffer char-index cursor, word chords,
/// paste, and the bound-task refusal (confirm lands on the id bound at open, not live selection).
#[test]
fn title_edit_e_obeys_editbuffer_char_index_word_chords_paste_and_bound_task_refusal() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Alpha Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));

    // Open via BeginEditTitle (the 'e' path).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("begin title");
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
    assert_eq!(model.edit_target(), Some(id));
    assert_eq!(model.edit_buffer(), "Alpha Task");

    // Char-index cursor movement and insert.
    for _ in 0..5 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditMoveLeft,
            None,
            None,
        )
        .expect("left");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('X'),
        None,
        None,
    )
    .expect("insert");
    assert_eq!(model.edit_buffer(), "AlphaX Task");

    // Word chord: move word left then right; cursor must be char-granular.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditMoveWordLeft,
        None,
        None,
    )
    .expect("word left");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditMoveWordRight,
        None,
        None,
    )
    .expect("word right");

    // Paste must route (EditInsertText).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsertText(" pasted".to_string()),
        None,
        None,
    )
    .expect("paste");
    assert!(model.edit_buffer().contains("pasted"));

    // Confirm must land on the bound id even if a sync reorders selection.
    // (The heavy cross-actor reorder cases live in edit_target_binding.rs; here we assert
    // the open path binds and confirm uses the binding.)
    let expected = model.edit_buffer().to_string();
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("confirm title");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").title, expected);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert!(model.edit_target().is_none());
}

/// Palette notes edit obeys notes save chord pair (Ctrl/Alt+Enter) and bound-task refusal.
#[test]
fn palette_notes_edit_obeys_notes_save_chord_pair_and_bound_task_refusal() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Notes Task",
            Some("orig notes".into()),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));

    // Notes is palette-routed: BeginEditNotes (the 'n' or palette "edit notes" path).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditNotes,
        None,
        None,
    )
    .expect("begin notes");
    assert_eq!(model.input_mode(), BoardInputMode::EditNotes);
    assert_eq!(model.edit_target(), Some(id));

    // Type a multi-line draft (notes accept breaks).
    for ch in "line1\nline2".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("insert");
    }
    // Confirm via the notes save chord intent (bare Enter in Notes inserts a line break;
    // Ctrl/Alt+Enter is ConfirmEdit here, since both are mapped to the same intent at the
    // reducer boundary this test drives).
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("confirm notes");
    assert_eq!(outcome, IntentOutcome::Persist);
    let saved = domain.get(id).expect("task");
    assert!(saved.notes.as_deref().unwrap_or("").contains("line1"));
    assert!(saved.notes.as_deref().unwrap_or("").contains("line2"));
}

/// An open edit session is not redirected by sync_from_domain reorder.
/// The bound task remains the confirm target even when a domain reorder changes visible order.
#[test]
fn open_edit_session_not_redirected_by_sync_from_domain_reorder() {
    let mut domain = DomainState::new();
    let a = domain
        .create(
            "A",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("a");
    let b = domain
        .create(
            "B",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("b");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));

    // Select A explicitly and open a title edit.
    let visible = model.visible_ids();
    let a_idx = visible.iter().position(|&id| id == a).unwrap();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(a_idx),
        None,
        None,
    )
    .expect("select a");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("begin title on a");
    assert_eq!(model.edit_target(), Some(a));

    // Simulate a domain reorder that would put B first (e.g., B updated later).
    domain.set_status(b, HumanStatus::Started).expect("b doing");
    // Force a sync that reorders visible list.
    model.sync_from_domain(&domain);
    // Selection may move, but the edit binding must not.
    assert_eq!(model.edit_target(), Some(a), "binding must survive reorder");
    // Edit buffer and mode stay.
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);

    // Confirm must still land on A, not whatever now sits at the old index.
    // Type a distinguishing suffix.
    for ch in " edited".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type");
    }
    let _ = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("confirm");
    assert_eq!(domain.get(a).expect("a").title, "A edited");
    assert_eq!(domain.get(b).expect("b").title, "B", "bystander untouched");
}

/// The cursor and visible window an open editor paints must be computed against the frame's
/// actual paint width, not a hardcoded one, otherwise the cursor detaches from what is on
/// screen once the draft is longer than the hardcoded width assumed.
#[test]
fn title_edit_cursor_and_window_track_the_actual_paint_width_not_a_hardcoded_one() {
    // 62 chars: longer than the old hardcoded 40-wide window in every tier under test.
    let title: String = "0123456789"
        .chars()
        .chain('A'..='Z')
        .chain('a'..='z')
        .collect();
    assert_eq!(title.chars().count(), 62);

    let mut domain = DomainState::new();
    domain
        .create(
            &title,
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("begin title");
    assert_eq!(
        model.edit_cursor(),
        62,
        "cursor opens parked at the draft's end"
    );

    // Wide (100 cols): the whole draft fits in the page header, so the window is unscrolled
    // and the cursor sits exactly at the glyph prefix (2) + the draft's full length, not a
    // column computed against a narrower hardcoded width.
    {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| draw_board(frame, &model))
            .expect("draw 100x30");
        let (title_y, _row) = (0..30)
            .map(|y| (y, row_text(&terminal, 100, y)))
            .find(|(_, row)| row.contains(&title))
            .expect("task page header row");
        assert_eq!(title_y, 1, "the page header paints under one blank row");
        let cursor = terminal.get_cursor_position().expect("cursor position");
        assert_eq!(
            cursor,
            ratatui::layout::Position::new(4 + 62, title_y),
            "cursor must land immediately after the full draft, not a 40-wide-window column"
        );
    }

    // Narrow (40 cols, compact): the draft overflows the header, so it WRAPS onto a
    // second bold header row -- nothing is cut, and the caret follows onto that
    // continuation row at its past-end column.
    {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| draw_board(frame, &model))
            .expect("draw 40x10");
        // 62 whitespace-free characters at a 30-cell field wrap as 30 / 30 / 2.
        let head_row = row_text(&terminal, 40, 1);
        assert!(
            head_row.contains("0123456789"),
            "40-wide header keeps the head on its first wrapped row: {head_row:?}"
        );
        let tail_row = (0..10)
            .map(|y| (y, row_text(&terminal, 40, y)))
            .find(|(_, row)| row.trim_end().contains("yz"))
            .expect("wrapped title continuation row");
        assert_ne!(
            tail_row.0, 1,
            "the title's tail must wrap below the first header row"
        );
        let cursor = terminal.get_cursor_position().expect("cursor position");
        assert_eq!(
            cursor,
            ratatui::layout::Position::new(4 + 2, tail_row.0),
            "caret lands at the two-character tail's past-end column"
        );
    }
}

/// /,,: `e`, palette Notes, and palette scope all open one
/// immutable task form. Its three drafts save together, while its scope dropdown is a child
/// surface, not a second editor or an implicit save.
#[test]
fn task_form_unifies_palette_field_routes_scope_dropdown_and_atomic_save() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Task",
            Some("old".into()),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create target");
    domain
        .create(
            "Other project",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create scope source");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let target_index = model
        .visible_ids()
        .iter()
        .position(|&visible| visible == id)
        .expect("target visible");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(target_index),
        None,
        None,
    )
    .expect("select target");

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("e opens task form");
    assert_eq!(model.edit_target(), Some(id));
    assert_eq!(model.form_focus(), Some(CaptureField::Title));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Notes),
        None,
        None,
    )
    .expect("a direct field focus keeps the same bound form");
    assert_eq!(model.edit_target(), Some(id));
    assert_eq!(model.input_mode(), BoardInputMode::EditNotes);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Title),
        None,
        None,
    )
    .expect("Title focus returns to the same draft");
    assert_eq!(model.form_focus(), Some(CaptureField::Title));
    assert_eq!(
        model.form_scope_options(),
        vec![
            project(THIS_REPO),
            project("/repos/other"),
            TaskScope::Global
        ],
        "form scope choices are the initial scope, current repo, live projects, then Global"
    );
    for character in " updated".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type title");
    }

    let tab = map_board_form_key(
        model.form_focus().expect("open form focus"),
        false,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
    )
    .expect("Tab moves a task form field");
    assert_eq!(tab, BoardIntent::FormFocusNext);
    apply_intent(&mut domain, &mut model, tab, None, None).expect("focus Notes");
    assert_eq!(model.form_focus(), Some(CaptureField::Notes));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsertLineBreak,
        None,
        None,
    )
    .expect("Notes newline");
    for character in "new".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type Notes");
    }

    let tab = map_board_form_key(
        model.form_focus().expect("Notes focus"),
        false,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
    )
    .expect("Tab moves to thread");
    apply_intent(&mut domain, &mut model, tab, None, None).expect("focus thread");
    assert_eq!(model.form_focus(), Some(CaptureField::Thread));
    let tab = map_board_form_key(
        model.form_focus().expect("Thread focus"),
        false,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
    )
    .expect("Tab moves to scope");
    apply_intent(&mut domain, &mut model, tab, None, None).expect("focus scope");
    assert_eq!(model.form_focus(), Some(CaptureField::Scope));
    let before_cycle = model.form_scope().cloned().expect("scope draft");
    assert_eq!(
        map_board_form_key(
            CaptureField::Scope,
            false,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)
        ),
        Some(BoardIntent::FormCycleScope)
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormCycleScope,
        None,
        None,
    )
    .expect("Space cycles direct scope");
    assert_ne!(model.form_scope(), Some(&before_cycle));

    let open_dropdown = map_board_form_key(
        CaptureField::Scope,
        false,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    )
    .expect("Scope Enter opens its dropdown");
    assert_eq!(open_dropdown, BoardIntent::OpenFormScopeDropdown);
    apply_intent(&mut domain, &mut model, open_dropdown, None, None).expect("open dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::FormScopeDropdown);
    let draft_before_dropdown = model.form_scope().cloned();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormScopeNext,
        None,
        None,
    )
    .expect("move dropdown selection");
    assert_eq!(
        model.form_scope(),
        draft_before_dropdown.as_ref(),
        "moving the dropdown selection must not apply it"
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CancelFormScopeDropdown,
        None,
        None,
    )
    .expect("Esc returns to parent form");
    assert_eq!(model.form_focus(), Some(CaptureField::Scope));
    assert_ne!(model.input_mode(), BoardInputMode::FormScopeDropdown);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenFormScopeDropdown,
        None,
        None,
    )
    .expect("open dropdown again");
    for _ in 0..model.form_scope_options().len() {
        if model.form_scope_dropdown_choice() == Some(&TaskScope::Global) {
            break;
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormScopeNext,
            None,
            None,
        )
        .expect("move toward Global");
    }
    assert_eq!(model.form_scope_dropdown_choice(), Some(&TaskScope::Global));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmFormScopeDropdown,
        None,
        None,
    )
    .expect("apply dropdown scope");
    assert_eq!(model.form_scope(), Some(&TaskScope::Global));

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormFocusNext,
        None,
        None,
    )
    .expect("scope wraps to title");
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("one atomic form save");
    assert_eq!(outcome, IntentOutcome::Persist);
    let saved = domain.get(id).expect("saved task");
    assert_eq!(saved.title, "Task updated");
    assert_eq!(saved.notes.as_deref(), Some("old\nnew"));
    assert_eq!(saved.scope, TaskScope::Global);

    let palette_notes_route = model
        .available_commands()
        .into_iter()
        .find(|command| command.label == "edit notes")
        .expect("palette Edit notes command")
        .intent;
    assert_eq!(palette_notes_route, BoardIntent::BeginEditNotes);
    apply_intent(&mut domain, &mut model, palette_notes_route, None, None)
        .expect("palette Notes route opens the shared form");
    assert_eq!(model.edit_target(), Some(id));
    assert_eq!(model.form_focus(), Some(CaptureField::Notes));
    apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None, None)
        .expect("close Notes form");
    let palette_scope_route = model
        .available_commands()
        .into_iter()
        .find(|command| command.label == "change scope")
        .expect("palette Change scope command")
        .intent;
    assert_eq!(palette_scope_route, BoardIntent::BeginEditScope);
    apply_intent(&mut domain, &mut model, palette_scope_route, None, None)
        .expect("palette Change scope route opens the shared form");
    assert_eq!(model.edit_target(), Some(id));
    assert_eq!(model.form_focus(), Some(CaptureField::Scope));
}

/// The mode-only mapper and the focused-form mapper share one field-edit implementation. The
fn row_text(terminal: &Terminal<TestBackend>, width: u16, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..width)
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

/// The page's scope dropdown answers the footer it belongs to: options stack directly
/// above the meta footer, left-aligned, and carry short project names -- never the
/// filesystem path, never the selector's corner.
#[test]
fn task_page_scope_dropdown_sits_above_the_footer_with_short_names() {
    let mut domain = DomainState::new();
    domain
        .create(
            "scoped task",
            None,
            TaskScope::Project {
                path: "/repos/tsk".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    domain
        .create(
            "other project task",
            None,
            TaskScope::Project {
                path: "/repos/other-project".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create second project");
    let mut model = BoardModel::from_domain(&domain, Some(std::path::PathBuf::from("/repos/tsk")));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenFormScopeDropdown,
        None,
        None,
    )
    .expect("open scope dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::FormScopeDropdown);

    let (width, height) = (80u16, 24u16);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw");

    let rows: Vec<String> = (0..height).map(|y| row_text(&terminal, width, y)).collect();
    let footer_y = rows
        .iter()
        .position(|row| row.contains("created"))
        .expect("the page footer paints its meta row");
    // Options sit directly above the footer, never down in the corner or below it.
    let option_rows: Vec<(usize, &String)> = rows[..footer_y]
        .iter()
        .enumerate()
        .filter(|(_, row)| row.contains("tsk") || row.contains("other-project"))
        .collect();
    assert!(
        !option_rows.is_empty(),
        "the dropdown must paint its options:\n{rows:?}"
    );
    let (last_option_y, _) = option_rows.last().expect("options painted");
    assert_eq!(
        *last_option_y,
        footer_y - 1,
        "the dropdown's last option must sit directly above the footer:\n{rows:?}"
    );
    // Short names only; left-aligned like the footer, not parked in the selector corner.
    for (_, row) in &option_rows {
        assert!(
            !row.contains("/repos/"),
            "dropdown options must show short names, not paths: {row:?}"
        );
        let first_char = row.chars().next().unwrap_or(' ');
        assert_eq!(first_char, ' ', "options are left-indented: {row:?}");
    }
}

#[test]
fn page_view_shows_thread_beside_scope() {
    let mut domain = DomainState::new();
    domain
        .create_with_thread(
            "Threaded page",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
            Some("release-2026".into()),
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw");
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        painted.contains("#release-2026"),
        "thread missing: {painted}"
    );
}

#[test]
fn task_page_form_tab_cycle_reaches_thread_and_shift_tab_reverses_it() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Unthreaded task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open form");

    let mut focus = CaptureField::Title;
    for expected in [
        CaptureField::Notes,
        CaptureField::Thread,
        CaptureField::Scope,
    ] {
        let tab = map_board_form_key(
            focus,
            false,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        )
        .expect("Tab maps on every editable form field");
        assert_eq!(tab, BoardIntent::FormFocusNext);
        apply_intent(&mut domain, &mut model, tab, None, None).expect("Tab focuses the next field");
        assert_eq!(model.form_focus(), Some(expected));
        focus = expected;
    }
    for expected in [
        CaptureField::Thread,
        CaptureField::Notes,
        CaptureField::Title,
    ] {
        let shift_tab = map_board_form_key(
            focus,
            false,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        )
        .expect("Shift+Tab maps on every editable form field");
        assert_eq!(shift_tab, BoardIntent::FormFocusPrev);
        apply_intent(&mut domain, &mut model, shift_tab, None, None)
            .expect("Shift+Tab reverses the prior focus move");
        assert_eq!(model.form_focus(), Some(expected));
        focus = expected;
    }
}

#[test]
fn page_edit_sets_thread_and_clearing_unthreads() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open form");
    for _ in 0..2 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormFocusNext,
            None,
            None,
        )
        .expect("focus next");
    }
    assert_eq!(model.form_focus(), Some(CaptureField::Thread));
    for character in "Release-2026".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type thread");
    }
    assert_eq!(
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ConfirmEdit,
            None,
            None
        )
        .expect("save thread"),
        IntentOutcome::Persist
    );
    assert_eq!(
        domain.get(id).expect("task").thread.as_deref(),
        Some("release-2026")
    );

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("reopen form");
    for _ in 0..2 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormFocusNext,
            None,
            None,
        )
        .expect("focus thread");
    }
    for _ in 0.."release-2026".len() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditBackspace,
            None,
            None,
        )
        .expect("clear thread");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("unthread");
    assert_eq!(domain.get(id).expect("task").thread, None);
}

#[test]
fn page_thread_field_refuses_invalid_name_without_persisting() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open");
    for _ in 0..2 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormFocusNext,
            None,
            None,
        )
        .expect("focus");
    }
    for character in "bad_name".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type");
    }
    assert_eq!(
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ConfirmEdit,
            None,
            None
        )
        .expect("refusal"),
        IntentOutcome::None
    );
    assert_eq!(model.input_mode(), BoardInputMode::EditThread);
    assert_eq!(domain.get(id).expect("task").thread, None);
}

#[test]
fn canceling_thread_edit_keeps_the_task_page_and_resets_the_thread_draft() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open task form");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Thread),
        None,
        None,
    )
    .expect("focus thread");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsertText("release".into()),
        None,
        None,
    )
    .expect("type thread");

    apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None, None)
        .expect("cancel thread field");
    assert!(
        model.board_form_open(),
        "field cancel retains the task page"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Thread),
        None,
        None,
    )
    .expect("reopen thread field");
    assert_eq!(
        model.edit_buffer(),
        "",
        "field cancel restores saved thread"
    );
}

#[test]
fn editing_an_unthreaded_task_paints_a_labeled_thread_footer_slot() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open task form");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Thread),
        None,
        None,
    )
    .expect("focus thread");

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw thread field");
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        painted.contains("app · thread · created"),
        "the unthreaded edit footer must label the Thread target: {painted}"
    );
    assert!(
        !painted.contains("app · # · created"),
        "an empty thread must not render as a dangling hash: {painted}"
    );
}

#[test]
fn thread_field_is_inert_while_step_editor_owns_the_footer() {
    let mut domain = DomainState::new();
    domain
        .create_with_thread(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
            Some("release".into()),
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("page");
    assert!(
        board_hit_map(Rect::new(0, 0, 80, 24), &model)
            .regions
            .iter()
            .any(|hit| hit.target == QueueHitTarget::FormThread),
        "the threaded page exposes its footer hit before a step editor opens"
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("item editor");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Thread),
        None,
        None,
    )
    .expect("inert focus");
    assert_eq!(model.input_mode(), BoardInputMode::EditStep);
    let hits = board_hit_map(Rect::new(0, 0, 80, 24), &model);
    assert!(
        !hits
            .regions
            .iter()
            .any(|hit| hit.target == QueueHitTarget::FormThread),
        "a step editor owns the footer, so thread has no hit target: {hits:?}"
    );
}

#[test]
fn thread_refusal_paints_inline_and_clears_without_status_leak() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open");
    for _ in 0..2 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormFocusNext,
            None,
            None,
        )
        .expect("focus");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('_'),
        None,
        None,
    )
    .expect("type");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("refuse");
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw refusal");
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        painted.contains("invalid thread name"),
        "missing inline refusal: {painted}"
    );
    assert_eq!(
        model.message(),
        None,
        "thread refusal must not use status message"
    );
    apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None, None).expect("close");
    assert_eq!(model.message(), None);
}

#[test]
fn task_page_footer_hits_use_display_columns_and_stay_within_the_painted_row() {
    let mut domain = DomainState::new();
    domain
        .create_with_thread(
            "Threaded task",
            None,
            project("/repos/プロジェクト"),
            None,
            None,
            ProvenanceOrigin::Manual,
            Some("a2345678901234567890123456789012".into()),
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");

    let width = 40;
    let hits = board_hit_map(Rect::new(0, 0, width, 10), &model);
    let scope = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormScope)
        .expect("scope hit");
    let thread = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormThread)
        .expect("thread hit");
    assert_eq!(scope.area.width, 14, "two-space inset plus six wide glyphs");
    assert_eq!(thread.area.x, 14, "thread starts after the painted scope");
    assert!(
        thread.area.right() <= width,
        "thread hit must not extend beyond the clipped footer: {thread:?}"
    );
}

#[test]
fn long_invalid_thread_refusal_remains_visible_at_40x10() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusFormField(CaptureField::Thread),
        None,
        None,
    )
    .expect("focus thread");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsertText("invalid_thread_name_here".into()),
        None,
        None,
    )
    .expect("type invalid thread");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("refuse");

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw compact refusal");
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        painted.contains("invalid thread name"),
        "thread refusal vanished at 40x10: {painted}"
    );
}

#[test]
fn page_footer_thread_edit_operable_at_40x10() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Task",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open");
    for _ in 0..2 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::FormFocusNext,
            None,
            None,
        )
        .expect("focus");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsertText("tiny".into()),
        None,
        None,
    )
    .expect("type");
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("draw compact");
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(painted.contains("tiny"), "thread input vanished: {painted}");
}
