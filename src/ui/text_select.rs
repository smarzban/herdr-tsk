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

// ---------------------------------------------------------------------------
// Drag edge auto-scroll
// ---------------------------------------------------------------------------

/// Direction for drag auto-scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoScrollDirection {
    Up,
    Down,
}

/// Timer-driven auto-scroll while a text drag sits near a content edge.
///
/// Each idle tick scrolls by `speed` rows in `direction`. Pointer motion
/// recomputes the state; leaving the edge zone or ending the drag clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragAutoScrollState {
    pub direction: AutoScrollDirection,
    /// Rows to scroll per tick.
    pub speed: u16,
}

/// Rows of near-edge interior that still arm auto-scroll.
const EDGE_THRESHOLD: u16 = 2;

/// Compute auto-scroll from the pointer row relative to a content area.
///
/// Returns `Some` when the pointer is above, below, or within [`EDGE_THRESHOLD`]
/// rows of the content boundary; `None` when it sits comfortably inside.
pub fn compute_autoscroll(mouse_row: u16, content_area: Rect) -> Option<DragAutoScrollState> {
    let top = content_area.y;
    let bottom = content_area.y.saturating_add(content_area.height);

    if content_area.height == 0 {
        return None;
    }

    if mouse_row < top.saturating_add(EDGE_THRESHOLD) {
        let distance = top.saturating_add(EDGE_THRESHOLD).saturating_sub(mouse_row);
        Some(DragAutoScrollState {
            direction: AutoScrollDirection::Up,
            speed: speed_for_distance(distance),
        })
    } else if mouse_row >= bottom.saturating_sub(EDGE_THRESHOLD) {
        let distance = mouse_row
            .saturating_sub(bottom.saturating_sub(EDGE_THRESHOLD))
            .saturating_add(1);
        Some(DragAutoScrollState {
            direction: AutoScrollDirection::Down,
            speed: speed_for_distance(distance),
        })
    } else {
        None
    }
}

fn speed_for_distance(distance: u16) -> u16 {
    match distance {
        0..=2 => 1,
        3..=5 => 2,
        6..=10 => 3,
        _ => 5,
    }
}

/// Left-button press → drag → release for board text selection.
///
/// Clicks are deferred until release so a real drag can copy without also firing
/// the Down-time peek/select path. Extracted from `run_board` so the transition
/// table is unit-testable without a live event loop.
#[derive(Debug, Default, Clone)]
pub struct DragSelectGesture {
    /// Down cell while a deferred click is still armed (`None` once a drag has area).
    pending_down: Option<Position>,
    /// Edge auto-scroll armed while a drag with area sits near the content edge.
    autoscroll: Option<DragAutoScrollState>,
    /// Last pointer row seen during an active drag (feeds idle ticks).
    last_drag_row: Option<u16>,
    /// Copyable lines that scrolled out of the top of the selection.
    captured_before: Vec<String>,
    /// Copyable lines that scrolled out of the bottom of the selection.
    captured_after: Vec<String>,
    /// Copyable text on the press row; copy never includes lines before this.
    copy_origin: Option<String>,
}

/// One phase of [`DragSelectGesture::handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSelectPhase {
    Press,
    Move,
    Release,
}

/// What `run_board` should do after feeding a left-button phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSelectOutcome {
    /// Event consumed; do not dispatch a board click.
    Continue,
    /// Selection has area — copy from the painted frame snapshot.
    Copy,
    /// Bare click — dispatch at the original Down cell.
    Click(Position),
}

impl DragSelectGesture {
    pub fn new() -> Self {
        Self::default()
    }

    /// Abandon a deferred click (e.g. a key pressed while the button is held).
    pub fn clear(&mut self) {
        self.pending_down = None;
        self.autoscroll = None;
        self.last_drag_row = None;
        self.captured_before.clear();
        self.captured_after.clear();
        self.copy_origin = None;
    }

    /// Whether edge auto-scroll is armed and needs short idle ticks.
    pub fn has_autoscroll(&self) -> bool {
        self.autoscroll.is_some()
    }

    /// Current auto-scroll state, if any.
    pub fn autoscroll(&self) -> Option<DragAutoScrollState> {
        self.autoscroll
    }

    pub fn last_drag_row(&self) -> Option<u16> {
        self.last_drag_row
    }

    /// Lines that left the selection through the top of the viewport.
    pub fn captured_before(&self) -> &[String] {
        &self.captured_before
    }

    /// Lines that left the selection through the bottom of the viewport.
    pub fn captured_after(&self) -> &[String] {
        &self.captured_after
    }

    pub fn copy_origin(&self) -> Option<&str> {
        self.copy_origin.as_deref()
    }

    /// Remember the press-row title once, before autoscroll moves it.
    pub fn ensure_copy_origin(&mut self, line: String) {
        if self.copy_origin.is_none() && !line.is_empty() {
            self.copy_origin = Some(line);
        }
    }

    /// Keep copyable lines that just scrolled out of the live highlight.
    pub fn capture_leaving_rows(
        &mut self,
        rows: &[String],
        copyable: &[Rect],
        selection: TextSelection,
        content: Rect,
        direction: AutoScrollDirection,
        delta: usize,
    ) {
        if delta == 0 || content.height == 0 {
            return;
        }
        let delta = u16::try_from(delta).unwrap_or(u16::MAX);
        // Sticky headers pin at content.y, so titles leave below them. Use the
        // first copyable row in the viewport as the top of scrolling content.
        let origin = first_copyable_row(copyable, content).unwrap_or(content.y);
        let (from, to, prefix) = match direction {
            AutoScrollDirection::Down => {
                let from = origin;
                let to = origin.saturating_add(delta.saturating_sub(1));
                (from, to, true)
            }
            AutoScrollDirection::Up => {
                let bottom = content.y.saturating_add(content.height);
                let to = bottom.saturating_sub(1);
                let from = bottom.saturating_sub(delta);
                (from, to, false)
            }
        };
        let lines = selection_lines(rows, copyable, &selection, from, to);
        if lines.is_empty() {
            return;
        }
        if prefix {
            self.captured_before.extend(lines);
        } else {
            self.captured_after.splice(0..0, lines);
        }
    }

    /// Recompute auto-scroll from the pointer row and content area.
    ///
    /// Only arms when a drag already has area; otherwise clears.
    pub fn update_autoscroll(
        &mut self,
        mouse_row: u16,
        content_area: Rect,
        selection_has_area: bool,
    ) {
        self.last_drag_row = Some(mouse_row);
        if selection_has_area {
            self.autoscroll = compute_autoscroll(mouse_row, content_area);
        } else {
            self.autoscroll = None;
        }
    }

    /// Feed one left-button phase.
    ///
    /// For `Press` / `Move`, `selection` is the model selection *after* the caller
    /// applied `begin_mouse_press` / `drag_text_selection`. For `Release`, it is the
    /// selection after synthesizing a final drag from the release cell when the host
    /// omitted intermediate Drag events.
    pub fn handle(
        &mut self,
        phase: DragSelectPhase,
        position: Position,
        selection: Option<TextSelection>,
    ) -> DragSelectOutcome {
        match phase {
            DragSelectPhase::Press => {
                self.pending_down = Some(position);
                self.autoscroll = None;
                self.last_drag_row = Some(position.y);
                self.captured_before.clear();
                self.captured_after.clear();
                self.copy_origin = None;
                DragSelectOutcome::Continue
            }
            DragSelectPhase::Move => {
                self.last_drag_row = Some(position.y);
                if selection.is_some_and(|sel| sel.has_area()) {
                    self.pending_down = None;
                } else {
                    self.autoscroll = None;
                }
                DragSelectOutcome::Continue
            }
            DragSelectPhase::Release => {
                self.autoscroll = None;
                self.last_drag_row = None;
                if selection.is_some_and(|sel| sel.has_area()) {
                    self.pending_down = None;
                    DragSelectOutcome::Copy
                } else if let Some(down) = self.pending_down.take() {
                    DragSelectOutcome::Click(down)
                } else {
                    DragSelectOutcome::Continue
                }
            }
        }
    }
}

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

    /// Whether the drag has covered enough cells to count as a selection.
    ///
    /// A single-cell wobble (`anchor` adjacent to `head`) stays a click: hosts often
    /// deliver one pixel of motion between Down and Up. Chebyshev distance ≥ 2
    /// (two steps in any direction, including diagonal) is the copy threshold.
    pub fn has_area(&self) -> bool {
        let dx = self.anchor.x.abs_diff(self.head.x);
        let dy = self.anchor.y.abs_diff(self.head.y);
        dx.max(dy) >= 2
    }

    /// Keep the anchor on the same content row after the viewport scrolled.
    /// Head stays on the pointer.
    pub fn shift_anchor_y(&mut self, delta: i16) {
        self.anchor.y = add_y(self.anchor.y, delta);
    }
}

fn add_y(y: u16, delta: i16) -> u16 {
    if delta >= 0 {
        y.saturating_add(delta as u16)
    } else {
        y.saturating_sub(delta.unsigned_abs())
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
    let (_, y0, _, y1) = selection.normalized();
    let lines = selection_lines(rows, copyable, selection, y0, y1);
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn first_copyable_row(copyable: &[Rect], content: Rect) -> Option<u16> {
    copyable
        .iter()
        .filter(|area| {
            area.width > 0
                && area.height > 0
                && area.y >= content.y
                && area.y < content.y.saturating_add(content.height)
        })
        .map(|area| area.y)
        .min()
}

/// Copyable lines of `selection` whose screen row sits in `y_from..=y_to`.
/// One-row slices are kept; this is how autoscroll records lines that left the window.
fn selection_lines(
    rows: &[String],
    copyable: &[Rect],
    selection: &TextSelection,
    y_from: u16,
    y_to: u16,
) -> Vec<String> {
    if y_from > y_to {
        return Vec::new();
    }
    let (x0, y0, x1, y1) = selection.normalized();
    let top = y0.max(y_from);
    let bottom = y1.min(y_to);
    if top > bottom {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for y in top..=bottom {
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
    lines
}

/// Copyable text on one screen row, same trim as a drag copy.
pub fn copyable_line_at(rows: &[String], copyable: &[Rect], y: u16) -> Option<String> {
    let dummy = TextSelection::new(Position::new(0, y), Position::new(u16::MAX, y));
    selection_lines(rows, copyable, &dummy, y, y)
        .into_iter()
        .next()
}

/// Prefix (scrolled off the top) + live frame + suffix (scrolled off the bottom).
/// `origin` is the press-row title: lines before it are dropped, and a duplicate
/// of it at the live/prefix join is dropped.
pub fn compose_selection_copy(
    before: &[String],
    live: Option<String>,
    after: &[String],
    origin: Option<&str>,
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    lines.extend(before.iter().cloned());
    if let Some(live) = live {
        let live_lines: Vec<String> = live.split('\n').map(str::to_string).collect();
        if let (Some(last), Some(first)) = (lines.last(), live_lines.first()) {
            if same_copy_line(last, first) {
                lines.extend(live_lines.into_iter().skip(1));
            } else {
                lines.extend(live_lines);
            }
        } else {
            lines.extend(live_lines);
        }
    }
    lines.extend(after.iter().cloned());
    lines.retain(|line| !line.is_empty());
    if let Some(origin) = origin {
        if let Some(i) = lines.iter().position(|line| same_copy_line(line, origin)) {
            lines.drain(..i);
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn same_copy_line(a: &str, b: &str) -> bool {
    a == b || a.ends_with(b) || b.ends_with(a)
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
/// glyphs, meta) stays unhighlighted — the same content bounds the copy path
/// uses, rather than painting full terminal rows.
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
    fn one_cell_wobble_is_not_a_selection() {
        assert!(!TextSelection::new(pos(5, 5), pos(5, 5)).has_area());
        assert!(!TextSelection::new(pos(5, 5), pos(6, 5)).has_area());
        assert!(!TextSelection::new(pos(5, 5), pos(5, 6)).has_area());
        assert!(!TextSelection::new(pos(5, 5), pos(6, 6)).has_area());
        assert!(TextSelection::new(pos(5, 5), pos(7, 5)).has_area());
        assert!(TextSelection::new(pos(5, 5), pos(5, 7)).has_area());
    }

    #[test]
    fn drag_select_gesture_defers_click_until_real_drag_or_release() {
        let mut g = DragSelectGesture::new();
        let down = pos(3, 4);
        assert_eq!(
            g.handle(DragSelectPhase::Press, down, None),
            DragSelectOutcome::Continue
        );
        // One-cell wobble: still a click.
        let wobble = TextSelection::new(down, pos(4, 4));
        assert_eq!(
            g.handle(DragSelectPhase::Move, pos(4, 4), Some(wobble)),
            DragSelectOutcome::Continue
        );
        assert_eq!(
            g.handle(DragSelectPhase::Release, pos(4, 4), Some(wobble)),
            DragSelectOutcome::Click(down)
        );

        let mut g = DragSelectGesture::new();
        assert_eq!(
            g.handle(DragSelectPhase::Press, down, None),
            DragSelectOutcome::Continue
        );
        let drag = TextSelection::new(down, pos(8, 4));
        assert_eq!(
            g.handle(DragSelectPhase::Move, pos(8, 4), Some(drag)),
            DragSelectOutcome::Continue
        );
        assert_eq!(
            g.handle(DragSelectPhase::Release, pos(8, 4), Some(drag)),
            DragSelectOutcome::Copy
        );

        let mut g = DragSelectGesture::new();
        let _ = g.handle(DragSelectPhase::Press, down, None);
        g.clear();
        assert_eq!(
            g.handle(
                DragSelectPhase::Release,
                down,
                Some(TextSelection::new(down, down))
            ),
            DragSelectOutcome::Continue
        );
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
        // Partial inclusive end (two cells apart so it clears the wobble threshold).
        let sel = TextSelection::new(pos(0, 0), pos(2, 0));
        assert_eq!(
            selection_text(&rows, &copyable, &sel).as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn autoscroll_arms_near_edges_and_clears_inside() {
        let area = Rect::new(0, 10, 40, 20);
        assert!(compute_autoscroll(20, area).is_none());
        let up = compute_autoscroll(11, area).expect("near top");
        assert_eq!(up.direction, AutoScrollDirection::Up);
        assert_eq!(up.speed, 1);
        let down = compute_autoscroll(28, area).expect("near bottom");
        assert_eq!(down.direction, AutoScrollDirection::Down);
        let far = compute_autoscroll(5, area).expect("above");
        assert!(far.speed >= 2);
    }

    #[test]
    fn gesture_autoscroll_only_while_selection_has_area() {
        let mut g = DragSelectGesture::new();
        let area = Rect::new(0, 0, 40, 20);
        g.update_autoscroll(0, area, false);
        assert!(!g.has_autoscroll());
        g.update_autoscroll(0, area, true);
        assert!(g.has_autoscroll());
        let _ = g.handle(DragSelectPhase::Release, pos(0, 0), None);
        assert!(!g.has_autoscroll());
    }

    #[test]
    fn shift_anchor_keeps_the_head_on_the_pointer() {
        let mut sel = TextSelection::new(pos(4, 10), pos(8, 20));
        sel.shift_anchor_y(-3);
        assert_eq!(sel.anchor, pos(4, 7));
        assert_eq!(sel.head, pos(8, 20));
        sel.shift_anchor_y(2);
        assert_eq!(sel.anchor, pos(4, 9));
        assert_eq!(sel.head, pos(8, 20));
    }

    #[test]
    fn autoscroll_keeps_scrolled_out_copyable_lines() {
        let rows = vec![
            "....title-one..........".to_string(),
            "....title-two..........".to_string(),
            "....title-three........".to_string(),
        ];
        let copyable = vec![Rect::new(4, 0, 12, 3)];
        let sel = TextSelection::new(pos(4, 0), pos(10, 2));
        let mut g = DragSelectGesture::new();
        // One viewport row leaving, tall selection: must still keep that line
        // (the old clip required has_area and dropped single rows).
        g.capture_leaving_rows(
            &rows,
            &copyable,
            sel,
            Rect::new(0, 0, 40, 3),
            AutoScrollDirection::Down,
            1,
        );
        assert!(
            g.captured_before()
                .first()
                .is_some_and(|line| line.starts_with("title-one")),
            "start title must be kept, got {:?}",
            g.captured_before()
        );
        let live = Some("title-two\ntitle-three".to_string());
        let text = compose_selection_copy(g.captured_before(), live, g.captured_after(), None)
            .expect("stitched copy");
        assert!(
            text.starts_with("title-one"),
            "copy must keep the start title: {text:?}"
        );
        assert!(text.contains("title-three"), "{text:?}");
    }

    #[test]
    fn downward_autoscroll_captures_titles_below_a_sticky_header() {
        let rows = vec![
            "....HEADER..............".to_string(),
            "....HEADER..............".to_string(),
            "....start-title.........".to_string(),
            "....next-title..........".to_string(),
        ];
        let copyable = vec![Rect::new(4, 2, 12, 2)];
        let sel = TextSelection::new(pos(4, 2), pos(10, 3));
        let mut g = DragSelectGesture::new();
        g.capture_leaving_rows(
            &rows,
            &copyable,
            sel,
            Rect::new(0, 0, 40, 4),
            AutoScrollDirection::Down,
            1,
        );
        assert!(
            g.captured_before()
                .first()
                .is_some_and(|line| line.contains("start-title")),
            "top-to-bottom crawl must keep the start title under a sticky header, got {:?}",
            g.captured_before()
        );
    }

    #[test]
    fn compose_drops_unselected_titles_above_the_origin() {
        let text = compose_selection_copy(
            &["above".into(), "start".into(), "next".into()],
            Some("next\nbelow".into()),
            &[],
            Some("start"),
        )
        .expect("copy");
        assert_eq!(text, "start\nnext\nbelow");
        let partial = compose_selection_copy(
            &["PR18 overflow 41".into(), "overflow 40".into()],
            Some("PR18 overflow 40\nPR18 overflow 39".into()),
            &[],
            Some("overflow 40"),
        )
        .expect("partial origin");
        assert!(
            partial.starts_with("overflow 40") || partial.starts_with("PR18 overflow 40"),
            "{partial:?}"
        );
        assert!(!partial.contains("overflow 41"), "{partial:?}");
    }
}
