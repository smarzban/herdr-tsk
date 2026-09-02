use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tsk_tui::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::{
    resolve_responsive, FocusedSurface, ResponsivePresentation, WIDE_SPLIT_MIN_WIDTH,
};
use tsk_tui::ui::input::{map_key, map_responsive_key};
use tsk_tui::ui::mouse::{
    left_click, map_board_mouse, map_responsive_board_mouse, map_scrollbar_mouse,
    wide_mouse_focus_intent, ScrollbarMouse,
};
use tsk_tui::ui::render::{assert_buffer_mono, QueueHitMap, QueueHitTarget};
use tsk_tui::ui::tier::Tier;
use tsk_tui::ui::{apply_intent, draw_board, BoardInputMode, BoardIntent, BoardModel};

fn domain_with_tasks(tasks: &[(&str, &str)]) -> DomainState {
    let mut domain = DomainState::new();
    for &(title, notes) in tasks {
        domain
            .create(
                title,
                Some(notes.to_string()),
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create fixture task");
    }
    domain
}

fn board_model(domain: &DomainState) -> BoardModel {
    BoardModel::from_domain(domain, Some(PathBuf::from("/repos/tsk")))
}

fn render_board(model: &BoardModel, width: u16, height: u16) -> (Vec<String>, QueueHitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    let mut hits = QueueHitMap::default();
    terminal
        .draw(|frame| hits = draw_board(frame, model))
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    assert_buffer_mono(buffer);
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    (rows, hits)
}

fn region_text(rows: &[String], area: Rect) -> String {
    rows.iter()
        .skip(area.y as usize)
        .take(area.height as usize)
        .map(|row| {
            row.chars()
                .skip(area.x as usize)
                .take(area.width as usize)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn wide_geometry_activates_at_110_and_109_stays_single() {
    let wide = resolve_responsive(WIDE_SPLIT_MIN_WIDTH, 24, FocusedSurface::Board);
    assert_eq!(wide.presentation, ResponsivePresentation::WideSplit);

    let single_board = resolve_responsive(WIDE_SPLIT_MIN_WIDTH - 1, 24, FocusedSurface::Board);
    assert_eq!(
        single_board.presentation,
        ResponsivePresentation::SingleBoard
    );
    assert_eq!(single_board.board, Rect::new(0, 0, 109, 24));
    assert_eq!(single_board.task, Rect::default());
    assert_eq!(single_board.divider, None);

    let single_task = resolve_responsive(WIDE_SPLIT_MIN_WIDTH - 1, 24, FocusedSurface::Task);
    assert_eq!(single_task.presentation, ResponsivePresentation::SingleTask);
    assert_eq!(single_task.board, Rect::default());
    assert_eq!(single_task.task, Rect::new(0, 0, 109, 24));
    assert_eq!(single_task.divider, None);
}

#[test]
fn wide_geometry_reserves_one_divider_and_balances_remaining_columns() {
    for width in WIDE_SPLIT_MIN_WIDTH..=201 {
        let geometry = resolve_responsive(width, 30, FocusedSurface::Board);
        assert_eq!(geometry.presentation, ResponsivePresentation::WideSplit);

        let divider = geometry.divider.expect("wide split divider");
        assert_eq!(divider.width, 1, "{width} columns");
        assert_eq!(geometry.board.x, 0, "{width} columns");
        assert_eq!(divider.x, geometry.board.width, "{width} columns");
        assert_eq!(geometry.task.x, divider.x + 1, "{width} columns");
        assert_eq!(
            geometry.board.width + divider.width + geometry.task.width,
            width,
            "{width} columns"
        );
        assert!(
            geometry.board.width.abs_diff(geometry.task.width) <= 1,
            "{width} columns produced {} and {}",
            geometry.board.width,
            geometry.task.width
        );
    }
}

#[test]
fn wide_geometry_uses_the_narrower_half_for_shared_density() {
    let uneven = resolve_responsive(156, 24, FocusedSurface::Task);
    assert_eq!(uneven.board.width, 77);
    assert_eq!(uneven.task.width, 78);
    assert_eq!(uneven.density, Tier::Compact);

    let both_standard = resolve_responsive(157, 24, FocusedSurface::Board);
    assert_eq!(both_standard.board.width, 78);
    assert_eq!(both_standard.task.width, 78);
    assert_eq!(both_standard.density, Tier::Standard);

    let short = resolve_responsive(200, 23, FocusedSurface::Board);
    assert_eq!(short.density, Tier::Compact);
}

#[test]
fn wide_geometry_is_bounded_across_supported_sizes() {
    for height in (10..=80).chain([u16::MAX]) {
        for width in 40..=u16::MAX {
            let frame = Rect::new(0, 0, width, height);
            for focus in [FocusedSurface::Board, FocusedSurface::Task] {
                let geometry = resolve_responsive(width, height, focus);
                for area in [geometry.board, geometry.task]
                    .into_iter()
                    .chain(geometry.divider)
                {
                    assert!(area.x <= frame.width, "{width}x{height}: x {}", area.x);
                    assert!(area.y <= frame.height, "{width}x{height}: y {}", area.y);
                    assert!(
                        u32::from(area.x) + u32::from(area.width) <= u32::from(frame.width),
                        "{width}x{height}: horizontal overflow for {area:?}"
                    );
                    assert!(
                        u32::from(area.y) + u32::from(area.height) <= u32::from(frame.height),
                        "{width}x{height}: vertical overflow for {area:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn wide_layout_activates_at_110_and_109_stays_single_pane() {
    let domain = domain_with_tasks(&[("selected wide task", "wide task notes")]);
    let model = board_model(&domain);

    let (wide_rows, _) = render_board(&model, 110, 24);
    let wide = resolve_responsive(110, 24, FocusedSurface::Board);
    assert!(region_text(&wide_rows, wide.board).contains("selected wide task"));
    assert!(region_text(&wide_rows, wide.task).contains("selected wide task"));
    let divider = region_text(&wide_rows, wide.divider.expect("divider"));
    assert_eq!(divider.matches('│').count(), 24);
    assert!(divider
        .chars()
        .filter(|character| !character.is_whitespace())
        .all(|character| character == '│'));

    let (single_rows, _) = render_board(&model, 109, 24);
    assert_eq!(
        single_rows.join("\n").matches("selected wide task").count(),
        1
    );
}

#[test]
fn wide_split_never_paints_or_hits_outside_supported_frames() {
    let long_notes = "bounded notes ".repeat(80);
    let mut domain = domain_with_tasks(&[("first bounded task", &long_notes)]);
    for index in 0..20 {
        domain
            .create(
                format!("bounded list task {index}"),
                Some("more bounded notes".to_string()),
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create overflowing fixture task");
    }
    let model = board_model(&domain);

    for (width, height) in [(110, 10), (111, 24), (156, 23), (157, 24), (240, 80)] {
        let (rows, hits) = render_board(&model, width, height);
        let geometry = resolve_responsive(width, height, FocusedSurface::Board);
        let divider = geometry.divider.expect("wide divider");
        let divider_text = region_text(&rows, divider);
        assert_eq!(divider_text.matches('│').count(), height as usize);
        assert!(divider_text
            .chars()
            .filter(|character| !character.is_whitespace())
            .all(|character| character == '│'));
        assert!(
            hits.copyable.iter().any(|area| area.x >= geometry.task.x),
            "{width}x{height}: selected task side must declare bounded copy regions"
        );
        for area in hits
            .regions
            .iter()
            .map(|hit| hit.area)
            .chain(hits.copyable.iter().copied())
        {
            assert!(
                u32::from(area.x) + u32::from(area.width) <= u32::from(width),
                "{width}x{height}: horizontal hit overflow for {area:?}"
            );
            assert!(
                u32::from(area.y) + u32::from(area.height) <= u32::from(height),
                "{width}x{height}: vertical hit overflow for {area:?}"
            );
            assert!(
                area.x.saturating_add(area.width) <= geometry.board.width
                    || area.x >= geometry.task.x,
                "{width}x{height}: hit or copy region crosses the divider: {area:?}"
            );
        }
    }
}

#[test]
fn wide_board_selection_repaints_task_side_without_inline_peek() {
    let mut domain = domain_with_tasks(&[
        ("first selected task", "first unique notes"),
        ("second selected task", "second unique notes"),
    ]);
    let mut model = board_model(&domain);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Board);

    let (first_rows, _) = render_board(&model, 110, 24);
    let first_task_side = region_text(&first_rows, geometry.task);
    let first_was_first = first_task_side.contains("first selected task");
    assert!(
        first_was_first || first_task_side.contains("second selected task"),
        "initial selection must paint on the task side"
    );
    let (next_title, next_notes) = if first_was_first {
        ("second selected task", "second unique notes")
    } else {
        ("first selected task", "first unique notes")
    };

    apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None).expect("select next task");
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None).expect("open peek");
    let (second_rows, _) = render_board(&model, 110, 24);
    let frame = second_rows.join("\n");
    assert!(region_text(&second_rows, geometry.task).contains(next_title));
    assert_eq!(
        frame.matches(next_notes).count(),
        1,
        "wide mode must show notes only on the task side, never in an inline peek"
    );
}

#[test]
fn wide_split_without_selection_paints_inert_task_empty_state() {
    let model = board_model(&DomainState::new());
    let geometry = resolve_responsive(110, 24, FocusedSurface::Board);
    let (rows, hits) = render_board(&model, 110, 24);

    assert!(region_text(&rows, geometry.task).contains("no task selected"));
    assert!(hits.regions.iter().all(|hit| hit.area.x < geometry.task.x));
    assert!(hits.copyable.iter().all(|area| area.x < geometry.task.x));
}

#[test]
fn wide_preview_task_side_controls_are_exposed_only_to_the_focus_router() {
    let domain = domain_with_tasks(&[("read only selected task", "read only notes")]);
    let model = board_model(&domain);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Board);
    let (_, hits) = render_board(&model, 110, 24);
    let edit = hits
        .regions
        .iter()
        .find(|hit| hit.area.x >= geometry.task.x && matches!(hit.target, QueueHitTarget::Verb(0)))
        .expect("preview edit verb hit");
    let click = left_click(edit.area.x, edit.area.y);

    assert_eq!(
        map_responsive_board_mouse(&model, &hits, Rect::new(0, 0, 110, 24), click),
        None
    );
    assert_eq!(
        wide_mouse_focus_intent(&model, &hits, Rect::new(0, 0, 110, 24), click),
        Some(BoardIntent::FocusTaskSurface)
    );
}

fn focus_task(domain: &mut DomainState, model: &mut BoardModel) {
    let outcome = apply_intent(domain, model, BoardIntent::FocusTaskSurface, None)
        .expect("focus selected task");
    assert_eq!(outcome, tsk_tui::ui::IntentOutcome::None);
    assert_eq!(model.focused_surface(), FocusedSurface::Task);
}

#[test]
fn wide_board_enter_and_right_focus_the_same_selected_task() {
    for code in [KeyCode::Enter, KeyCode::Right] {
        let mut domain = domain_with_tasks(&[("focus target", "focus notes")]);
        let mut model = board_model(&domain);
        let selected = model.selected_id();
        let intent = map_responsive_key(
            BoardInputMode::Normal,
            FocusedSurface::Board,
            ResponsivePresentation::WideSplit,
            false,
            KeyEvent::new(code, KeyModifiers::NONE),
        )
        .expect("wide board focus key");

        apply_intent(&mut domain, &mut model, intent, None).expect("apply focus transfer");
        assert_eq!(model.focused_surface(), FocusedSurface::Task);
        assert_eq!(model.selected_id(), selected);
        assert_eq!(model.edit_target(), selected);
        assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    }

    assert_eq!(
        map_responsive_key(
            BoardInputMode::Normal,
            FocusedSurface::Board,
            ResponsivePresentation::SingleBoard,
            false,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ),
        Some(BoardIntent::OpenTaskPage),
        "below-threshold Enter keeps the existing task-page route"
    );
}

#[test]
fn task_view_escape_and_left_return_focus_without_resetting_page_session() {
    for code in [KeyCode::Esc, KeyCode::Left] {
        let mut domain = domain_with_tasks(&[("retained task", &"long notes ".repeat(100))]);
        let id = domain.tasks()[0].id;
        domain.add_step(id, "retained step").expect("add step");
        let mut model = board_model(&domain);
        focus_task(&mut domain, &mut model);
        let _ = render_board(&model, 110, 24);
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageWheelScrollDown,
            None,
        )
        .expect("scroll page");
        apply_intent(&mut domain, &mut model, BoardIntent::PageScrollDown, None)
            .expect("select first step");
        let before = (
            model.page_scroll(),
            model.step_cursor(),
            model.edit_target(),
        );
        assert!(before.0 > 0, "fixture must exercise retained page scroll");
        assert_eq!(
            before.1,
            Some(0),
            "fixture must exercise retained step cursor"
        );
        let intent = map_responsive_key(
            BoardInputMode::TaskPage,
            FocusedSurface::Task,
            ResponsivePresentation::WideSplit,
            false,
            KeyEvent::new(code, KeyModifiers::NONE),
        )
        .expect("task return key");

        apply_intent(&mut domain, &mut model, intent, None).expect("return board focus");
        assert_eq!(model.focused_surface(), FocusedSurface::Board);
        assert_eq!(model.input_mode(), BoardInputMode::Normal);
        assert_eq!(
            (
                model.page_scroll(),
                model.step_cursor(),
                model.edit_target()
            ),
            before
        );
    }

    assert_eq!(
        map_responsive_key(
            BoardInputMode::EditTitle,
            FocusedSurface::Task,
            ResponsivePresentation::WideSplit,
            true,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        ),
        Some(BoardIntent::EditMoveLeft),
        "an active editor retains its existing left-arrow handling"
    );
}

#[test]
fn shrinking_with_board_focus_preserves_selection_and_list_scroll() {
    let tasks: Vec<(String, String)> = (0..30)
        .map(|index| (format!("board task {index}"), "notes".to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = tasks
        .iter()
        .map(|(title, notes)| (title.as_str(), notes.as_str()))
        .collect();
    let mut domain = domain_with_tasks(&refs);
    let mut model = board_model(&domain);
    for _ in 0..8 {
        apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
            .expect("move selection");
    }
    apply_intent(&mut domain, &mut model, BoardIntent::ListScrollTo(3), None)
        .expect("set list scroll");
    let before = (model.selected_id(), model.list_scroll());
    assert_eq!(before.1, 3, "fixture must exercise retained list scroll");

    let _ = render_board(&model, 110, 10);
    let _ = render_board(&model, 109, 10);

    assert_eq!(model.focused_surface(), FocusedSurface::Board);
    assert_eq!((model.selected_id(), model.list_scroll()), before);
}

#[test]
fn shrinking_with_task_focus_preserves_page_scroll_and_session() {
    let mut domain = domain_with_tasks(&[
        ("task survives shrink", &"wide notes ".repeat(100)),
        ("board-only sibling", "other notes"),
    ]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let bound = model.edit_target().expect("bound task");
    let sibling_title = domain
        .tasks()
        .iter()
        .find(|task| task.id != bound)
        .expect("sibling")
        .title
        .clone();
    let _ = render_board(&model, 110, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageWheelScrollDown,
        None,
    )
    .expect("scroll task");
    let before = (
        model.page_scroll(),
        model.step_cursor(),
        model.edit_target(),
    );

    let (rows, _) = render_board(&model, 109, 24);

    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(
        (
            model.page_scroll(),
            model.step_cursor(),
            model.edit_target()
        ),
        before
    );
    let rendered = rows.join("\n");
    assert!(rendered.contains(&domain.get(bound).expect("bound task").title));
    assert!(!rendered.contains(&sibling_title));
}

#[test]
fn growing_back_to_wide_restores_focus_and_page_session() {
    let mut domain = domain_with_tasks(&[("task survives growth", &"growth notes ".repeat(100))]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let _ = render_board(&model, 109, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageWheelScrollDown,
        None,
    )
    .expect("scroll task");
    let before = (
        model.page_scroll(),
        model.step_cursor(),
        model.edit_target(),
    );

    let (rows, _) = render_board(&model, 110, 24);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Task);

    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(
        (
            model.page_scroll(),
            model.step_cursor(),
            model.edit_target()
        ),
        before
    );
    assert!(region_text(&rows, geometry.board).contains("task survives growth"));
    assert!(region_text(&rows, geometry.task).contains("task survives growth"));
}

#[test]
fn board_focused_primary_verb_ignores_parked_step_cursor() {
    let mut domain = domain_with_tasks(&[("parked task", "notes"), ("board target", "notes")]);
    let parked = board_model(&domain)
        .selected_id()
        .expect("initial selection");
    domain.add_step(parked, "parked step").expect("add step");
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let bound = model.edit_target().expect("parked binding");
    apply_intent(&mut domain, &mut model, BoardIntent::PageScrollDown, None)
        .expect("select parked step");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("return board focus");
    apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
        .expect("select board target");
    let selected = model.selected_id().expect("selected board target");
    assert_ne!(selected, bound);

    apply_intent(&mut domain, &mut model, BoardIntent::PrimaryVerb, None)
        .expect("apply board primary verb");

    assert_eq!(
        domain.get(selected).expect("selected task").status,
        tsk_tui::domain::HumanStatus::Started
    );
    assert!(!domain.get(bound).expect("parked task").steps[0].done);
}

#[test]
fn board_selection_after_focus_return_repaints_right_side() {
    let mut domain = domain_with_tasks(&[("parked right side", "old"), ("new right side", "new")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let parked = model.edit_target().expect("parked binding");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("return board focus");
    apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
        .expect("change board selection");
    let selected = model.selected_id().expect("new selection");
    assert_ne!(selected, parked);

    let (rows, _) = render_board(&model, 110, 24);
    let task = resolve_responsive(110, 24, FocusedSurface::Board).task;
    let task_text = region_text(&rows, task);

    assert!(task_text.contains(&domain.get(selected).expect("selected task").title));
    assert!(!task_text.contains(&domain.get(parked).expect("parked task").title));
}

#[test]
fn narrow_board_after_focus_return_uses_board_verbs_and_row_clicks() {
    let mut domain = domain_with_tasks(&[("narrow parked", "notes"), ("narrow row", "notes")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("return board focus");

    let (rows, hits) = render_board(&model, 109, 24);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    let verb_row = &rows[23];
    assert!(verb_row.contains("enter open"), "board verbs: {verb_row}");
    assert!(!verb_row.contains("esc close"), "board verbs: {verb_row}");
    let row = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Task(_)))
        .expect("board row hit");
    assert!(matches!(
        map_board_mouse(&model, &hits, left_click(row.area.x, row.area.y)),
        Some(BoardIntent::SelectIndex(_))
    ));
}

#[test]
fn editing_task_session_escape_and_left_keep_editor_semantics() {
    for code in [KeyCode::Esc, KeyCode::Left] {
        let mut domain = domain_with_tasks(&[("editor target", "notes")]);
        let mut model = board_model(&domain);
        focus_task(&mut domain, &mut model);
        apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None)
            .expect("begin title edit");
        apply_intent(&mut domain, &mut model, BoardIntent::EditInsert('!'), None)
            .expect("change title draft");
        for _ in 0..8 {
            if model.input_mode() == BoardInputMode::TaskPage {
                break;
            }
            apply_intent(&mut domain, &mut model, BoardIntent::FormFocusNext, None)
                .expect("advance task edit focus");
        }
        assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
        assert!(model.task_editing());
        let intent = map_responsive_key(
            BoardInputMode::TaskPage,
            FocusedSurface::Task,
            ResponsivePresentation::WideSplit,
            true,
            KeyEvent::new(code, KeyModifiers::NONE),
        );

        if code == KeyCode::Esc {
            assert_eq!(intent, Some(BoardIntent::CloseLayer));
            apply_intent(&mut domain, &mut model, intent.expect("Esc intent"), None)
                .expect("cancel task edit");
            assert!(!model.task_editing());
        } else {
            assert_eq!(intent, None);
            assert!(model.task_editing());
        }
        assert_eq!(model.focused_surface(), FocusedSurface::Task);
    }
}

fn translated_task_hit_signature(hits: &QueueHitMap, area: Rect) -> Vec<(String, Rect)> {
    let mut signature: Vec<_> = hits
        .regions
        .iter()
        .filter(|hit| {
            hit.area.x >= area.x
                && hit.area.y >= area.y
                && hit.area.x.saturating_add(hit.area.width) <= area.x.saturating_add(area.width)
                && hit.area.y.saturating_add(hit.area.height) <= area.y.saturating_add(area.height)
        })
        .map(|hit| {
            (
                format!("{:?}", hit.target),
                Rect::new(
                    hit.area.x - area.x,
                    hit.area.y - area.y,
                    hit.area.width,
                    hit.area.height,
                ),
            )
        })
        .collect();
    signature.sort_by_key(|(target, area)| (area.y, area.x, target.clone()));
    signature
}

fn translated_copy_signature(hits: &QueueHitMap, area: Rect) -> Vec<Rect> {
    let mut signature: Vec<_> = hits
        .copyable
        .iter()
        .filter(|rect| {
            rect.x >= area.x
                && rect.y >= area.y
                && rect.x.saturating_add(rect.width) <= area.x.saturating_add(area.width)
                && rect.y.saturating_add(rect.height) <= area.y.saturating_add(area.height)
        })
        .map(|rect| Rect::new(rect.x - area.x, rect.y - area.y, rect.width, rect.height))
        .collect();
    signature.sort_by_key(|area| (area.y, area.x, area.width, area.height));
    signature
}

fn assert_model_and_domain_outcome_equal(
    domain: &DomainState,
    model: &BoardModel,
    intent: BoardIntent,
    label: &str,
) {
    let mut single_domain = domain.clone();
    let mut wide_domain = domain.clone();
    let mut single_model = model.clone();
    let mut wide_model = model.clone();
    let single_outcome = apply_intent(&mut single_domain, &mut single_model, intent.clone(), None)
        .expect("single-pane action");
    let wide_outcome =
        apply_intent(&mut wide_domain, &mut wide_model, intent, None).expect("wide action");
    assert_eq!(single_outcome, wide_outcome, "{label}: reducer outcome");
    assert_eq!(single_domain.tasks().len(), wide_domain.tasks().len());
    for single_task in single_domain.tasks() {
        let wide_task = wide_domain.get(single_task.id).expect("matching wide task");
        assert_eq!(single_task.title, wide_task.title, "{label}: title");
        assert_eq!(single_task.notes, wide_task.notes, "{label}: notes");
        assert_eq!(single_task.thread, wide_task.thread, "{label}: thread");
        assert_eq!(single_task.status, wide_task.status, "{label}: status");
        assert_eq!(single_task.scope, wide_task.scope, "{label}: scope");
        assert_eq!(single_task.steps, wide_task.steps, "{label}: steps");
        assert_eq!(
            single_task.soft_deleted, wide_task.soft_deleted,
            "{label}: deletion"
        );
    }
    assert_eq!(
        (
            single_model.input_mode(),
            single_model.focused_surface(),
            single_model.selected_id(),
            single_model.edit_target(),
            single_model.page_scroll(),
            single_model.step_cursor(),
            single_model.form_focus(),
            single_model.edit_buffer(),
            single_model.edit_cursor(),
            single_model.task_editing(),
        ),
        (
            wide_model.input_mode(),
            wide_model.focused_surface(),
            wide_model.selected_id(),
            wide_model.edit_target(),
            wide_model.page_scroll(),
            wide_model.step_cursor(),
            wide_model.form_focus(),
            wide_model.edit_buffer(),
            wide_model.edit_cursor(),
            wide_model.task_editing(),
        ),
        "{label}: page-session transition"
    );
}

fn assert_equal_task_surface_mouse_outcomes(label: &str, domain: &DomainState, model: &BoardModel) {
    let single_area = Rect::new(0, 0, 55, 24);
    let wide_area = Rect::new(0, 0, 111, 24);
    let task_area = resolve_responsive(111, 24, FocusedSurface::Task).task;
    assert_eq!(task_area.width, single_area.width);
    let (single_rows, single_hits) = render_board(model, single_area.width, single_area.height);
    let (wide_rows, wide_hits) = render_board(model, wide_area.width, wide_area.height);

    assert_eq!(
        region_text(&single_rows, single_area),
        region_text(&wide_rows, task_area),
        "{label}: task text"
    );
    assert_eq!(
        translated_task_hit_signature(&single_hits, single_area),
        translated_task_hit_signature(&wide_hits, task_area),
        "{label}: translated hits"
    );
    assert_eq!(
        translated_copy_signature(&single_hits, single_area),
        translated_copy_signature(&wide_hits, task_area),
        "{label}: text-selection copy regions"
    );

    for wide_hit in wide_hits.regions.iter().filter(|hit| {
        hit.area.x >= task_area.x
            && hit.area.x.saturating_add(hit.area.width)
                <= task_area.x.saturating_add(task_area.width)
    }) {
        let local = Rect::new(
            wide_hit.area.x - task_area.x,
            wide_hit.area.y,
            wide_hit.area.width,
            wide_hit.area.height,
        );
        let single_hit = single_hits
            .regions
            .iter()
            .find(|candidate| candidate.target == wide_hit.target && candidate.area == local)
            .expect("equal-geometry single-pane hit");
        let single_click = left_click(single_hit.area.x, single_hit.area.y);
        let wide_click = left_click(wide_hit.area.x, wide_hit.area.y);
        let single_intent = map_board_mouse(model, &single_hits, single_click);
        let wide_intent = map_responsive_board_mouse(model, &wide_hits, wide_area, wide_click);
        assert_eq!(single_intent, wide_intent, "{label}: {:?}", wide_hit.target);
        if let Some(intent) = single_intent {
            assert_model_and_domain_outcome_equal(
                domain,
                model,
                intent,
                &format!("{label}: {:?}", wide_hit.target),
            );
        }
    }

    if let (Some(single_scroll), Some(wide_scroll)) = (
        single_hits
            .regions
            .iter()
            .find(|hit| matches!(hit.target, QueueHitTarget::PageScroll(_))),
        wide_hits
            .regions
            .iter()
            .find(|hit| matches!(hit.target, QueueHitTarget::PageScroll(_))),
    ) {
        let mut single_drag = false;
        let mut wide_drag = false;
        assert_eq!(
            map_scrollbar_mouse(
                model.input_mode(),
                &single_hits,
                left_click(single_scroll.area.x, single_scroll.area.y),
                &mut single_drag,
            ),
            map_scrollbar_mouse(
                model.input_mode(),
                &wide_hits,
                left_click(wide_scroll.area.x, wide_scroll.area.y),
                &mut wide_drag,
            ),
            "{label}: scrollbar"
        );
        if model.input_mode() == BoardInputMode::TaskPage {
            let mut dragging = false;
            assert!(matches!(
                map_scrollbar_mouse(
                    model.input_mode(),
                    &wide_hits,
                    left_click(wide_scroll.area.x, wide_scroll.area.y),
                    &mut dragging,
                ),
                ScrollbarMouse::Intent(BoardIntent::PageScrollTo(_))
            ));
        }
    }
}

#[test]
fn focused_wide_task_surface_matches_single_pane_keyboard_and_mouse_outcomes() {
    let mut domain = domain_with_tasks(&[("parity task", &"parity notes ".repeat(120))]);
    let id = domain.tasks()[0].id;
    domain.add_step(id, "first parity step").expect("step");
    domain.add_step(id, "second parity step").expect("step");
    let mut view = board_model(&domain);
    focus_task(&mut domain, &mut view);
    let _ = render_board(&view, 55, 24);

    let mut scenarios = vec![("task view", view.clone())];
    for (label, intent) in [
        ("title edit", BoardIntent::BeginEditTitle),
        ("notes edit", BoardIntent::BeginEditNotes),
    ] {
        let mut state = view.clone();
        let mut state_domain = domain.clone();
        apply_intent(&mut state_domain, &mut state, intent, None).expect("enter field edit");
        scenarios.push((label, state));
    }

    let mut scope = scenarios[1].1.clone();
    let mut scope_domain = domain.clone();
    apply_intent(
        &mut scope_domain,
        &mut scope,
        BoardIntent::FocusFormField(tsk_tui::ui::CaptureField::Scope),
        None,
    )
    .expect("focus scope");
    scenarios.push(("scope edit", scope.clone()));
    apply_intent(
        &mut scope_domain,
        &mut scope,
        BoardIntent::OpenFormScopeDropdown,
        None,
    )
    .expect("open scope dropdown");
    scenarios.push(("scope dropdown", scope));

    let mut thread = scenarios[1].1.clone();
    let mut thread_domain = domain.clone();
    apply_intent(
        &mut thread_domain,
        &mut thread,
        BoardIntent::FocusFormField(tsk_tui::ui::CaptureField::Thread),
        None,
    )
    .expect("select thread");
    scenarios.push(("thread selection", thread.clone()));
    apply_intent(
        &mut thread_domain,
        &mut thread,
        BoardIntent::ToggleThreadEditing,
        None,
    )
    .expect("edit thread");
    scenarios.push(("thread edit", thread));

    let mut step = scenarios[1].1.clone();
    let mut step_domain = domain.clone();
    apply_intent(
        &mut step_domain,
        &mut step,
        BoardIntent::SelectStep(0),
        None,
    )
    .expect("edit step");
    scenarios.push(("step edit", step));

    for (label, model) in &scenarios {
        assert_equal_task_surface_mouse_outcomes(label, &domain, model);
    }

    let keyboard = [
        (
            0,
            BoardInputMode::TaskPage,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
        ),
        (
            1,
            BoardInputMode::EditTitle,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ),
        (
            2,
            BoardInputMode::EditNotes,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ),
        (
            3,
            BoardInputMode::EditScope,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ),
        (
            4,
            BoardInputMode::FormScopeDropdown,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        ),
        (
            5,
            BoardInputMode::SelectThread,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ),
        (
            6,
            BoardInputMode::EditThread,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ),
        (
            7,
            BoardInputMode::EditStep,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ),
    ];
    for (scenario, mode, key) in keyboard {
        let wide_intent = map_responsive_key(
            mode,
            FocusedSurface::Task,
            ResponsivePresentation::WideSplit,
            !matches!(mode, BoardInputMode::TaskPage),
            key,
        );
        let single_intent = map_key(mode, key);
        assert_eq!(wide_intent, single_intent, "{mode:?} keyboard parity");
        if let Some(intent) = single_intent {
            assert_model_and_domain_outcome_equal(
                &domain,
                &scenarios[scenario].1,
                intent,
                &format!("{mode:?} keyboard"),
            );
        }
    }
}

#[test]
fn wide_task_control_click_preserves_clicked_status_action_with_foreign_parked_cursor() {
    let mut domain = domain_with_tasks(&[("parked cursor", "notes"), ("status target", "notes")]);
    let parked = domain
        .tasks()
        .iter()
        .find(|task| task.title == "parked cursor")
        .expect("parked task")
        .id;
    let target = domain
        .tasks()
        .iter()
        .find(|task| task.title == "status target")
        .expect("target task")
        .id;
    domain.add_step(parked, "parked step").expect("add step");
    domain.add_step(target, "target step").expect("add step");
    domain
        .set_status(target, HumanStatus::Started)
        .expect("start target");
    let mut model = board_model(&domain);
    let parked_index = model
        .visible_ids()
        .iter()
        .position(|&id| id == parked)
        .expect("parked row");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(parked_index),
        None,
    )
    .expect("select parked task");
    focus_task(&mut domain, &mut model);
    apply_intent(&mut domain, &mut model, BoardIntent::PageScrollDown, None)
        .expect("select parked step");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("park page");
    let target_index = model
        .visible_ids()
        .iter()
        .position(|&id| id == target)
        .expect("target row");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(target_index),
        None,
    )
    .expect("select target");

    let (preview_rows, preview_hits) = render_board(&model, 110, 24);
    let task_area = resolve_responsive(110, 24, FocusedSurface::Board).task;
    let verb_row = &preview_rows[task_area.height.saturating_sub(1) as usize];
    let preview_verbs = verb_row
        .chars()
        .skip(task_area.x as usize)
        .take(task_area.width as usize)
        .collect::<String>();
    assert!(
        !preview_verbs.contains("toggle step"),
        "a foreign parked cursor must not change the selected task's controls"
    );
    let done_x = task_area.x
        + u16::try_from(preview_verbs.find("done").expect("preview done verb"))
            .expect("done column");
    let click = left_click(done_x, task_area.height.saturating_sub(1));
    assert_eq!(
        wide_mouse_focus_intent(&model, &preview_hits, Rect::new(0, 0, 110, 24), click),
        Some(BoardIntent::FocusTaskSurface)
    );

    focus_task(&mut domain, &mut model);
    let (_, focused_hits) = render_board(&model, 110, 24);
    let dispatched =
        map_responsive_board_mouse(&model, &focused_hits, Rect::new(0, 0, 110, 24), click);
    assert_eq!(dispatched, Some(BoardIntent::Complete));
    apply_intent(
        &mut domain,
        &mut model,
        dispatched.expect("clicked status action"),
        None,
    )
    .expect("dispatch clicked action");
    assert_eq!(
        domain.get(target).expect("target task").status,
        HumanStatus::Done
    );
}

#[test]
fn wide_task_step_click_preserves_scrolled_step_target() {
    let mut domain = domain_with_tasks(&[("scrolled task", "short notes")]);
    let id = domain.tasks()[0].id;
    for index in 0..20 {
        domain
            .add_step(id, format!("step {index}"))
            .expect("add step");
    }
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let _ = render_board(&model, 110, 24);
    apply_intent(&mut domain, &mut model, BoardIntent::PageScrollTo(3), None).expect("scroll page");
    assert_eq!(model.page_scroll(), 3);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("park scrolled page");

    let (_, preview_hits) = render_board(&model, 110, 24);
    let task_area = resolve_responsive(110, 24, FocusedSurface::Board).task;
    let step_zero = preview_hits
        .regions
        .iter()
        .find(|hit| {
            task_area.contains(hit.area.as_position())
                && matches!(hit.target, QueueHitTarget::Step(0))
        })
        .expect("preview step zero");
    let click = left_click(step_zero.area.x, step_zero.area.y);
    assert_eq!(
        wide_mouse_focus_intent(&model, &preview_hits, Rect::new(0, 0, 110, 24), click),
        Some(BoardIntent::FocusTaskSurface)
    );

    focus_task(&mut domain, &mut model);
    let (_, focused_hits) = render_board(&model, 110, 24);
    assert_eq!(
        map_responsive_board_mouse(&model, &focused_hits, Rect::new(0, 0, 110, 24), click,),
        Some(BoardIntent::SelectStep(0))
    );
}

#[test]
fn wide_board_row_click_is_inert_during_active_title_edit() {
    let mut domain = domain_with_tasks(&[("editing task", "notes"), ("other row", "notes")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None).expect("edit title");
    let selected = model.selected_id();
    let (_, hits) = render_board(&model, 110, 24);
    let other_row = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Task(id) if Some(id) != selected))
        .expect("other board row");
    let click = left_click(other_row.area.x, other_row.area.y);

    assert_eq!(
        map_responsive_board_mouse(&model, &hits, Rect::new(0, 0, 110, 24), click),
        None
    );
    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
}

#[test]
fn task_editor_ignores_unfocused_board_verb_hits() {
    let mut domain = domain_with_tasks(&[("original title", "notes")]);
    let id = domain.tasks()[0].id;
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None).expect("edit title");
    apply_intent(&mut domain, &mut model, BoardIntent::EditInsert('!'), None)
        .expect("change title draft");
    let draft = model.edit_buffer().to_string();
    let (_, hits) = render_board(&model, 110, 24);
    let board_area = resolve_responsive(110, 24, FocusedSurface::Task).board;
    let foreign_verb = hits
        .regions
        .iter()
        .find(|hit| {
            board_area.contains(hit.area.as_position())
                && matches!(hit.target, QueueHitTarget::Verb(0))
        })
        .expect("board verb");
    let click = left_click(foreign_verb.area.x, foreign_verb.area.y);
    let mapped = map_responsive_board_mouse(&model, &hits, Rect::new(0, 0, 110, 24), click);
    if let Some(intent) = mapped.clone() {
        apply_intent(&mut domain, &mut model, intent, None).expect("route foreign board verb");
    }

    assert_eq!(mapped, None);
    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
    assert_eq!(model.edit_buffer(), draft);
    assert_eq!(domain.get(id).expect("edited task").title, "original title");
}

#[test]
fn quick_add_ignores_unfocused_task_preview_verb_hits() {
    let mut domain = domain_with_tasks(&[("existing task", "notes")]);
    let original_tasks = domain.tasks().len();
    let mut model = board_model(&domain);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenCapture, None).expect("open quick add");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddInsertText("draft task".to_string()),
        None,
    )
    .expect("type quick-add draft");
    let draft = model.quick_add_title_value().to_string();
    let (_, hits) = render_board(&model, 110, 24);
    let task_area = resolve_responsive(110, 24, FocusedSurface::Board).task;
    let foreign_verb = hits
        .regions
        .iter()
        .find(|hit| {
            task_area.contains(hit.area.as_position())
                && matches!(hit.target, QueueHitTarget::Verb(0))
        })
        .expect("task preview verb");
    let click = left_click(foreign_verb.area.x, foreign_verb.area.y);
    let mapped = map_responsive_board_mouse(&model, &hits, Rect::new(0, 0, 110, 24), click);
    if let Some(intent) = mapped.clone() {
        apply_intent(&mut domain, &mut model, intent, None).expect("route foreign task verb");
    }

    assert_eq!(mapped, None);
    assert_eq!(model.focused_surface(), FocusedSurface::Board);
    assert_eq!(model.input_mode(), BoardInputMode::QuickAdd);
    assert_eq!(model.quick_add_title_value(), draft);
    assert_eq!(domain.tasks().len(), original_tasks);
}

#[test]
fn wide_help_and_palette_close_on_foreign_surface_verbs_without_dispatching_them() {
    for (open, expected, mode) in [
        (
            BoardIntent::OpenHelp,
            BoardIntent::CloseLayer,
            BoardInputMode::Help,
        ),
        (
            BoardIntent::OpenCommandPalette,
            BoardIntent::CloseCommandSurface,
            BoardInputMode::Palette,
        ),
    ] {
        let mut domain = domain_with_tasks(&[("modal task", "notes")]);
        let mut model = board_model(&domain);
        apply_intent(&mut domain, &mut model, open, None).expect("open modal");
        assert_eq!(model.input_mode(), mode);
        let (_, hits) = render_board(&model, 110, 24);
        let task_area = resolve_responsive(110, 24, FocusedSurface::Board).task;
        let foreign_verb = hits
            .regions
            .iter()
            .find(|hit| {
                task_area.contains(hit.area.as_position())
                    && matches!(hit.target, QueueHitTarget::Verb(_))
            })
            .expect("foreign task verb");

        assert_eq!(
            map_responsive_board_mouse(
                &model,
                &hits,
                Rect::new(0, 0, 110, 24),
                left_click(foreign_verb.area.x, foreign_verb.area.y),
            ),
            Some(expected)
        );
    }
}

#[test]
fn wide_board_task_click_selects_and_returns_board_focus() {
    let mut domain = domain_with_tasks(&[("clicked board row", "notes"), ("bound task", "notes")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Task);
    let (_, hits) = render_board(&model, 110, 24);
    let row = hits
        .regions
        .iter()
        .find(|hit| {
            hit.area.x < geometry.divider.expect("divider").x
                && matches!(hit.target, QueueHitTarget::Task(id) if Some(id) != model.selected_id())
        })
        .expect("other board row");
    let intent = map_responsive_board_mouse(
        &model,
        &hits,
        Rect::new(0, 0, 110, 24),
        left_click(row.area.x, row.area.y),
    )
    .expect("wide board row intent");

    apply_intent(&mut domain, &mut model, intent.clone(), None).expect("select board row");
    let selected_once = model.selected_id();
    assert_eq!(model.focused_surface(), FocusedSurface::Board);
    assert_eq!(model.detail_open(), None);
    apply_intent(&mut domain, &mut model, intent, None).expect("repeat board row click");
    assert_eq!(model.selected_id(), selected_once);
    assert_eq!(model.focused_surface(), FocusedSurface::Board);
    assert_eq!(model.detail_open(), None);
}

#[test]
fn wide_task_control_click_focuses_task_before_dispatch() {
    let mut domain = domain_with_tasks(&[("task control", "notes")]);
    let mut model = board_model(&domain);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Board);
    let (_, hits) = render_board(&model, 110, 24);
    let verb = hits
        .regions
        .iter()
        .find(|hit| hit.area.x >= geometry.task.x && matches!(hit.target, QueueHitTarget::Verb(0)))
        .expect("task edit verb");
    let click = left_click(verb.area.x, verb.area.y);
    let focus = wide_mouse_focus_intent(&model, &hits, Rect::new(0, 0, 110, 24), click)
        .expect("task-side control requests focus");

    apply_intent(&mut domain, &mut model, focus, None).expect("focus task first");
    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    let action = map_responsive_board_mouse(&model, &hits, Rect::new(0, 0, 110, 24), click);
    assert_eq!(action, Some(BoardIntent::BeginEditTitle));
}

#[test]
fn wide_mouse_coordinates_outside_live_surface_hits_are_inert() {
    let mut domain = domain_with_tasks(&[("inert coordinates", "notes")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let area = Rect::new(0, 0, 110, 24);
    let geometry = resolve_responsive(area.width, area.height, FocusedSurface::Task);
    let (_, hits) = render_board(&model, area.width, area.height);
    let board_verb = hits
        .regions
        .iter()
        .find(|hit| hit.area.x < geometry.task.x && matches!(hit.target, QueueHitTarget::Verb(_)))
        .expect("board-side verb");

    assert_eq!(
        map_responsive_board_mouse(
            &model,
            &hits,
            area,
            left_click(board_verb.area.x, board_verb.area.y),
        ),
        None
    );
    let divider = geometry.divider.expect("divider");
    assert_eq!(
        map_responsive_board_mouse(&model, &hits, area, left_click(divider.x, divider.y),),
        None
    );
}

#[test]
fn parked_help_round_trip_reopens_task_page_mode() {
    let mut domain =
        domain_with_tasks(&[("parked help task", "notes"), ("hidden sibling", "notes")]);
    let mut model = board_model(&domain);
    focus_task(&mut domain, &mut model);
    let bound = model.edit_target();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::FocusBoardSurface,
        None,
    )
    .expect("park task page");
    apply_intent(&mut domain, &mut model, BoardIntent::OpenHelp, None).expect("open help");
    apply_intent(&mut domain, &mut model, BoardIntent::CloseLayer, None).expect("close help");
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None)
        .expect("refocus task page");

    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
        .expect("task page selection stays pinned");
    assert_eq!(model.selected_id(), bound);
}
