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
use herdr_tasks::ui::board::{apply_intent, draw_board, BoardInputMode, BoardModel, IntentOutcome};
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
fn alt_a_opens_focused_bar_and_typing_enter_uses_the_invocation_scope_then_closes() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/invocation")));
    let snap = snapshot();

    assert_eq!(
        map_key(
            BoardInputMode::Normal,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT)
        ),
        Some(BoardIntent::OpenCapture)
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

fn render_text(model: &BoardModel, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| draw_board(frame, model))
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    herdr_tasks::ui::render::assert_buffer_mono(buffer);
    buffer.content().iter().map(|cell| cell.symbol()).collect()
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
fn capture_bar_renders_two_bottom_rows_at_standard_and_compact_sizes_without_color_sgr() {
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
    let compact = render_text(&model, 40, 10);
    for text in ["visible task", "title…", "enter save"] {
        assert!(compact.contains(text), "missing {text:?}: {compact}");
    }

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
