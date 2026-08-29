//! Vertical mono scrollbar: dim track (`│`) and solid thumb (`█`).
//!
//! Space is reserved only when content overflows the viewport. Callers paint the
//! track in the last column and leave a one-column gap beside it.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

fn style_dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// Columns reserved beside content when a scrollbar is shown: gap + track.
pub const SCROLLBAR_RESERVE_COLS: u16 = 2;

/// Whether content overflows the viewport enough to need a scrollbar.
pub fn needs_scrollbar(total_rows: usize, viewport_rows: usize) -> bool {
    total_rows > viewport_rows && viewport_rows > 0
}

/// Content width and optional one-column track rect when a scrollbar is shown.
///
/// Returns the original width and `None` when the frame is too narrow or content
/// fits. The track sits on the last column; the column before it is the gap.
pub fn split_for_scrollbar(
    area_x: u16,
    area_y: u16,
    width: u16,
    height: u16,
    total_rows: usize,
) -> (u16, Option<Rect>) {
    if !needs_scrollbar(total_rows, height as usize) || width <= SCROLLBAR_RESERVE_COLS {
        return (width, None);
    }
    let content_width = width.saturating_sub(SCROLLBAR_RESERVE_COLS);
    let track = Rect {
        x: area_x.saturating_add(width.saturating_sub(1)),
        y: area_y,
        width: 1,
        height,
    };
    (content_width, Some(track))
}

/// Map a click on the track to a content scroll offset.
///
/// First / last track rows jump to the extremes; interior rows place the thumb
/// centered on the click (inverse of [`thumb_range`]).
pub fn click_to_offset(
    cell_index: u16,
    track_cells: u16,
    total_rows: usize,
    viewport_rows: usize,
) -> usize {
    if track_cells == 0 || !needs_scrollbar(total_rows, viewport_rows) {
        return 0;
    }
    let max_scroll = total_rows.saturating_sub(viewport_rows);
    if cell_index == 0 {
        return 0;
    }
    if cell_index >= track_cells.saturating_sub(1) {
        return max_scroll;
    }
    let (thumb_start, thumb_rows) = thumb_range(0, total_rows, viewport_rows);
    let _ = thumb_start;
    let travel = (track_cells as usize).saturating_sub(thumb_rows);
    if travel == 0 || max_scroll == 0 {
        return 0;
    }
    // Center the thumb on the clicked cell.
    let centered = (cell_index as usize).saturating_sub(thumb_rows / 2);
    centered
        .saturating_mul(max_scroll)
        .checked_div(travel)
        .unwrap_or(0)
        .min(max_scroll)
}

/// Inclusive-exclusive thumb span within the track, in track-local rows.
pub fn thumb_range(scroll: usize, total_rows: usize, viewport_rows: usize) -> (usize, usize) {
    if !needs_scrollbar(total_rows, viewport_rows) {
        return (0, viewport_rows);
    }
    let thumb_rows = (viewport_rows * viewport_rows)
        .div_ceil(total_rows)
        .max(1)
        .min(viewport_rows);
    let travel = viewport_rows.saturating_sub(thumb_rows);
    let max_scroll = total_rows.saturating_sub(viewport_rows);
    let thumb_start = scroll
        .saturating_mul(travel)
        .checked_div(max_scroll)
        .unwrap_or(0)
        .min(travel);
    (thumb_start, thumb_rows)
}

/// Paint a one-column vertical scrollbar into `track`.
pub fn paint(frame: &mut Frame<'_>, track: Rect, scroll: usize, total_rows: usize) {
    let viewport = track.height as usize;
    if track.width == 0 || viewport == 0 || !needs_scrollbar(total_rows, viewport) {
        return;
    }
    let (thumb_start, thumb_rows) = thumb_range(scroll, total_rows, viewport);
    for row in 0..viewport {
        let glyph = if (thumb_start..thumb_start + thumb_rows).contains(&row) {
            "█"
        } else {
            "│"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(glyph, style_dim()))),
            Rect::new(track.x, track.y.saturating_add(row as u16), 1, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_scrollbar_only_when_overflowing() {
        assert!(needs_scrollbar(20, 10));
        assert!(!needs_scrollbar(10, 10));
        assert!(!needs_scrollbar(5, 10));
        assert!(!needs_scrollbar(20, 0));
    }

    #[test]
    fn split_reserves_gap_and_track() {
        let (content, track) = split_for_scrollbar(0, 2, 40, 10, 20);
        assert_eq!(content, 38);
        let track = track.expect("track");
        assert_eq!(track.x, 39);
        assert_eq!(track.width, 1);
        assert_eq!(track.y, 2);
        assert_eq!(track.height, 10);
    }

    #[test]
    fn split_keeps_full_width_when_content_fits() {
        let (content, track) = split_for_scrollbar(0, 0, 40, 10, 5);
        assert_eq!(content, 40);
        assert!(track.is_none());
    }

    #[test]
    fn click_extremes_and_interior() {
        assert_eq!(click_to_offset(0, 10, 100, 10), 0);
        assert_eq!(click_to_offset(9, 10, 100, 10), 90);
        let mid = click_to_offset(5, 10, 100, 10);
        assert!(mid > 0 && mid < 90, "mid={mid}");
    }

    #[test]
    fn thumb_moves_with_scroll() {
        let (top, rows) = thumb_range(0, 100, 10);
        let (bottom, rows2) = thumb_range(90, 100, 10);
        assert_eq!(rows, rows2);
        assert!(top < bottom);
    }
}
