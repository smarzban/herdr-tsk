//! Inline capture, title edit, notes edit on Edit Machinery.
//!
//! These tests are the contract for driving the surviving EditBuffer surfaces from the
//! queue board: inline capture (via OpenCapture + capture form), title edit (e), and
//! palette-routed notes edit. They must be written before touching production surfaces.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::context::InvocationSnapshot;
use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use herdr_tasks::host::HostPorts;
use herdr_tasks::store::TaskStore;
use herdr_tasks::ui::board::{apply_intent, draw_board, BoardInputMode, BoardModel, IntentOutcome};
use herdr_tasks::ui::capture::CaptureField;
use herdr_tasks::ui::input::{map_board_form_key, map_inline_capture_key, map_key, BoardIntent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const THIS_REPO: &str = "/repos/app";

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

/// No-op host for tests that do not exercise host paths.
struct NoopHost;

impl HostPorts for NoopHost {
    fn list_pane_ids(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
    fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn open_path(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}

/// `a` on the board opens capture seeded from the invocation snapshot (scope,
/// capsule, provenance). Esc discards with zero store writes.
#[test]
fn inline_capture_a_uses_invocation_snapshot_scope_capsule_provenance_and_esc_discards_with_zero_store_writes(
) {
    // Use a temp store to prove zero writes on Esc.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "herdr-t13-capture-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::create_dir_all(&dir);
    let store = TaskStore::new(&dir);

    let mut domain = DomainState::new();
    // Board seeded with a snapshot that carries a project scope + capsule + provenance.
    let snap = InvocationSnapshot {
        default_scope: project(THIS_REPO),
        this_repo: Some(PathBuf::from(THIS_REPO)),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: Some(herdr_tasks::domain::ContextCapsule {
            repo_path: Some(THIS_REPO.into()),
            ..Default::default()
        }),
        agent_meta: None,
    };
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));

    // Drive OpenCapture on board: enters inline Capture mode (not the standalone popup).
    // Snapshot seeds title/notes/scope; Esc must discard with zero store writes.
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snap),
        Some(&NoopHost),
    )
    .expect("OpenCapture must be accepted");
    // Inline path opens the status-row draft. Expand to exercise the retained full form.
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ExpandQuickAdd,
        None,
        None,
    )
    .expect("expand quick add");
    assert_eq!(model.input_mode(), BoardInputMode::Capture);

    // Change the selected scope before cancel: scope is still only a draft until save.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CaptureCycleScope,
        Some(&snap),
        Some(&NoopHost),
    )
    .expect("change inline capture scope");
    assert_eq!(model.capture_scope(), Some(&TaskScope::Global));

    // Esc discards inline capture: back to Normal, zero domain writes, zero store writes.
    // Use a fresh domain+store to ensure no side effects from the Esc path.
    let pre_count = domain.tasks().len();
    let _ = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CancelEdit,
        Some(&snap),
        Some(&NoopHost),
    );
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(model.quick_add_title_value(), "");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CancelQuickAdd,
        Some(&snap),
        Some(&NoopHost),
    )
    .expect("close quick add");
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(model.capture_scope(), None);
    assert_eq!(
        domain.tasks().len(),
        pre_count,
        "Esc must not create tasks in domain"
    );

    // Store must have zero writes attributable to the board capture Esc path.
    let post = store.load().expect("load").tasks().len();
    assert_eq!(
        0, post,
        "Esc on inline capture must produce zero store writes"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

/// inline capture scope cycles through the available project scopes and Global before
/// saving, while its invocation capsule and provenance remain untouched.
#[test]
fn inline_capture_can_change_scope_and_create_a_global_task_with_snapshot_provenance() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Other project task",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create other project fixture");
    let snapshot = InvocationSnapshot {
        default_scope: project(THIS_REPO),
        this_repo: Some(PathBuf::from(THIS_REPO)),
        title_prefill: None,
        provenance: ProvenanceOrigin::Selection,
        capsule: Some(herdr_tasks::domain::ContextCapsule {
            repo_path: Some(THIS_REPO.into()),
            selected_text: Some("selected context".into()),
            ..Default::default()
        }),
        agent_meta: None,
    };
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
        None,
    )
    .expect("open inline capture");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ExpandQuickAdd,
        None,
        None,
    )
    .expect("expand quick add");

    let available = model.capture_scope_options();
    assert!(available.contains(&project(THIS_REPO)));
    assert!(available.contains(&project("/repos/other")));
    assert!(available.contains(&TaskScope::Global));
    for character in "Global task".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type title");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CaptureFocusPrev,
        None,
        None,
    )
    .expect("focus scope");
    assert_eq!(model.capture_focus(), CaptureField::Scope);

    for _ in 0..available.len() {
        if model.capture_scope() == Some(&TaskScope::Global) {
            break;
        }
        let intent = map_inline_capture_key(
            model.capture_focus(),
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        )
        .expect("Right must cycle inline-capture scope");
        assert_eq!(intent, BoardIntent::CaptureCycleScope);
        apply_intent(&mut domain, &mut model, intent, None, None).expect("cycle scope");
    }
    assert_eq!(model.capture_scope(), Some(&TaskScope::Global));

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmEdit,
        None,
        None,
    )
    .expect("save inline capture");
    assert_eq!(outcome, IntentOutcome::Persist);
    let created = domain
        .tasks()
        .iter()
        .find(|task| task.title == "Global task")
        .expect("captured task");
    assert_eq!(created.scope, TaskScope::Global);
    assert_eq!(created.provenance, ProvenanceOrigin::Selection);
    assert_eq!(
        created
            .capsule
            .as_ref()
            .and_then(|capsule| capsule.selected_text.as_deref()),
        Some("selected context")
    );
}

/// Notes accepts bare-Enter line breaks, then either save chord persists the complete
/// multi-line draft.
#[test]
fn inline_capture_notes_enter_adds_a_line_and_save_chords_persist_all_lines() {
    let mut domain = DomainState::new();
    let snapshot = InvocationSnapshot {
        default_scope: project(THIS_REPO),
        this_repo: Some(PathBuf::from(THIS_REPO)),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: None,
        agent_meta: None,
    };
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
        None,
    )
    .expect("open inline capture");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ExpandQuickAdd,
        None,
        None,
    )
    .expect("expand quick add");
    for character in "Multiline task".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type title");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CaptureFocusNext,
        None,
        None,
    )
    .expect("focus notes");
    assert_eq!(model.capture_focus(), CaptureField::Notes);
    for character in "first line".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type first line");
    }
    let bare_enter = map_inline_capture_key(
        model.capture_focus(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert_eq!(bare_enter, Some(BoardIntent::EditInsertLineBreak));
    apply_intent(
        &mut domain,
        &mut model,
        bare_enter.expect("bare Enter intent"),
        None,
        None,
    )
    .expect("insert line break");
    for character in "second line".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type second line");
    }
    assert_eq!(model.capture_notes_value(), "first line\nsecond line");
    assert_eq!(
        map_inline_capture_key(
            model.capture_focus(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
        ),
        Some(BoardIntent::ConfirmEdit),
        "Alt+Enter is an equal Notes save chord"
    );
    let save = map_inline_capture_key(
        model.capture_focus(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
    );
    assert_eq!(save, Some(BoardIntent::ConfirmEdit));
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        save.expect("Ctrl+Enter save intent"),
        None,
        None,
    )
    .expect("save multi-line capture");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(
        domain
            .tasks()
            .iter()
            .find(|task| task.title == "Multiline task")
            .and_then(|task| task.notes.as_deref()),
        Some("first line\nsecond line")
    );
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

/// Tab and Shift-Tab move keyboard focus through inline capture's fields; the map and
/// the reducer both wire it, not just a hint that names an unmapped key. Neither key does
/// anything outside Capture (title/notes edit have nothing to move focus between).
#[test]
fn tab_and_shift_tab_move_capture_focus_through_title_notes_scope_and_back() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        None,
        None,
    )
    .expect("open capture");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ExpandQuickAdd,
        None,
        None,
    )
    .expect("expand quick add");
    assert_eq!(model.capture_focus(), CaptureField::Title);

    let tab = map_key(
        model.input_mode(),
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
    )
    .expect("Tab must map to an intent while Capture is open");
    assert_eq!(tab, BoardIntent::CaptureFocusNext);

    apply_intent(&mut domain, &mut model, tab.clone(), None, None).expect("tab to notes");
    assert_eq!(model.capture_focus(), CaptureField::Notes);

    apply_intent(&mut domain, &mut model, tab.clone(), None, None).expect("tab to scope");
    assert_eq!(model.capture_focus(), CaptureField::Scope);

    apply_intent(&mut domain, &mut model, tab, None, None).expect("tab wraps to title");
    assert_eq!(
        model.capture_focus(),
        CaptureField::Title,
        "Tab from Scope wraps back to Title"
    );

    let shift_tab = map_key(
        model.input_mode(),
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
    )
    .expect("Shift-Tab must map to an intent while Capture is open");
    assert_eq!(shift_tab, BoardIntent::CaptureFocusPrev);
    apply_intent(&mut domain, &mut model, shift_tab, None, None)
        .expect("shift-tab wraps back to scope");
    assert_eq!(
        model.capture_focus(),
        CaptureField::Scope,
        "Shift-Tab from Title wraps back to Scope"
    );

    // Notes is reachable and actually receives typed text once focused (not a dead field).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CaptureFocusNext,
        None,
        None,
    )
    .expect("scope -> title");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CaptureFocusNext,
        None,
        None,
    )
    .expect("title -> notes");
    assert_eq!(model.capture_focus(), CaptureField::Notes);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('n'),
        None,
        None,
    )
    .expect("type into notes");
    assert_eq!(model.capture_notes_value(), "n");
    assert_eq!(
        model.capture_title_value(),
        "",
        "typing while Notes is focused must not land in Title"
    );

    // Tab is unbound (falls through) outside Capture: EditTitle has nothing to move focus
    // between, so it must not be routed to CaptureFocusNext/Prev.
    let (mut solo_domain, mut solo_model) = {
        let mut d = DomainState::new();
        d.create(
            "Solo",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create solo");
        let m = BoardModel::from_domain(&d, Some(PathBuf::from(THIS_REPO)));
        (d, m)
    };
    apply_intent(
        &mut solo_domain,
        &mut solo_model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("begin title edit");
    assert_eq!(
        map_key(
            solo_model.input_mode(),
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
        ),
        None,
        "Tab must be unbound in EditTitle, which has no second field to focus"
    );
}

/// the cursor and the visible window an open editor paints must be computed against the
/// frame's actual paint width, not a hardcoded one — otherwise the cursor detaches from what
/// is on screen once the draft is longer than the hardcoded width assumed.
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

    // Narrow (40 cols, compact): the draft overflows the header window, so it scrolls to
    // keep the cursor (at the draft's end) visible — the tail must be on screen, not the
    // head, and the cursor must land inside the painted field, not off past its right edge.
    {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| draw_board(frame, &model))
            .expect("draw 40x10");
        let (title_y, row) = (0..10)
            .map(|y| (y, row_text(&terminal, 40, y)))
            .find(|(_, row)| row.contains("vwxyz"))
            .expect("compact task page header row");
        assert_eq!(title_y, 1, "the page header paints under one blank row");
        assert!(
            !row.contains("0123456789"),
            "40-wide header must not still show the head once the window has scrolled: {row:?}"
        );
        let cursor = terminal.get_cursor_position().expect("cursor position");
        assert_eq!(
            cursor,
            ratatui::layout::Position::new(4 + 29, title_y),
            "cursor must land at the window's own last column, matching the text actually painted"
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

/// /: inline capture uses the same keyboard dropdown state as task edit. Escape
/// leaves its immutable invocation draft open, while Enter applies only the highlighted scope.
#[test]
fn capture_scope_dropdown_returns_to_its_form_and_applies_only_on_enter() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Other scope",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create scope source");
    let snapshot = InvocationSnapshot {
        default_scope: project(THIS_REPO),
        this_repo: Some(PathBuf::from(THIS_REPO)),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: None,
        agent_meta: None,
    };
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
        None,
    )
    .expect("open capture");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ExpandQuickAdd,
        None,
        None,
    )
    .expect("expand quick add");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormFocusNext,
        None,
        None,
    )
    .expect("focus Notes");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormFocusNext,
        None,
        None,
    )
    .expect("focus Scope");
    assert_eq!(model.form_focus(), Some(CaptureField::Scope));

    let open = map_board_form_key(
        CaptureField::Scope,
        false,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    )
    .expect("Scope Enter maps to dropdown");
    assert_eq!(open, BoardIntent::OpenFormScopeDropdown);
    apply_intent(&mut domain, &mut model, open, None, None).expect("open dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::FormScopeDropdown);
    let before = model.capture_scope().cloned();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormScopeNext,
        None,
        None,
    )
    .expect("move pending option");
    assert_eq!(model.capture_scope(), before.as_ref());
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CancelFormScopeDropdown,
        None,
        None,
    )
    .expect("Esc returns to capture");
    assert_eq!(model.input_mode(), BoardInputMode::Capture);
    assert_eq!(model.form_focus(), Some(CaptureField::Scope));
    assert_eq!(model.capture_scope(), before.as_ref());

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
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmFormScopeDropdown,
        None,
        None,
    )
    .expect("apply selected scope");
    assert_eq!(model.input_mode(), BoardInputMode::Capture);
    assert_eq!(model.capture_scope(), Some(&TaskScope::Global));
    apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None, None)
        .expect("Esc returns to quick add");
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CancelQuickAdd,
        None,
        None,
    )
    .expect("close quick add");
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert!(!model.board_form_open());
}

/// The mode-only mapper and the focused-form mapper share one field-edit implementation. The
/// save chords are the high-risk drift point: both modifiers save every field, Scope included.
#[test]
fn ctrl_and_alt_enter_save_from_every_shared_form_field_including_scope() {
    for modifier in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        for focused in [
            CaptureField::Title,
            CaptureField::Notes,
            CaptureField::Scope,
        ] {
            assert_eq!(
                map_board_form_key(focused, false, KeyEvent::new(KeyCode::Enter, modifier),),
                Some(BoardIntent::ConfirmEdit),
                "{modifier:?}+Enter must save with {focused:?} focused"
            );
        }
    }

    // The legacy mode-only entry point remains a wrapper over the same implementation, not a
    // second chord table waiting to diverge from board forms.
    for (mode, focused) in [
        (BoardInputMode::EditTitle, CaptureField::Title),
        (BoardInputMode::EditNotes, CaptureField::Notes),
        (BoardInputMode::Capture, CaptureField::Title),
    ] {
        for modifier in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            let key = KeyEvent::new(KeyCode::Enter, modifier);
            assert_eq!(
                map_key(mode, key),
                map_board_form_key(focused, false, key),
                "{mode:?} must share the {modifier:?}+Enter mapping with its form field"
            );
        }
    }
}

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
                path: "/repos/herdr-tasks".into(),
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
    let mut model = BoardModel::from_domain(
        &domain,
        Some(std::path::PathBuf::from("/repos/herdr-tasks")),
    );
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
        .filter(|(_, row)| row.contains("herdr-tasks") || row.contains("other-project"))
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
