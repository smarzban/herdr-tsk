//! Verb Surface reducers — primary verbs, done/reopen/block, drawer, Esc layers.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
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
