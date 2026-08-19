//! F6 board save-failure recovery.

use std::fs;
use std::path::PathBuf;

use herdr_tasks::context::InvocationSnapshot;
use herdr_tasks::store::TaskStore;
use herdr_tasks::ui::capture::{
    apply_capture_intent, CaptureField, CaptureModel, CaptureOutcome, CaptureScopeChoice,
};
use herdr_tasks::ui::input::{map_capture_key_state, CaptureIntent};
use herdr_tasks::ui::mouse::{
    capture_layout_for_model, capture_recovery_layout, map_capture_mouse,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::app::{apply_board_intent_with_save_recovery, BoardSaveContext};
use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use herdr_tasks::save_recovery::SaveRecovery;
use herdr_tasks::ui::board::{
    apply_intent, board_hit_map, resolve_board_command, BoardInputMode, BoardModel, CommandSurface,
    IntentOutcome,
};
use herdr_tasks::ui::capture::draw_capture;
use herdr_tasks::ui::input::{map_key, BoardIntent};
use herdr_tasks::ui::mouse::{left_click, map_board_mouse, BoardPopup};
use herdr_tasks::ui::render::QueueHitTarget;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

/// Capture areas: roomy, and narrow enough for the compact scope controls.
const WIDE_CAPTURE: (u16, u16) = (80, 16);
const NARROW_CAPTURE: (u16, u16) = (50, 14);

fn area((width, height): (u16, u16)) -> Rect {
    Rect::new(0, 0, width, height)
}

fn painted(mut draw: impl FnMut(&mut ratatui::Frame), (width, height): (u16, u16)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal.draw(|frame| draw(frame)).expect("draw");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// Painted Capture text for one terminal area, exactly as the capture loop draws it.
fn capture_painted(model: &CaptureModel, mode: (u16, u16)) -> String {
    painted(|frame| draw_capture(frame, model), mode)
}

fn capture_snapshot() -> InvocationSnapshot {
    InvocationSnapshot {
        default_scope: TaskScope::Project {
            path: "/repos/app".into(),
        },
        this_repo: Some(PathBuf::from("/repos/app")),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: None,
        agent_meta: None,
    }
}

fn bad_store_path() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "herdr-tasks-capture-recovery-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    let file = root.join("not-a-directory");
    fs::write(&file, "not a state directory").expect("create blocking file");
    (root, file)
}

fn enter_capture_title(
    domain: &mut DomainState,
    snapshot: &InvocationSnapshot,
    model: &mut CaptureModel,
    title: &str,
) {
    for c in title.chars() {
        apply_capture_intent(domain, None, snapshot, model, CaptureIntent::Insert(c))
            .expect("insert title");
    }
}

#[test]
fn standalone_capture_save_failure_keeps_draft_then_keyboard_retry_persists_once_and_closes() {
    let (root, bad_path) = bad_store_path();
    let store = TaskStore::new(bad_path);
    let snapshot = capture_snapshot();
    let mut domain = DomainState::new();
    let mut model = CaptureModel::from_snapshot(&snapshot);
    enter_capture_title(&mut domain, &snapshot, &mut model, "Recover this capture");
    apply_capture_intent(
        &mut domain,
        None,
        &snapshot,
        &mut model,
        CaptureIntent::FocusNext,
    )
    .expect("focus notes");

    let outcome = apply_capture_intent(
        &mut domain,
        Some(&store),
        &snapshot,
        &mut model,
        CaptureIntent::Save,
    )
    .expect("save errors must stay in Capture");

    assert_eq!(outcome, CaptureOutcome::None);
    assert_eq!(model.title(), "Recover this capture");
    assert_eq!(model.focused(), CaptureField::Notes);
    assert!(model
        .message()
        .is_some_and(|message| message.contains("save failed")));
    assert!(model.is_save_recovery());
    assert!(
        domain.tasks().is_empty(),
        "working create must not become current state"
    );

    let retry = map_capture_key_state(
        model.focused(),
        model.is_path_editing(),
        model.is_save_recovery(),
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("recovery retry key");
    assert_eq!(retry, CaptureIntent::RetrySave);
    assert_eq!(
        map_capture_key_state(
            model.focused(),
            model.is_path_editing(),
            model.is_save_recovery(),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        ),
        Some(CaptureIntent::CancelSave)
    );
    let layout = capture_recovery_layout(Rect::new(0, 0, 80, 16));
    assert_eq!(
        map_capture_mouse(
            &layout,
            left_click(layout.save_chip.rect.x + 1, layout.save_chip.rect.y),
        ),
        Some(CaptureIntent::RetrySave)
    );
    assert_eq!(
        map_capture_mouse(
            &layout,
            left_click(layout.cancel_chip.rect.x + 1, layout.cancel_chip.rect.y),
        ),
        Some(CaptureIntent::CancelSave)
    );

    let good_path = root.join("good-state");
    let good_store = TaskStore::new(&good_path);
    let outcome =
        apply_capture_intent(&mut domain, Some(&good_store), &snapshot, &mut model, retry)
            .expect("retry save");
    assert!(
        matches!(outcome, CaptureOutcome::Saved(_)),
        "standalone closes only after retry success"
    );
    assert!(!model.is_save_recovery());
    assert_eq!(domain.tasks().len(), 1);
    assert_eq!(
        good_store
            .load()
            .expect("reload persisted task")
            .tasks()
            .len(),
        1
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn capture_scope_choice_survives_save_failure_with_retry_cancel_retained() {
    let (root, bad_path) = bad_store_path();
    let store = TaskStore::new(bad_path);
    let snapshot = capture_snapshot();
    let mut domain = DomainState::new();
    let mut model = CaptureModel::from_snapshot(&snapshot);
    enter_capture_title(&mut domain, &snapshot, &mut model, "Other scope capture");

    // Choose Other and type an explicit project path, then confirm it.
    apply_capture_intent(
        &mut domain,
        None,
        &snapshot,
        &mut model,
        CaptureIntent::SelectScope(CaptureScopeChoice::Other),
    )
    .expect("select other scope");
    while model.scope_path_edit().is_some_and(|path| !path.is_empty()) {
        apply_capture_intent(
            &mut domain,
            None,
            &snapshot,
            &mut model,
            CaptureIntent::Backspace,
        )
        .expect("clear path");
    }
    for c in "/repos/other".chars() {
        apply_capture_intent(
            &mut domain,
            None,
            &snapshot,
            &mut model,
            CaptureIntent::Insert(c),
        )
        .expect("type path");
    }
    apply_capture_intent(
        &mut domain,
        None,
        &snapshot,
        &mut model,
        CaptureIntent::Save,
    )
    .expect("confirm path");
    assert_eq!(
        model.scope(),
        &TaskScope::Project {
            path: "/repos/other".into(),
        }
    );

    apply_capture_intent(
        &mut domain,
        Some(&store),
        &snapshot,
        &mut model,
        CaptureIntent::Save,
    )
    .expect("save failure stays inline");
    assert!(model.is_save_recovery());

    // Retry / Cancel remain the only Capture mouse routes while the failure is unresolved.
    let area = Rect::new(0, 0, 80, 16);
    let layout = capture_layout_for_model(area, &model);
    assert!(layout.save_recovery);
    assert_eq!(
        map_capture_mouse(
            &layout,
            left_click(layout.cancel_chip.rect.x + 1, layout.cancel_chip.rect.y)
        ),
        Some(CaptureIntent::CancelSave)
    );
    for chip in &layout.scope_chips {
        assert_eq!(
            map_capture_mouse(&layout, left_click(chip.rect.x + 1, chip.rect.y)),
            None,
            "scope control must stay inert during save recovery"
        );
    }
    let retry = map_capture_mouse(
        &layout,
        left_click(layout.save_chip.rect.x + 1, layout.save_chip.rect.y),
    )
    .expect("retry click");
    assert_eq!(retry, CaptureIntent::RetrySave);

    let good_store = TaskStore::new(root.join("good-state"));
    let outcome =
        apply_capture_intent(&mut domain, Some(&good_store), &snapshot, &mut model, retry)
            .expect("retry save");
    let CaptureOutcome::Saved(id) = outcome else {
        panic!("expected Saved, got {outcome:?}");
    };
    assert_eq!(
        domain.get(id).expect("saved task").scope,
        TaskScope::Project {
            path: "/repos/other".into(),
        },
        "durable scope value must survive the failed save"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn inline_capture_save_failure_then_mouse_cancel_restores_baseline_before_board_return() {
    let (root, bad_path) = bad_store_path();
    let store = TaskStore::new(bad_path);
    let snapshot = capture_snapshot();
    let mut domain = DomainState::new();
    let mut model = CaptureModel::from_snapshot(&snapshot);
    enter_capture_title(&mut domain, &snapshot, &mut model, "Cancel this capture");
    apply_capture_intent(
        &mut domain,
        Some(&store),
        &snapshot,
        &mut model,
        CaptureIntent::Save,
    )
    .expect("save failure remains inline");
    assert!(model.is_save_recovery());
    assert!(domain.tasks().is_empty());

    let layout = capture_recovery_layout(Rect::new(0, 0, 80, 16));
    let cancel = map_capture_mouse(
        &layout,
        left_click(layout.cancel_chip.rect.x + 1, layout.cancel_chip.rect.y),
    )
    .expect("recovery cancel click");
    assert_eq!(cancel, CaptureIntent::CancelSave);
    assert_eq!(
        apply_capture_intent(&mut domain, Some(&store), &snapshot, &mut model, cancel)
            .expect("cancel recovery"),
        CaptureOutcome::Cancelled
    );
    assert!(!model.is_save_recovery());
    assert!(
        domain.tasks().is_empty(),
        "cancel restores the pre-create baseline"
    );

    // A fresh board model derived from the post-cancel domain is exactly what the real
    // app loop shows on return: no re-sync helper is needed once `handle_board_intent`
    // resyncs from `domain` on every successful path.
    let board = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
    assert!(
        board.visible_ids().is_empty(),
        "inline return shows no cancelled task"
    );

    let _ = fs::remove_dir_all(root);
}

fn board_state() -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Persist me",
            None,
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
    (domain, model, id)
}

#[test]
fn board_save_failure_retains_working_state_blocks_mutations_and_retries_exactly_that_state() {
    let (mut domain, mut model, id) = board_state();
    let baseline =
        serde_json::from_str(&serde_json::to_string(&domain).expect("serialize baseline"))
            .expect("deserialize baseline");
    let mut recovery = SaveRecovery::new();
    let mut saves = 0;

    let outcome = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline,
            intent: BoardIntent::Complete,
            snapshot: None,
            host: None,
        },
        |_| {
            saves += 1;
            Err("injected board save failure".into())
        },
    )
    .expect("reducer");

    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(saves, 1);
    assert!(recovery.is_pending());
    assert_eq!(
        recovery.baseline().unwrap().get(id).unwrap().status,
        HumanStatus::Ready
    );
    assert_eq!(
        recovery.working().unwrap().get(id).unwrap().status,
        HumanStatus::Done
    );
    assert!(
        domain.tasks().is_empty(),
        "working state belongs to recovery until resolved"
    );
    assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);
    assert_eq!(model.popup(), BoardPopup::SaveRecovery);
    assert_eq!(model.help_line(), "↑↓  ·  r retry  ·  c cancel");
    let failure_message = model.message().unwrap();
    assert!(failure_message.contains("save failed"), "{failure_message}");
    assert!(!failure_message.contains("completed"), "{failure_message}");

    let blocked = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::SetStatus(HumanStatus::Blocked),
            snapshot: None,
            host: None,
        },
        |_| {
            saves += 1;
            Ok(())
        },
    )
    .expect("blocked reducer");
    assert_eq!(blocked, IntentOutcome::None);
    assert_eq!(saves, 1, "blocked mutation must not reach persistence");
    assert_eq!(
        recovery.working().unwrap().get(id).unwrap().status,
        HumanStatus::Done
    );

    let retry = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::RetrySave,
            snapshot: None,
            host: None,
        },
        |state| {
            saves += 1;
            assert_eq!(state.get(id).unwrap().status, HumanStatus::Done);
            Ok(())
        },
    )
    .expect("retry");
    assert_eq!(retry, IntentOutcome::Persisted);
    assert_eq!(saves, 2);
    assert!(!recovery.is_pending());
    assert_eq!(domain.get(id).unwrap().status, HumanStatus::Done);
    assert!(model
        .message()
        .is_some_and(|message| message.contains("saved")));
}

#[test]
fn board_save_recovery_cancel_restores_baseline_and_keyboard_reaches_retry_cancel() {
    let (mut domain, mut model, id) = board_state();
    let baseline =
        serde_json::from_str(&serde_json::to_string(&domain).expect("serialize baseline"))
            .expect("deserialize baseline");
    let mut recovery = SaveRecovery::new();

    apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline,
            intent: BoardIntent::Complete,
            snapshot: None,
            host: None,
        },
        |_| Err("injected board save failure".into()),
    )
    .expect("failure reducer");

    assert_eq!(
        map_key(
            BoardInputMode::SaveRecovery,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        ),
        Some(BoardIntent::RetrySave)
    );
    assert_eq!(
        map_key(
            BoardInputMode::SaveRecovery,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        ),
        Some(BoardIntent::CancelSave)
    );
    assert_eq!(
        map_key(
            BoardInputMode::SaveRecovery,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        ),
        Some(BoardIntent::SelectNext),
        "keyboard navigation remains available while save recovery is unresolved"
    );
    for retired in ['1', '2', '3', '[', ']'] {
        assert_eq!(
            map_key(
                BoardInputMode::SaveRecovery,
                KeyEvent::new(KeyCode::Char(retired), KeyModifiers::NONE),
            ),
            None,
            "retired lens key {retired:?} stays inert during save recovery"
        );
    }
    // Save recovery paints no queue overlay yet: Retry/Cancel stay
    // keyboard-only (`r`/`c` above) until the renderer gains a control for them.

    let cancelled = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::CancelSave,
            snapshot: None,
            host: None,
        },
        |_| panic!("cancel must not persist"),
    )
    .expect("cancel");
    assert_eq!(cancelled, IntentOutcome::None);
    assert!(!recovery.is_pending());
    assert_eq!(domain.get(id).unwrap().status, HumanStatus::Ready);
    assert_eq!(model.selected_id(), Some(id));
    let message = model.message().unwrap();
    assert!(message.contains("cancelled"), "{message}");
    assert!(!message.contains("completed"), "{message}");
}

#[test]
fn command_surface_reaches_save_failure_retry_and_cancel_through_the_same_boundary() {
    let (mut domain, mut model, id) = board_state();
    let baseline =
        serde_json::from_str(&serde_json::to_string(&domain).expect("serialize baseline"))
            .expect("deserialize baseline");
    let mut recovery = SaveRecovery::new();
    let mut saves = 0;

    apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline,
            intent: BoardIntent::Complete,
            snapshot: None,
            host: None,
        },
        |_| {
            saves += 1;
            Err("injected board save failure".into())
        },
    )
    .expect("failure reducer");
    assert_eq!(saves, 1);
    assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);

    // Opening the palette while the failure is unresolved is presentation only.
    for intent in [
        BoardIntent::OpenCommandPalette,
        BoardIntent::CommandQueryInsert('R'),
        BoardIntent::CommandQueryInsert('e'),
        BoardIntent::CommandQueryInsert('T'),
    ] {
        let outcome = apply_board_intent_with_save_recovery(
            &mut domain,
            &mut model,
            &mut recovery,
            BoardSaveContext {
                baseline: DomainState::new(),
                intent,
                snapshot: None,
                host: None,
            },
            |_| panic!("command surface state must not persist"),
        )
        .expect("surface intent");
        assert_eq!(outcome, IntentOutcome::None);
    }
    assert_eq!(model.command_surface(), CommandSurface::Palette);
    assert_eq!(model.input_mode(), BoardInputMode::Palette);
    assert!(recovery.is_pending(), "the failed save is still unresolved");
    assert_eq!(
        model
            .visible_commands()
            .iter()
            .map(|command| command.intent.clone())
            .collect::<Vec<_>>(),
        vec![BoardIntent::RetrySave],
        "only the available recovery route is exposed"
    );

    // Mouse and keyboard reach Retry from the same resolved hit-map.
    let area = Rect::new(0, 0, 120, 24);
    let hits = board_hit_map(area, &model);
    let chip = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Command(0)))
        .expect("Retry hit region");
    // a command row click is `SelectCommand(index)`, resolved through
    // `resolve_board_command` exactly the way the keyboard's `ConfirmCommand` is (the same
    // reason `SelectIndex`/`SelectProjectOption` are not raw-equal to their keyboard
    // counterparts either); resolve on a clone so this check cannot perturb `model` below.
    let clicked = map_board_mouse(&model, &hits, left_click(chip.area.x + 1, chip.area.y))
        .expect("mouse command intent");
    let mut resolved_model = model.clone();
    assert_eq!(
        resolve_board_command(&mut resolved_model, clicked),
        Some(BoardIntent::RetrySave)
    );

    let retried = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::ConfirmCommand,
            snapshot: None,
            host: None,
        },
        |state| {
            saves += 1;
            assert_eq!(
                state.get(id).unwrap().status,
                HumanStatus::Done,
                "retry persists exactly the failed working state"
            );
            Err("injected board save failure".into())
        },
    )
    .expect("palette retry");
    assert_eq!(retried, IntentOutcome::None);
    assert_eq!(saves, 2, "the palette retries through the same boundary");
    assert!(recovery.is_pending());
    assert_eq!(model.command_surface(), CommandSurface::None);
    assert_eq!(model.popup(), BoardPopup::SaveRecovery);

    // V1 has no action sheet. Its direct recovery route restores the same baseline without a
    // second command surface.
    let cancelled = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::CancelSave,
            snapshot: None,
            host: None,
        },
        |_| panic!("cancel must not persist"),
    )
    .expect("direct cancel");
    assert_eq!(cancelled, IntentOutcome::None);
    assert_eq!(saves, 2);
    assert!(!recovery.is_pending());
    assert_eq!(domain.get(id).unwrap().status, HumanStatus::Ready);
    assert_eq!(model.command_surface(), CommandSurface::None);
    let message = model.message().unwrap();
    assert!(message.contains("cancelled"), "{message}");
}

/// A board whose Complete failed to persist, left waiting for Retry or Cancel.
fn failed_board_save() -> (
    DomainState,
    BoardModel,
    SaveRecovery<DomainState>,
    uuid::Uuid,
) {
    let (mut domain, mut model, id) = board_state();
    let baseline =
        serde_json::from_str(&serde_json::to_string(&domain).expect("serialize baseline"))
            .expect("deserialize baseline");
    let mut recovery = SaveRecovery::new();
    apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline,
            intent: BoardIntent::Complete,
            snapshot: None,
            host: None,
        },
        |_| Err("injected board save failure".into()),
    )
    .expect("failure reducer");
    assert!(recovery.is_pending());
    (domain, model, recovery, id)
}

/// / F7: a paste into the palette narrows the query exactly as typing the same run does,
/// even while a failed save is unresolved. The palette is how a user finds Retry, so a paste
/// that silently did nothing there would strand the very route it is opened for.
#[test]
fn a_palette_paste_during_save_recovery_narrows_the_query_like_typing() {
    // Typed baseline: three key presses into the palette opened over the failed save.
    let (mut typed_domain, mut typed_model, mut typed_recovery, _) = failed_board_save();
    presentation_only(
        &mut typed_domain,
        &mut typed_model,
        &mut typed_recovery,
        BoardIntent::OpenCommandPalette,
    );
    for character in ['R', 'e', 'T'] {
        presentation_only(
            &mut typed_domain,
            &mut typed_model,
            &mut typed_recovery,
            BoardIntent::CommandQueryInsert(character),
        );
    }
    let typed_query = typed_model.command_query().to_string();
    let typed_commands = typed_model
        .visible_commands()
        .iter()
        .map(|command| command.intent.clone())
        .collect::<Vec<_>>();
    assert_eq!(typed_query, "ReT");
    assert_eq!(typed_commands, vec![BoardIntent::RetrySave]);

    // Pasted run: the same characters arriving as one bracketed paste.
    let (mut pasted_domain, mut pasted_model, mut pasted_recovery, pasted_id) = failed_board_save();
    presentation_only(
        &mut pasted_domain,
        &mut pasted_model,
        &mut pasted_recovery,
        BoardIntent::OpenCommandPalette,
    );
    presentation_only(
        &mut pasted_domain,
        &mut pasted_model,
        &mut pasted_recovery,
        BoardIntent::CommandQueryInsertText("ReT".to_string()),
    );

    assert_eq!(
        pasted_model.command_query(),
        typed_query,
        "a paste narrows the recovery palette query exactly as typing does"
    );
    assert_eq!(
        pasted_model
            .visible_commands()
            .iter()
            .map(|command| command.intent.clone())
            .collect::<Vec<_>>(),
        typed_commands,
        "the same query resolves to the same command list"
    );
    assert_eq!(pasted_model.command_surface(), CommandSurface::Palette);
    assert_eq!(pasted_model.input_mode(), BoardInputMode::Palette);

    // And the paste is presentation only: `presentation_only` panics if the save boundary is
    // reached at all, the failed save is still waiting for Retry or Cancel, and the domain the
    // paste ran against is exactly the one the typed run left.
    assert!(pasted_recovery.is_pending());
    assert_eq!(
        serde_json::to_string(&pasted_domain).expect("serialize pasted domain"),
        serde_json::to_string(&typed_domain).expect("serialize typed domain"),
        "the pasted run writes nothing the typed run does not"
    );

    // Retry still reaches the exact failed state from the palette the paste narrowed.
    let mut saves = 0;
    let retried = apply_board_intent_with_save_recovery(
        &mut pasted_domain,
        &mut pasted_model,
        &mut pasted_recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent: BoardIntent::ConfirmCommand,
            snapshot: None,
            host: None,
        },
        |state| {
            saves += 1;
            assert_eq!(state.get(pasted_id).unwrap().status, HumanStatus::Done);
            Err("injected board save failure".into())
        },
    )
    .expect("palette retry after paste");
    assert_eq!(retried, IntentOutcome::None);
    assert_eq!(
        saves, 1,
        "the narrowed palette still retries the failed save"
    );
}

/// Drive a presentation-only intent, proving it cannot reach the save boundary.
fn presentation_only(
    domain: &mut DomainState,
    model: &mut BoardModel,
    recovery: &mut SaveRecovery<DomainState>,
    intent: BoardIntent,
) {
    let outcome = apply_board_intent_with_save_recovery(
        domain,
        model,
        recovery,
        BoardSaveContext {
            baseline: DomainState::new(),
            intent,
            snapshot: None,
            host: None,
        },
        |_| panic!("presentation state must not persist"),
    )
    .expect("presentation intent");
    assert_eq!(outcome, IntentOutcome::None);
}

/// F7 / / /: board Retry and Cancel stay both mouse- and
/// keyboard-reachable in every board mode, resolve through the one save boundary with the
/// direct route's result, and never appear in resize guidance.
#[test]
fn capture_retry_and_cancel_stay_mouse_and_keyboard_reachable_at_every_capture_width() {
    for mode in [WIDE_CAPTURE, NARROW_CAPTURE] {
        let (root, bad_path) = bad_store_path();
        let store = TaskStore::new(&bad_path);
        let snapshot = capture_snapshot();
        let mut domain = DomainState::new();
        let mut model = CaptureModel::from_snapshot(&snapshot);
        enter_capture_title(&mut domain, &snapshot, &mut model, "Narrow recovery");
        apply_capture_intent(
            &mut domain,
            Some(&store),
            &snapshot,
            &mut model,
            CaptureIntent::Save,
        )
        .expect("save failure stays inline");
        assert!(model.is_save_recovery());
        assert!(domain.tasks().is_empty());

        let screen = capture_painted(&model, mode);
        assert!(screen.contains("save failed"), "{mode:?}: {screen:?}");

        let layout = capture_layout_for_model(area(mode), &model);
        assert!(layout.save_recovery);
        for (chip, intent, key) in [
            (layout.save_chip.clone(), CaptureIntent::RetrySave, 'r'),
            (layout.cancel_chip.clone(), CaptureIntent::CancelSave, 'c'),
        ] {
            assert!(
                chip.rect.width > 0,
                "{mode:?}: {intent:?} needs a mouse hit region"
            );
            let clicked = map_capture_mouse(&layout, left_click(chip.rect.x + 1, chip.rect.y));
            assert_eq!(clicked, Some(intent.clone()));
            assert_eq!(
                clicked,
                map_capture_key_state(
                    model.focused(),
                    model.is_path_editing(),
                    model.is_save_recovery(),
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                ),
                "{mode:?}: {intent:?} mouse and keyboard must agree"
            );
            assert!(
                screen.contains(chip.label.trim()),
                "{mode:?}: {intent:?} control must be visible: {screen:?}"
            );
        }

        // Retry from this width persists the retained draft exactly once.
        let good_store = TaskStore::new(root.join("good-state"));
        let saved = apply_capture_intent(
            &mut domain,
            Some(&good_store),
            &snapshot,
            &mut model,
            CaptureIntent::RetrySave,
        )
        .expect("retry save");
        assert!(matches!(saved, CaptureOutcome::Saved(_)), "{saved:?}");
        assert!(!model.is_save_recovery());
        assert_eq!(good_store.load().expect("reload").tasks().len(), 1);

        // Cancel from the same width restores the pre-create baseline instead.
        let mut cancel_domain = DomainState::new();
        let mut cancel_model = CaptureModel::from_snapshot(&snapshot);
        enter_capture_title(
            &mut cancel_domain,
            &snapshot,
            &mut cancel_model,
            "Cancel narrow recovery",
        );
        apply_capture_intent(
            &mut cancel_domain,
            Some(&store),
            &snapshot,
            &mut cancel_model,
            CaptureIntent::Save,
        )
        .expect("save failure stays inline");
        assert!(cancel_model.is_save_recovery());
        let cancel_layout = capture_layout_for_model(area(mode), &cancel_model);
        let cancel_click = map_capture_mouse(
            &cancel_layout,
            left_click(
                cancel_layout.cancel_chip.rect.x + 1,
                cancel_layout.cancel_chip.rect.y,
            ),
        )
        .expect("cancel click");
        assert_eq!(
            apply_capture_intent(
                &mut cancel_domain,
                Some(&store),
                &snapshot,
                &mut cancel_model,
                cancel_click,
            )
            .expect("cancel recovery"),
            CaptureOutcome::Cancelled
        );
        assert!(!cancel_model.is_save_recovery());
        assert!(cancel_domain.tasks().is_empty());

        let _ = fs::remove_dir_all(root);
    }
}

// ----: the F6 save-recovery contract with the F8 surfaces present ----

/// Two tasks, so a delete still leaves the board something to select.
fn board_with_two_tasks() -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let doomed = domain
        .create(
            "Delete me",
            None,
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create the doomed task");
    domain
        .create(
            "Keep me",
            None,
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create the surviving task");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/app")));
    // Queue order is updated_at desc; pin the doomed task explicitly.
    let idx = model
        .visible_ids()
        .iter()
        .position(|&id| id == doomed)
        .expect("doomed visible");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(idx),
        None,
        None,
    )
    .expect("select doomed");
    (domain, model, doomed)
}

/// A deep copy of a domain, as the board loop's baseline snapshot is taken.
fn snapshot_of(domain: &DomainState) -> DomainState {
    serde_json::from_str(&serde_json::to_string(domain).expect("serialize baseline"))
        .expect("deserialize baseline")
}

/// The injected error every failing persist in this section reports, so two routes that
/// failed the same way can be compared on the message the user actually reads.
const INJECTED: &str = "injected board save failure";

/// A delete whose save fails produces the same typed result, the same
/// persistence outcome, and the same visible banner as any other failed board save -- and the
/// recovery notice it armed is suspended rather than left offering to undo a deletion that
/// never reached disk. Retry makes the deletion durable and hands the notice back; Cancel
/// unwinds the deletion and discards it.
///
/// STRENGTH: the reference route is computed by this same binary on a differently-shaped
/// board (a Complete before any notice exists), not by a pre-F8 build. That proves the F8
/// surface does not perturb this route, which is what asks; it is blind to an F8 change
/// that moved *both* routes equally. Every journey in this section carries that limit.
#[test]
fn a_failed_delete_save_reports_exactly_what_a_failed_complete_save_reports() {
    // The reference route: a Complete that could not be saved, before any notice exists.
    let (mut reference_domain, mut reference_model, _) = board_with_two_tasks();
    let mut reference_recovery = SaveRecovery::new();
    let reference_baseline = snapshot_of(&reference_domain);
    let reference_outcome = apply_board_intent_with_save_recovery(
        &mut reference_domain,
        &mut reference_model,
        &mut reference_recovery,
        BoardSaveContext {
            baseline: reference_baseline,
            intent: BoardIntent::Complete,
            snapshot: None,
            host: None,
        },
        |_| Err(INJECTED.into()),
    )
    .expect("reference reducer");
    assert_eq!(reference_outcome, IntentOutcome::None);
    let reference_message = reference_model.message().expect("a banner").to_string();
    assert!(
        reference_message.contains("save failed"),
        "{reference_message}"
    );

    // The route under test: a delete, which arms a recovery notice before the save fails.
    for resolution in ["retry", "cancel"] {
        let (mut domain, mut model, doomed) = board_with_two_tasks();
        let baseline = snapshot_of(&domain);
        assert_eq!(model.selected_id(), Some(doomed));
        let mut recovery = SaveRecovery::new();
        let mut saves = 0;

        let outcome = apply_board_intent_with_save_recovery(
            &mut domain,
            &mut model,
            &mut recovery,
            BoardSaveContext {
                baseline,
                intent: BoardIntent::SoftDelete,
                snapshot: None,
                host: None,
            },
            |_| {
                saves += 1;
                Err(INJECTED.into())
            },
        )
        .expect("delete reducer");

        assert_eq!(outcome, reference_outcome, "{resolution}");
        assert_eq!(
            model.message(),
            Some(reference_message.as_str()),
            "{resolution}"
        );
        assert_eq!(model.popup(), BoardPopup::SaveRecovery);
        assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);
        assert_eq!(saves, 1);
        assert!(recovery.is_pending());
        assert_eq!(
            model.delete_notice(),
            None,
            "{resolution}: an undeletable deletion offers nothing to undo"
        );
        if resolution == "retry" {
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
                |state| {
                    saves += 1;
                    assert!(state.get(doomed).expect("task").soft_deleted);
                    Ok(())
                },
            )
            .expect("retry");
            assert_eq!(retried, IntentOutcome::Persisted);
            assert_eq!(saves, 2);
            assert!(!recovery.is_pending());
            assert!(domain.get(doomed).expect("task").soft_deleted);
            assert_eq!(model.message(), Some("saved"));
            assert_eq!(
                model.delete_notice(),
                Some("Delete me"),
                "a durable deletion gets its way back"
            );
        } else {
            let cancelled = apply_board_intent_with_save_recovery(
                &mut domain,
                &mut model,
                &mut recovery,
                BoardSaveContext {
                    baseline: DomainState::new(),
                    intent: BoardIntent::CancelSave,
                    snapshot: None,
                    host: None,
                },
                |_| panic!("cancel must not persist"),
            )
            .expect("cancel");
            assert_eq!(cancelled, IntentOutcome::None);
            assert_eq!(saves, 1);
            assert!(!recovery.is_pending());
            assert!(
                !domain.get(doomed).expect("task").soft_deleted,
                "cancel restores the pre-failure baseline"
            );
            assert_eq!(model.message(), Some("save cancelled"));
            assert_eq!(
                model.delete_notice(),
                None,
                "the deletion never happened, so nothing offers to undo it"
            );
        }
    }
}

#[test]
fn task_form_save_failure_retries_the_exact_atomic_title_notes_and_scope_mutation() {
    let (mut domain, mut model, id) = board_with_two_tasks();
    let baseline = snapshot_of(&domain);
    let mut recovery = SaveRecovery::new();

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open task form");
    for character in " retry".chars() {
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
        BoardIntent::FormFocusNext,
        None,
        None,
    )
    .expect("focus Notes");
    for character in "retry notes".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(character),
            None,
            None,
        )
        .expect("type Notes");
    }
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormFocusNext,
        None,
        None,
    )
    .expect("focus Scope");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FormCycleScope,
        None,
        None,
    )
    .expect("cycle scope to Global");
    assert_eq!(model.form_scope(), Some(&TaskScope::Global));

    let failed = apply_board_intent_with_save_recovery(
        &mut domain,
        &mut model,
        &mut recovery,
        BoardSaveContext {
            baseline,
            intent: BoardIntent::ConfirmEdit,
            snapshot: None,
            host: None,
        },
        |_| Err(INJECTED.into()),
    )
    .expect("failure enters recovery");
    assert_eq!(failed, IntentOutcome::None);
    assert!(recovery.is_pending());
    assert_eq!(model.input_mode(), BoardInputMode::SaveRecovery);

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
        |working| {
            let task = working.get(id).expect("task in retained working state");
            assert_eq!(task.title, "Delete me retry");
            assert_eq!(task.notes.as_deref(), Some("retry notes"));
            assert_eq!(task.scope, TaskScope::Global);
            Ok(())
        },
    )
    .expect("retry succeeds");
    assert_eq!(retried, IntentOutcome::Persisted);
    let task = domain.get(id).expect("retried task");
    assert_eq!(task.title, "Delete me retry");
    assert_eq!(task.notes.as_deref(), Some("retry notes"));
    assert_eq!(task.scope, TaskScope::Global);
}

#[test]
fn save_recovery_unbinds_retired_lens_keys() {
    for key in ['1', '2', '3', '[', ']'] {
        assert_eq!(
            map_key(
                BoardInputMode::SaveRecovery,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            ),
            None,
            "save recovery must not mutate retired lens state behind its modal: {key}"
        );
    }
}
