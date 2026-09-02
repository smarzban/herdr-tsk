use ratatui::layout::Rect;
use tsk_tui::ui::board::{
    resolve_responsive, FocusedSurface, ResponsivePresentation, WIDE_SPLIT_MIN_WIDTH,
};
use tsk_tui::ui::tier::Tier;

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
