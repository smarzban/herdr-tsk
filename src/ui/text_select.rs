//! Mouse text selection over the painted frame, and its clipboard bridge.
//!
//! The board captures the mouse (clicks peek rows, drags were dead weight), so
//! native terminal selection is unavailable while tsk runs. This module gives that
//! dead weight a job: a left-button drag marks a screen region, the frame's painted
//! cells inside that region are the copied text ("copy what you see"), and release
//! hands the text to the system clipboard through the OSC 52 escape sequence. No
//! clipboard crate: the terminal (or the host multiplexer) performs the copy,
//! exactly as it would for a native selection.
//!
//! What counts as copyable is declared by the painters, not inferred: every
//! surface pushes its content rects into `QueueHitMap::copyable` beside the paint
//! (task rows, page title, notes, steps, drawer body, options). Selection text is
//! the intersection of the drag region with those rects, so chrome (scrollbars,
//! borders, verb bars, dividers) is excluded by construction instead of by
//! pattern-matching glyphs after the fact. The selection is still purely
//! geometric: screen cells, not document ranges, re-read from the last painted
//! frame at copy time.

use std::io::{self, Write};

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

/// A drag-selected screen region: where the press landed and where the drag sits.
///
/// `anchor` is the press cell, `head` the latest drag cell; either may end up
/// visually first. Both are frame cells (column, row), not document offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: Position,
    pub head: Position,
}

impl TextSelection {
    pub fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// The region ordered so it can be walked top-left to bottom-right.
    pub fn normalized(&self) -> (u16, u16, u16, u16) {
        let (ax, ay) = (self.anchor.x, self.anchor.y);
        let (hx, hy) = (self.head.x, self.head.y);
        if (ay, ax) <= (hy, hx) {
            (ax, ay, hx, hy)
        } else {
            (hx, hy, ax, ay)
        }
    }

    /// Whether the drag has actually covered cells (a bare click is no selection).
    pub fn has_area(&self) -> bool {
        self.anchor != self.head
    }
}

/// The painted frame's text, one `String` per row, cells concatenated in order.
///
/// Empty cells read as spaces, so padded chrome rows reconstruct exactly as
/// painted; this snapshot is what [`selection_text`] slices at copy time.
pub fn frame_text_rows(buffer: &Buffer) -> Vec<String> {
    let width = buffer.area.width as usize;
    let height = buffer.area.height as usize;
    let mut rows = Vec::with_capacity(height);
    for y in 0..height {
        let mut row = String::with_capacity(width);
        for x in 0..width {
            // Default cells are already `" "`; wide-glyph continuations leave an
            // empty symbol and must not invent a column or unicode-width walks
            // drift from the buffer.
            row.push_str(buffer[(x as u16, y as u16)].symbol());
        }
        rows.push(row);
    }
    rows
}

/// The text a selection covers, taken from the frame snapshot.
///
/// Each covered row contributes its cells between the selection edges (the first
/// row from its start column onward, the last from the line start to its end
/// column, interior rows whole); rows join with newlines. Only cells inside the
/// painters' declared `copyable` rects are taken, so chrome never reaches the
/// clipboard. Every line is trimmed on both ends: tsk paints rows with a chrome
/// gutter, and painted padding is never part of what a drag meant to copy. Empty
/// rows drop entirely. `None` when the region covers no copyable text at all.
pub fn selection_text(
    rows: &[String],
    copyable: &[Rect],
    selection: &TextSelection,
) -> Option<String> {
    if !selection.has_area() {
        return None;
    }
    let (x0, y0, x1, y1) = selection.normalized();
    let mut lines = Vec::new();
    for y in y0..=y1 {
        let Some(row) = rows.get(y as usize) else {
            continue;
        };
        let start = if y == y0 { x0 } else { 0 };
        let end = if y == y1 { x1 } else { u16::MAX };
        let mut pieces: Vec<String> = Vec::new();
        for (span_start, span_end) in copyable_spans(copyable, y) {
            let from = span_start.max(start);
            let to = span_end.min(end);
            if from > to {
                continue;
            }
            let piece = slice_cells(row, from, to);
            let piece = piece.trim();
            if !piece.is_empty() {
                pieces.push(piece.to_string());
            }
        }
        if !pieces.is_empty() {
            lines.push(pieces.join(" "));
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// The copyable column spans covering one row, merged and in paint order.
///
/// Overlapping or adjacent spans coalesce; truly disjoint spans stay
/// separate and join with a space at copy time, so two side-by-side content
/// regions on one painted row do not invent the chrome between them.
fn copyable_spans(copyable: &[Rect], y: u16) -> Vec<(u16, u16)> {
    let mut spans: Vec<(u16, u16)> = copyable
        .iter()
        .filter(|area| {
            area.width > 0
                && area.height > 0
                && y >= area.y
                && y < area.y.saturating_add(area.height)
        })
        .map(|area| (area.x, area.x.saturating_add(area.width.saturating_sub(1))))
        .collect();
    spans.sort_by_key(|(start, _)| *start);
    let mut merged: Vec<(u16, u16)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some((_, last_end)) if start <= last_end.saturating_add(1) => {
                *last_end = (*last_end).max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Slice `row` by display-cell columns `[start, end]` inclusive.
fn slice_cells(row: &str, start: u16, end: u16) -> String {
    let mut out = String::new();
    let mut col: u16 = 0;
    for ch in row.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if width == 0 {
            continue;
        }
        let ch_end = col.saturating_add(width.saturating_sub(1));
        if ch_end >= start && col <= end {
            out.push(ch);
        }
        col = col.saturating_add(width);
        if col > end {
            break;
        }
    }
    out
}

/// Reverse-highlight the cells a selection covers, clipped to `copyable`.
///
/// Geometry matches [`selection_text`]: first/last rows use the drag edges, interior
/// rows span their copyable columns. Chrome outside those rects (peek `│` gutter,
/// glyphs, meta) stays unhighlighted — the same content bounds grok-build-style
/// app selection uses, rather than painting full terminal rows.
pub fn paint_selection(frame: &mut Frame<'_>, selection: &TextSelection, copyable: &[Rect]) {
    if !selection.has_area() {
        return;
    }
    let (x0, y0, x1, y1) = selection.normalized();
    let area = frame.area();
    let buffer = frame.buffer_mut();
    for y in y0..=y1 {
        if y >= area.height {
            break;
        }
        let start = if y == y0 { x0 } else { 0 };
        let end = if y == y1 { x1 } else { u16::MAX };
        for (span_start, span_end) in copyable_spans(copyable, y) {
            let from = span_start.max(start);
            let to = span_end.min(end).min(area.width.saturating_sub(1));
            if from > to {
                continue;
            }
            for x in from..=to {
                let cell = &mut buffer[(x, y)];
                let style = cell.style().add_modifier(Modifier::REVERSED);
                cell.set_style(style);
            }
        }
    }
}

/// The OSC 52 sequence that sets the system clipboard to `text`.
///
/// The terminal (or host multiplexer that understands OSC 52) performs the
/// actual copy; terminals without OSC 52 support ignore it silently.
pub fn osc52_clipboard(text: &str) -> String {
    format!("\u{1b}]52;c;{}\u{7}", base64_encode(text.as_bytes()))
}

/// Generous ceiling so a pathological drag cannot flood the terminal with a
/// multi-megabyte OSC payload.
pub const COPY_MAX_CHARS: usize = 100_000;

/// Hand `text` to the system clipboard via OSC 52.
///
/// Returns `false` when the payload exceeds [`COPY_MAX_CHARS`] or stdout write
/// fails.
pub fn copy_to_clipboard(text: &str) -> bool {
    if text.chars().count() > COPY_MAX_CHARS {
        return false;
    }
    let mut out = io::stdout();
    write!(out, "{}", osc52_clipboard(text)).is_ok() && out.flush().is_ok()
}

/// Standard Base64 (with padding) for OSC 52 payloads.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from);
        let b2 = chunk.get(2).copied().map(u32::from);
        let n = (b0 << 16) | (b1.unwrap_or(0) << 8) | b2.unwrap_or(0);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        if b1.is_some() {
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if b2.is_some() {
            out.push(TABLE[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: u16, y: u16) -> Position {
        Position::new(x, y)
    }

    #[test]
    fn osc52_wraps_base64_payload() {
        let text = "hello";
        let seq = osc52_clipboard(text);
        assert!(seq.starts_with("\u{1b}]52;c;"));
        assert!(seq.ends_with('\u{7}'));
        assert!(seq.contains(&base64_encode(text.as_bytes())));
        let seq = osc52_clipboard("foo");
        assert_eq!(seq, format!("\u{1b}]52;c;{}\u{7}", base64_encode(b"foo")));
    }

    #[test]
    fn reverse_drag_reads_the_same_text() {
        let rows = vec!["hello world something".to_string()];
        let copyable = [Rect::new(0, 0, 21, 1)];
        let sel = TextSelection::new(pos(0, 0), pos(10, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("hello world")
        );
        // Reversed drag direction reads the same.
        let rev = TextSelection::new(pos(10, 0), pos(0, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &rev).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn multi_line_selection_joins_with_newlines() {
        let rows = vec![
            "alpha     ".to_string(),
            "bravo     ".to_string(),
            "charlie   ".to_string(),
        ];
        let copyable = [Rect::new(0, 0, 10, 3)];
        let sel = TextSelection::new(pos(0, 0), pos(5, 2));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("alpha\nbravo\ncharli")
        );
    }

    #[test]
    fn interior_rows_are_taken_whole_within_copyable() {
        let rows = vec![
            "abcdefghij".to_string(),
            "0123456789".to_string(),
            "ABCDEFGHIJ".to_string(),
        ];
        let copyable = [Rect::new(0, 0, 10, 3)];
        let sel = TextSelection::new(pos(3, 0), pos(2, 2));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("defghij\n0123456789\nABC")
        );
    }

    #[test]
    fn undeclared_chrome_columns_never_reach_the_copy() {
        // Glyph / meta sit outside the declared content rect; painters push
        // only the content columns, so a drag to the frame edge leaves it behind.
        let rows = vec!["▸ title here          meta".to_string()];
        let copyable = [
            // title content starts after the glyph column
            Rect::new(2, 0, 14, 1),
        ];
        let sel = TextSelection::new(pos(0, 0), pos(25, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("title here")
        );
    }

    #[test]
    fn leading_and_trailing_padding_trim_off() {
        // Gutter spaces inside the selection but outside the trimmed content
        // is outside the declared content rect, whichever edge the drag entered.
        let rows = vec![
            "  notes line one                                   ".to_string(),
            "  notes line two                                   ".to_string(),
        ];
        let copyable = [Rect::new(2, 0, 44, 2)];
        let sel = TextSelection::new(pos(0, 0), pos(50, 1));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("notes line one\nnotes line two")
        );
    }

    #[test]
    fn disjoint_copyable_spans_on_one_row_join_with_a_space() {
        let rows = vec!["left        right".to_string()];
        let copyable = [Rect::new(0, 0, 4, 1), Rect::new(12, 0, 5, 1)];
        let sel = TextSelection::new(pos(0, 0), pos(20, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("left right")
        );
    }

    #[test]
    fn rows_outside_any_copyable_rect_copy_nothing() {
        let rows = vec![
            "chrome".to_string(),
            "content!".to_string(),
            "more".to_string(),
        ];
        let copyable = [Rect::new(0, 1, 8, 1)];
        let sel = TextSelection::new(pos(0, 0), pos(5, 2));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("content!")
        );
    }

    #[test]
    fn empty_rows_inside_the_drag_drop_out() {
        let rows = vec![
            "first line".to_string(),
            "          ".to_string(),
            "third line".to_string(),
        ];
        let copyable = [Rect::new(0, 0, 19, 3)];
        let sel = TextSelection::new(pos(0, 0), pos(9, 2));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("first line\nthird line")
        );
        // A drag that only covers the blank middle yields nothing.
        let sel = TextSelection::new(pos(0, 1), pos(5, 1));
        assert_eq!(selection_text(&rows, &copyable, &sel).as_deref(), None);
    }

    #[test]
    fn paint_selection_stays_inside_copyable_not_gutter() {
        use ratatui::backend::TestBackend;
        use ratatui::style::Modifier;
        use ratatui::Terminal;

        // Peek-shaped layout: columns 0..5 are `    │ ` chrome; content starts at 6.
        let copyable = [Rect::new(6, 0, 14, 3)];
        let sel = TextSelection::new(pos(8, 0), pos(12, 2));
        let mut terminal = Terminal::new(TestBackend::new(20, 4)).expect("terminal");
        terminal
            .draw(|frame| {
                paint_selection(frame, &sel, &copyable);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        for y in 0..=2 {
            for x in 0..6 {
                assert!(
                    !buffer[(x, y)].modifier.contains(Modifier::REVERSED),
                    "gutter cell ({x},{y}) must not reverse-highlight"
                );
            }
        }
        // Interior row: full copyable span is highlighted (line-oriented within content).
        for x in 6..=19 {
            assert!(
                buffer[(x, 1)].modifier.contains(Modifier::REVERSED),
                "content cell ({x},1) should reverse-highlight"
            );
        }
        // First row: only from drag start (8) through copyable end.
        assert!(!buffer[(6, 0)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(8, 0)].modifier.contains(Modifier::REVERSED));
        // Last row: only through drag end (12).
        assert!(buffer[(12, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(13, 2)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn bare_click_and_blank_cells_copy_nothing() {
        let rows = vec!["hello".to_string()];
        let copyable = [Rect::new(0, 0, 5, 1)];
        assert_eq!(
            selection_text(&rows, &copyable, &TextSelection::new(pos(2, 0), pos(2, 0))),
            None
        );
        let blank = vec!["     ".to_string()];
        assert_eq!(
            selection_text(&blank, &copyable, &TextSelection::new(pos(1, 0), pos(3, 0))),
            None
        );
    }

    #[test]
    fn selection_with_no_overlap_of_copyable_is_none() {
        let rows = vec!["hello".to_string()];
        let copyable = [Rect::new(0, 0, 5, 1)];
        // Drag entirely past the copyable rect.
        let sel = TextSelection::new(pos(10, 0), pos(12, 0));
        assert_eq!(selection_text(&rows, &copyable, &sel), None);
    }

    #[test]
    fn base64_encode_pads_short_inputs() {
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        let rows = vec!["abcd".to_string()];
        let copyable = [Rect::new(0, 0, 4, 1)];
        let sel = TextSelection::new(pos(0, 0), pos(3, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("abcd")
        );
        // Partial inclusive end.
        let sel = TextSelection::new(pos(1, 0), pos(2, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("bc")
        );
    }
}
