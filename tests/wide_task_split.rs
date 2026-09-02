use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::{
    resolve_responsive, FocusedSurface, ResponsivePresentation, WIDE_SPLIT_MIN_WIDTH,
};
use tsk_tui::ui::mouse::{left_click, map_board_mouse};
use tsk_tui::ui::render::{assert_buffer_mono, QueueHitMap, QueueHitTarget};
use tsk_tui::ui::tier::Tier;
use tsk_tui::ui::{apply_intent, draw_board, BoardIntent, BoardModel};

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
fn wide_preview_task_side_controls_are_inert_until_focus_routing_exists() {
    let domain = domain_with_tasks(&[("read only selected task", "read only notes")]);
    let model = board_model(&domain);
    let geometry = resolve_responsive(110, 24, FocusedSurface::Board);
    let (rows, hits) = render_board(&model, 110, 24);
    let task_verb_row: String = rows[23]
        .chars()
        .skip(geometry.task.x as usize)
        .take(geometry.task.width as usize)
        .collect();
    let edit_x = task_verb_row
        .find("edit")
        .expect("preview edit verb paints") as u16;

    assert!(hits.regions.iter().all(|hit| {
        hit.area.x < geometry.task.x || matches!(hit.target, QueueHitTarget::TaskNumber(_))
    }));
    assert_eq!(
        map_board_mouse(
            &model,
            &hits,
            left_click(geometry.task.x + edit_x, geometry.task.height - 1)
        ),
        None,
        "task-side preview controls must not enter the board mouse map"
    );
}
