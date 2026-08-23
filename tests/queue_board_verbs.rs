//! Verb Surface reducers — primary verbs, done/reopen/block, drawer, Esc layers.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskEventKind, TaskScope};
use herdr_tasks::ui::board::{
    apply_intent, board_intent_may_persist, draw_board, resolve_board_command, BoardInputMode,
    BoardModel, CommandSurface, IntentOutcome, ProjectScopeOption,
};
use herdr_tasks::ui::input::{map_key, normal_help_bindings, BoardIntent};
use herdr_tasks::ui::mouse::BoardPopup;
use herdr_tasks::ui::queue::SectionKind;
use herdr_tasks::ui::tier;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const THIS_REPO: &str = "/repos/app";

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

fn board_with_task(title: &str, status: HumanStatus) -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            title,
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    if status != HumanStatus::Ready {
        domain.set_status(id, status).expect("status");
    }
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    (domain, model, id)
}

#[test]
fn space_on_todo_sets_doing_via_domain() {
    let (mut domain, mut model, id) = board_with_task("start me", HumanStatus::Ready);
    assert_eq!(model.selected_id(), Some(id));

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PrimaryVerb,
        None,
        None,
    )
    .expect("primary");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Started);
    assert_eq!(
        model
            .visible_tasks()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.status),
        Some(HumanStatus::Started)
    );
    assert!(
        board_intent_may_persist(&BoardIntent::PrimaryVerb),
        "PrimaryVerb must load a save baseline when it can mutate"
    );
}

#[test]
fn space_on_done_reopens() {
    let (mut domain, mut model, id) = board_with_task("reopen me", HumanStatus::Done);
    // Done tasks live in the drawer; open it so selection can hold the done id.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("drawer");
    // Re-select the done task if reanchor moved off it.
    if model.selected_id() != Some(id) {
        let idx = model
            .visible_ids()
            .iter()
            .position(|&row| row == id)
            .expect("done row visible with drawer open");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::SelectIndex(idx),
            None,
            None,
        )
        .expect("select done");
    }

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PrimaryVerb,
        None,
        None,
    )
    .expect("primary");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);
}

#[test]
fn space_on_doing_is_noop_with_visible_notice() {
    let (mut domain, mut model, id) = board_with_task("already going", HumanStatus::Started);
    let before = domain.get(id).expect("task").clone();

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PrimaryVerb,
        None,
        None,
    )
    .expect("primary");
    assert_eq!(outcome, IntentOutcome::None);
    let after = domain.get(id).expect("task");
    assert_eq!(after.status, before.status);
    assert_eq!(after.revision, before.revision);
    let notice = model.message().expect("visible notice");
    assert!(
        !notice.is_empty(),
        "doing primary must leave a status-line notice"
    );
}

#[test]
fn d_completes_non_done_and_o_reopens_done() {
    // d completes non-done
    let (mut domain, mut model, id) = board_with_task("finish me", HumanStatus::Ready);
    let outcome =
        apply_intent(&mut domain, &mut model, BoardIntent::Complete, None, None).expect("complete");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Done);

    // o reopens done (drawer so the done row stays addressable)
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("drawer");
    if model.selected_id() != Some(id) {
        let idx = model
            .visible_ids()
            .iter()
            .position(|&row| row == id)
            .expect("done visible");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::SelectIndex(idx),
            None,
            None,
        )
        .expect("select");
    }
    let outcome =
        apply_intent(&mut domain, &mut model, BoardIntent::Reopen, None, None).expect("reopen");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);

    assert!(board_intent_may_persist(&BoardIntent::Complete));
    assert!(board_intent_may_persist(&BoardIntent::Reopen));
}

#[test]
fn b_on_blocked_sets_todo_and_keeps_task_on_deck_not_in_motion() {
    let (mut domain, mut model, id) = board_with_task("unblock me", HumanStatus::Blocked);

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleBlock,
        None,
        None,
    )
    .expect("unblock");

    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);
    let view = model.queue_view();
    assert!(view
        .sections
        .iter()
        .any(|section| section.kind == SectionKind::OnDeck && section.task_ids.contains(&id)));
    assert!(!view
        .sections
        .iter()
        .any(|section| section.kind == SectionKind::InMotion && section.task_ids.contains(&id)));
    assert!(board_intent_may_persist(&BoardIntent::ToggleBlock));
}

#[test]
fn b_blocks_todo_doing_and_review() {
    for status in [
        HumanStatus::Ready,
        HumanStatus::Started,
        HumanStatus::Review,
    ] {
        let (mut domain, mut model, id) = board_with_task("block me", status);

        let outcome = apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ToggleBlock,
            None,
            None,
        )
        .expect("block");

        assert_eq!(outcome, IntentOutcome::Persist, "{status:?}");
        assert_eq!(
            domain.get(id).expect("task").status,
            HumanStatus::Blocked,
            "{status:?}"
        );
    }
}

#[test]
fn enter_opens_the_task_page_and_enter_again_closes_it_without_mutating() {
    let (mut domain, mut model, id) = board_with_task("inspect me", HumanStatus::Ready);
    let rev_before = domain.get(id).expect("task").revision;
    assert_eq!(model.input_mode(), BoardInputMode::Normal);

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open task page");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert_eq!(model.detail_open(), None, "the page replaces any peek");
    assert_eq!(domain.get(id).expect("task").revision, rev_before);
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("close task page");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(domain.get(id).expect("task").revision, rev_before);
    assert!(!board_intent_may_persist(&BoardIntent::OpenTaskPage));
}

#[test]
fn right_arrow_peeks_detail_and_left_arrow_collapses_it() {
    let (mut domain, mut model, id) = board_with_task("peek me", HumanStatus::Ready);
    let rev_before = domain.get(id).expect("task").revision;
    assert_eq!(model.detail_open(), None);

    // Right opens the peek on the selected row without touching the domain.
    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None, None)
        .expect("peek detail");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.detail_open(), Some(id));
    assert_eq!(domain.get(id).expect("task").revision, rev_before);

    // Right again is idempotent: still open, still no mutation.
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None, None).expect("peek again");
    assert_eq!(model.detail_open(), Some(id));

    // Left collapses the open peek.
    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CollapseDetail,
        None,
        None,
    )
    .expect("collapse detail");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.detail_open(), None);

    // Left with nothing open is a clean no-op.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CollapseDetail,
        None,
        None,
    )
    .expect("collapse no-op");
    assert_eq!(model.detail_open(), None);
    assert_eq!(domain.get(id).expect("task").revision, rev_before);

    assert!(!board_intent_may_persist(&BoardIntent::PeekDetail));
    assert!(!board_intent_may_persist(&BoardIntent::CollapseDetail));
}

#[test]
fn detail_closes_when_its_task_leaves_the_visible_set() {
    let (mut domain, mut model, id) = board_with_task("inspect then remove", HumanStatus::Ready);
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None, None).expect("open peek");
    assert_eq!(model.detail_open(), Some(id));

    domain.soft_delete(id).expect("soft delete");
    model.sync_from_domain(&domain);
    assert_eq!(model.detail_open(), None);
}

#[test]
fn z_toggles_done_drawer_membership_on_list() {
    let (mut domain, mut model, done_id) = board_with_task("archived", HumanStatus::Done);
    let open_id = domain
        .create(
            "still open",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("open task");
    model.sync_from_domain(&domain);

    assert!(!model.drawer_open());
    assert!(
        !model.visible_ids().contains(&done_id),
        "done row hidden while drawer closed"
    );
    assert!(model.visible_ids().contains(&open_id));

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("open drawer");
    assert!(model.drawer_open());
    assert!(
        model.visible_ids().contains(&done_id),
        "done row joins the list when drawer opens"
    );
    let has_done_section = model
        .queue_view()
        .sections
        .iter()
        .any(|s| s.kind == SectionKind::Done && s.task_ids.contains(&done_id));
    assert!(has_done_section);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("close drawer");
    assert!(!model.drawer_open());
    assert!(!model.visible_ids().contains(&done_id));
}

#[test]
fn block_on_done_and_verbs_without_selection_leave_visible_notices() {
    let (mut domain, mut model, id) = board_with_task("finished", HumanStatus::Done);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("open drawer");
    let index = model
        .visible_ids()
        .iter()
        .position(|&row| row == id)
        .expect("done task visible in drawer");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(index),
        None,
        None,
    )
    .expect("select done task");

    let outcome = apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleBlock,
        None,
        None,
    )
    .expect("block done");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(domain.get(id).expect("done task").status, HumanStatus::Done);
    assert!(model.message().is_some_and(|message| !message.is_empty()));

    let mut empty_domain = DomainState::new();
    let mut empty_model = BoardModel::from_domain(&empty_domain, Some(PathBuf::from(THIS_REPO)));
    for intent in [BoardIntent::PrimaryVerb, BoardIntent::ToggleBlock] {
        let outcome = apply_intent(&mut empty_domain, &mut empty_model, intent, None, None)
            .expect("verb without selection");
        assert_eq!(outcome, IntentOutcome::None);
        assert!(empty_model
            .message()
            .is_some_and(|message| !message.is_empty()));
    }
}

#[test]
fn esc_closes_transient_then_detail_then_quit_and_q_quits_only_in_normal() {
    let (mut domain, mut model, id) = board_with_task("layered", HumanStatus::Ready);

    // Transient: help card
    apply_intent(&mut domain, &mut model, BoardIntent::OpenHelp, None, None).expect("help");
    assert_eq!(model.input_mode(), BoardInputMode::Help);
    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None, None)
        .expect("esc help");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);

    // Transient: palette (command surface)
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("palette");
    assert_ne!(model.command_surface(), CommandSurface::None);
    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None, None)
        .expect("esc palette");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.command_surface(), CommandSurface::None);

    // Peek / accordion
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None, None).expect("peek");
    assert_eq!(model.detail_open(), Some(id));
    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None, None)
        .expect("esc detail");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.detail_open(), None);

    // Nothing open → quit
    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None, None)
        .expect("esc quit");
    assert_eq!(outcome, IntentOutcome::Quit);

    // alt+q quits from normal only; bare q is dead
    assert_eq!(
        map_key(BoardInputMode::Normal, press(KeyCode::Char('q'))),
        None
    );
    assert_eq!(
        map_key(
            BoardInputMode::Normal,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT),
        ),
        Some(BoardIntent::Quit)
    );
    assert_ne!(
        map_key(BoardInputMode::Help, press(KeyCode::Char('q'))),
        Some(BoardIntent::Quit),
        "q must not quit while help is open"
    );
    assert_ne!(
        map_key(BoardInputMode::Palette, press(KeyCode::Char('q'))),
        Some(BoardIntent::Quit),
        "q must not quit while palette is open"
    );
    assert_ne!(
        map_key(BoardInputMode::EditTitle, press(KeyCode::Char('q'))),
        Some(BoardIntent::Quit),
        "q must not quit while editing"
    );
}

fn rendered_board(model: &BoardModel, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| draw_board(frame, model))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The queue status line exactly as painted (the chrome), matching f6's helper.
fn board_chrome_row(model: &BoardModel, mode: (u16, u16)) -> String {
    let geo = tier::resolve(mode.0, mode.1);
    let status_row = geo
        .status_row
        .expect("status row present at supported sizes");
    let mut terminal = Terminal::new(TestBackend::new(mode.0, mode.1)).expect("test terminal");
    terminal
        .draw(|frame| draw_board(frame, model))
        .expect("draw board");
    let buffer = terminal.backend().buffer().clone();
    (0..mode.0)
        .map(|x| buffer[(x, status_row)].symbol())
        .collect()
}

/// the palette catalog, subsequence filter, same intents as key routes.
#[test]
fn palette_lists_exactly_m1_commands_for_selection_filters_by_subsequence_and_dispatches_same_intents_as_keys(
) {
    let (mut domain, mut model, id) = board_with_task("palette me", HumanStatus::Ready);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");
    assert_eq!(model.command_surface(), CommandSurface::Palette);

    let labels: Vec<&str> = model.visible_commands().iter().map(|c| c.label).collect();
    let expected = [
        "set status: ready",
        "set status: started",
        "set status: blocked",
        "set status: review",
        "edit notes",
        "change scope",
        "new task",
        "delete",
        "undo",
        "done drawer",
        "verb keys: use ctrl",
        "help",
        "quit",
    ];
    assert_eq!(
        labels, expected,
        "M1 palette catalog with a selection must match AC-17 exactly"
    );
    // / the `o` key: reopen is only valid for a done selection, so a ready selection's
    // catalog must not offer it at all (not merely filtered out above by the exact-match).
    assert!(
        !labels.contains(&"reopen"),
        "a ready selection must not offer reopen"
    );

    // Subsequence (not substring): "started" matches only that status label.
    for ch in "started".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::CommandQueryInsert(ch),
            None,
            None,
        )
        .expect("type");
    }
    let filtered: Vec<&str> = model.visible_commands().iter().map(|c| c.label).collect();
    assert_eq!(filtered, vec!["set status: started"]);
    // Substring-only would need the contiguous run "ssg"; none of the labels contain it.
    assert!(
        model
            .visible_commands()
            .iter()
            .all(|c| !c.label.to_ascii_lowercase().contains("ssg")),
        "filter must be subsequence, not substring"
    );

    // Confirm dispatches the same intent as SetStatus(Doing) would.
    let resolved = resolve_board_command(&mut model, BoardIntent::ConfirmCommand).expect("resolve");
    assert_eq!(resolved, BoardIntent::SetStatus(HumanStatus::Started));
    apply_intent(&mut domain, &mut model, resolved, None, None).expect("apply");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Started);

    // a doing selection still has nothing to reopen (SetStatus closed the palette
    // above; reopen it to read the catalog for the new status).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("palette while doing");
    assert!(
        !model.visible_commands().iter().any(|c| c.label == "reopen"),
        "a doing selection must not offer reopen"
    );

    // Key-equivalent: `o` reopen vs palette "reopen".
    domain.set_status(id, HumanStatus::Done).expect("done");
    model.sync_from_domain(&domain);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("drawer");
    if model.selected_id() != Some(id) {
        let idx = model
            .visible_ids()
            .iter()
            .position(|&row| row == id)
            .expect("done visible");
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::SelectIndex(idx),
            None,
            None,
        )
        .expect("select");
    }

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("palette again");
    // the done selection's catalog offers reopen before any query narrows it.
    assert!(
        model.visible_commands().iter().any(|c| c.label == "reopen"),
        "a done selection must offer reopen"
    );
    for ch in "reopen".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::CommandQueryInsert(ch),
            None,
            None,
        )
        .expect("type reopen");
    }
    let via_palette =
        resolve_board_command(&mut model, BoardIntent::ConfirmCommand).expect("reopen cmd");
    assert_eq!(via_palette, BoardIntent::Reopen);
    apply_intent(&mut domain, &mut model, via_palette, None, None).expect("reopen");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);

    // Direct key route lands on the same intent.
    assert_eq!(
        map_key(BoardInputMode::Normal, alt(KeyCode::Char('o'))),
        Some(BoardIntent::Reopen)
    );

    // Painted palette shows the catalog header / a command row.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("paint palette");
    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.lines().any(|line| line.contains(" command")),
        "palette overlay must paint its header: {frame:?}"
    );
}

/// park / resume / link / dispatch never appear in the the palette.
///
/// The dispatch-recovery entries (`RecoveryResume`/`BeginCleanup`) only ever reach the
/// catalog when the selected task carries an active attempt, so a fixture with no attempt
/// would pass this test whether or not the catalog is fixed: the second block below builds
/// that state so the assertion is load-bearing.
#[test]
fn palette_excludes_park_resume_link_dispatch() {
    let (mut domain, mut model, _id) = board_with_task("no classic tail", HumanStatus::Ready);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("palette");

    for cmd in model.visible_commands() {
        let lower = cmd.label.to_ascii_lowercase();
        for word in ["park", "resume", "link", "dispatch", "cleanup"] {
            assert!(
                !lower.contains(word),
                "palette label {:?} must not mention {word}",
                cmd.label
            );
        }
    }

    // The case that actually exercises the exclusion: a selected task with an active
    // dispatch attempt, which is exactly the state that used to add "Resume dispatch" /
    // "Cleanup dispatch" to the catalog.
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "has an attempt",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    domain
        .start_dispatch_attempt(id, herdr_tasks::domain::DispatchAttemptMode::Here, "grok")
        .expect("start attempt");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    assert!(
        model.selected_id() == Some(id),
        "fixture must select the task carrying the active attempt"
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("palette with active attempt");
    for cmd in model.visible_commands() {
        let lower = cmd.label.to_ascii_lowercase();
        for word in ["park", "resume", "link", "dispatch", "cleanup"] {
            assert!(
                !lower.contains(word),
                "palette label {:?} must not mention {word} even with an active attempt selected",
                cmd.label
            );
        }
    }
}

/// The palette window follows a wrapped selection, so Enter never targets an invisible row.
#[test]
fn palette_window_keeps_wrapped_last_command_visible() {
    let (mut domain, mut model, _id) = board_with_task("palette window", HumanStatus::Ready);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::CommandPrev,
        None,
        None,
    )
    .expect("wrap to last command");
    assert_eq!(
        model.selected_command().expect("last command").label,
        "quit"
    );

    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.lines().any(|line| line.contains("▸ quit")),
        "the selected tail command must be painted: {frame:?}"
    );
}

/// help card lists every active-tier binding and closes on any key.
#[test]
fn help_card_lists_every_active_tier_binding_and_closes_on_any_key() {
    let (mut domain, mut model, _id) = board_with_task("help me", HumanStatus::Ready);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenHelp, None, None).expect("help");
    assert_eq!(model.input_mode(), BoardInputMode::Help);

    let frame = rendered_board(&model, 80, 24);
    let bindings = normal_help_bindings();
    assert!(
        !bindings.is_empty(),
        "help bindings table must not be empty"
    );
    for (chord, label) in bindings {
        // Chord may be multi-token ("j/k · ↑/↓"); require each significant token.
        for token in chord.split(|c: char| c.is_whitespace() || c == '·') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            assert!(
                frame.contains(token),
                "help card missing chord token {token:?} from {chord:?}\n{frame}"
            );
        }
        assert!(
            frame
                .to_ascii_lowercase()
                .contains(&label.to_ascii_lowercase())
                || frame.contains(label),
            "help card missing label {label:?}\n{frame}"
        );
    }

    // Compact is a takeover, not a truncated card: every binding fits at the minimum.
    let compact = rendered_board(&model, 40, 10);
    for (chord, label) in normal_help_bindings() {
        assert!(
            compact.contains(chord),
            "compact help missing {chord:?}\n{compact}"
        );
        assert!(
            compact.contains(label),
            "compact help missing {label:?}\n{compact}"
        );
    }

    // Any key closes (including a letter that would quit in normal mode).
    let close = map_key(BoardInputMode::Help, press(KeyCode::Char('q'))).expect("any key");
    assert_eq!(close, BoardIntent::CloseLayer);
    apply_intent(&mut domain, &mut model, close, None, None).expect("close");
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(
        map_key(BoardInputMode::Help, press(KeyCode::Char('x'))),
        Some(BoardIntent::CloseLayer)
    );
}

/// project-scope chip/dropdown filters every visible section and count.
#[test]
fn project_scope_chip_and_dropdown_filter_all_visible_sections_matching_ac5() {
    let mut domain = DomainState::new();
    let motion_app = domain
        .create(
            "motion app",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("motion app");
    domain
        .set_status(motion_app, HumanStatus::Started)
        .expect("doing");
    let motion_other = domain
        .create(
            "motion other",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("motion other");
    domain
        .set_status(motion_other, HumanStatus::Started)
        .expect("doing other");
    let motion_global = domain
        .create(
            "motion global",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("motion global");
    domain
        .set_status(motion_global, HumanStatus::Started)
        .expect("doing global");
    let deck_app = domain
        .create(
            "deck app",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("deck app");
    let deck_other = domain
        .create(
            "deck other",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("deck other");
    let deck_global = domain
        .create(
            "deck global",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("deck global");
    let done_app = domain
        .create(
            "done app",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("done app");
    domain
        .set_status(done_app, HumanStatus::Done)
        .expect("done");
    let done_other = domain
        .create(
            "done other",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("done other");
    domain
        .set_status(done_other, HumanStatus::Done)
        .expect("done");
    let done_global = domain
        .create(
            "done global",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("done global");
    domain
        .set_status(done_global, HumanStatus::Done)
        .expect("done");
    // Empty-scoped project path with no open deck tasks (only a done task).
    let done_empty = domain
        .create(
            "done only",
            None,
            project("/repos/empty"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("empty project done");
    domain
        .set_status(done_empty, HumanStatus::Done)
        .expect("done");

    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("open done drawer");

    // Chip path: open selector without needing the classic Projects lens.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("open scope dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::ProjectPicker);
    assert_eq!(model.popup(), BoardPopup::ProjectPicker);

    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.lines().any(|line| line.contains("all projects")),
        "scope dropdown must paint the all-projects option: {frame:?}"
    );
    assert!(
        frame.lines().any(|line| line.contains("global")),
        "scope dropdown must paint the global option: {frame:?}"
    );

    // Move to /repos/other and confirm (session-only deck scope).
    let options = model.project_options();
    let other_idx = options
        .iter()
        .position(|opt| opt == &ProjectScopeOption::Project(PathBuf::from("/repos/other")))
        .expect("/repos/other in options");
    for _ in 0..options.len() {
        if model.project_picker_index() == Some(other_idx) {
            break;
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ProjectPickerNext,
            None,
            None,
        )
        .expect("next");
    }
    assert_eq!(model.project_picker_index(), Some(other_idx));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmProjectChoice,
        None,
        None,
    )
    .expect("confirm scope");

    assert_eq!(model.selected_project(), Some(Path::new("/repos/other")));
    let other_view = model.queue_view();
    assert_eq!(other_view.counts.in_motion, 1);
    assert_eq!(other_view.counts.done, 1);
    assert_eq!(
        model.visible_ids(),
        vec![motion_other, deck_other, done_other],
        "a project scope leaves no task from another scope visible"
    );
    assert!(!model.visible_ids().contains(&done_app));
    assert!(!model.visible_ids().contains(&done_global));
    model.clear_message();
    let other_status = board_chrome_row(&model, (80, 24));
    assert!(
        other_status.contains("1 done") && !other_status.contains("in motion"),
        "scoped status counts must be painted: {other_status:?}"
    );
    assert!(!model.visible_ids().contains(&motion_app));
    assert!(!model.visible_ids().contains(&deck_app));

    // Global is a real selector option, not an alias for all projects.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("reopen for global");
    let global_idx = model
        .project_options()
        .iter()
        .position(|option| option == &ProjectScopeOption::Global)
        .expect("global option");
    for _ in 0..model.project_options().len() {
        if model.project_picker_index() == Some(global_idx) {
            break;
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ProjectPickerNext,
            None,
            None,
        )
        .expect("next global");
    }
    assert_eq!(model.project_picker_index(), Some(global_idx));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmProjectChoice,
        None,
        None,
    )
    .expect("scope global");
    let global_view = model.queue_view();
    assert_eq!(global_view.counts.in_motion, 1);
    assert_eq!(global_view.counts.done, 1);
    assert_eq!(
        model.visible_ids(),
        vec![motion_global, deck_global, done_global],
        "Global scope leaves no project-scoped task visible"
    );
    model.clear_message();
    let global_status = board_chrome_row(&model, (80, 24));
    assert!(
        global_status.contains("1 done") && !global_status.contains("in motion"),
        "global status counts must be painted: {global_status:?}"
    );

    // Scoped project with zero open deck tasks → header + empty hint, no invented row.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("reopen");
    let empty_idx = model
        .project_options()
        .iter()
        .position(|opt| opt == &ProjectScopeOption::Project(PathBuf::from("/repos/empty")))
        .expect("empty project option (from selected history or tasks)");
    // Ensure empty is reachable: if not in options because only done tasks, select via path.
    // project_options includes paths from non-soft-deleted tasks, including done.
    for _ in 0..model.project_options().len() {
        if model.project_picker_index() == Some(empty_idx) {
            break;
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ProjectPickerNext,
            None,
            None,
        )
        .expect("next empty");
    }
    assert_eq!(model.project_picker_index(), Some(empty_idx));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmProjectChoice,
        None,
        None,
    )
    .expect("scope empty");

    let empty_sections: Vec<_> = model
        .queue_view()
        .sections
        .iter()
        .filter(|s| s.kind == SectionKind::OnDeck)
        .cloned()
        .collect();
    assert_eq!(empty_sections.len(), 1, "one scoped deck group");
    assert!(empty_sections[0].empty_hint, "empty hint required");
    assert!(
        empty_sections[0].task_ids.is_empty(),
        "no invented deck rows"
    );
    assert_eq!(model.queue_view().counts.in_motion, 0);
    assert_eq!(model.queue_view().counts.done, 1);
    assert_eq!(model.visible_ids(), vec![done_empty]);

    let scoped_frame = rendered_board(&model, 80, 24);
    assert!(
        scoped_frame.to_ascii_lowercase().contains("no open")
            || scoped_frame.contains("empty")
            || scoped_frame.contains("—"),
        "empty scoped deck should paint a hint: {scoped_frame:?}"
    );
}

#[test]
fn x_soft_deletes_and_status_line_names_task_with_undo_hint() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "delete me",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    domain
        .create(
            "keep me",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("keep");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    // pin the one we will delete
    if model.selected_id() != Some(id) {
        if let Some(idx) = model.visible_ids().iter().position(|&rid| rid == id) {
            let _ = apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::SelectIndex(idx),
                None,
                None,
            );
        }
    }
    assert_eq!(model.selected_id(), Some(id));

    let outcome = apply_intent(&mut domain, &mut model, BoardIntent::SoftDelete, None, None)
        .expect("soft delete");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert!(domain.get(id).expect("task").soft_deleted);
    assert!(!model.visible_ids().contains(&id));
    // Status line carries the framed delete notice when armed.
    // Read the status row explicitly.
    let row = board_chrome_row(&model, (80, 24));
    assert!(
        row.contains("Deleted \"delete me\""),
        "status line must name deleted task with framed wording: {row:?}"
    );
    assert!(
        row.contains("· u Undo"),
        "status line must include undo hint: {row:?}"
    );
    // selection reanchored to remaining task
    assert!(model.selected_id().is_some());
    assert_ne!(model.selected_id(), Some(id));
}

#[test]
fn u_undoes_with_domain_coverage_and_stale_undo_refused_visibly() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "undo me",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(&mut domain, &mut model, BoardIntent::SoftDelete, None, None).expect("delete");
    assert!(domain.get(id).expect("task").soft_deleted);
    assert!(!model.visible_ids().contains(&id));

    // undo restores
    let outcome =
        apply_intent(&mut domain, &mut model, BoardIntent::Undo, None, None).expect("undo");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert!(!domain.get(id).expect("task").soft_deleted);
    assert!(model.visible_ids().contains(&id));
    // selection reanchored to the restored task
    assert_eq!(model.selected_id(), Some(id));

    // stale undo: delete again (records entry with current rev), then direct-restore (advances rev
    // without popping the entry). Next undo must refuse visibly.
    apply_intent(&mut domain, &mut model, BoardIntent::SoftDelete, None, None).expect("delete2");
    assert!(domain.get(id).expect("task").soft_deleted);
    let _ = domain.restore(id);
    let outcome =
        apply_intent(&mut domain, &mut model, BoardIntent::Undo, None, None).expect("stale undo");
    assert_eq!(outcome, IntentOutcome::None);
    // refusal visible on status line (with notice also armed); domain state unchanged.
    // Read the painted status row, not only model.message().
    let row = board_chrome_row(&model, (80, 24));
    assert!(
        row.contains("changed since") || row.contains("since the undoable"),
        "stale undo refusal must be visible on status row: {row:?}"
    );
    // Also ensure the refusal reached the model message channel.
    let msg = model
        .message()
        .expect("stale refusal visible on status line");
    let m = msg.to_ascii_lowercase();
    assert!(
        m.contains("changed") || m.contains("since") || m.contains("stale"),
        "stale undo refusal must be visible: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Task page: Enter opens a view-first full page; `e`/`n`/Tab enter field edits;
// verbs act on the page's task; Ctrl+Enter saves; Esc layers back.
// ---------------------------------------------------------------------------

fn board_with_noted_task() -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "paged task",
            Some("line one\nline two".into()),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    (domain, model, id)
}

#[test]
fn esc_and_q_close_the_task_page_from_view_mode() {
    for (key, mods) in [
        (KeyCode::Esc, KeyModifiers::NONE),
        (KeyCode::Char('q'), KeyModifiers::ALT),
    ] {
        let (mut domain, mut model, _id) = board_with_noted_task();
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::OpenTaskPage,
            None,
            None,
        )
        .expect("open page");
        assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
        let close = map_key(BoardInputMode::TaskPage, KeyEvent::new(key, mods)).expect("close key");
        assert_eq!(close, BoardIntent::CloseLayer);
        apply_intent(&mut domain, &mut model, close, None, None).expect("close page");
        assert_eq!(model.input_mode(), BoardInputMode::Normal);
    }
}

#[test]
fn task_page_navigation_ignores_unrelated_modifier_chords() {
    for (code, mods) in [
        (KeyCode::Esc, KeyModifiers::CONTROL),
        (KeyCode::Esc, KeyModifiers::ALT),
        (KeyCode::Char('j'), KeyModifiers::ALT),
        (KeyCode::Down, KeyModifiers::CONTROL),
        (KeyCode::Char('k'), KeyModifiers::CONTROL),
        (KeyCode::Tab, KeyModifiers::ALT),
    ] {
        assert_eq!(
            map_key(BoardInputMode::TaskPage, KeyEvent::new(code, mods)),
            None,
            "{code:?} + {mods:?} must not navigate or close the page"
        );
    }
    assert_eq!(
        map_key(
            BoardInputMode::TaskPage,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
        ),
        Some(BoardIntent::ConfirmEdit)
    );
    assert_eq!(
        map_key(
            BoardInputMode::TaskPage,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        ),
        Some(BoardIntent::PageScrollDown)
    );
}

#[test]
fn page_verbs_act_on_the_page_task_and_the_page_stays_open() {
    let (mut domain, mut model, id) = board_with_noted_task();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");

    // `d` completes the page's task; the page remains its surface.
    let complete = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('d'))).expect("d");
    assert_eq!(complete, BoardIntent::Complete);
    apply_intent(&mut domain, &mut model, complete, None, None).expect("complete");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Done);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // `o` reopens it, still from the page.
    let reopen = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('o'))).expect("o");
    apply_intent(&mut domain, &mut model, reopen, None, None).expect("reopen");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // `b` blocks, `b` again unblocks.
    let block = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('b'))).expect("b");
    apply_intent(&mut domain, &mut model, block.clone(), None, None).expect("block");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Blocked);
    apply_intent(&mut domain, &mut model, block, None, None).expect("unblock");
    assert_eq!(domain.get(id).expect("task").status, HumanStatus::Ready);
}

#[test]
fn deleting_from_the_page_closes_it_and_undo_restores() {
    let (mut domain, mut model, id) = board_with_noted_task();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    let delete = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('x'))).expect("x");
    assert_eq!(delete, BoardIntent::SoftDelete);
    apply_intent(&mut domain, &mut model, delete, None, None).expect("delete");
    assert!(domain.get(id).expect("task").soft_deleted);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::Normal,
        "the page closes"
    );
    // The undo route back works from the board.
    apply_intent(&mut domain, &mut model, BoardIntent::Undo, None, None).expect("undo");
    assert!(!domain.get(id).expect("task").soft_deleted);
}

#[test]
fn tab_cycles_view_mode_through_title_notes_and_scope_edits() {
    let (mut domain, mut model, _id) = board_with_noted_task();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    let tab = map_key(BoardInputMode::TaskPage, press(KeyCode::Tab)).expect("tab");
    assert_eq!(tab, BoardIntent::FormFocusNext);

    apply_intent(&mut domain, &mut model, tab.clone(), None, None).expect("focus title");
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
    apply_intent(&mut domain, &mut model, tab.clone(), None, None).expect("focus notes");
    assert_eq!(model.input_mode(), BoardInputMode::EditNotes);
    apply_intent(&mut domain, &mut model, tab, None, None).expect("focus scope");
    assert_eq!(model.input_mode(), BoardInputMode::EditScope);
}

#[test]
fn field_entry_intents_refocus_an_open_page_without_recreating_its_drafts() {
    let (mut domain, mut model, _id) = board_with_noted_task();
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
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("edit title");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('!'),
        None,
        None,
    )
    .expect("type");
    // Moving into Notes must keep the title draft alive (no form recreation).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditNotes,
        None,
        None,
    )
    .expect("edit notes");
    assert_eq!(model.input_mode(), BoardInputMode::EditNotes);
    assert_eq!(model.edit_buffer(), "line one\nline two");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("back to title");
    assert_eq!(model.edit_buffer(), "paged task!");
}

#[test]
fn ctrl_enter_from_the_page_saves_and_returns_to_the_board() {
    let (mut domain, mut model, id) = board_with_noted_task();
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
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("edit title");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('!'),
        None,
        None,
    )
    .expect("type");
    let save = map_key(
        BoardInputMode::EditTitle,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
    )
    .expect("ctrl+enter");
    assert_eq!(save, BoardIntent::ConfirmEdit);
    let outcome = apply_intent(&mut domain, &mut model, save, None, None).expect("save");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(domain.get(id).expect("task").title, "paged task!");
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
}

#[test]
fn page_scroll_intents_are_session_only_and_bounded() {
    let (mut domain, mut model, id) = board_with_noted_task();
    let rev_before = domain.get(id).expect("task").revision;
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    for intent in [BoardIntent::PageScrollUp, BoardIntent::PageScrollDown] {
        let outcome =
            apply_intent(&mut domain, &mut model, intent.clone(), None, None).expect("scroll");
        assert_eq!(outcome, IntentOutcome::None);
        assert!(!board_intent_may_persist(&intent));
    }
    // Far more downs than the notes have lines must stay bounded and mutation-free.
    for _ in 0..50 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageScrollDown,
            None,
            None,
        )
        .expect("scroll down");
    }
    assert_eq!(domain.get(id).expect("task").revision, rev_before);
}

/// The bottom of a WRAPPING note must be reachable by scrolling.
///
/// The sibling test above uses `"line one\nline two"`, two short lines that never wrap at any
/// real page width, so its "stays bounded" assertion passed trivially and could not observe the
/// bound being wrong. The scroll horizon was counted in logical lines while the page rendered
/// wrapped rows, and wrapping only ever adds rows, so the tail of a long note was unreachable
/// while the divider still advertised the hidden rows.
#[test]
fn page_scroll_reaches_the_bottom_of_a_wrapping_note() {
    let mut domain = DomainState::new();
    // 10 logical lines that each wrap ~4x: ~40 wrapped rows against a ~15-row body. The gap
    // between the two counts is the bug's whole surface, so it must exceed the viewport or a
    // buggy bound still happens to reach the end.
    let long = (0..10)
        .map(|i| format!("{i}{}", "w".repeat(300)))
        .collect::<Vec<_>>()
        .join("\n");
    domain
        .create(
            "wrapping note",
            Some(long),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");

    // One painted frame establishes the wrap geometry the scroll bound is derived from.
    let paint = |model: &BoardModel| {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_board(frame, model))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
    };
    paint(&model);

    // Scroll far past any plausible row count; the bound must clamp, not strand content.
    for _ in 0..400 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageScrollDown,
            None,
            None,
        )
        .expect("scroll down");
    }
    let rows = paint(&model);

    // At the bottom the divider must not still be promising rows below.
    let more = rows.iter().find(|row| row.contains("more"));
    assert!(
        more.is_none(),
        "scrolled to the bottom but the divider still advertises hidden rows: {more:?}"
    );
}

#[test]
fn opening_the_page_without_a_selection_is_a_no_op() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("no-op open");
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
}

/// Closing the page after completing its task must not leave selection on an invisible row.
///
/// While the page owns input its verbs act on the bound task, so selection is pinned to that
/// id even after completing it removes it from the open deck. Closing the page ends that
/// contract: if the pin survives the close, the board has no highlighted visible row while
/// `selected_id` still names the hidden done task, so the next verb mutates a task the user
/// cannot see.
#[test]
fn closing_the_page_after_completing_its_task_reanchors_to_a_visible_row() {
    let mut domain = DomainState::new();
    let target = domain
        .create(
            "page target",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create target");
    let other = domain
        .create(
            "still open",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create other");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let idx = model
        .visible_ids()
        .iter()
        .position(|id| *id == target)
        .expect("target visible");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(idx),
        None,
        None,
    )
    .expect("select target");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    apply_intent(&mut domain, &mut model, BoardIntent::Complete, None, None)
        .expect("complete from the page");
    apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None, None).expect("close page");

    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    let visible = model.visible_ids();
    assert!(
        !visible.contains(&target),
        "the completed task should have left the open deck"
    );
    let selected = model.selected_id().expect("a visible row stays selected");
    assert!(
        visible.contains(&selected),
        "selection stayed on the hidden completed task after the page closed"
    );
    assert_eq!(
        selected, other,
        "selection should reanchor to the open task"
    );
}

// ---------------------------------------------------------------------------
// T-3: page step cursor and modifier-protected steps verbs. Bare arrows own
// the cursor lifecycle (first Down activates, Up from the first step
// deactivates); Alt+space toggles the highlighted step, Alt+x marks then
// removes, Alt+e renames the highlighted step or the title by cursor state,
// Alt+a opens the one-line add editor.
// ---------------------------------------------------------------------------

/// A task page opened on a task carrying `steps`, painted at the standard
/// 80x24 board size. Returns the opened page.
fn board_with_steps(
    title: &str,
    notes: Option<&str>,
    steps: &[&str],
) -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            title,
            notes.map(str::to_string),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    for step in steps {
        domain.add_step(id, step).expect("add step");
    }
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open task page");
    (domain, model, id)
}

/// Ten note lines that each wrap ~5x at the 80-column page width (~50 wrapped
/// rows against a ~14-row notes window), each carrying a unique `L{n}` marker on
/// its first wrapped row so scroll position is paint-observable.
fn wrapping_notes() -> String {
    (0..10)
        .map(|i| format!("L{i} {}", "w".repeat(300)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Keyboard arrows walk the active cursor through the shared stream, keeping the
/// selected step visible while the cursor advances.
#[test]
fn arrow_keys_move_the_cursor_through_shared_steps() {
    let steps: Vec<String> = (1..=30).map(|i| format!("step {i:02}")).collect();
    let step_refs: Vec<&str> = steps.iter().map(String::as_str).collect();
    let (mut domain, mut model, _) =
        board_with_steps("Window walker", Some("the notes body"), &step_refs);

    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.contains('█'),
        "overflow must show the page scrollbar:\n{frame}"
    );
    assert!(
        !frame.contains("+23 more"),
        "there is no separate steps window:\n{frame}"
    );
    assert!(frame.contains("step 01"), "first step visible:\n{frame}");

    for _ in 0..30 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageScrollDown,
            None,
            None,
        )
        .expect("scroll down");
    }
    let bottom = rendered_board(&model, 80, 24);
    assert!(
        bottom.contains("▸ ▪ step 30"),
        "the cursor reaches the final step:\n{bottom}"
    );
    assert!(
        !bottom.contains("step 01"),
        "the first step leaves the viewport:\n{bottom}"
    );
}

/// Alt+space with the cursor active toggles exactly the highlighted step
/// through the real apply/save path: domain command, revision bump, journaled
/// event, Persist outcome -- and the task's human status never changes.
#[test]
fn modifier_toggle_flips_step_under_cursor() {
    let (mut domain, mut model, id) = board_with_steps(
        "Toggle witness",
        None,
        &["alpha step", "bravo step", "charlie step"],
    );
    let revision_before = domain.get(id).expect("task").revision;

    // Activate + one Down: cursor on "bravo step".
    for intent in [BoardIntent::PageScrollDown, BoardIntent::PageScrollDown] {
        apply_intent(&mut domain, &mut model, intent, None, None).expect("cursor down");
    }

    let toggle = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char(' '))).expect("alt+space");
    assert_eq!(toggle, BoardIntent::PrimaryVerb);
    let outcome = apply_intent(&mut domain, &mut model, toggle, None, None).expect("toggle");
    assert_eq!(
        outcome,
        IntentOutcome::Persist,
        "the step toggle must ride the real persist path"
    );

    let task = domain.get(id).expect("task");
    let dones: Vec<bool> = task.steps.iter().map(|step| step.done).collect();
    assert_eq!(
        dones,
        vec![false, true, false],
        "exactly the highlighted step flips"
    );
    assert_eq!(
        task.status,
        HumanStatus::Ready,
        "toggling an step never changes human status"
    );
    assert_ne!(
        task.revision, revision_before,
        "the toggle must bump the revision"
    );
    assert_eq!(
        task.history.last().expect("event").kind,
        TaskEventKind::StepChecked,
        "the toggle must journal its typed event"
    );

    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.contains("steps 1/3"),
        "done/total counts must follow the toggle:\n{frame}"
    );
    assert!(
        frame.contains("▸ ✓ bravo step"),
        "the toggled step stays under the cursor:\n{frame}"
    );
}

/// Alt+e is contextual: with the cursor active it opens the section's one-line
/// editor seeded with the highlighted step's text (Enter renames the step);
/// with the cursor inactive it is the existing title-edit verb, unchanged.
#[test]
fn rename_verb_targets_step_or_title_by_cursor() {
    let (mut domain, mut model, id) =
        board_with_steps("Rename target", None, &["alpha step", "bravo step"]);

    // Cursor on the first step, then Alt+e opens the step editor seeded with it.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let rename = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('e'))).expect("alt+e");
    assert_eq!(rename, BoardIntent::BeginEditTitle);
    apply_intent(&mut domain, &mut model, rename.clone(), None, None).expect("open step editor");

    assert_ne!(
        model.input_mode(),
        BoardInputMode::EditTitle,
        "with the cursor active the rename verb must not edit the title"
    );
    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame
            .lines()
            .any(|row| row.contains("▎") && row.contains("alpha step")),
        "the footer step input must paint seeded with the step's text:\n{frame}"
    );

    // Edit the draft, then Enter applies the rename to the step, not the title.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::EditInsert('!'),
        None,
        None,
    )
    .expect("type into step editor");
    let save = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    assert_eq!(save, BoardIntent::ConfirmEdit);
    let outcome = apply_intent(&mut domain, &mut model, save, None, None).expect("rename step");
    assert_eq!(outcome, IntentOutcome::Persist);
    // The app boundary persisted; its confirmed sync closes the held line (T-4).
    model.sync_from_domain(&domain);

    let task = domain.get(id).expect("task");
    assert_eq!(
        task.steps[0].text, "alpha step!",
        "Enter must rename the highlighted step"
    );
    assert_eq!(task.title, "Rename target", "the title is untouched");
    assert_eq!(
        task.history.last().expect("event").kind,
        TaskEventKind::StepRenamed
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // Cursor inactive (fresh page session): Alt+e is the existing title edit.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("close page");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("reopen page");
    apply_intent(&mut domain, &mut model, rename, None, None).expect("alt+e inactive cursor");
    assert_eq!(
        model.input_mode(),
        BoardInputMode::EditTitle,
        "with the cursor inactive Alt+e edits the title, unchanged"
    );
    assert_eq!(model.edit_buffer(), "Rename target");
}

/// Alt+x is mark-then-confirm: the first press visibly marks the cursor's step,
/// any intervening key clears the mark without removing, and a second Alt+x
/// with nothing between removes the step through the domain command.
#[test]
fn delete_verb_marks_then_removes_on_second_press() {
    let (mut domain, mut model, id) = board_with_steps(
        "Delete witness",
        None,
        &["alpha step", "bravo step", "charlie step"],
    );
    let revision_before = domain.get(id).expect("task").revision;

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let delete = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('x'))).expect("alt+x");
    assert_eq!(delete, BoardIntent::SoftDelete);

    // First press: visible mark, no mutation.
    let outcome =
        apply_intent(&mut domain, &mut model, delete.clone(), None, None).expect("mark step");
    assert_eq!(
        outcome,
        IntentOutcome::None,
        "the marking press persists nothing"
    );
    let task = domain.get(id).expect("task");
    assert_eq!(task.steps.len(), 3, "nothing removed yet");
    assert!(!task.soft_deleted, "the task itself is not deleted");
    assert_eq!(task.revision, revision_before, "no journaled mutation yet");
    let marked = rendered_board(&model, 80, 24);
    assert!(
        marked.contains("✗ alpha step"),
        "the marked step must paint visibly:\n{marked}"
    );

    // An intervening key (a cursor move) clears the mark without removing.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("intervening key");
    let cleared = rendered_board(&model, 80, 24);
    assert!(
        !cleared.contains("✗"),
        "the intervening key must clear the mark:\n{cleared}"
    );
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        3,
        "clearing the mark removes nothing"
    );

    // Mark again, then confirm with a second Alt+x and nothing between.
    apply_intent(&mut domain, &mut model, delete.clone(), None, None).expect("mark again");
    let removed = rendered_board(&model, 80, 24);
    assert!(
        removed.contains("✗ bravo step"),
        "the mark follows the cursor's step:\n{removed}"
    );
    let outcome = apply_intent(&mut domain, &mut model, delete, None, None).expect("remove step");
    assert_eq!(outcome, IntentOutcome::Persist);
    let task = domain.get(id).expect("task");
    let texts: Vec<&str> = task.steps.iter().map(|step| step.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["alpha step", "charlie step"],
        "the second press removes the marked step"
    );
    assert_eq!(
        task.history.last().expect("event").kind,
        TaskEventKind::StepRemoved
    );
    assert_eq!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "the page stays open"
    );
}

/// With overflowing notes and steps, the first Down activates the cursor and
/// leaves the shared content in place. Wheel input is the reading route.
#[test]
fn first_bare_down_activates_cursor_before_shared_content() {
    let notes = wrapping_notes();
    let (mut domain, mut model, _) = board_with_steps(
        "Scroll witness",
        Some(&notes),
        &["alpha step", "bravo step"],
    );
    let before = rendered_board(&model, 80, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let after = rendered_board(&model, 80, 24);
    assert!(
        after.contains("▸ ▪ alpha step"),
        "Down must activate the first step cursor"
    );
    assert!(
        before.contains("L0"),
        "fixture must begin at the notes head"
    );
}

/// Up reverses the shared content scroll while the page chrome remains fixed.
#[test]
fn page_scroll_up_reverses_the_shared_content_region() {
    let notes = wrapping_notes();
    let (mut domain, mut model, _) =
        board_with_steps("Deactivate witness", Some(&notes), &["alpha step"]);
    rendered_board(&model, 80, 24);
    for _ in 0..4 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageScrollDown,
            None,
            None,
        )
        .expect("scroll down");
    }
    let down = rendered_board(&model, 80, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollUp,
        None,
        None,
    )
    .expect("scroll up");
    let up = rendered_board(&model, 80, 24);
    assert_ne!(down, up, "Up must move the shared body back");
    assert!(up.contains("Deactivate witness") && up.contains("created"));
}

/// T-10 (AC-17, AC-18, AC-26): keys own the active cursor before they move the
/// stream, while wheel input remains stream-only.
#[test]
fn active_cursor_up_precedes_shared_scroll() {
    let (mut domain, mut model, _) = board_with_steps(
        "Cursor priority",
        Some(&wrapping_notes()),
        &["first step", "second step"],
    );
    rendered_board(&model, 80, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectStep(1),
        None,
        None,
    )
    .expect("select second step");
    let selected = rendered_board(&model, 80, 24);
    assert!(
        selected.contains("▸ ▪ second step"),
        "second step selected:\n{selected}"
    );

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageWheelScrollDown,
        None,
        None,
    )
    .expect("wheel scroll");
    let after_wheel = rendered_board(&model, 80, 24);
    assert!(
        after_wheel.contains("▸ ▪ second step"),
        "wheel scrolling must not move the step cursor:\n{after_wheel}"
    );

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollUp,
        None,
        None,
    )
    .expect("key up");
    let after_key = rendered_board(&model, 80, 24);
    assert!(
        after_key.contains("▸ ▪ first step"),
        "Up must move the active cursor before scrolling the stream:\n{after_key}"
    );
}

/// Step cursor navigation stays available when the shared content already fits.
#[test]
fn down_activates_the_cursor_when_shared_content_does_not_overflow() {
    let (mut domain, mut model, _) = board_with_steps(
        "Reactivation witness",
        Some("short note"),
        &["alpha step", "bravo step"],
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let activated = rendered_board(&model, 80, 24);
    assert!(activated.contains("▸ ▪ alpha step"));
}

/// AC-19: a task with no steps steps never activates a cursor; bare
/// arrows scroll the page exactly as before this feature.
#[test]
fn no_step_task_arrows_scroll_notes_unchanged() {
    let notes = wrapping_notes();
    let (mut domain, mut model, _id) = board_with_steps("Notes-only task", Some(&notes), &[]);

    let start = rendered_board(&model, 80, 24);
    assert!(start.contains("L0"), "notes painted from the top:\n{start}");
    assert!(
        !start.contains("steps") && !start.contains("▸"),
        "no section, no cursor, on an empty steps:\n{start}"
    );

    // Exactly pre-feature behavior: every arrow press scrolls the notes by one row.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("scroll down one");
    let down = rendered_board(&model, 80, 24);
    assert!(
        !down.contains("L0") && down.contains("L1"),
        "a bare Down must scroll the notes exactly one row:\n{down}"
    );
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollUp,
        None,
        None,
    )
    .expect("scroll up one");
    let up = rendered_board(&model, 80, 24);
    assert!(
        up.contains("L0"),
        "a bare Up must scroll the notes back:\n{up}"
    );
    assert!(
        !up.contains("▸"),
        "an empty steps collection never activates a cursor:\n{up}"
    );
}

/// Alt+a opens the section's one-line editor empty; Enter applies the add
/// through the domain command and closes, Esc cancels and closes.
#[test]
fn add_verb_opens_editor_enter_applies_esc_cancels() {
    let (mut domain, mut model, id) = board_with_steps("Add witness", None, &["alpha step"]);

    let add = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('a')))
        .expect("alt+a must open the step editor");
    apply_intent(&mut domain, &mut model, add, None, None).expect("open add editor");
    assert_ne!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "the editor owns input"
    );
    let frame = rendered_board(&model, 80, 24);
    assert!(
        frame.lines().any(|row| row.contains("▎")),
        "the empty editor must paint its prompt on the footer line:\n{frame}"
    );

    for ch in "zed step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type");
    }
    let save = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    assert_eq!(save, BoardIntent::ConfirmEdit);
    let outcome = apply_intent(&mut domain, &mut model, save, None, None).expect("add step");
    assert_eq!(outcome, IntentOutcome::Persist);
    // The app boundary persisted; its confirmed sync closes the held line (T-4).
    model.sync_from_domain(&domain);
    let task = domain.get(id).expect("task");
    let texts: Vec<&str> = task.steps.iter().map(|step| step.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["alpha step", "zed step"],
        "Enter must append the typed step"
    );
    assert_eq!(
        task.history.last().expect("event").kind,
        TaskEventKind::StepAdded
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // Esc cancels: the draft is discarded, nothing is appended.
    let add = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('a')))
        .expect("alt+a reopens the editor");
    apply_intent(&mut domain, &mut model, add, None, None).expect("reopen add editor");
    for ch in "junk".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type junk");
    }
    let cancel = map_key(model.input_mode(), press(KeyCode::Esc)).expect("esc");
    assert_eq!(cancel, BoardIntent::CancelEdit);
    let outcome = apply_intent(&mut domain, &mut model, cancel, None, None).expect("cancel");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        2,
        "Esc appends nothing"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    let closed = rendered_board(&model, 80, 24);
    assert!(
        !closed.lines().any(|row| row.contains("▎")),
        "the footer input line is gone on close:\n{closed}"
    );
}

/// T-4 (AC-12, add mode): Enter saves and closes; Ctrl+Enter saves and reopens the
/// line empty — the rapid-capture loop, proven by two steps added back-to-back; Esc
/// discards. Every save here drives the reducer then the confirmed sync
/// (`sync_from_domain`), which is exactly the pair the app save boundary runs on a
/// successful persist.
#[test]
fn step_editor_enter_saves_ctrl_enter_reopens_esc_cancels() {
    let (mut domain, mut model, id) = board_with_steps("Loop witness", None, &["alpha step"]);

    // Alt+a opens the add line; type, then Ctrl+Enter saves and reopens it empty.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("open add editor");
    for ch in "bravo step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type first step");
    }
    let ctrl_enter = map_key(
        model.input_mode(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
    )
    .expect("ctrl+enter maps in the step editor");
    let outcome = apply_intent(&mut domain, &mut model, ctrl_enter, None, None)
        .expect("save and reopen the line");
    assert_eq!(outcome, IntentOutcome::Persist);
    model.sync_from_domain(&domain);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::EditStep,
        "Ctrl+Enter in add mode reopens the line for the next step"
    );
    let reopened = rendered_board(&model, 80, 24);
    assert!(
        reopened.lines().any(|row| row.contains("▎")),
        "the reopened line paints its prompt on the footer:\n{reopened}"
    );

    // The reopened line is empty: the second step is exactly what is typed next.
    for ch in "charlie step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type second step");
    }
    let enter = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    let outcome =
        apply_intent(&mut domain, &mut model, enter.clone(), None, None).expect("save and close");
    assert_eq!(outcome, IntentOutcome::Persist);
    model.sync_from_domain(&domain);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "Enter saves and closes"
    );
    let texts: Vec<&str> = domain
        .get(id)
        .expect("task")
        .steps
        .iter()
        .map(|step| step.text.as_str())
        .collect();
    assert_eq!(
        texts,
        vec!["alpha step", "bravo step", "charlie step"],
        "the rapid-capture loop adds two steps back-to-back"
    );

    // Esc discards: reopen, type junk, Esc — nothing is appended.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("reopen add editor");
    for ch in "junk".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type junk");
    }
    let esc = map_key(model.input_mode(), press(KeyCode::Esc)).expect("esc");
    assert_eq!(esc, BoardIntent::CancelEdit);
    let outcome = apply_intent(&mut domain, &mut model, esc, None, None).expect("cancel");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        3,
        "Esc discards the draft"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
}

/// T-4 (AC-12, rename mode): Ctrl+Enter behaves as plain Enter — save and close,
/// no reopen — and the editor outlives the save call until the boundary confirms.
#[test]
fn rename_mode_ctrl_enter_saves_and_closes() {
    let (mut domain, mut model, id) =
        board_with_steps("Rename ctrl witness", None, &["alpha step"]);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let rename = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('e'))).expect("alt+e");
    apply_intent(&mut domain, &mut model, rename, None, None).expect("open rename editor");
    assert_eq!(model.input_mode(), BoardInputMode::EditStep);
    for ch in " twice".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("extend step text");
    }
    let ctrl_enter = map_key(
        model.input_mode(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
    )
    .expect("ctrl+enter maps in the step editor");
    let outcome = apply_intent(&mut domain, &mut model, ctrl_enter, None, None)
        .expect("rename via ctrl+enter");
    assert_eq!(outcome, IntentOutcome::Persist);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::EditStep,
        "the editor and its mode outlive the save call until the boundary confirms"
    );
    model.sync_from_domain(&domain);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "Ctrl+Enter in rename mode saves and closes like Enter"
    );
    assert_eq!(
        domain.get(id).expect("task").steps[0].text,
        "alpha step twice",
        "the rename landed"
    );
    let closed = rendered_board(&model, 80, 24);
    assert!(
        !closed.lines().any(|row| row.contains("▎")),
        "no reopened line in rename mode:\n{closed}"
    );
}

/// T-4 (AC-13): empty or whitespace-only step text is refused by a message painted
/// on the editor line itself — never the board status message — and the refusal
/// clears when the line closes (Esc, or a successful save).
#[test]
fn empty_step_text_refusal_paints_on_line_and_clears_on_close() {
    let (mut domain, mut model, id) = board_with_steps("Refusal witness", None, &["alpha step"]);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("open add editor");
    let enter = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    let outcome = apply_intent(&mut domain, &mut model, enter.clone(), None, None)
        .expect("an empty draft is refused on the line, not by a propagated error");
    assert_eq!(
        outcome,
        IntentOutcome::None,
        "an empty draft persists nothing"
    );
    assert_eq!(
        model.input_mode(),
        BoardInputMode::EditStep,
        "the line stays open on refusal"
    );
    assert_eq!(
        model.message(),
        None,
        "the refusal must not leak onto the board status message"
    );
    let refused = rendered_board(&model, 80, 24);
    assert!(
        refused
            .lines()
            .any(|row| row.contains("▎") && row.contains("text required")),
        "the refusal paints on the editor line itself:\n{refused}"
    );
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        1,
        "the refused Enter appends nothing"
    );

    // Whitespace-only text is refused the same way.
    for ch in "   ".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type whitespace");
    }
    let outcome = apply_intent(&mut domain, &mut model, enter.clone(), None, None)
        .expect("whitespace is refused on the line too");
    assert_eq!(outcome, IntentOutcome::None);
    let whitespace = rendered_board(&model, 80, 24);
    assert!(
        whitespace
            .lines()
            .any(|row| row.contains("▎") && row.contains("text required")),
        "whitespace-only text paints the same refusal:\n{whitespace}"
    );

    // Closing the line with Esc clears the refusal; nothing leaks to the board.
    let esc = map_key(model.input_mode(), press(KeyCode::Esc)).expect("esc");
    apply_intent(&mut domain, &mut model, esc, None, None).expect("cancel the line");
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert_eq!(
        model.message(),
        None,
        "closing the line leaves no message behind"
    );
    let closed = rendered_board(&model, 80, 24);
    assert!(
        !closed.contains("text required"),
        "the refusal clears when the line closes:\n{closed}"
    );

    // A successful save clears it too: refuse once, type valid text, Enter saves.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("reopen add editor");
    apply_intent(&mut domain, &mut model, enter.clone(), None, None)
        .expect("refuse the empty line again");
    for ch in "zed step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type valid text");
    }
    let outcome =
        apply_intent(&mut domain, &mut model, enter, None, None).expect("save the valid text");
    assert_eq!(outcome, IntentOutcome::Persist);
    model.sync_from_domain(&domain);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    let saved = rendered_board(&model, 80, 24);
    assert!(
        !saved.contains("text required"),
        "a successful save clears the refusal:\n{saved}"
    );
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        2,
        "the valid text after a refusal still saves"
    );
}

/// A step add/rename uses the reusable quick-add slot: it reserves the two
/// breathing rows around the input at the bottom of the frame, leaves the task
/// page's meta footer visible above it, and never puts the editor in the steps
/// section. Enter/Ctrl+Enter/Esc semantics hold through that shared surface, its
/// empty-text refusal stays on the input line, and nothing leaks to board status.
#[test]
fn step_input_uses_the_shared_quick_add_slot() {
    let (mut domain, mut model, id) = board_with_steps("Footer witness", None, &["alpha step"]);

    // The footer row is the row the closed page paints its scope/meta footer on.
    let closed = rendered_board(&model, 80, 24);
    let closed_rows: Vec<&str> = closed.lines().collect();
    let footer_row = closed_rows
        .iter()
        .position(|row| row.contains("created"))
        .expect("the closed page paints its meta footer");

    // The add verb uses the same status-row slot as board quick-add: two rows
    // below the normal page footer, leaving a blank row around the input and
    // retaining a relocated page footer above it.
    let quick_add_row = footer_row + 1;
    let open_meta_row = footer_row - 2;
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("open add editor");
    for ch in "bravo step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type");
    }
    let open = rendered_board(&model, 80, 24);
    let open_rows: Vec<&str> = open.lines().collect();
    assert!(
        open_rows[quick_add_row].contains("▎") && open_rows[quick_add_row].contains("bravo step"),
        "the input line must use the quick-add row with its prompt pattern:\n{open}"
    );
    assert!(
        open_rows[open_meta_row].contains("created"),
        "the page meta footer must remain visible above the shared input slot:\n{open}"
    );
    assert!(
        !open_rows[footer_row].contains("▎"),
        "the old footer row must be reserved as spacing, not host the input:\n{open}"
    );
    // Not in the steps section: its label row keeps the plain counts, and no
    // section row carries the prompt or the draft.
    let label_row = open_rows
        .iter()
        .position(|row| row.contains("steps 0/1"))
        .expect("the section label still paints its counts");
    assert_ne!(
        label_row, quick_add_row,
        "the editor line must not paint on the section's label row"
    );
    assert!(
        !open_rows[label_row].contains("▎") && !open_rows[label_row].contains("bravo step"),
        "the editor must not paint inside the steps section:\n{open}"
    );

    // Enter saves and closes through the footer surface.
    let enter = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    let outcome = apply_intent(&mut domain, &mut model, enter, None, None).expect("save");
    assert_eq!(outcome, IntentOutcome::Persist);
    model.sync_from_domain(&domain);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        2,
        "Enter appends through the footer line"
    );
    let saved = rendered_board(&model, 80, 24);
    let saved_rows: Vec<&str> = saved.lines().collect();
    assert!(
        !saved_rows.iter().any(|row| row.contains("▎")),
        "the footer line closes on save:\n{saved}"
    );
    assert!(
        saved_rows[footer_row].contains("created"),
        "the meta footer returns when the line closes:\n{saved}"
    );

    // Ctrl+Enter saves and reopens the line empty in add mode, on the footer.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("reopen add editor");
    for ch in "charlie step".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type second step");
    }
    let ctrl_enter = map_key(
        model.input_mode(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
    )
    .expect("ctrl+enter maps in the step editor");
    let outcome = apply_intent(&mut domain, &mut model, ctrl_enter, None, None)
        .expect("save and reopen the line");
    assert_eq!(outcome, IntentOutcome::Persist);
    model.sync_from_domain(&domain);
    assert_eq!(
        model.input_mode(),
        BoardInputMode::EditStep,
        "Ctrl+Enter in add mode reopens the footer line for the next step"
    );
    let reopened = rendered_board(&model, 80, 24);
    let reopened_rows: Vec<&str> = reopened.lines().collect();
    assert!(
        reopened_rows[quick_add_row].contains("▎")
            && !reopened_rows[quick_add_row].contains("charlie"),
        "the reopened shared quick-add line is empty:\n{reopened}"
    );

    // Esc cancels: the draft is discarded, the line closes, nothing is appended.
    for ch in "junk".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::EditInsert(ch),
            None,
            None,
        )
        .expect("type junk");
    }
    let esc = map_key(model.input_mode(), press(KeyCode::Esc)).expect("esc");
    let outcome = apply_intent(&mut domain, &mut model, esc, None, None).expect("cancel");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        3,
        "Esc appends nothing"
    );

    // The empty-text refusal paints on the shared input line itself, never the
    // board status message, and clears when the line closes.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginAddStep,
        None,
        None,
    )
    .expect("reopen add editor");
    let enter = map_key(model.input_mode(), press(KeyCode::Enter)).expect("enter");
    let outcome = apply_intent(&mut domain, &mut model, enter, None, None).expect("refuse");
    assert_eq!(outcome, IntentOutcome::None);
    assert_eq!(model.input_mode(), BoardInputMode::EditStep);
    assert_eq!(
        model.message(),
        None,
        "the refusal must not leak to the board status message"
    );
    let refused = rendered_board(&model, 80, 24);
    let refused_rows: Vec<&str> = refused.lines().collect();
    assert!(
        refused_rows[quick_add_row].contains("▎")
            && refused_rows[quick_add_row].contains("text required"),
        "the refusal paints on the shared quick-add line itself:\n{refused}"
    );
    let esc = map_key(model.input_mode(), press(KeyCode::Esc)).expect("esc");
    apply_intent(&mut domain, &mut model, esc, None, None).expect("close the line");
    assert_eq!(
        model.message(),
        None,
        "closing the line leaves no message behind"
    );
    let shut = rendered_board(&model, 80, 24);
    assert!(
        !shut.contains("text required") && !shut.contains("▎"),
        "the refusal and the line clear on close:\n{shut}"
    );
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        3,
        "the refused Enter appends nothing"
    );
}

/// T-7 (AC-23): the first delete-verb press arms the visible mark AND paints the
/// press-again hint in the footer's message slot; any intervening key clears both;
/// the second press with nothing between removes the step and the hint goes with
/// the mark.
#[test]
fn delete_mark_shows_press_again_footer_message() {
    let (mut domain, mut model, id) =
        board_with_steps("Hint witness", None, &["alpha step", "bravo step"]);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("activate cursor");
    let delete = map_key(BoardInputMode::TaskPage, alt(KeyCode::Char('x'))).expect("alt+x");
    assert_eq!(delete, BoardIntent::SoftDelete);

    // First press: the mark is visible AND the footer hint is painted.
    let outcome =
        apply_intent(&mut domain, &mut model, delete.clone(), None, None).expect("mark step");
    assert_eq!(outcome, IntentOutcome::None);
    let marked = rendered_board(&model, 80, 24);
    assert!(
        marked.contains("✗ alpha step"),
        "the marked step must stay visible:\n{marked}"
    );
    assert!(
        marked.contains("press alt+x again to remove"),
        "the footer must prompt the second press while the mark is armed:\n{marked}"
    );
    assert_eq!(
        model.message().expect("hint message"),
        "press alt+x again to remove"
    );

    // An intervening key clears the mark AND the hint.
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("intervening key");
    let cleared = rendered_board(&model, 80, 24);
    assert!(!cleared.contains("✗"), "the mark clears:\n{cleared}");
    assert!(
        !cleared.contains("again to remove"),
        "the hint clears with the mark:\n{cleared}"
    );
    assert_eq!(model.message(), None);
    assert_eq!(
        domain.get(id).expect("task").steps.len(),
        2,
        "clearing the mark removes nothing"
    );

    // Second press with nothing between: the step is removed, the hint is gone.
    apply_intent(&mut domain, &mut model, delete.clone(), None, None).expect("mark again");
    let outcome = apply_intent(&mut domain, &mut model, delete, None, None).expect("remove step");
    assert_eq!(outcome, IntentOutcome::Persist);
    let texts: Vec<&str> = domain
        .get(id)
        .expect("task")
        .steps
        .iter()
        .map(|step| step.text.as_str())
        .collect();
    assert_eq!(
        texts,
        vec!["alpha step"],
        "the second press removes the marked step (the intervening key moved the \
         cursor, and the re-mark follows it)"
    );
    let removed = rendered_board(&model, 80, 24);
    assert!(
        !removed.contains("✗"),
        "no mark survives the removal:\n{removed}"
    );
    assert!(
        !removed.contains("again to remove"),
        "the hint goes with the mark:\n{removed}"
    );
    assert_eq!(model.message(), None);
}
