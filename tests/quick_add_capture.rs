//! Capture-bar regression coverage.

use std::cell::RefCell;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::app::{apply_board_intent_with_save_recovery, BoardSaveContext};
use herdr_tasks::context::InvocationSnapshot;
use herdr_tasks::domain::{DomainState, ProvenanceOrigin, TaskScope};
use herdr_tasks::save_recovery::SaveRecovery;
use herdr_tasks::ui::board::{
    apply_intent, board_hit_map, draw_board, BoardInputMode, BoardModel, IntentOutcome,
};
use herdr_tasks::ui::input::{map_key, BoardIntent};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

fn snapshot() -> InvocationSnapshot {
    InvocationSnapshot {
        default_scope: TaskScope::Project {
            path: "/repos/invocation".into(),
        },
        this_repo: Some(PathBuf::from("/repos/invocation")),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: None,
        agent_meta: None,
    }
}

fn apply(
    domain: &mut DomainState,
    model: &mut BoardModel,
    intent: BoardIntent,
    snapshot: Option<&InvocationSnapshot>,
) -> IntentOutcome {
    apply_intent(domain, model, intent, snapshot, None).expect("apply capture-bar intent")
}

fn open(domain: &mut DomainState, model: &mut BoardModel, snapshot: &InvocationSnapshot) {
    assert_eq!(
        apply(domain, model, BoardIntent::OpenCapture, Some(snapshot)),
        IntentOutcome::None
    );
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
}

fn type_title(domain: &mut DomainState, model: &mut BoardModel, title: &str) {
    for character in title.chars() {
        apply(domain, model, BoardIntent::QuickAddInsert(character), None);
    }
}

#[test]
fn plus_opens_focused_bar_regardless_of_shift_and_legacy_chord_is_unbound() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/invocation")));
    let snap = snapshot();

    for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
        assert_eq!(
            map_key(
                BoardInputMode::Normal,
                KeyEvent::new(KeyCode::Char('+'), modifiers)
            ),
            Some(BoardIntent::OpenCapture)
        );
    }
    assert_eq!(
        map_key(
            BoardInputMode::Normal,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT)
        ),
        None
    );
    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "capture this");
    assert_eq!(
        apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None),
        IntentOutcome::Persist
    );
    // The reducer retains the draft until the save boundary refreshes from the durable state.
    model.sync_from_domain(&domain);

    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(model.visible_tasks().len(), 1);
    let task = domain.tasks().first().expect("created task");
    assert_eq!(task.title, "capture this");
    assert_eq!(
        task.scope,
        TaskScope::Project {
            path: "/repos/invocation".into()
        }
    );
    assert_eq!(task.provenance, ProvenanceOrigin::Capture);
}

fn create_project_fixture(domain: &mut DomainState, path: &str) {
    domain
        .create(
            "known project",
            None,
            TaskScope::Project { path: path.into() },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("fixture project task");
}

fn save_quick_add(domain: &mut DomainState, model: &mut BoardModel, title: &str) {
    open(domain, model, &snapshot());
    type_title(domain, model, title);
    assert_eq!(
        apply(domain, model, BoardIntent::QuickAddSave, None),
        IntentOutcome::Persist
    );
    model.sync_from_domain(domain);
}

#[test]
fn unique_project_basename_resolves_for_expansion_and_saved_status() {
    let mut domain = DomainState::new();
    create_project_fixture(&mut domain, "/work/herdr-tasks");
    let mut model = BoardModel::from_domain(&domain, None);

    open(&mut domain, &mut model, &snapshot());
    type_title(&mut domain, &mut model, "expanded task !p herdr-tasks");
    apply(&mut domain, &mut model, BoardIntent::ExpandQuickAdd, None);
    assert_eq!(
        model.capture_scope(),
        Some(&TaskScope::Project {
            path: "/work/herdr-tasks".into()
        })
    );
    assert_eq!(
        apply(&mut domain, &mut model, BoardIntent::ConfirmEdit, None),
        IntentOutcome::Persist
    );
    model.sync_from_domain(&domain);
    assert_eq!(
        domain.tasks().last().expect("saved task").scope,
        TaskScope::Project {
            path: "/work/herdr-tasks".into()
        }
    );
    assert_eq!(model.message(), Some("saved to herdr-tasks"));
}

#[test]
fn invocation_default_scope_and_this_repo_are_project_candidates() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);
    let mut snap = snapshot();
    snap.default_scope = TaskScope::Project {
        path: "/repos/default-project".into(),
    };
    snap.this_repo = Some(PathBuf::from("/repos/current-project"));

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "from default !p default-project");
    apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None);
    model.sync_from_domain(&domain);
    assert_eq!(
        domain.tasks().last().expect("default task").scope,
        TaskScope::Project {
            path: "/repos/default-project".into()
        }
    );

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "from repo !p current-project");
    apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None);
    model.sync_from_domain(&domain);
    assert_eq!(
        domain.tasks().last().expect("repo task").scope,
        TaskScope::Project {
            path: "/repos/current-project".into()
        }
    );
}

#[test]
fn unmatched_project_basename_stays_verbatim() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);

    save_quick_add(&mut domain, &mut model, "unmatched task !p missing");
    assert_eq!(
        domain.tasks().last().expect("saved task").scope,
        TaskScope::Project {
            path: "missing".into()
        }
    );
}

#[test]
fn ambiguous_project_basename_stays_verbatim() {
    let mut domain = DomainState::new();
    create_project_fixture(&mut domain, "/work/one/shared");
    create_project_fixture(&mut domain, "/work/two/shared");
    let mut model = BoardModel::from_domain(&domain, None);

    save_quick_add(&mut domain, &mut model, "ambiguous task !p shared");
    assert_eq!(
        domain.tasks().last().expect("saved task").scope,
        TaskScope::Project {
            path: "shared".into()
        }
    );
}

#[test]
fn project_token_with_a_slash_stays_verbatim() {
    let mut domain = DomainState::new();
    create_project_fixture(&mut domain, "/work/herdr-tasks");
    let mut model = BoardModel::from_domain(&domain, None);

    save_quick_add(
        &mut domain,
        &mut model,
        "explicit task !p elsewhere/herdr-tasks",
    );
    assert_eq!(
        domain.tasks().last().expect("saved task").scope,
        TaskScope::Project {
            path: "elsewhere/herdr-tasks".into()
        }
    );
}

#[test]
fn ctrl_enter_uses_the_same_project_basename_resolution() {
    let mut domain = DomainState::new();
    create_project_fixture(&mut domain, "/work/ctrl-target");
    let mut model = BoardModel::from_domain(&domain, None);

    open(&mut domain, &mut model, &snapshot());
    type_title(&mut domain, &mut model, "keep open !p ctrl-target");
    assert_eq!(
        apply(&mut domain, &mut model, BoardIntent::QuickAddSaveNext, None),
        IntentOutcome::Persist
    );
    model.sync_from_domain(&domain);
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(
        domain.tasks().last().expect("saved task").scope,
        TaskScope::Project {
            path: "/work/ctrl-target".into()
        }
    );
}

#[test]
fn capture_bar_strips_global_and_project_scope_tokens() {
    let snap = snapshot();
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "global task !g");
    apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None);
    model.sync_from_domain(&domain);
    assert_eq!(domain.tasks()[0].title, "global task");
    assert_eq!(domain.tasks()[0].scope, TaskScope::Global);

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "project task !p /repos/other");
    apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None);
    model.sync_from_domain(&domain);
    assert_eq!(domain.tasks()[1].title, "project task");
    assert_eq!(
        domain.tasks()[1].scope,
        TaskScope::Project {
            path: "/repos/other".into()
        }
    );

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "next task");
    apply(&mut domain, &mut model, BoardIntent::QuickAddSaveNext, None);
    model.sync_from_domain(&domain);
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(model.quick_add_title_value(), "");
    apply(&mut domain, &mut model, BoardIntent::CancelQuickAdd, None);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(model.message(), Some("saved to invocation"));
}

#[test]
fn empty_enter_stays_open_esc_discards_and_alt_enter_expands_the_seeded_full_form() {
    let snap = snapshot();
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);

    open(&mut domain, &mut model, &snap);
    assert_eq!(
        apply(&mut domain, &mut model, BoardIntent::QuickAddSave, None),
        IntentOutcome::None
    );
    assert_eq!(model.message(), Some("Title required"));
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert!(domain.tasks().is_empty());

    apply(&mut domain, &mut model, BoardIntent::CancelQuickAdd, None);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert!(domain.tasks().is_empty());

    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "needs notes");
    assert_eq!(
        map_key(
            BoardInputMode::QuickAdd,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        ),
        Some(BoardIntent::ExpandQuickAdd)
    );
    apply(&mut domain, &mut model, BoardIntent::ExpandQuickAdd, None);
    assert_eq!(model.input_mode(), BoardInputMode::Capture);
    assert_eq!(model.capture_title_value(), "needs notes");
    assert_eq!(
        model.capture_scope(),
        Some(&TaskScope::Project {
            path: "/repos/invocation".into()
        })
    );
    apply(&mut domain, &mut model, BoardIntent::CancelEdit, None);
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(model.quick_add_title_value(), "needs notes");
}

#[test]
fn failed_save_keeps_the_draft_for_retry_or_cancel() {
    let snap = snapshot();
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);
    let mut recovery = SaveRecovery::new();
    open(&mut domain, &mut model, &snap);
    type_title(&mut domain, &mut model, "recover me");

    let failed = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::QuickAddSave,
            snapshot: None,
            host: None,
        },
        |_| Err("injected failure".into()),
    )
    .expect("save failure remains in bar");
    assert_eq!(failed, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);
    assert_eq!(model.quick_add_title_value(), "recover me");
    assert!(domain.tasks().is_empty());

    let retried = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::RetrySave,
            snapshot: None,
            host: None,
        },
        |_| Ok(()),
    )
    .expect("retry");
    assert_eq!(retried, IntentOutcome::Persisted);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(domain.tasks()[0].title, "recover me");

    let mut cancelled_domain = DomainState::new();
    let mut cancelled_model = BoardModel::from_domain(&cancelled_domain, None);
    let mut cancelled_recovery = SaveRecovery::new();
    open(&mut cancelled_domain, &mut cancelled_model, &snap);
    type_title(
        &mut cancelled_domain,
        &mut cancelled_model,
        "keep this draft",
    );
    apply_board_intent_with_save_recovery(
        &mut cancelled_domain,
        &mut cancelled_model,
        &mut cancelled_recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::QuickAddSave,
            snapshot: None,
            host: None,
        },
        |_| Err("injected failure".into()),
    )
    .expect("save failure");
    apply_board_intent_with_save_recovery(
        &mut cancelled_domain,
        &mut cancelled_model,
        &mut cancelled_recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::CancelSave,
            snapshot: None,
            host: None,
        },
        |_| Ok(()),
    )
    .expect("cancel recovery");
    assert_eq!(cancelled_model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(cancelled_model.quick_add_title_value(), "keep this draft");
    assert!(cancelled_domain.tasks().is_empty());
}

fn render_rows(model: &BoardModel, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| draw_board(frame, model))
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    herdr_tasks::ui::render::assert_buffer_mono(buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

fn render_text(model: &BoardModel, width: u16, height: u16) -> String {
    render_rows(model, width, height).concat()
}

#[derive(Clone, Default)]
struct AnsiWriter(Rc<RefCell<Vec<u8>>>);

impl Write for AnsiWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn capture_bar_renders_spaced_three_row_block_and_stays_bounded_without_color_sgr() {
    let mut domain = DomainState::new();
    domain
        .create(
            "visible task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("fixture task");
    let mut model = BoardModel::from_domain(&domain, None);
    open(&mut domain, &mut model, &snapshot());

    let standard = render_text(&model, 80, 24);
    for text in [
        "visible task",
        "title…",
        "enter save · ctrl+enter save+next · alt+enter more · esc close",
    ] {
        assert!(standard.contains(text), "missing {text:?}: {standard}");
    }
    let standard_rows = render_rows(&model, 80, 24);
    assert!(standard_rows[20].trim().is_empty(), "blank row above input");
    assert!(standard_rows[21].contains("title…"), "input row");
    assert!(standard_rows[22].trim().is_empty(), "blank row below input");
    let hits = board_hit_map(Rect::new(0, 0, 80, 24), &model);
    assert!(
        hits.regions
            .iter()
            .all(|hit| !matches!(hit.area.y, 20 | 22)),
        "blank quick-add rows must have no mouse hit targets: {hits:?}"
    );

    let compact_rows = render_rows(&model, 40, 10);
    let compact = compact_rows.concat();
    for text in ["visible task", "title…", "enter save"] {
        assert!(compact.contains(text), "missing {text:?}: {compact}");
    }
    assert!(compact_rows.iter().all(|row| row.chars().count() == 40));

    let writer = AnsiWriter::default();
    let bytes = Rc::clone(&writer.0);
    let backend = CrosstermBackend::new(writer);
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 80, 24)),
        },
    )
    .expect("ANSI terminal");
    terminal
        .draw(|frame| draw_board(frame, &model))
        .expect("ANSI draw");
    drop(terminal);
    let output = String::from_utf8(bytes.borrow().clone()).expect("ANSI output");
    herdr_tasks::ui::render::assert_no_color_sgr(&output);
}
