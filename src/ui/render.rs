//! Queue Board Renderer: mono painters and full-frame queue chrome.
//!
//! Pure helpers that take geometry + row content → ratatui `Line`/`Span` using only
//! `Modifier::{BOLD, DIM, UNDERLINED, REVERSED}`, plus [`draw_queue_frame`] for the
//! the deck-only board skeleton.

use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;
use uuid::Uuid;

use super::capture::CaptureField;
use super::edit::{place_edit_cursor, place_edit_cursor_at};
use super::present_line;
use super::queue::{
    BoardTab, QueueSection, QueueView, SectionKind, StatusCounts, ThreadProjectCollapseKey,
};
use super::scrollbar;
use super::tier::{Tier, TierGeometry};
use crate::domain::{HumanStatus, Task, TaskScope};

/// Cells reserved by every field label (`" title "`, `" notes "`, `" scope "`): all
/// three are built to the same width so the field column lines up across rows.
pub(crate) const EDIT_FIELD_LABEL_WIDTH: u16 = 9;

/// Cells left for a field's text after its label, at the frame's actual paint width.
///
/// This is the width the board must window the draft to *before* handing it to the
/// overlay: painting re-truncates from the head with `present_line`, which has no cursor
/// context, so a window computed against any other width detaches the cursor from what is
/// actually on screen.
pub(crate) fn editor_field_width(geo: &TierGeometry) -> usize {
    (geo.row_width as usize).saturating_sub(EDIT_FIELD_LABEL_WIDTH as usize)
}

/// Rows an open title/notes/capture editor may paint from `viewport_top` without writing
/// into the rule, status, or verb chrome below it.
pub(crate) fn editor_row_budget(geo: &TierGeometry) -> u16 {
    let top = geo.viewport_top;
    let mut limit = geo.height;
    for row in [geo.rule_row, geo.status_row, geo.verb_row]
        .into_iter()
        .flatten()
    {
        limit = limit.min(row);
    }
    limit.saturating_sub(top).max(1)
}

/// Modifiers allowed on the queue surfaces.
pub const MONO_MODIFIERS: Modifier = Modifier::BOLD
    .union(Modifier::DIM)
    .union(Modifier::UNDERLINED)
    .union(Modifier::REVERSED);

/// Style with only the allowed mono modifiers; never sets fg/bg.
pub fn mono_style(modifier: Modifier) -> Style {
    Style::default().add_modifier(modifier.intersection(MONO_MODIFIERS))
}

pub fn style_plain() -> Style {
    Style::default()
}

pub fn style_bold() -> Style {
    mono_style(Modifier::BOLD)
}

/// Heading body: bold and underlined, so it does not collapse into `**strong**`.
pub fn style_heading() -> Style {
    mono_style(Modifier::BOLD.union(Modifier::UNDERLINED))
}

pub fn style_dim() -> Style {
    mono_style(Modifier::DIM)
}

pub fn style_underline() -> Style {
    mono_style(Modifier::UNDERLINED)
}

pub fn style_reverse() -> Style {
    mono_style(Modifier::REVERSED)
}

pub fn style_reverse_dim() -> Style {
    mono_style(Modifier::REVERSED.union(Modifier::DIM))
}

pub fn style_reverse_bold() -> Style {
    mono_style(Modifier::REVERSED.union(Modifier::BOLD))
}

/// Logical content for one task row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRowPaint<'a> {
    pub glyph: &'a str,
    /// Presentation-only task identifier, for example `T30`. Drafts have none.
    pub identifier: Option<&'a str>,
    pub title: &'a str,
    /// Trailing age / project metadata. The task identifier belongs with the title, not here.
    pub meta: &'a str,
    pub selected: bool,
    /// Bold the title when unselected (e.g. attention emphasis).
    pub title_bold: bool,
}

/// One painted task-row line plus the title-content cells a text selection may copy.
pub struct TaskRowLine {
    pub line: Line<'static>,
    pub content_x: u16,
    pub content_width: u16,
    /// The leading identifier's exact cells, on the first line only.
    pub identifier: Option<(u16, u16)>,
}

/// Paint one task as ONE OR MORE list lines. Persisted tasks lead with a dim,
/// presentation-only identifier; wrapped title continuations align under the title text.
pub fn paint_task_row_lines(
    row: &TaskRowPaint<'_>,
    geo: &TierGeometry,
    leading_indent: usize,
) -> Vec<TaskRowLine> {
    let row_w = geo.row_width as usize;
    let meta_budget = task_meta_budget(row, geo);
    let title_budget = row_w.saturating_sub(meta_budget);
    let glyph_cells = display_width(&super::terminal_text(row.glyph));
    let prefix_cells = leading_indent + 2 + glyph_cells + 1;
    let identifier_width = row.identifier.map(display_width).unwrap_or(0);
    let identifier_gap = usize::from(identifier_width > 0);
    let title_x = prefix_cells.saturating_add(identifier_width + identifier_gap);
    let room = title_budget.saturating_sub(title_x).max(1);
    let head_content_x = u16::try_from(prefix_cells).unwrap_or(u16::MAX);
    let head_content_width = u16::try_from(identifier_width + identifier_gap + room)
        .unwrap_or(u16::MAX)
        .max(1);
    let title_content_x = u16::try_from(title_x).unwrap_or(u16::MAX);
    let title_content_width = u16::try_from(room).unwrap_or(1).max(1);
    let identifier = row.identifier.map(|_| {
        (
            head_content_x,
            u16::try_from(identifier_width).unwrap_or(u16::MAX),
        )
    });
    let segments: Vec<String> = crate::ui::edit::wrap_text(row.title, room)
        .into_iter()
        .map(|wrapped| wrapped.text)
        .collect();

    let mut lines = Vec::with_capacity(segments.len());
    let head = TaskRowPaint {
        title: &segments[0],
        ..*row
    };
    lines.push(TaskRowLine {
        line: paint_task_row_with_indent(&head, geo, leading_indent),
        content_x: head_content_x,
        content_width: head_content_width,
        identifier,
    });
    let indent = " ".repeat(title_x);
    let continuation_style = if row.selected {
        style_reverse()
    } else if row.title_bold {
        style_bold()
    } else {
        style_plain()
    };
    for segment in segments.iter().skip(1) {
        lines.push(TaskRowLine {
            line: bound_line(
                Line::from(Span::styled(
                    format!("{indent}{segment}"),
                    continuation_style,
                )),
                row_w,
            ),
            content_x: title_content_x,
            content_width: title_content_width,
            identifier: None,
        });
    }
    lines
}

/// Paint one task row: glyph+title on the left, right-aligned meta in the meta budget.
///
/// Title and meta never share cells. Over-budget text is clipped through
/// [`present_line`] so truncation always shows `…`. Compact geometry
/// (`meta_column_width == 0`) drops meta and gives the title the full row.
pub fn paint_task_row(row: &TaskRowPaint<'_>, geo: &TierGeometry) -> Line<'static> {
    paint_task_row_with_indent(row, geo, 0)
}

/// Paint a task row with extra leading cells reserved for a containing visual group.
fn paint_task_row_with_indent(
    row: &TaskRowPaint<'_>,
    geo: &TierGeometry,
    leading_indent: usize,
) -> Line<'static> {
    let row_w = geo.row_width as usize;
    let meta_budget = task_meta_budget(row, geo);
    let title_budget = row_w.saturating_sub(meta_budget);

    let glyph = super::terminal_text(row.glyph);
    let prefix = format!("{}  {glyph} ", " ".repeat(leading_indent));
    let identifier = row.identifier.unwrap_or_default();
    let identifier_gap = if identifier.is_empty() { "" } else { " " };
    let title_room = title_budget
        .saturating_sub(display_width(&prefix))
        .saturating_sub(display_width(identifier))
        .saturating_sub(display_width(identifier_gap));
    let title = present_line(row.title, title_room);
    let left_w = display_width(&prefix)
        .saturating_add(display_width(identifier))
        .saturating_add(display_width(identifier_gap))
        .saturating_add(display_width(&title));

    let margin_w = if meta_budget == 0 { 0 } else { 1 };
    let meta_content_budget = meta_budget.saturating_sub(margin_w);
    let meta = if meta_budget == 0 || row.meta.is_empty() {
        String::new()
    } else {
        present_line(row.meta, meta_content_budget)
    };
    let meta_w = display_width(&meta);
    let leader_w = if meta.is_empty() {
        row_w.saturating_sub(left_w)
    } else {
        let title_pad = title_budget.saturating_sub(left_w);
        let meta_pad = meta_content_budget.saturating_sub(meta_w);
        title_pad + meta_pad
    };
    let leader = " ".repeat(leader_w);

    let (title_style, identifier_style, leader_style, meta_style) = if row.selected {
        (
            style_reverse(),
            style_reverse_dim(),
            style_reverse_dim(),
            style_reverse_dim(),
        )
    } else {
        (
            if row.title_bold {
                style_bold()
            } else {
                style_plain()
            },
            style_dim(),
            style_dim(),
            style_dim(),
        )
    };

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
    spans.push(Span::styled(prefix, title_style));
    if !identifier.is_empty() {
        spans.push(Span::styled(identifier.to_string(), identifier_style));
        spans.push(Span::styled(identifier_gap.to_string(), title_style));
    }
    spans.push(Span::styled(title, title_style));
    if !leader.is_empty() {
        spans.push(Span::styled(leader, leader_style));
    }
    let meta_present = !meta.is_empty();
    if meta_present {
        spans.push(Span::styled(meta, meta_style));
    }
    if margin_w > 0 && meta_present {
        spans.push(Span::styled(" ".repeat(margin_w), leader_style));
    }

    bound_line(Line::from(spans), row_w)
}

fn task_meta_budget(_row: &TaskRowPaint<'_>, geo: &TierGeometry) -> usize {
    geo.meta_column_width as usize
}

/// Present untrusted text into a single mono-styled line bounded to `width` cells.
pub fn paint_bounded_line(text: &str, width: u16, style: Style) -> Line<'static> {
    let shown = present_line(text, width as usize);
    let style = strip_color(style);
    bound_line(Line::from(Span::styled(shown, style)), width as usize)
}

/// Drop any color from a style and keep only mono modifiers.
pub fn strip_color(style: Style) -> Style {
    let mut cleaned =
        Style::default().add_modifier(style.add_modifier.intersection(MONO_MODIFIERS));
    if !style.sub_modifier.is_empty() {
        cleaned = cleaned.remove_modifier(style.sub_modifier.intersection(MONO_MODIFIERS));
    }
    cleaned
}

/// Panic if `text` contains ANSI SGR foreground/background/palette color codes.
///
/// Bold (1), dim (2), underline (4), reverse (7), reset (0), and their off codes
/// are allowed. Intended for golden-frame scans.
pub fn assert_no_color_sgr(text: &str) {
    if let Some(hit) = find_color_sgr(text) {
        panic!("AC-13: forbidden SGR color sequence in frame: {hit:?}");
    }
}

/// Panic if any cell carries a non-reset color or a modifier outside the mono set.
pub fn assert_buffer_mono(buffer: &Buffer) {
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            assert!(
                cell.fg == Color::Reset,
                "AC-13: cell ({x},{y}) has fg color {:?}",
                cell.fg
            );
            assert!(
                cell.bg == Color::Reset,
                "AC-13: cell ({x},{y}) has bg color {:?}",
                cell.bg
            );
            let extra = cell.modifier.difference(MONO_MODIFIERS);
            assert!(
                extra.is_empty(),
                "AC-13: cell ({x},{y}) has non-mono modifiers {extra:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Full-frame queue board
// ---------------------------------------------------------------------------

/// One verb-bar entry (key chord + short label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerbEntry<'a> {
    pub key: &'a str,
    pub label: &'a str,
}

/// One palette command row for overlay paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCommandRow<'a> {
    pub label: &'a str,
    pub selected: bool,
}

/// One steps step as the task page paints it: done flag + text, already extracted
/// from storage by the view model. The page payload consumes these views and never the
/// raw `Task.steps`, so later surfaces swap consumers without touching storage.
#[derive(Debug, Clone)]
pub struct StepView {
    pub done: bool,
    /// Word-boundary wrapped text rows. The first carries the step glyph, later rows align
    /// under its text so step content never truncates at the page edge.
    pub rows: Vec<String>,
}

/// The task page's in-place step editor. Its rows replace the selected stored step's text, or
/// follow the stored rows while an add is pending, so the editor never takes over the footer.
#[derive(Debug, Clone)]
pub struct InlineStepEditor<'a> {
    pub index: usize,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub refusal: Option<&'a str>,
}

/// A focused one-line input in the board's shared bottom slot. The slot reserves
/// breathing rows around the status-row input, places the cursor after the two-cell
/// prompt, and keeps any refusal on the line itself. New one-line capture surfaces
/// supply their text, placeholder, and optional message rather than creating their
/// own footer geometry.
#[derive(Debug, Clone)]
pub struct BottomInputSlot<'a> {
    pub text: String,
    pub cursor_col: u16,
    pub placeholder: &'static str,
    pub refusal: Option<&'a str>,
    /// A contextual refusal that needs its own row above the input. Empty-text
    /// refusals belong in `refusal` so they never cover the cursor.
    pub message: Option<&'a str>,
    /// Wrapped continuation rows painting ABOVE the input line, top row first.
    /// Empty for single-line drafts; a multiline draft also suppresses `message`,
    /// which shares those rows.
    pub above_rows: Vec<String>,
    /// Vertical offset of the terminal caret above the input line (0 = on it).
    pub cursor_row_offset: u16,
}

impl<'a> BottomInputSlot<'a> {
    /// The single-line shape every existing editor paints.
    pub fn single_line(text: String, cursor_col: u16, placeholder: &'static str) -> Self {
        Self {
            text,
            cursor_col,
            placeholder,
            refusal: None,
            message: None,
            above_rows: Vec::new(),
            cursor_row_offset: 0,
        }
    }
}

/// Transient overlay painted above the queue frame (palette, help, scope dropdown).
/// Variant payloads live one per frame and rebuild each paint, so the size
/// difference between them is not worth boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default)]
pub enum QueueOverlay<'a> {
    #[default]
    None,
    /// Searchable command palette (`:`).
    Palette {
        query: &'a str,
        commands: &'a [PaletteCommandRow<'a>],
    },
    /// Help card (`?`).
    Help { lines: &'a [String] },
    /// Project-scope dropdown from the selector chip.
    ScopeDropdown {
        options: &'a [String],
        selected: usize,
    },
    /// Inline title edit surface (accordion row on standard; full takeover on compact).
    EditTitle { draft: String, cursor_col: u16 },
    /// Full-width notes editor (standard or compact takeover).
    EditNotes {
        /// Pre-wrapped lines for the draft (label already accounted by caller).
        rows: Vec<String>,
        cursor_row: u16,
        cursor_col: u16,
    },
    /// Single-line capture painted into the two bottom chrome rows, never covering the queue.
    QuickAdd {
        input: BottomInputSlot<'a>,
        project_scope: bool,
        recovery: bool,
    },
    /// An inert task surface shown when wide split has no selected task.
    TaskEmpty,
    /// The task page: a full-height, view-first takeover for one bound task. `focus` is
    /// `None` in view mode; field edits focus the same drafts the board form carries.
    TaskPage {
        /// The title's wrapped rows. Row 0 carries the status glyph; later rows are
        /// bare segments the painter indents under it. View mode wraps the stored
        /// title; edit mode wraps the draft.
        header_rows: Vec<String>,
        /// Dim leading identifier on a persisted task page in view mode only.
        header_identifier: Option<String>,
        /// Task the header identifier refers to, kept separate from paint text for hit testing.
        header_identifier_task: Option<Uuid>,
        /// Terminal cursor (row, col) inside `header_rows` while the title is edited.
        title_cursor: Option<(u16, u16)>,
        /// The task's status word, dim and right-aligned on the header row.
        status_word: &'static str,
        /// View mode: the wrapped notes windowed by the page scroll. Edit mode: the
        /// cursor-windowed draft rows.
        notes_rows: Vec<String>,
        /// Notes cursor (row, col) while the notes are being edited.
        notes_cursor: Option<(u16, u16)>,
        /// Wrapped note rows hidden below the window, named by the divider's tail.
        more_lines: usize,
        /// The extracted steps step views, in storage order. An inline add contributes a
        /// final transient view, while `stored_step_count` stays tied to persisted steps.
        step_views: Vec<StepView>,
        /// Persisted step count, excluding an inline unsaved add draft.
        stored_step_count: usize,
        /// Absolute index of the step cursor's row, when active. The painter turns it
        /// into the row's `▸` gutter marker.
        step_cursor: Option<usize>,
        /// Whether the trailing `+ step` control owns selection.
        step_add_selected: bool,
        /// First step index the section's window shows (the cursor's scroll window).
        step_scroll: usize,
        /// Absolute index of the step the delete verb visibly marked, when armed.
        step_marked: Option<usize>,
        /// The page's in-place add/rename step draft, if one is active.
        inline_step_editor: Option<InlineStepEditor<'a>>,
        /// The thread field still uses the shared bottom input slot.
        bottom_input: Option<BottomInputSlot<'a>>,
        /// Footer: task number · scope · thread · created · updated.
        meta: String,
        /// Display width before the scope inside `meta`. The number is chrome, not a scope hit.
        meta_scope_x: u16,
        /// Display width of scope inside `meta`, carried separately so mouse geometry never
        /// parses user-controlled project names from rendered text.
        meta_scope_width: u16,
        /// Display width of the rendered Thread segment, including its separator.
        thread_slot_width: Option<u16>,
        /// Which field owns the cursor, if any (view mode: none).
        focus: Option<CaptureField>,
        scope_dropdown: Option<FormScopeDropdown<'a>>,
    },
}

/// Form-scope chooser state embedded in its parent form overlay. Its options intentionally
/// carry `TaskScope` labels only, never the board selector's session-only all-projects value.
#[derive(Debug, Clone, Copy)]
pub struct FormScopeDropdown<'a> {
    pub options: &'a [String],
    pub selected: usize,
}

/// Pure paint input for one queue frame. No app-loop state machines.
#[derive(Debug, Clone)]
pub struct QueueFrameModel<'a> {
    /// Task snapshot (lookups by id; soft-deleted rows are already absent from `view`).
    pub tasks: &'a [Task],
    /// Precomputed sections + status-line counts.
    pub view: &'a QueueView,
    /// Selected task id, if any.
    pub selection_id: Option<Uuid>,
    /// Home tabs are visible (project focus hides them).
    pub at_home: bool,
    /// Active home tab.
    pub home_tab: BoardTab,
    /// Project-focus label on the selector row's right chip.
    pub scope_label: &'a str,
    /// Collapsed project groups on the Projects tab.
    pub collapsed_projects: &'a HashSet<String>,
    /// Collapsed thread groups on the Threads tab.
    pub collapsed_threads: &'a HashSet<String>,
    /// Collapsed project rows under a thread group.
    pub collapsed_thread_projects: &'a HashSet<ThreadProjectCollapseKey>,
    /// Optional status-line notice; replaces the default counts when set.
    pub status_message: Option<&'a str>,
    /// Column offset of the delete-notice `u Undo` control inside `status_message`, when
    /// the notice's own composition put one there: the caller
    /// computes this from the same composition that built `status_message` (see
    /// `board.rs`'s `notice_framed`), so the hit region is never re-derived by searching
    /// `status_message` itself, which is partly user text (the deleted task's title).
    pub status_undo_offset: Option<usize>,
    /// Verb-bar entries (trimmed to the geometry budget at paint time).
    pub verb_items: &'a [VerbEntry<'a>],
    /// Clock for age labels (tests inject a fixed instant).
    pub now: SystemTime,
    /// Optional transient overlay (palette / help / scope dropdown).
    pub overlay: QueueOverlay<'a>,
    /// Session-only read-detail accordion/takeover target (Enter), independent of `overlay`.
    /// Standard: expands inline under the selected row. Compact: full-viewport takeover.
    /// Only painted while `overlay` is `QueueOverlay::None`.
    pub detail_open: Option<Uuid>,
    /// List viewport offset. Scrollbar and the last paint persist this; row click leaves it.
    pub list_scroll: usize,
    /// Nudge `list_scroll` so the selection (or its peek) stays on screen.
    pub follow_list: bool,
}

/// Logical control under a painted rectangle (rebuilt every frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueHitTarget {
    ProjectChip,
    /// The quick-add input row. Clicking it keeps the already-focused line focused.
    QuickAddInput,
    Task(Uuid),
    /// Dim presentation-only identifier on a persisted task.
    TaskNumber(Uuid),
    Verb(usize),
    /// The DONE section header (painted only while the drawer is open): toggles it shut,
    /// the mouse path onto the same [`BoardIntent::ToggleDoneDrawer`] `z` dispatches
    ///. There is no click to *open* the drawer: closed, it paints no header
    /// row at all, so the palette's "done drawer" command and `z` stay the only routes in.
    ///
    /// [`BoardIntent::ToggleDoneDrawer`]: crate::ui::input::BoardIntent::ToggleDoneDrawer
    Drawer,
    /// One painted row of the open palette, indexed exactly as
    /// `BoardModel::visible_commands()` orders them, so a click resolves straight to that
    /// command's own intent ( residual: mapped through the model's command list, never
    /// a fixed table, the same discipline `Verb` already follows).
    Command(usize),
    /// One painted row of the open project-scope dropdown, indexed exactly as
    /// `BoardModel::project_options()` orders them.
    ProjectOption(usize),
    /// One painted home tab on the selector row.
    HomeTab(BoardTab),
    /// One ON DECK project-group header on the Projects tab, indexed into sections.
    SectionProject(usize),
    /// One thread-group header on the Threads tab, indexed into sections.
    SectionThread(usize),
    /// One project sub-header under a thread group.
    SectionThreadProject {
        section_idx: usize,
        subgroup_idx: usize,
    },
    /// The open help card's full-frame dismiss hit -- a click anywhere the card's own
    /// `ModalClose`/`ModalChrome`/body hits do not shadow closes it, matching the
    /// keyboard's "any key closes". The board behind the card stays visible and painted;
    /// only [`paint_modal_card`]'s own rect is cleared and repainted.
    HelpDismiss,
    /// Shared-form title row. A click focuses Title without recreating the form.
    FormTitle,
    /// One painted shared-form Notes row. Any row focuses Notes; its index keeps the hit-map
    /// tied to the cursor-windowed row the renderer actually painted.
    FormNotes(usize),
    /// Shared-form scope row. A click opens the pending scope dropdown, never cycles scope.
    FormScope,
    /// Shared-form thread portion of the task-page footer.
    FormThread,
    /// One painted steps step row on the open task page, indexed by the step's
    /// absolute position in the task's steps (storage order), whatever window
    /// scroll painted it — the same absolute-index discipline [`Command`] follows.
    /// A click moves the step cursor onto that step (AC-21): select, never toggle.
    Step(usize),
    /// The dim trailing task-page control that starts a new inline step.
    StepAdd,
    /// One painted option in a shared form's scope dropdown, indexed into that form's own
    /// `TaskScope` choices. It cannot name the board selector's all-projects choice.
    FormScopeOption(usize),
    /// One cell of the board list's overflow scrollbar (track or thumb). The usize is the
    /// content offset that cell jumps the viewport to. Does not change selection or peek.
    ListScroll(usize),
    /// One cell of the task-page body scrollbar. The usize is the notes/steps offset.
    PageScroll(usize),
    /// The `u Undo` control inside the delete-recovery notice on the status line (I2,
    /// round 2), positioned wherever [`paint_status_line`] actually put it -- which shifts
    /// with the deleted title's length and with whether a later message is composed after
    /// it -- rather than a fixed column. Dispatches the same [`BoardIntent::Undo`] the `u`
    /// key does; only painted (and so only ever pushed) while `model.input_mode()` is
    /// `Normal`, the one mode `map_board_mouse` reads this target in.
    ///
    /// [`BoardIntent::Undo`]: crate::ui::input::BoardIntent::Undo
    DeleteNoticeUndo,
    /// The command surface's own painted chrome -- its `command` header row, the query
    /// row, and the gap rows between command rows -- that is *not* one of its rows (I4,
    /// ). A click here is deliberately inert (it neither runs a command nor
    /// dismisses the surface), matching the pre-rewrite behavior a click on the palette's
    /// own furniture had; only a click genuinely outside the whole surface closes it.
    CommandChrome,
    /// The shared modal card's `[x]` close control (Help / Palette / project-scope
    /// picker; [`paint_modal_card`]). Dismisses whatever overlay painted it -- the same
    /// effect the surface's own Esc/close route already has.
    ModalClose,
    /// The shared modal card's own chrome -- its border, title row, and footer rule +
    /// legend -- that is neither `ModalClose` nor the caller's own body hit. A click here
    /// is deliberately inert, the same rule `CommandChrome` already applies to the
    /// palette's pre-card furniture.
    ModalChrome,
}

/// One hit-testable region produced beside the paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueHit {
    pub target: QueueHitTarget,
    pub area: Rect,
}

/// Fresh mouse hit-map for one frame (never reused across resize).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueHitMap {
    pub regions: Vec<QueueHit>,
    /// Rects the painters declared as copyable content (task rows, page title,
    /// notes, steps, drawer body, options). Text selection copies only cells
    /// inside these, so chrome — scrollbars, borders, verb bars, dividers — is
    /// excluded by construction.
    pub copyable: Vec<Rect>,
}

impl QueueHitMap {
    fn push(&mut self, target: QueueHitTarget, area: Rect) {
        if area.width > 0 && area.height > 0 {
            self.regions.push(QueueHit { target, area });
        }
    }

    /// Declare a rect as copyable content, beside the paint that drew it.
    pub fn push_copyable(&mut self, area: Rect) {
        if area.width > 0 && area.height > 0 {
            self.copyable.push(area);
        }
    }

    fn translate_and_clip(&mut self, area: Rect) {
        for hit in &mut self.regions {
            hit.area = local_rect(area, hit.area);
        }
        self.regions
            .retain(|hit| hit.area.width > 0 && hit.area.height > 0);
        for copyable in &mut self.copyable {
            *copyable = local_rect(area, *copyable);
        }
        self.copyable
            .retain(|copyable| copyable.width > 0 && copyable.height > 0);
    }
}

fn clipped_area(area: Rect, frame: Rect) -> Rect {
    let x = area.x.max(frame.x);
    let y = area.y.max(frame.y);
    let right = u32::from(area.x)
        .saturating_add(u32::from(area.width))
        .min(u32::from(frame.x).saturating_add(u32::from(frame.width)));
    let bottom = u32::from(area.y)
        .saturating_add(u32::from(area.height))
        .min(u32::from(frame.y).saturating_add(u32::from(frame.height)));
    Rect::new(
        x,
        y,
        u16::try_from(right.saturating_sub(u32::from(x))).unwrap_or(0),
        u16::try_from(bottom.saturating_sub(u32::from(y))).unwrap_or(0),
    )
}

fn local_rect(area: Rect, local: Rect) -> Rect {
    let x = local.x.min(area.width);
    let y = local.y.min(area.height);
    Rect::new(
        area.x.saturating_add(x),
        area.y.saturating_add(y),
        local.width.min(area.width.saturating_sub(x)),
        local.height.min(area.height.saturating_sub(y)),
    )
}

/// Status glyph for a human status (the: static; no agent spin).
pub fn status_glyph(status: HumanStatus) -> &'static str {
    match status {
        HumanStatus::Ready => "○",
        HumanStatus::Started => "▸",
        HumanStatus::Blocked => "■",
        HumanStatus::Review => "▲",
        HumanStatus::Done => "✓",
    }
}

/// Paint the the queue frame: selector, list, rule, status, verb bar.
///
/// Returns the hit-map recorded beside the paint, plus the list viewport offset
/// used this frame (`None` when the list was not painted).
pub fn draw_queue_frame(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
) -> (QueueHitMap, Option<(usize, usize)>) {
    draw_queue_frame_impl(frame, model, geo, surface, false)
}

/// Paint the stage G rail: the board list at rail width, no meta column, no done drawer,
/// wrapped titles with a four-cell continuation indent, and every cell dimmed. The selected
/// row paints a hollow `▹` marker instead of reverse video. `geo` is a footer-less column
/// geometry ([`crate::ui::tier::resolve_column`]); the shared footer paints separately.
pub fn draw_rail_frame(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
) -> QueueHitMap {
    let (hits, _) = draw_queue_frame_impl(frame, model, geo, surface, true);
    let surface = clipped_area(surface, frame.area());
    let buffer = frame.buffer_mut();
    for y in surface.top()..surface.bottom() {
        for x in surface.left()..surface.right() {
            let cell = &mut buffer[(x, y)];
            let modifier = cell
                .modifier
                .difference(Modifier::BOLD)
                .union(Modifier::DIM);
            cell.set_style(Style::default().add_modifier(modifier));
        }
    }
    hits
}

/// Paint only the shared footer (rule, status, verb bar) of a wide frame across `surface`.
///
/// `model.overlay` names the surface that owns the footer this frame: a bottom input paints
/// on the status row, a modal card blanks the verb bar, and the palette's query row lands
/// here rather than inside its column. `hint` is the dim stage crumb on the status row.
pub fn draw_queue_footer(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
    hint: Option<StatusHint<'_>>,
) -> QueueHitMap {
    let surface = clipped_area(surface, frame.area());
    let mut hits = QueueHitMap::default();
    if geo.row_width == 0 || geo.height == 0 {
        return hits;
    }
    paint_footer(frame, model, geo, surface, &mut hits, hint, true);
    hits.translate_and_clip(surface);
    hits
}

/// Header rule of the wide task column: `T12 title ──── started · tsk` on the selector row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskColumnHeader<'a> {
    /// Status glyph (`▸`, `○`, …) restored before the identifier.
    pub glyph: &'a str,
    /// Dim `T<number>` prefix on a persisted task; drafts have none.
    pub identifier: Option<&'a str>,
    /// Task the identifier copies, for its hit region.
    pub identifier_task: Option<Uuid>,
    /// Title text (the draft, windowed, while the title is being edited).
    pub title: &'a str,
    /// Caret column inside `title` while the title editor is active.
    pub title_cursor_col: Option<u16>,
    /// Right state slot: `status · project`, `editing <field>`, or `unsaved`.
    pub state: &'a str,
    /// BOLD in the task-owned stages, DIM for the stage A preview.
    pub bold: bool,
}

/// Paint the wide task column: the header rule on the selector row, then the task page
/// body (or the empty-pane hint), then any modal card the column hosts. The overlay in
/// `model` is the column's own content ([`QueueOverlay::TaskPage`] or
/// [`QueueOverlay::TaskEmpty`]); `header` is `None` exactly when there is no task.
pub fn draw_task_column(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
    header: Option<TaskColumnHeader<'_>>,
    modal: Option<&QueueOverlay<'_>>,
) -> QueueHitMap {
    let surface = clipped_area(surface, frame.area());
    let mut hits = QueueHitMap::default();
    let width = geo.row_width;
    if width == 0 || geo.height == 0 {
        return hits;
    }
    frame.render_widget(Clear, surface);
    let lay = task_column_layout(geo);
    let header_row = lay.title_y;
    let weight = |bold: bool| if bold { style_bold() } else { style_dim() };
    let rule_y = header_row.saturating_add(1);
    match header {
        Some(header) => {
            // Title row: glyph, identifier, title, and the right state slot. The dash rule
            // lives on the row under it, and the body starts below that.
            let state = format!("{} ", header.state);
            let state_w = display_width(&state);
            let glyph_w = display_width(header.glyph);
            let identifier = header.identifier.unwrap_or_default();
            let identifier_gap = if identifier.is_empty() { "" } else { " " };
            let lead_w =
                1 + glyph_w + 1 + display_width(identifier) + display_width(identifier_gap);
            // Chrome cap, not task text: the title yields to one gap cell and the state.
            let title_room = (width as usize)
                .saturating_sub(lead_w)
                .saturating_sub(1)
                .saturating_sub(state_w);
            let title = present_line(header.title, title_room);
            let title_w = display_width(&title);
            let pad_w = (width as usize).saturating_sub(lead_w + title_w + state_w);
            let spans = vec![
                Span::styled(" ".to_string(), style_plain()),
                Span::styled(header.glyph.to_string(), weight(header.bold)),
                Span::styled(" ".to_string(), weight(header.bold)),
                Span::styled(identifier.to_string(), style_dim()),
                Span::styled(identifier_gap.to_string(), weight(header.bold)),
                Span::styled(title.clone(), weight(header.bold)),
                Span::styled(" ".repeat(pad_w), style_plain()),
                Span::styled(state, style_dim()),
            ];
            put_line(
                frame,
                surface,
                header_row,
                width,
                bound_line(Line::from(spans), width as usize),
            );
            if rule_y < lay.bottom {
                put_line(frame, surface, rule_y, width, paint_rule_row(width));
            }
            if let (Some(task), false) = (header.identifier_task, identifier.is_empty()) {
                hits.push(
                    QueueHitTarget::TaskNumber(task),
                    Rect::new(
                        u16::try_from(1 + glyph_w + 1).unwrap_or(u16::MAX),
                        header_row,
                        u16::try_from(display_width(identifier)).unwrap_or(u16::MAX),
                        1,
                    ),
                );
            }
            let title_x = u16::try_from(lead_w).unwrap_or(u16::MAX);
            if title_w > 0 {
                hits.push_copyable(Rect::new(
                    title_x,
                    header_row,
                    u16::try_from(title_w).unwrap_or(u16::MAX),
                    1,
                ));
            }
            hits.push(
                QueueHitTarget::FormTitle,
                Rect::new(0, header_row, width, 1),
            );
            if let Some(col) = header.title_cursor_col {
                place_edit_cursor_at(
                    frame,
                    local_rect(
                        surface,
                        Rect::new(title_x, header_row, width.saturating_sub(title_x), 1),
                    ),
                    0,
                    col.min(width.saturating_sub(title_x).saturating_sub(1)),
                );
            }
        }
        None => {
            put_line(
                frame,
                surface,
                header_row,
                width,
                bound_line(
                    Line::from(Span::styled(" no task".to_string(), style_dim())),
                    width as usize,
                ),
            );
            if rule_y < lay.bottom {
                put_line(frame, surface, rule_y, width, paint_rule_row(width));
            }
            if lay.notes_y < lay.bottom {
                put_line(
                    frame,
                    surface,
                    lay.notes_y,
                    width,
                    paint_bounded_line("  select a task to preview it here", width, style_dim()),
                );
            }
        }
    }
    if let QueueOverlay::TaskPage {
        ref header_rows,
        ref header_identifier,
        header_identifier_task,
        ref title_cursor,
        status_word,
        ref notes_rows,
        notes_cursor,
        more_lines,
        ref step_views,
        stored_step_count,
        step_cursor,
        step_add_selected,
        step_scroll,
        step_marked,
        ref inline_step_editor,
        bottom_input: _,
        ref meta,
        meta_scope_x,
        meta_scope_width,
        thread_slot_width,
        focus,
        scope_dropdown,
    } = model.overlay
    {
        paint_task_page(
            frame,
            geo,
            surface,
            header_rows,
            header_identifier.as_deref(),
            header_identifier_task,
            *title_cursor,
            status_word,
            notes_rows,
            notes_cursor,
            more_lines,
            step_views,
            stored_step_count,
            step_cursor,
            step_add_selected,
            step_scroll,
            step_marked,
            inline_step_editor.as_ref(),
            meta,
            meta_scope_x,
            meta_scope_width,
            thread_slot_width,
            focus,
            bottom_input_slot(&model.overlay).is_some(),
            true,
            &mut hits,
        );
        if let Some(dropdown) = scope_dropdown {
            paint_page_scope_dropdown(frame, geo, surface, dropdown, &mut hits);
        }
    }
    if let Some(modal) = modal {
        paint_overlay(frame, modal, geo, surface, &mut hits);
    }
    hits.translate_and_clip(surface);
    hits
}

/// Whether this overlay paints a shared bottom input on the status row.
pub fn has_bottom_input(overlay: &QueueOverlay<'_>) -> bool {
    bottom_input_slot(overlay).is_some()
}

/// Dim right-aligned stage crumb and key hints on the wide status row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusHint<'a> {
    /// `board ▸ task` style crumb, dropped first when the row is short.
    pub crumb: Option<&'a str>,
    /// The stage keys that apply right now, dropped after the crumb.
    pub keys: &'a str,
}

fn draw_queue_frame_impl(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
    rail: bool,
) -> (QueueHitMap, Option<(usize, usize)>) {
    let surface = clipped_area(surface, frame.area());
    let input_slot_geo = bottom_input_slot_geometry(*geo, &model.overlay);
    let geo = &input_slot_geo;
    let mut hits = QueueHitMap::default();
    let mut painted_list_scroll = None;
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return (hits, None);
    }

    // Clear the surface so leftover cells never leak chrome between sizes.
    let full = local_rect(surface, Rect::new(0, 0, width, height));
    frame.render_widget(Paragraph::new(Line::from("")), full);

    // The task page owns the whole surface above the bottom chrome: the selector row stays
    // hidden while it is open (list navigation does not apply to a single-task surface).
    let page_active = matches!(
        model.overlay,
        QueueOverlay::TaskPage { .. } | QueueOverlay::TaskEmpty
    );
    if let Some(row) = geo.selector_row {
        if !page_active {
            let (line, regions) = paint_selector_row(model, geo);
            put_line(frame, surface, row, width, line);
            for (target, x, w) in regions {
                hits.push(target, Rect::new(x, row, w, 1));
            }
        }
    }

    // Task pages replace the viewport entirely. Accordion detail remains inline, so its task
    // rows stay interactive in both tiers.
    let base_list_interactive = true;

    if geo.viewport_height > 0 && !page_active {
        // First pass measures overflow at the full row width; when a scrollbar is
        // needed, rebuild at the narrowed content width so wrapped titles and the
        // thumb share one consistent row count.
        let (mut list_rows, mut anchor_last_idx, mut selected_idx) =
            build_list_rows(model, geo, rail);
        let top = geo.viewport_top;
        let viewport_h = geo.viewport_height as usize;
        let (_, provisional_track) =
            scrollbar::split_for_scrollbar(0, top, width, geo.viewport_height, list_rows.len());
        let list_geo = if provisional_track.is_some() {
            let narrowed =
                geo.with_row_width(width.saturating_sub(scrollbar::SCROLLBAR_RESERVE_COLS));
            let rebuilt = build_list_rows(model, &narrowed, rail);
            list_rows = rebuilt.0;
            anchor_last_idx = rebuilt.1;
            selected_idx = rebuilt.2;
            narrowed
        } else {
            *geo
        };
        let content_width = list_geo.row_width;
        let (_, track) =
            scrollbar::split_for_scrollbar(0, top, width, geo.viewport_height, list_rows.len());
        // Row click and scrollbar drag persist list_scroll and leave the viewport
        // where it is. Keyboard selection sets follow_list so a row that walked
        // off-screen is nudged back, without parking the peek at the bottom.
        let follow_idx = anchor_last_idx.or(selected_idx);
        let has_headers = list_rows.iter().any(ListRow::is_sticky_header);
        let min_content = if has_headers {
            viewport_h.saturating_sub(2).max(1)
        } else {
            viewport_h
        };
        let max_scroll = list_rows.len().saturating_sub(min_content);
        let mut scroll = model.list_scroll.min(max_scroll);
        if model.follow_list {
            let pin = if model.detail_open.is_some() && model.detail_open == model.selection_id {
                selected_idx
            } else {
                follow_idx
            };
            if let Some(idx) = pin {
                let visible_h = viewport_h
                    .saturating_sub(header_chrome_rows(&list_rows, scroll))
                    .max(1);
                if idx < scroll {
                    scroll = idx;
                } else if idx >= scroll.saturating_add(visible_h) {
                    scroll = idx
                        .saturating_add(1)
                        .saturating_sub(min_content)
                        .min(max_scroll);
                }
            }
        }
        let sticky = sticky_header_at(&list_rows, scroll);
        let first_is_header = list_rows.get(scroll).is_some_and(ListRow::is_sticky_header);
        let mut y = top;
        if sticky.is_some() || first_is_header {
            put_line(frame, surface, y, content_width, Line::from(""));
            y = y.saturating_add(1);
        }
        if let Some(header_idx) = sticky {
            paint_list_row(
                frame,
                surface,
                &mut hits,
                &list_rows[header_idx],
                y,
                content_width,
                base_list_interactive,
            );
            y = y.saturating_add(1);
        }
        let content_h = viewport_h.saturating_sub((y - top) as usize);
        for (offset, list_row) in list_rows.iter().skip(scroll).take(content_h).enumerate() {
            paint_list_row(
                frame,
                surface,
                &mut hits,
                list_row,
                y.saturating_add(offset as u16),
                content_width,
                base_list_interactive,
            );
        }
        if let Some(track) = track {
            let total = list_rows.len();
            scrollbar::paint(frame, local_rect(surface, track), scroll, total);
            if base_list_interactive {
                let zone = scrollbar::grab_zone(track);
                for row in 0..track.height {
                    let jump = scrollbar::click_to_offset(row, track.height, total, min_content);
                    hits.push(
                        QueueHitTarget::ListScroll(jump),
                        Rect::new(zone.x, track.y.saturating_add(row), zone.width, 1),
                    );
                }
            }
        }
        painted_list_scroll = Some((scroll, max_scroll));
    }

    paint_footer(frame, model, geo, surface, &mut hits, None, false);

    paint_overlay(frame, &model.overlay, geo, surface, &mut hits);
    hits.translate_and_clip(surface);

    (hits, painted_list_scroll)
}

/// Rule, status and verb rows. `shared` marks the wide footer, which also owns the palette
/// query row its column can no longer paint.
fn paint_footer(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    surface: Rect,
    hits: &mut QueueHitMap,
    hint: Option<StatusHint<'_>>,
    shared: bool,
) {
    let width = geo.row_width;
    if let Some(row) = geo.rule_row {
        put_line(frame, surface, row, width, paint_rule_row(width));
    }

    if let Some(row) = geo.status_row {
        if let Some(input) = bottom_input_slot(&model.overlay) {
            // Every slot has the same reserved row above the cursor line. A surface
            // can use it for a contextual refusal without replacing its input.
            if let Some(message_row) = row
                .checked_sub(1)
                .filter(|message_row| geo.rule_row.is_some_and(|rule_row| *message_row > rule_row))
            {
                if let Some(message) = input.message {
                    paint_bottom_input_message(frame, surface, message_row, width, message);
                }
            }
            paint_bottom_input_slot(frame, surface, row, width, input);
            if matches!(model.overlay, QueueOverlay::QuickAdd { .. }) {
                hits.push(QueueHitTarget::QuickAddInput, Rect::new(0, row, width, 1));
            }
        } else {
            let (line, undo_hit) = paint_status_line(
                model.status_message,
                model.status_undo_offset,
                model.view.counts,
                width,
                hint,
            );
            put_line(frame, surface, row, width, line);
            if let Some((x, w)) = undo_hit {
                hits.push(QueueHitTarget::DeleteNoticeUndo, Rect::new(x, row, w, 1));
            }
            if shared {
                if let QueueOverlay::Palette { query, .. } = &model.overlay {
                    paint_palette_query(frame, surface, row, width, query, hits);
                }
            }
        }
    }

    if let Some(row) = geo.verb_row {
        let budget = geo.verb_bar_entry_budget as usize;
        let verb_items = match &model.overlay {
            // The modal card painted for each of these owns its own footer legend now;
            // the board's verb bar stays blank underneath it.
            QueueOverlay::Palette { .. }
            | QueueOverlay::Help { .. }
            | QueueOverlay::ScopeDropdown { .. } => &[],
            QueueOverlay::QuickAdd { recovery, .. } if *recovery => &[],
            QueueOverlay::QuickAdd { .. } => QUICK_ADD_VERBS,
            QueueOverlay::TaskEmpty => &[],
            // The page's field edits keep the form legends; its view mode reads the
            // model-computed page verbs (status-dependent, like the board row's own).
            QueueOverlay::TaskPage {
                focus,
                scope_dropdown,
                ..
            } => match focus {
                Some(focus) => form_verb_items(*focus, scope_dropdown.is_some()),
                None => model.verb_items,
            },
            // Retained only for direct renderer fixtures. Live task edits use TaskForm.
            QueueOverlay::EditTitle { .. } => EDIT_TITLE_VERBS,
            QueueOverlay::EditNotes { .. } => EDIT_NOTES_VERBS,
            QueueOverlay::None => model.verb_items,
        };
        let prefix_verbs = matches!(
            model.overlay,
            QueueOverlay::None | QueueOverlay::TaskPage { focus: None, .. },
        );
        if let QueueOverlay::QuickAdd {
            project_scope,
            recovery: true,
            ..
        } = &model.overlay
        {
            paint_quick_add_hint(
                frame,
                surface,
                row,
                width,
                *project_scope,
                model.status_message,
                geo.tier,
            );
        } else {
            let (line, verb_hits) = paint_verb_bar(verb_items, budget, width, prefix_verbs);
            put_line(frame, surface, row, width, line);
            for (index, x, w) in verb_hits {
                hits.push(QueueHitTarget::Verb(index), Rect::new(x, row, w, 1));
            }
        }
    }
}

/// Reserve breathing room around a shared bottom input by taking two rows from the list.
///
/// At the 40×10 operating floor this leaves four list rows, so both blank rows remain. On
/// shorter frames with fewer than two viewport rows we retain the ordinary compact geometry:
/// functional chrome wins over decorative spacing.
fn bottom_input_slot_geometry(geo: TierGeometry, overlay: &QueueOverlay<'_>) -> TierGeometry {
    bottom_input_geometry(geo, bottom_input_slot(overlay).is_some())
}

/// Return the frame geometry after reserving a shared bottom input slot. Payload
/// builders use this too, so their wrapped content has the same row budget as the
/// renderer that eventually paints it.
pub(crate) fn bottom_input_geometry(mut geo: TierGeometry, active: bool) -> TierGeometry {
    if !active || geo.viewport_height < 2 {
        return geo;
    }

    geo.viewport_height -= 2;
    geo.rule_row = geo.rule_row.map(|row| row.saturating_sub(2));
    geo.status_row = geo.status_row.map(|row| row.saturating_sub(1));
    geo
}

/// Extract the input supplied by either current user of the shared bottom slot.
/// Keeping this seam beside the geometry prevents a new capture surface from
/// accidentally reserving rows differently from the line it paints.
fn bottom_input_slot<'a>(overlay: &'a QueueOverlay<'a>) -> Option<&'a BottomInputSlot<'a>> {
    match overlay {
        QueueOverlay::QuickAdd { input, .. } => Some(input),
        QueueOverlay::TaskPage { bottom_input, .. } => bottom_input.as_ref(),
        _ => None,
    }
}

pub(crate) const QUICK_ADD_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "enter",
        label: "save",
    },
    VerbEntry {
        key: "shift+enter",
        label: "save+next",
    },
    VerbEntry {
        key: "tab",
        label: "details",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
];

pub(crate) const PALETTE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "enter",
        label: "run",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
    VerbEntry {
        key: "type",
        label: "to filter",
    },
];

/// Shared-form verb rows. The renderer and mouse mapper both read these exact arrays, so a
/// painted Save or Cancel control cannot promise a key route different from the one it sends.
const FORM_TITLE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "shift+enter",
        label: "save",
    },
    VerbEntry {
        key: "tab",
        label: "next",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];
const FORM_NOTES_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "shift+enter",
        label: "save",
    },
    VerbEntry {
        key: "tab",
        label: "next",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];
const FORM_THREAD_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "shift+enter",
        label: "save",
    },
    VerbEntry {
        key: "tab",
        label: "next",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];
const FORM_SCOPE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "enter",
        label: "scopes",
    },
    VerbEntry {
        key: "space",
        label: "cycle",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];
const FORM_SCOPE_DROPDOWN_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "j/k",
        label: "choose",
    },
    VerbEntry {
        key: "enter",
        label: "scope",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
];

pub(crate) fn form_verb_items(
    focus: CaptureField,
    scope_dropdown_open: bool,
) -> &'static [VerbEntry<'static>] {
    if scope_dropdown_open {
        return FORM_SCOPE_DROPDOWN_VERBS;
    }
    match focus {
        CaptureField::Title => FORM_TITLE_VERBS,
        CaptureField::Notes => FORM_NOTES_VERBS,
        CaptureField::Thread => FORM_THREAD_VERBS,
        CaptureField::Scope => FORM_SCOPE_VERBS,
    }
}

/// Verb bar for the Title editor: one field, so Tab has nothing to move focus
/// between and Shift+Enter saves.
const EDIT_TITLE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "shift+enter",
        label: "save",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];

/// Verb bar for the Notes editor: plain Enter opens a line, so Shift+Enter saves. Tab stays
/// unbound.
const EDIT_NOTES_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "shift+enter",
        label: "save",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];

pub(crate) const SCOPE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "j/k",
        label: "choose",
    },
    VerbEntry {
        key: "enter",
        label: "scope",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
];

/// Paint palette / help / scope dropdown over the base frame.
///
/// Standard: floating overlay. Compact: full-viewport takeover when height is tight.
fn paint_overlay(
    frame: &mut Frame<'_>,
    overlay: &QueueOverlay<'_>,
    geo: &TierGeometry,
    surface: Rect,
    hits: &mut QueueHitMap,
) {
    match overlay {
        QueueOverlay::None => {}
        QueueOverlay::Palette { query, commands } => {
            paint_palette_overlay(frame, geo, surface, query, commands, hits);
        }
        QueueOverlay::Help { lines } => {
            paint_help_overlay(frame, geo, surface, lines, hits);
        }
        QueueOverlay::ScopeDropdown { options, selected } => {
            paint_scope_dropdown(frame, geo, surface, options, *selected, hits);
        }
        QueueOverlay::EditTitle {
            ref draft,
            cursor_col,
        } => {
            paint_edit_title_overlay(frame, geo, surface, draft, *cursor_col);
        }
        QueueOverlay::EditNotes {
            ref rows,
            cursor_row,
            cursor_col,
        } => {
            paint_edit_notes_overlay(frame, geo, surface, rows, *cursor_row, *cursor_col);
        }
        QueueOverlay::QuickAdd { .. } => {}
        QueueOverlay::TaskEmpty => {
            put_line(
                frame,
                surface,
                1.min(geo.height.saturating_sub(1)),
                geo.row_width,
                paint_bounded_line("  no task selected", geo.row_width, style_dim()),
            );
        }
        QueueOverlay::TaskPage {
            ref header_rows,
            ref header_identifier,
            header_identifier_task,
            ref title_cursor,
            status_word,
            ref notes_rows,
            notes_cursor,
            more_lines,
            ref step_views,
            stored_step_count,
            step_cursor,
            step_add_selected,
            step_scroll,
            step_marked,
            ref inline_step_editor,
            bottom_input: _,
            ref meta,
            meta_scope_x,
            meta_scope_width,
            thread_slot_width,
            focus,
            scope_dropdown,
        } => {
            paint_task_page(
                frame,
                geo,
                surface,
                header_rows,
                header_identifier.as_deref(),
                *header_identifier_task,
                *title_cursor,
                status_word,
                notes_rows,
                *notes_cursor,
                *more_lines,
                step_views,
                *stored_step_count,
                *step_cursor,
                *step_add_selected,
                *step_scroll,
                *step_marked,
                inline_step_editor.as_ref(),
                meta,
                *meta_scope_x,
                *meta_scope_width,
                *thread_slot_width,
                *focus,
                bottom_input_slot(overlay).is_some(),
                false,
                hits,
            );
            if let Some(dropdown) = scope_dropdown {
                paint_page_scope_dropdown(frame, geo, surface, *dropdown, hits);
            }
        }
    }
}

fn clear_compact_takeover(frame: &mut Frame<'_>, geo: &TierGeometry, surface: Rect) {
    // `Clear` resets every cell in the region to a blank default cell. A `Paragraph::new("")`
    // does not: it only sets style over the area and writes no symbols, so it never erases
    // whatever the base list already painted underneath it. Compact is a full-viewport
    // takeover, so the whole viewport is cleared before the editor paints its own rows.
    if geo.tier == Tier::Compact && geo.viewport_height > 0 {
        let area = Rect::new(0, geo.viewport_top, geo.row_width, geo.viewport_height);
        frame.render_widget(Clear, local_rect(surface, area));
    }
}

fn paint_edit_title_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    draft: &str,
    cursor_col: u16,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return;
    }
    clear_compact_takeover(frame, geo, surface);
    let y = geo.viewport_top;
    let label = "  title  ";
    let avail = editor_field_width(geo);
    // `draft` already carries the cursor-aware horizontal window computed by the board at
    // this exact width; `present_line` here is idempotent on a string already inside
    // `avail`, matching every other overlay's paint step.
    let shown = present_line(draft, avail);
    let text = format!("{}{}", label, shown);
    put_line(
        frame,
        surface,
        y,
        width,
        paint_bounded_line(&text, width, style_bold()),
    );
    let region = Rect::new(
        (label.len() as u16).min(width.saturating_sub(1)),
        y,
        (avail as u16).min(width),
        1,
    );
    place_edit_cursor(
        frame,
        local_rect(surface, region),
        cursor_col.min(avail as u16),
    );
}

/// Paint full-width notes editor (standard or compact takeover).
fn paint_edit_notes_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    rows: &[String],
    cursor_row: u16,
    cursor_col: u16,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return;
    }
    clear_compact_takeover(frame, geo, surface);
    let start_y = geo.viewport_top;
    // Bound by the takeover region, not just the frame height: an over-tall draft must not
    // paint into the rule or status rows below it.
    let limit_y = start_y.saturating_add(editor_row_budget(geo));
    let label = "  notes  ";
    let label_w = label.len() as u16;
    let avail = editor_field_width(geo);
    for (i, row) in rows.iter().enumerate() {
        let y = start_y.saturating_add(i as u16);
        if y >= height || y >= limit_y {
            break;
        }
        let pad = if i == 0 { "" } else { "         " };
        let body = present_line(row, avail);
        let text = if i == 0 {
            format!("{}{}", label, body)
        } else {
            format!("{}{}", pad, body)
        };
        put_line(
            frame,
            surface,
            y,
            width,
            paint_bounded_line(&text, width, style_bold()),
        );
    }
    if !rows.is_empty() && start_y < limit_y {
        // The region spans every row actually painted above so `place_edit_cursor_at` can
        // clamp `cursor_row` inside it instead of the caret always landing on row 0 (N1).
        let painted_rows = (rows.len() as u16).min(height.saturating_sub(start_y));
        let painted_rows = painted_rows.min(limit_y.saturating_sub(start_y)).max(1);
        let region = Rect::new(
            label_w.min(width.saturating_sub(1)),
            start_y,
            (avail as u16).min(width),
            painted_rows,
        );
        place_edit_cursor_at(
            frame,
            local_rect(surface, region),
            cursor_row,
            cursor_col.min(avail as u16),
        );
    }
}

/// Shared centered mono modal card for `P` / `?` / `:` (help / command palette / project
/// scope): a bordered box with the title and `[x]` close control integrated into the top
/// border, an optional footer rule + dim key legend, and the board still visible behind
/// it (only the card's own rect is cleared and repainted).
///
/// `bounds` is the region the card may center within -- a caller with nothing worth
/// preserving underneath (Help, project-scope) passes the full frame; the palette passes
/// a shorter band so its own query row (painted separately, on the status row) stays
/// clear. Compact tier drops the blank padding row/column around the content but always
/// keeps the border and footer; the card otherwise sizes itself to `content_rows`,
/// clamped to what `bounds` can hold.
///
/// Hits are pushed in this order -- later pushes win under `hit_at`'s reverse search --
/// so a caller's own body hits (and `push_copyable` calls) painted after this returns
/// correctly shadow the blanket chrome below for their own rects:
/// 1. A full-frame `dismiss` hit (when given), so a click anywhere on the board behind
///    the card resolves to it unless something more specific shadows it.
/// 2. A blanket `ModalChrome` hit over the whole card, so every cell defaults to inert.
/// 3. `ModalClose` on the `[x]` control.
///
/// Per-call content for [`paint_modal_card`]: everything that varies between Help /
/// Palette / project-scope beyond the shared `frame` / `geo` / `bounds` / `hits` plumbing.
struct ModalCardSpec<'a> {
    title: &'a str,
    content_rows: u16,
    legend: &'a [VerbEntry<'a>],
    /// A full-frame dismiss hit to push first, before the card's own chrome (see below).
    dismiss: Option<QueueHitTarget>,
}

/// Fixed row overhead a [`paint_modal_card`] with a footer legend (or not) spends on its
/// own chrome -- border top+bottom, the footer's rule+legend rows when it has one, and
/// blank vertical padding outside `Tier::Compact`. Callers that need to know how many
/// body rows will actually be visible *before* the card paints (e.g. to decide whether a
/// title needs a `▼` scroll marker) go through this, so that decision can never drift
/// from what the card itself later carves out of `bounds`.
fn modal_chrome_rows(tier: Tier, has_footer: bool) -> u16 {
    let pad: u16 = if tier == Tier::Compact { 0 } else { 1 };
    let footer_rows: u16 = if has_footer { 2 } else { 0 };
    2 + footer_rows + 2 * pad
}

/// Returns the content `Rect` the caller paints its body into (zero-area when nothing
/// fits, e.g. a zero-sized frame).
fn paint_modal_card(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    bounds: Rect,
    spec: ModalCardSpec<'_>,
    hits: &mut QueueHitMap,
) -> Rect {
    let ModalCardSpec {
        title,
        content_rows,
        legend,
        dismiss,
    } = spec;
    if geo.row_width == 0 || geo.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return Rect::default();
    }
    if let Some(target) = dismiss {
        hits.push(target, Rect::new(0, 0, geo.row_width, geo.height));
    }

    let card_w = bounds.width.saturating_sub(4).clamp(1, 62);
    // Blank inset around the content, shed on both axes in compact so its scarce cells
    // go to actual body text instead of decorative breathing room.
    let pad: u16 = if geo.tier == Tier::Compact { 0 } else { 1 };
    let chrome_rows = modal_chrome_rows(geo.tier, !legend.is_empty());
    let shown_content = content_rows.min(bounds.height.saturating_sub(chrome_rows));
    let card_h = (chrome_rows + shown_content).min(bounds.height).max(1);
    let x0 = bounds.x + bounds.width.saturating_sub(card_w) / 2;
    let y0 = bounds.y + bounds.height.saturating_sub(card_h) / 2;
    let area = Rect::new(x0, y0, card_w, card_h);

    frame.render_widget(Clear, local_rect(surface, area));
    hits.push(QueueHitTarget::ModalChrome, area);

    let (top_line, close_x) = modal_title_border_row(title, card_w);
    paint_row_in(frame, surface, area, 0, top_line);
    if close_x < card_w {
        let close_w = 3.min(card_w.saturating_sub(close_x));
        hits.push(
            QueueHitTarget::ModalClose,
            Rect::new(x0 + close_x, y0, close_w, 1),
        );
    }

    let bottom_offset = card_h.saturating_sub(1);
    if bottom_offset > 0 {
        paint_row_in(
            frame,
            surface,
            area,
            bottom_offset,
            modal_plain_border_row('└', '─', '┘', card_w),
        );
    }
    let rule_offset = if !legend.is_empty() {
        Some(bottom_offset.saturating_sub(2))
    } else {
        None
    };
    if let Some(rule_offset) = rule_offset {
        let legend_offset = bottom_offset.saturating_sub(1);
        paint_row_in(
            frame,
            surface,
            area,
            rule_offset,
            modal_plain_border_row('├', '─', '┤', card_w),
        );
        paint_row_in(
            frame,
            surface,
            area,
            legend_offset,
            modal_legend_line(legend, card_w as usize),
        );
    }

    // Side borders for every middle row that is not already a full `├─┤` / top / bottom
    // rule. Content, padding, and the legend line only had corners before — the box
    // looked open on the left and right.
    for row_offset in 1..bottom_offset {
        if rule_offset == Some(row_offset) {
            continue;
        }
        paint_modal_side_borders(frame, surface, area, row_offset);
    }

    Rect::new(
        x0.saturating_add(1 + pad),
        y0.saturating_add(1 + pad),
        card_w.saturating_sub(2 + 2 * pad),
        shown_content,
    )
}

/// Dim `│` on the left and right edges of one card row (content / padding / legend).
fn paint_modal_side_borders(frame: &mut Frame<'_>, surface: Rect, area: Rect, row_offset: u16) {
    if row_offset >= area.height || area.width == 0 {
        return;
    }
    let area = local_rect(surface, area);
    let y = area.y.saturating_add(row_offset);
    let style = style_dim();
    let buffer = frame.buffer_mut();
    buffer[(area.x, y)].set_symbol("│").set_style(style);
    if area.width > 1 {
        let right = area.x.saturating_add(area.width.saturating_sub(1));
        buffer[(right, y)].set_symbol("│").set_style(style);
    }
}

/// One row of a modal card's border/legend, painted at `area`'s `row_offset`-th row
/// (never past `area`'s own height).
fn paint_row_in(
    frame: &mut Frame<'_>,
    surface: Rect,
    area: Rect,
    row_offset: u16,
    line: Line<'static>,
) {
    if row_offset >= area.height {
        return;
    }
    let rect = Rect::new(area.x, area.y.saturating_add(row_offset), area.width, 1);
    put_line_at(frame, surface, rect, line);
}

/// A plain dim border row: one corner glyph, a fill of `card_w - 2` cells, the other
/// corner glyph (`┌─…─┐` / `├─…─┤` / `└─…─┘`).
fn modal_plain_border_row(left: char, fill: char, right: char, card_w: u16) -> Line<'static> {
    let width = card_w as usize;
    let inner = width.saturating_sub(2);
    let mut text = String::with_capacity(width.max(1));
    text.push(left);
    text.extend(std::iter::repeat_n(fill, inner));
    text.push(right);
    bound_line(Line::from(Span::styled(text, style_dim())), width)
}

/// The card's top border, with its title and `[x]` close control integrated
/// (`┌─ Title ──…── [x]─┐`). Returns the painted line plus the close control's column
/// offset from the card's own left edge, so the caller can push a hit region that can
/// never drift from what was actually painted.
fn modal_title_border_row(title: &str, card_w: u16) -> (Line<'static>, u16) {
    let width = card_w as usize;
    let inner = width.saturating_sub(2);
    // Fixed furniture around the title and close control: "─ " + " " + " " + "[x]" + "─".
    const FIXED: usize = 8;
    let title_budget = inner.saturating_sub(FIXED + 1);
    let shown_title = present_line(title, title_budget);
    let title_w = display_width(&shown_title);
    let fill = inner.saturating_sub(FIXED + title_w).max(1);
    let close_x = (1 + 2 + title_w + 1 + fill + 1) as u16;

    let spans = vec![
        Span::styled("┌".to_string(), style_dim()),
        Span::styled("─ ".to_string(), style_dim()),
        Span::styled(shown_title, style_bold()),
        Span::styled(" ".to_string(), style_dim()),
        Span::styled("─".repeat(fill), style_dim()),
        Span::styled(" ".to_string(), style_dim()),
        Span::styled("[x]".to_string(), style_bold()),
        Span::styled("─".to_string(), style_dim()),
        Span::styled("┐".to_string(), style_dim()),
    ];
    (bound_line(Line::from(spans), width), close_x)
}

/// A modal card's footer legend line: dim separators and labels, bold keys, e.g.
/// `  esc close · enter run`.
fn modal_legend_line(entries: &[VerbEntry<'_>], width: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::styled("  ".to_string(), style_plain())];
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ".to_string(), style_dim()));
        }
        spans.push(Span::styled(entry.key.to_string(), style_bold()));
        spans.push(Span::styled(format!(" {}", entry.label), style_dim()));
    }
    bound_line(Line::from(spans), width)
}

/// The vertical band a modal card ( [`paint_modal_card`] ) may center within: the whole
/// row width, but never the rule/status/verb rows underneath -- the palette keeps its
/// query on the status row exactly as before, and the (now-blank) verb row stays intact
/// for every card so the board's own bottom chrome never gets painted over.
fn modal_bounds(geo: &TierGeometry) -> Rect {
    Rect::new(0, 0, geo.row_width, geo.rule_row.unwrap_or(geo.height))
}

/// Legend footer for the Help card: any key (Esc included) closes it.
const HELP_FOOTER: &[VerbEntry<'static>] = &[VerbEntry {
    key: "any key",
    label: "close",
}];

/// Legend footer for the command palette card.
const PALETTE_FOOTER: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "↑/↓",
        label: "move",
    },
    VerbEntry {
        key: "enter",
        label: "run",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
];

/// Legend footer for the project-scope card.
const SCOPE_FOOTER: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "↑/↓",
        label: "move",
    },
    VerbEntry {
        key: "enter",
        label: "choose",
    },
    VerbEntry {
        key: "esc",
        label: "close",
    },
];

/// Append a scroll marker to a card title when its body is windowed (`▲`/`▼`/both).
fn titled_with_scroll_marker(title: &str, above: bool, below: bool) -> String {
    if !above && !below {
        return title.to_string();
    }
    let mut owned = format!("{title} ");
    if above {
        owned.push('▲');
    }
    if below {
        owned.push('▼');
    }
    owned
}

fn paint_palette_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    query: &str,
    commands: &[PaletteCommandRow<'_>],
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height < 3 {
        return;
    }
    let bounds = modal_bounds(geo);
    // Derived from the same chrome math `paint_modal_card` itself uses, rather than a
    // separate guess (`viewport_height` belongs to the board list, not this card) -- a
    // mismatch here would either scroll past what the card can paint (leaving the
    // "selected" row visually missing) or paint into the card's own footer.
    let capacity = bounds
        .height
        .saturating_sub(modal_chrome_rows(geo.tier, true));
    let max_rows = if geo.tier == Tier::Compact {
        capacity.max(1) as usize
    } else {
        6usize.min(capacity.max(1) as usize)
    };
    let rows = max_rows.min(commands.len());
    let selected = commands
        .iter()
        .position(|command| command.selected)
        .unwrap_or(0);
    let scroll = if rows == 0 || selected < rows {
        0
    } else {
        (selected + 1 - rows).min(commands.len() - rows)
    };
    let above = scroll > 0;
    let below = rows > 0 && scroll + rows < commands.len();
    let title = titled_with_scroll_marker("command", above, below);

    let content = paint_modal_card(
        frame,
        geo,
        surface,
        bounds,
        ModalCardSpec {
            title: &title,
            content_rows: rows as u16,
            legend: PALETTE_FOOTER,
            dismiss: None,
        },
        hits,
    );
    if content.width > 0 && content.height > 0 {
        let paintable = rows.min(content.height as usize);
        for (j, cmd) in commands.iter().skip(scroll).take(paintable).enumerate() {
            let y = content.y.saturating_add(j as u16);
            let pre = if cmd.selected { "▸ " } else { "  " };
            let text = format!("{pre}{}", cmd.label);
            let style = if cmd.selected {
                style_reverse()
            } else {
                style_plain()
            };
            let rect = Rect::new(content.x, y, content.width, 1);
            put_line_at(
                frame,
                surface,
                rect,
                paint_bounded_line(&text, content.width, style),
            );
            // Index into the full (unscrolled) command list, so a click resolves to the
            // same command `BoardModel::visible_commands()` would name at that position
            // regardless of which window is currently painted.
            hits.push(QueueHitTarget::Command(scroll + j), rect);
            hits.push_copyable(rect);
        }
    }

    // Query sits on the status row when present; otherwise the last content row -- the
    // card's own bounds stop above this row, so caret placement here stays exactly as
    // simple as before the card existed. A footer-less wide column leaves the query to the
    // shared footer.
    if !geo.owns_footer() {
        return;
    }
    let query_row = geo.status_row.unwrap_or(height.saturating_sub(2));
    paint_palette_query(frame, surface, query_row, width, query, hits);
}

fn paint_palette_query(
    frame: &mut Frame<'_>,
    surface: Rect,
    query_row: u16,
    width: u16,
    query: &str,
    hits: &mut QueueHitMap,
) {
    let q = format!(" :{query}");
    put_line(
        frame,
        surface,
        query_row,
        width,
        paint_bounded_line(&q, width, style_bold()),
    );
    // the query row is chrome too -- typing to filter is the only
    // route it offers, so a click on it is inert rather than a dismissal.
    hits.push(
        QueueHitTarget::CommandChrome,
        Rect::new(0, query_row, width, 1),
    );
}

fn paint_help_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    lines: &[String],
    hits: &mut QueueHitMap,
) {
    if geo.row_width == 0 || geo.height == 0 || lines.is_empty() {
        return;
    }
    // Compact: omit decorative blank rows, and drop each line's leading indent column --
    // the card's own border already insets the body, so the width that bought is worth
    // more here than the indent. Neither line up needs an inset from the other; the
    // card's `pad` already covers it.
    let shown: Vec<String> = if geo.tier == Tier::Compact {
        lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(|line| line.trim_start().to_string())
            .collect()
    } else {
        lines.to_vec()
    };

    // Help has nothing worth preserving underneath it (no query row like the palette),
    // so it takes the whole frame as its ceiling -- the one place that actually matters:
    // a narrow compact terminal, where every row the border+footer would otherwise leave
    // idle buys another binding into view.
    let bounds = Rect::new(0, 0, geo.row_width, geo.height);
    let capacity = bounds
        .height
        .saturating_sub(modal_chrome_rows(geo.tier, true));
    // Never scrolls -- there is no selection to seek with, and closing on any key rules
    // out a dedicated scroll chord -- so `▼` here means "more exists" (resize to see it),
    // not "more is reachable".
    let title = titled_with_scroll_marker("help", false, (shown.len() as u16) > capacity);
    let content = paint_modal_card(
        frame,
        geo,
        surface,
        bounds,
        ModalCardSpec {
            title: &title,
            content_rows: shown.len() as u16,
            legend: HELP_FOOTER,
            dismiss: Some(QueueHitTarget::HelpDismiss),
        },
        hits,
    );
    // The card's blanket `ModalChrome` (pushed for the whole card, border included) would
    // otherwise make the body text itself inert too. Help's body is not an interactive
    // surface like the palette's command rows or the picker's options -- there is nothing
    // to select inside it -- so it keeps mouse parity with the keyboard's "any key" by
    // reclaiming its own content rect as a `HelpDismiss` hit, the same close its own `[x]`
    // and the frame outside the card already resolve to.
    hits.push(QueueHitTarget::HelpDismiss, content);
    for (j, ln) in shown.iter().take(content.height as usize).enumerate() {
        let y = content.y.saturating_add(j as u16);
        let rect = Rect::new(content.x, y, content.width, 1);
        put_line_at(
            frame,
            surface,
            rect,
            paint_bounded_line(ln, content.width, style_plain()),
        );
        hits.push_copyable(rect);
    }
}

/// The task page's fixed frame: header, divider, and meta footer surround one
/// scrollable content viewport. Notes and steps flow through that viewport together.
pub struct TaskPageLayout {
    /// First row the page must not paint (the lowest chrome row, or the frame height).
    pub bottom: u16,
    pub title_y: u16,
    pub divider_y: Option<u16>,
    pub notes_y: u16,
    pub notes_rows: u16,
    pub meta_y: Option<u16>,
}

/// What the page's steps section asks of the layout (AC-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepsSection {
    /// No stored or draft steps: no section paints, so notes keep the full content region.
    /// An inline add draft contributes a transient step view and therefore uses `Steps`.
    None,
    /// At least one step: the section follows the notes after two blank rows, and
    /// the shared content viewport scrolls when the resulting page overflows.
    Steps,
}

/// Classify the page's steps section from its payload facts: steps alone
/// decide it.
pub fn steps_section(steps: usize) -> StepsSection {
    if steps > 0 {
        StepsSection::Steps
    } else {
        StepsSection::None
    }
}

/// The window of steps steps the section's step rows show.
///
/// `avail` is the row count the section has for steps (its block minus the label). When
/// steps remain hidden below, the last row becomes the dim `+N more ↓` affordance —
/// unless that would leave no step row at all, the one degenerate window (a single step
/// row beside a long list) where the step wins and the affordance is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepsWindow {
    /// First painted step's absolute index.
    pub first: usize,
    /// Step rows painted.
    pub count: usize,
    /// Steps hidden below the window.
    pub hidden_after: usize,
    /// Whether the last section row paints the affordance instead of an step.
    pub affordance: bool,
}

/// A shared page-content flow: steps follow the notes with two blank rows between
/// them, and the whole resulting stream scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageContentLayout {
    pub steps_start: usize,
    pub total_rows: usize,
    pub max_scroll: usize,
}

pub fn page_content_layout(
    note_rows: usize,
    steps: usize,
    viewport_rows: u16,
) -> PageContentLayout {
    let note_rows = note_rows.max(1);
    let viewport = viewport_rows as usize;
    // Two blank rows after notes keep the steps visually separated from the note,
    // regardless of viewport size. A trailing blank similarly separates the final
    // content section from the fixed footer when it scrolls into view.
    let steps_start = if steps == 0 {
        note_rows
    } else {
        note_rows.saturating_add(2)
    };
    let total_rows = steps_start + usize::from(steps > 0) + steps + 1;
    PageContentLayout {
        steps_start,
        total_rows,
        max_scroll: total_rows.saturating_sub(viewport),
    }
}

pub fn steps_window(total: usize, scroll: usize, avail: u16) -> StepsWindow {
    let avail = avail as usize;
    if total == 0 || avail == 0 {
        return StepsWindow {
            first: 0,
            count: 0,
            hidden_after: 0,
            affordance: false,
        };
    }
    // Never start past the last step, whatever a stale scroll offset claims.
    let first = scroll.min(total - 1);
    let fitting = avail.min(total - first);
    let hidden_after = total - first - fitting;
    let affordance = hidden_after > 0 && fitting >= 2;
    let count = if affordance { fitting - 1 } else { fitting };
    StepsWindow {
        first,
        count,
        hidden_after,
        affordance,
    }
}

/// Build the fixed frame around the task page's shared content viewport. `section`
/// remains an input for callers that classify a task, but the content itself owns
/// the notes/steps allocation and scrolls as one region.
pub fn task_page_layout(
    geo: &TierGeometry,
    _section: StepsSection,
    _notes_floor: u16,
    title_rows: u16,
) -> TaskPageLayout {
    let height = geo.height;
    let bottom = [geo.rule_row, geo.status_row, geo.verb_row]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(height);
    // Blank row 0, the title's wrapped rows from 1, divider, notes, steps, meta last.
    let title_rows = title_rows.max(1);
    let title_y: u16 = if bottom >= 2 { 1 } else { 0 };
    let title_end = title_y.saturating_add(title_rows);
    let meta_y = if bottom >= 4 { Some(bottom - 1) } else { None };
    // The divider shows only when a note row survives under it; otherwise the
    // notes body starts directly under the title.
    let divider_y = if bottom >= title_end.saturating_add(3) {
        Some(title_end)
    } else {
        None
    };
    let notes_y = if bottom > title_end {
        divider_y.map(|y| y + 1).unwrap_or(title_end)
    } else {
        bottom
    };
    let content_end = meta_y.unwrap_or(bottom);
    let content_rows = content_end.saturating_sub(notes_y);
    TaskPageLayout {
        bottom,
        title_y,
        divider_y,
        notes_y,
        notes_rows: content_rows,
        meta_y,
    }
}

/// Layout for the task page painted as a wide column: the header rule owns the selector row,
/// the body starts on the row after it, and the meta footer keeps the last row above the
/// shared footer. There is no in-page title block and no divider.
pub fn task_column_layout(geo: &TierGeometry) -> TaskPageLayout {
    let bottom = [geo.rule_row, geo.status_row, geo.verb_row]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(geo.height);
    let title_y = geo.selector_row.unwrap_or(0).min(bottom.saturating_sub(1));
    // The header is two rows: the title (glyph, identifier, state slot) on the selector
    // row and its dash rule directly under it. The body starts on the row below the rule.
    let notes_y = title_y.saturating_add(2).min(bottom);
    let meta_y = (bottom >= notes_y.saturating_add(2)).then(|| bottom - 1);
    let content_end = meta_y.unwrap_or(bottom);
    TaskPageLayout {
        bottom,
        title_y,
        divider_y: None,
        notes_y,
        notes_rows: content_end.saturating_sub(notes_y),
        meta_y,
    }
}

/// The task page: full-height takeover hiding the selector row, header + notes + meta
/// footer. Field clicks reuse the shared-form hit targets, so one mouse map serves the
/// page and the form alike. `column` paints the wide-column body only: the header rule on
/// the selector row belongs to [`draw_task_column`].
#[allow(clippy::too_many_arguments)]
fn paint_task_page(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    header_rows: &[String],
    header_identifier: Option<&str>,
    header_identifier_task: Option<Uuid>,
    title_cursor: Option<(u16, u16)>,
    status_word: &str,
    notes_rows: &[String],
    notes_cursor: Option<(u16, u16)>,
    more_lines: usize,
    step_views: &[StepView],
    stored_step_count: usize,
    step_cursor: Option<usize>,
    step_add_selected: bool,
    step_scroll: usize,
    step_marked: Option<usize>,
    inline_step_editor: Option<&InlineStepEditor<'_>>,
    meta: &str,
    meta_scope_x: u16,
    meta_scope_width: u16,
    thread_slot_width: Option<u16>,
    focus: Option<CaptureField>,
    footer_input_open: bool,
    column: bool,
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    if width == 0 || geo.height == 0 {
        return;
    }
    // `focus == Notes` arrives from the same frame's input mode the payload builder
    // used, so both sides of the payload/paint seam budget the same notes floor.
    let title_row_count = header_rows.len().max(1) as u16;
    let lay = if column {
        task_column_layout(geo)
    } else {
        task_page_layout(
            geo,
            steps_section(step_views.len()),
            u16::from(focus == Some(CaptureField::Notes)),
            title_row_count,
        )
    };
    if lay.bottom == 0 {
        return;
    }
    if !column {
        frame.render_widget(
            Clear,
            local_rect(surface, Rect::new(0, 0, width, lay.bottom)),
        );
    }

    // Header: every wrapped title row paints bold under a shared left gutter;
    // row 0 carries the status glyph and frames the dim right-aligned status
    // word, later rows indent into the glyph column. The scrollbar belongs only
    // to the content viewport below.
    let header_width = width.saturating_sub(2);
    let word = display_width(status_word);
    // The broad title region sits beneath the identifier hit so a view-mode header can
    // reserve the prefix for copy without making the rest of the title interactive.
    if !column {
        hits.push(
            QueueHitTarget::FormTitle,
            Rect::new(0, lay.title_y, width, title_row_count),
        );
    }
    for (offset, row_text) in header_rows.iter().enumerate().filter(|_| !column) {
        let y = lay.title_y.saturating_add(offset as u16);
        // The builder caps `header_rows` to the page body; this clamp holds even
        // if a future caller forgets, because painting past `bottom` would
        // overwrite the rule/status/verb chrome the page must keep.
        if y >= lay.bottom {
            break;
        }
        let line = if offset == 0 {
            let (glyph, title) = row_text.split_once(' ').unwrap_or((row_text, ""));
            let prefix = format!("  {glyph} ");
            let identifier = header_identifier.unwrap_or_default();
            let identifier_gap = if identifier.is_empty() { "" } else { " " };
            let used = display_width(&prefix)
                .saturating_add(display_width(identifier))
                .saturating_add(display_width(identifier_gap))
                .saturating_add(display_width(title));
            let mut spans = vec![Span::styled(prefix, style_bold())];
            if !identifier.is_empty() {
                spans.push(Span::styled(identifier.to_string(), style_dim()));
                spans.push(Span::styled(identifier_gap.to_string(), style_bold()));
                if let Some(task) = header_identifier_task {
                    hits.push(
                        QueueHitTarget::TaskNumber(task),
                        Rect::new(
                            4,
                            y,
                            u16::try_from(display_width(identifier)).unwrap_or(u16::MAX),
                            1,
                        ),
                    );
                }
            }
            spans.push(Span::styled(title.to_string(), style_bold()));
            if used + word < header_width as usize {
                spans.push(Span::raw(" ".repeat(header_width as usize - used - word)));
                spans.push(Span::styled(status_word.to_string(), style_dim()));
            }
            let title_cells = used.saturating_sub(4);
            if title_cells > 0 {
                hits.push_copyable(Rect::new(
                    4,
                    y,
                    u16::try_from(title_cells).unwrap_or(u16::MAX),
                    1,
                ));
            }
            Line::from(spans)
        } else {
            let has_identifier = header_identifier.is_some_and(|identifier| !identifier.is_empty());
            let title_indent = 4usize
                .saturating_add(header_identifier.map(display_width).unwrap_or(0))
                .saturating_add(usize::from(has_identifier));
            let painted = format!("{}{row_text}", " ".repeat(title_indent));
            let title_cells = display_width(&painted).saturating_sub(title_indent);
            if title_cells > 0 {
                hits.push_copyable(Rect::new(
                    u16::try_from(title_indent).unwrap_or(u16::MAX),
                    y,
                    u16::try_from(title_cells).unwrap_or(u16::MAX),
                    1,
                ));
            }
            Line::from(Span::styled(painted, style_bold()))
        };
        put_line(frame, surface, y, header_width, line);
    }
    if let Some((cursor_row, cursor_col)) = title_cursor.filter(|_| !column) {
        // Every title row shares the four-cell gutter (two leading blanks plus
        // glyph and space), so the wrapped field starts at column 4 on all of them.
        let field_x = 4u16.min(width.saturating_sub(1));
        place_edit_cursor_at(
            frame,
            local_rect(
                surface,
                Rect::new(
                    field_x,
                    lay.title_y,
                    width.saturating_sub(field_x),
                    title_row_count,
                ),
            ),
            cursor_row.min(title_row_count.saturating_sub(1)),
            cursor_col.min(width.saturating_sub(field_x).saturating_sub(1)),
        );
    }

    // Divider, its right end naming wrapped note rows the window does not show.
    if let Some(y) = lay.divider_y {
        let tail = if more_lines > 0 {
            format!(" {more_lines} more ↓ ")
        } else {
            String::new()
        };
        let tail_w = display_width(&tail);
        let divider_width = width.saturating_sub(2);
        let dash_count = (divider_width as usize)
            .saturating_sub(2)
            .saturating_sub(tail_w);
        let dashes = "─".repeat(dash_count);
        put_line(
            frame,
            surface,
            y,
            divider_width,
            Line::from(vec![
                Span::styled(format!("  {dashes}"), style_dim()),
                Span::styled(tail, style_dim()),
            ]),
        );
    }

    // Notes and steps form one vertical stream. Steps begin two blank rows after the
    // notes, and the header and metadata footer never participate in this scroll.
    let note_count = notes_rows.len().max(1);
    let step_rows: usize = step_views.iter().map(|step| step.rows.len().max(1)).sum();
    // Every checklist has a trailing add control, including an empty one.
    let content = page_content_layout(note_count, step_rows + 1, lay.notes_rows);
    let scroll = step_scroll.min(content.max_scroll);
    // Content has a two-cell gutter on both sides. An overflowing page keeps its
    // scrollbar outside that right gutter at the frame edge.
    let content_width = width.saturating_sub(if content.max_scroll > 0 { 3 } else { 2 });
    let notes_style = if focus == Some(CaptureField::Notes) {
        style_bold()
    } else {
        style_plain()
    };
    let done = step_views
        .iter()
        .take(stored_step_count)
        .filter(|step| step.done)
        .count();
    let inline_editor_first_row = inline_step_editor.map(|editor| {
        content.steps_start.saturating_add(1).saturating_add(
            step_views
                .iter()
                .take(editor.index)
                .map(|step| step.rows.len().max(1))
                .sum::<usize>(),
        )
    });
    for visible in 0..lay.notes_rows as usize {
        let absolute = scroll + visible;
        if absolute >= content.total_rows {
            break;
        }
        let y = lay.notes_y.saturating_add(visible as u16);
        if absolute < note_count {
            let text = notes_rows
                .get(absolute)
                .map(String::as_str)
                .unwrap_or("no notes yet");
            let style = if notes_rows.is_empty() {
                style_dim()
            } else {
                notes_style
            };
            let painted = if notes_rows.is_empty() || focus == Some(CaptureField::Notes) {
                paint_bounded_line(&format!("  {text} "), content_width, style)
            } else {
                let mut in_fence = notes_rows
                    .iter()
                    .take(absolute)
                    .filter(|row| crate::ui::markdown::is_fence_line(row))
                    .count()
                    % 2
                    == 1;
                crate::ui::markdown::paint_notes_line(
                    &format!("  {text} "),
                    content_width as usize,
                    style,
                    &mut in_fence,
                )
            };
            put_line(frame, surface, y, content_width, painted);
            hits.push(
                QueueHitTarget::FormNotes(absolute),
                Rect::new(0, y, content_width, 1),
            );
            // Notes body past the two-cell gutter (and the trailing pad space).
            hits.push_copyable(Rect::new(2, y, content_width.saturating_sub(3), 1));
        } else if absolute == content.steps_start {
            put_line(
                frame,
                surface,
                y,
                content_width,
                paint_bounded_line(
                    &format!("  steps {done}/{stored_step_count}"),
                    content_width,
                    style_dim(),
                ),
            );
        } else if absolute > content.steps_start {
            let mut row = absolute - content.steps_start - 1;
            let mut found = None;
            for (index, step) in step_views.iter().enumerate() {
                let rows = step.rows.len().max(1);
                if row < rows {
                    found = Some((index, step, row));
                    break;
                }
                row -= rows;
            }
            if let Some((index, step, row_in_step)) = found {
                let first_row = row_in_step == 0;
                let gutter = if first_row && Some(index) == step_cursor {
                    "▸ "
                } else {
                    "  "
                };
                let glyph = if Some(index) == step_marked {
                    "✗"
                } else if step.done {
                    "✓"
                } else {
                    "▪"
                };
                let text = step
                    .rows
                    .get(row_in_step)
                    .map(String::as_str)
                    .unwrap_or_default();
                let prefix = if first_row {
                    format!("{gutter}{glyph} ")
                } else {
                    "    ".to_string()
                };
                let editing = inline_step_editor.is_some_and(|editor| editor.index == index);
                let shown_text = if editing && text.trim().is_empty() && first_row {
                    inline_step_editor
                        .and_then(|editor| editor.refusal)
                        .map(|refusal| format!("step… · {refusal}"))
                        .unwrap_or_else(|| "step…".to_string())
                } else {
                    text.to_string()
                };
                put_line(
                    frame,
                    surface,
                    y,
                    content_width,
                    paint_bounded_line(
                        &format!("{prefix}{shown_text} "),
                        content_width,
                        if editing { style_bold() } else { style_plain() },
                    ),
                );
                hits.push(
                    QueueHitTarget::Step(index),
                    Rect::new(0, y, content_width, 1),
                );
                // Every wrapped row belongs to the same step. The continuation aligns with
                // the first row's text and remains copyable without its selection gutter.
                let step_prefix = 4u16;
                hits.push_copyable(Rect::new(
                    step_prefix,
                    y,
                    content_width.saturating_sub(step_prefix).saturating_sub(1),
                    1,
                ));
            } else if row == 0 {
                put_line(
                    frame,
                    surface,
                    y,
                    content_width,
                    paint_bounded_line(
                        if step_add_selected {
                            "▸ + step"
                        } else {
                            "   + step"
                        },
                        content_width,
                        if step_add_selected {
                            style_plain()
                        } else {
                            style_dim()
                        },
                    ),
                );
                hits.push(QueueHitTarget::StepAdd, Rect::new(0, y, content_width, 1));
            }
        }
    }
    if let (Some(editor), Some(first_row)) = (inline_step_editor, inline_editor_first_row) {
        let cursor_row = first_row.saturating_add(editor.cursor_row as usize);
        if cursor_row >= scroll && cursor_row < scroll.saturating_add(lay.notes_rows as usize) {
            place_edit_cursor_at(
                frame,
                local_rect(
                    surface,
                    Rect::new(
                        4,
                        lay.notes_y,
                        content_width.saturating_sub(5),
                        lay.notes_rows,
                    ),
                ),
                u16::try_from(cursor_row.saturating_sub(scroll)).unwrap_or(u16::MAX),
                editor.cursor_col.min(content_width.saturating_sub(5)),
            );
        }
    }
    if let Some((row, col)) = notes_cursor {
        place_edit_cursor_at(
            frame,
            local_rect(
                surface,
                Rect::new(
                    2,
                    lay.notes_y,
                    content_width.saturating_sub(3),
                    lay.notes_rows,
                ),
            ),
            row.saturating_sub(u16::try_from(scroll).unwrap_or(u16::MAX))
                .min(lay.notes_rows.saturating_sub(1)),
            col.min(content_width.saturating_sub(2)),
        );
    }
    if content.max_scroll > 0 {
        paint_page_scrollbar(
            frame,
            surface,
            hits,
            &lay,
            width,
            scroll,
            content.total_rows,
        );
    }

    // Meta footer: scope · thread · created · updated. Inline step drafts leave this footer
    // visible and do not claim its input slot.
    if let Some(y) = lay.meta_y {
        put_line(
            frame,
            surface,
            y,
            width,
            paint_bounded_line(&format!("  {meta}"), width, style_dim()),
        );

        let scope_x = 2u16.saturating_add(meta_scope_x).min(width);
        let thread_x = scope_x.saturating_add(meta_scope_width).min(width);
        let selected = match focus {
            Some(CaptureField::Scope) => Some((scope_x, meta_scope_width)),
            // The separator belongs to footer chrome. Only the thread marker and name are
            // the selected control, so ` · #auth` keeps its dot dim while `#auth` reverses.
            Some(CaptureField::Thread) => thread_slot_width.map(|slot_width| {
                // ` · #auth` inside a longer footer; a leading `#auth` when the thread opens it.
                let slot: String = meta
                    .chars()
                    .skip(usize::from(meta_scope_x.saturating_add(meta_scope_width)))
                    .collect();
                let thread_prefix = if slot.starts_with(" · ") {
                    3u16
                } else {
                    u16::from(slot.starts_with('#'))
                };
                (
                    thread_x.saturating_add(thread_prefix),
                    slot_width.saturating_sub(thread_prefix),
                )
            }),
            _ => None,
        };
        if let Some((selected_x, selected_width)) = selected {
            let selected_width = selected_width.min(width.saturating_sub(selected_x));
            let buffer = frame.buffer_mut();
            for x in selected_x..selected_x.saturating_add(selected_width) {
                buffer[(surface.x.saturating_add(x), surface.y.saturating_add(y))]
                    .set_style(style_reverse());
            }
        }
        if footer_input_open {
            return;
        }
        hits.push(
            QueueHitTarget::FormScope,
            Rect::new(
                scope_x,
                y,
                meta_scope_width.min(width.saturating_sub(scope_x)),
                1,
            ),
        );
        if let Some(thread_slot_width) = thread_slot_width.filter(|_| thread_x < width) {
            let thread_width = thread_slot_width.min(width.saturating_sub(thread_x));
            hits.push(
                QueueHitTarget::FormThread,
                Rect::new(thread_x, y, thread_width, 1),
            );
        }
    }
}

/// Paint the shared-content scroll indicator on the viewport's right edge. The
/// thumb reports the visible fraction and moves with the same row offset used to
/// render notes and steps.
fn paint_page_scrollbar(
    frame: &mut Frame<'_>,
    surface: Rect,
    hits: &mut QueueHitMap,
    lay: &TaskPageLayout,
    width: u16,
    scroll: usize,
    total_rows: usize,
) {
    let viewport = lay.notes_rows;
    if width == 0 || viewport == 0 || !scrollbar::needs_scrollbar(total_rows, viewport as usize) {
        return;
    }
    let track = Rect::new(width - 1, lay.notes_y, 1, viewport);
    scrollbar::paint(frame, local_rect(surface, track), scroll, total_rows);
    let zone = scrollbar::grab_zone(track);
    for row in 0..track.height {
        let jump = scrollbar::click_to_offset(row, track.height, total_rows, viewport as usize);
        hits.push(
            QueueHitTarget::PageScroll(jump),
            Rect::new(zone.x, track.y.saturating_add(row), zone.width, 1),
        );
    }
}

/// The page's scope chooser stacks its options directly above the scope footer (left,
/// indented like the footer), never in the selector's corner: the footer is the control
/// being answered, so the chooser sits beside it.
fn paint_page_scope_dropdown(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    dropdown: FormScopeDropdown<'_>,
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    if width == 0 || dropdown.options.is_empty() {
        return;
    }
    // The dropdown anchors on the meta footer, which never moves with the steps,
    // and never opens while a field edit owns the page.
    let lay = task_page_layout(geo, StepsSection::None, 0, 1);
    let Some(meta_y) = lay.meta_y else {
        return;
    };
    let max_label = dropdown
        .options
        .iter()
        .map(|option| display_width(option))
        .max()
        .unwrap_or(0);
    let col_w = (max_label + 4).min(width as usize).max(8);
    let rows_fit = meta_y.saturating_sub(1) as usize;
    let rows = rows_fit.min(dropdown.options.len());
    if rows == 0 {
        return;
    }
    let selected = dropdown
        .selected
        .min(dropdown.options.len().saturating_sub(1));
    // Keep the selection visible: window the options like the corner dropdown does.
    let scroll = if selected < rows {
        0
    } else {
        (selected + 1 - rows).min(dropdown.options.len() - rows)
    };
    for (j, opt) in dropdown.options.iter().enumerate().skip(scroll).take(rows) {
        let y = meta_y.saturating_sub((j - scroll + 1) as u16);
        let marker = if j == selected { "▸ " } else { "  " };
        let body = present_line(&format!("{marker}{opt}"), col_w);
        let text = if display_width(&body) >= col_w {
            body
        } else {
            format!("{}{}", body, " ".repeat(col_w - display_width(&body)))
        };
        put_line(
            frame,
            surface,
            y,
            width,
            paint_bounded_line(&format!("  {text}"), width, style_plain()),
        );
        hits.push(
            QueueHitTarget::FormScopeOption(j),
            Rect::new(0, y, width, 1),
        );
        hits.push_copyable(Rect::new(0, y, width, 1));
    }
}

/// The board's `P` project-scope picker: a centered modal card, not the chip-anchored
/// dropdown this used to paint. [`paint_page_scope_dropdown`] (the task-page form's own
/// scope control) is a separate, unrelated painter and keeps its chip-anchored panel.
fn paint_scope_dropdown(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    surface: Rect,
    options: &[String],
    selected: usize,
    hits: &mut QueueHitMap,
) {
    if geo.row_width == 0 || options.is_empty() {
        return;
    }
    // Help/project-scope have nothing worth preserving underneath (no query row like the
    // palette), so the full frame is the ceiling: a narrow compact terminal gets every row
    // the border+footer would otherwise leave idle.
    let bounds = Rect::new(0, 0, geo.row_width, geo.height);
    // Derived from the same chrome math `paint_modal_card` itself uses (see the palette's
    // identical comment) so the window this picks always matches what actually paints.
    let capacity = bounds
        .height
        .saturating_sub(modal_chrome_rows(geo.tier, true));
    let max_n = if geo.tier == Tier::Compact {
        capacity.max(1) as usize
    } else {
        options.len().min(12).min(capacity.max(1) as usize)
    };
    let rows = max_n.min(options.len());
    let selected = selected.min(options.len().saturating_sub(1));
    let scroll = if selected < rows {
        0
    } else {
        (selected + 1 - rows).min(options.len() - rows)
    };
    let above = scroll > 0;
    let below = scroll + rows < options.len();
    let title = titled_with_scroll_marker("project", above, below);

    let content = paint_modal_card(
        frame,
        geo,
        surface,
        bounds,
        ModalCardSpec {
            title: &title,
            content_rows: rows as u16,
            legend: SCOPE_FOOTER,
            dismiss: None,
        },
        hits,
    );
    if content.width == 0 || content.height == 0 {
        return;
    }
    let paintable = rows.min(content.height as usize);
    for (j, opt) in options.iter().enumerate().skip(scroll).take(paintable) {
        let y = content.y.saturating_add((j - scroll) as u16);
        let marker = if j == selected { "▸ " } else { "  " };
        let text = format!("{marker}{opt}");
        let style = if j == selected {
            style_reverse()
        } else {
            style_plain()
        };
        let rect = Rect::new(content.x, y, content.width, 1);
        put_line_at(
            frame,
            surface,
            rect,
            paint_bounded_line(&text, content.width, style),
        );
        // `j` remains the source index after windowing, so a click selects the same option
        // Up/Down plus Enter would confirm rather than its position within this paint slice.
        hits.push(QueueHitTarget::ProjectOption(j), rect);
        hits.push_copyable(rect);
    }
}

enum ListRow {
    Blank,
    /// Section header row for IN MOTION, DONE, desk ON DECK, or project focus ON DECK.
    Header(SectionKind, usize, Line<'static>),
    /// Collapsible project-group header on the Projects tab.
    ProjectGroupHeader {
        section_idx: usize,
        line: Line<'static>,
    },
    /// Collapsible thread-group header on the Threads tab.
    ThreadGroupHeader {
        section_idx: usize,
        line: Line<'static>,
    },
    /// Collapsible project sub-header under a thread group.
    ThreadProjectHeader {
        section_idx: usize,
        subgroup_idx: usize,
        line: Line<'static>,
    },
    Hint(Line<'static>),
    /// Decorative scoped ON DECK thread-block label inside project focus.
    ThreadHeader(Line<'static>),
    Task {
        id: Uuid,
        line: Line<'static>,
        /// Exact first-line identifier cells, if this persisted task has one.
        identifier: Option<(u16, u16)>,
        /// First copyable content column, past indent + glyph.
        content_x: u16,
        /// Content width (identifier + title on the first line, title only on continuations).
        content_width: u16,
    },
    /// Read-only accordion content under a task, full-width and un-hit-tested.
    Detail {
        line: Line<'static>,
        /// First column past the peek `│` gutter.
        content_x: u16,
        content_width: u16,
    },
}

impl ListRow {
    fn is_sticky_header(&self) -> bool {
        matches!(
            self,
            ListRow::Header(..)
                | ListRow::ProjectGroupHeader { .. }
                | ListRow::ThreadGroupHeader { .. }
                | ListRow::ThreadProjectHeader { .. }
                | ListRow::ThreadHeader(_)
        )
    }
}

/// Last section header strictly above `scroll`. None when `scroll` is already on a header
/// (that header is naturally at the top) or nothing has been scrolled past.
fn sticky_header_at(rows: &[ListRow], scroll: usize) -> Option<usize> {
    if scroll == 0 || rows.is_empty() {
        return None;
    }
    let scroll = scroll.min(rows.len());
    if scroll < rows.len() && rows[scroll].is_sticky_header() {
        return None;
    }
    rows[..scroll]
        .iter()
        .rposition(|row| row.is_sticky_header())
}

/// Rows the viewport spends on a blank-under-tabs gap and a pinned header.
fn header_chrome_rows(rows: &[ListRow], scroll: usize) -> usize {
    if sticky_header_at(rows, scroll).is_some() {
        2
    } else if rows.get(scroll).is_some_and(ListRow::is_sticky_header) {
        1
    } else {
        0
    }
}

fn paint_list_row(
    frame: &mut Frame<'_>,
    surface: Rect,
    hits: &mut QueueHitMap,
    list_row: &ListRow,
    y: u16,
    content_width: u16,
    base_list_interactive: bool,
) {
    match list_row {
        ListRow::Blank => put_line(frame, surface, y, content_width, Line::from("")),
        ListRow::Header(kind, _section_idx, line) => {
            put_line(frame, surface, y, content_width, line.clone());
            if *kind == SectionKind::Done && base_list_interactive {
                hits.push(QueueHitTarget::Drawer, Rect::new(0, y, content_width, 1));
            }
        }
        ListRow::ProjectGroupHeader { section_idx, line } => {
            put_line(frame, surface, y, content_width, line.clone());
            if base_list_interactive {
                hits.push(
                    QueueHitTarget::SectionProject(*section_idx),
                    Rect::new(0, y, content_width, 1),
                );
            }
        }
        ListRow::ThreadGroupHeader { section_idx, line } => {
            put_line(frame, surface, y, content_width, line.clone());
            if base_list_interactive {
                hits.push(
                    QueueHitTarget::SectionThread(*section_idx),
                    Rect::new(0, y, content_width, 1),
                );
            }
        }
        ListRow::ThreadProjectHeader {
            section_idx,
            subgroup_idx,
            line,
        } => {
            put_line(frame, surface, y, content_width, line.clone());
            if base_list_interactive {
                hits.push(
                    QueueHitTarget::SectionThreadProject {
                        section_idx: *section_idx,
                        subgroup_idx: *subgroup_idx,
                    },
                    Rect::new(0, y, content_width, 1),
                );
            }
        }
        ListRow::Hint(line) | ListRow::ThreadHeader(line) => {
            put_line(frame, surface, y, content_width, line.clone())
        }
        ListRow::Task {
            id,
            line,
            identifier,
            content_x,
            content_width: copy_w,
        } => {
            put_line(frame, surface, y, content_width, line.clone());
            if base_list_interactive {
                hits.push(QueueHitTarget::Task(*id), Rect::new(0, y, content_width, 1));
                if let Some((x, width)) = identifier {
                    hits.push(QueueHitTarget::TaskNumber(*id), Rect::new(*x, y, *width, 1));
                }
            }
            hits.push_copyable(Rect::new(*content_x, y, *copy_w, 1));
        }
        ListRow::Detail {
            line,
            content_x,
            content_width: copy_w,
        } => {
            put_line(frame, surface, y, content_width, line.clone());
            hits.push_copyable(Rect::new(*content_x, y, *copy_w, 1));
        }
    }
}

/// Read-only accordion body under an expanded task: a short notes preview only.
///
/// The peek (`→`) shows up to [`PEEK_NOTES_LINE_LIMIT`] wrapped note lines; a dim
/// "… N more lines" tail names whatever did not fit. A final corner closes the gutter,
/// while the full task details remain behind `Enter` on the task page.
const PEEK_NOTES_LINE_LIMIT: usize = 5;

/// Peek accordion gutter (`    │ `). Copyable content starts after these cells.
const PEEK_DETAIL_INDENT: &str = "    │ ";
/// The final corner joins the note gutter back to the task row above.
const PEEK_DETAIL_END: &str = "    └";

fn detail_lines_for_task(task: &Task, width: u16) -> Vec<(Line<'static>, u16, u16)> {
    let indent = PEEK_DETAIL_INDENT;
    let content_x = u16::try_from(display_width(indent)).unwrap_or(0);
    let content_width = width.saturating_sub(content_x);
    let mut lines: Vec<(Line<'static>, u16, u16)> = Vec::new();
    let push = |lines: &mut Vec<(Line<'static>, u16, u16)>, line: Line<'static>| {
        lines.push((line, content_x, content_width));
    };
    let notes_text = task.notes.as_deref().map(str::trim).unwrap_or_default();
    if notes_text.is_empty() {
        push(
            &mut lines,
            paint_bounded_line(&format!("{indent}no notes yet"), width, style_dim()),
        );
    } else {
        // Preview rows wrap at word boundaries like every other note surface, so
        // a long line continues under itself instead of being cut at the edge.
        let room = (width as usize).saturating_sub(display_width(indent) + 1);
        let mut shown = 0usize;
        let mut remaining = 0usize;
        let mut in_fence = false;
        for note_row in crate::ui::edit::wrap_text(notes_text, room) {
            if shown < PEEK_NOTES_LINE_LIMIT {
                let md = crate::ui::markdown::paint_notes_line(
                    &note_row.text,
                    room,
                    style_plain(),
                    &mut in_fence,
                );
                let mut spans = vec![Span::styled(indent.to_string(), style_dim())];
                spans.extend(md.spans);
                push(&mut lines, crate::ui::markdown::dim_line(Line::from(spans)));
                shown += 1;
            } else {
                remaining += 1;
            }
        }
        if remaining > 0 {
            push(
                &mut lines,
                paint_bounded_line(
                    &format!("{indent}… {remaining} more lines"),
                    width,
                    style_dim(),
                ),
            );
        }
    }
    push(
        &mut lines,
        paint_bounded_line(PEEK_DETAIL_END, width, style_dim()),
    );
    lines
}

/// Builds the list rows plus the index of an open accordion detail or selected task, which
/// must stay fully visible in the viewport.
fn build_list_rows(
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    rail: bool,
) -> (Vec<ListRow>, Option<usize>, Option<usize>) {
    let mut out = Vec::new();
    let mut anchor_last_idx: Option<usize> = None;
    let mut selected_idx: Option<usize> = None;
    // Every section heading has one blank list row above it. This row remains ordinary list
    // content, so selection scrolling keeps the section and its first task reachable.
    out.push(ListRow::Blank);

    // The peek weaves inline under its row in BOTH tiers: rows keep their tier styling,
    // the peek body is ordinary list content that scrolls with the section. The rail has
    // no peek: the task column already shows the task.
    let detail_target = if matches!(model.overlay, QueueOverlay::None) && !rail {
        model.detail_open
    } else {
        None
    };

    let push_task = |id: Uuid,
                     out: &mut Vec<ListRow>,
                     selected_idx: &mut Option<usize>,
                     anchor_last_idx: &mut Option<usize>,
                     in_project_section: bool,
                     indented_under_thread: bool| {
        let Some(task) = model.tasks.iter().find(|task| task.id == id) else {
            // Stale ids may outlive a snapshot refresh. Skip them without inventing a row.
            return;
        };
        let meta = row_meta(task, model.now, in_project_section);
        let selected = model.selection_id == Some(task.id)
            && !matches!(model.overlay, QueueOverlay::ScopeDropdown { .. });
        // A long title wraps onto continuation lines indented under its own first
        // row; every painted line carries the task's hit target and selection.
        let identifier = task.number.map(|number| format!("T{number}"));
        let lines = if rail {
            paint_rail_row_lines(
                task,
                identifier.as_deref(),
                selected,
                geo.row_width,
                usize::from(indented_under_thread) * 2,
            )
        } else {
            paint_task_row_lines(
                &TaskRowPaint {
                    glyph: status_glyph(task.status),
                    identifier: identifier.as_deref(),
                    title: &task.title,
                    meta: &meta,
                    selected,
                    title_bold: false,
                },
                geo,
                usize::from(indented_under_thread) * 2,
            )
        };
        if selected {
            *selected_idx = Some(out.len());
        }
        for painted in lines {
            out.push(ListRow::Task {
                id: task.id,
                line: painted.line,
                identifier: painted.identifier,
                content_x: painted.content_x,
                content_width: painted.content_width,
            });
        }
        if selected {
            // Selection follow anchors the whole block, so a two-line selection
            // never leaves its tail below the fold.
            *selected_idx = Some(out.len() - 1);
        }
        if detail_target == Some(task.id) {
            for (line, content_x, content_width) in detail_lines_for_task(task, geo.row_width) {
                out.push(ListRow::Detail {
                    line,
                    content_x,
                    content_width,
                });
            }
            *anchor_last_idx = Some(out.len() - 1);
        }
    };

    for (section_idx, section) in model.view.sections.iter().enumerate() {
        // The rail drops the done drawer: it is a navigation strip for open work.
        if rail && section.kind == SectionKind::Done {
            continue;
        }
        if !matches!(out.last(), Some(&ListRow::Blank)) {
            out.push(ListRow::Blank);
        }

        if model.at_home && model.home_tab == BoardTab::Projects && section.project_label.is_some()
        {
            let path = section.project_label.clone().unwrap_or_default();
            let collapsed = model.collapsed_projects.contains(&path);
            out.push(ListRow::ProjectGroupHeader {
                section_idx,
                line: paint_project_group_header(section, geo.row_width, collapsed),
            });
            out.push(ListRow::Blank);
            if collapsed {
                continue;
            }
            if section.empty_hint {
                out.push(ListRow::Hint(paint_empty_hint(geo.row_width)));
                continue;
            }
            for id in section.task_ids.iter().copied() {
                push_task(
                    id,
                    &mut out,
                    &mut selected_idx,
                    &mut anchor_last_idx,
                    true,
                    false,
                );
            }
            continue;
        }

        if model.at_home && model.home_tab == BoardTab::Threads && section.thread_label.is_some() {
            let thread = section.thread_label.clone().unwrap_or_default();
            let collapsed = model.collapsed_threads.contains(&thread);
            out.push(ListRow::ThreadGroupHeader {
                section_idx,
                line: paint_thread_group_header(section, geo.row_width, collapsed),
            });
            out.push(ListRow::Blank);
            if collapsed {
                continue;
            }
            if section.empty_hint {
                out.push(ListRow::Hint(paint_empty_hint(geo.row_width)));
                continue;
            }
            for (subgroup_idx, subgroup) in section.thread_subgroups.iter().enumerate() {
                let key = ThreadProjectCollapseKey {
                    thread: thread.clone(),
                    project_path: subgroup.project_path.clone(),
                };
                let sub_collapsed = model.collapsed_thread_projects.contains(&key);
                out.push(ListRow::ThreadProjectHeader {
                    section_idx,
                    subgroup_idx,
                    line: paint_thread_project_header(
                        subgroup.project_path.as_deref(),
                        subgroup.task_ids.len(),
                        geo.row_width,
                        geo,
                    ),
                });
                if sub_collapsed {
                    continue;
                }
                for id in subgroup.task_ids.iter().copied() {
                    push_task(
                        id,
                        &mut out,
                        &mut selected_idx,
                        &mut anchor_last_idx,
                        true,
                        true,
                    );
                }
                out.push(ListRow::Blank);
            }
            continue;
        }

        out.push(ListRow::Header(
            section.kind,
            section_idx,
            paint_section_header(section, geo.row_width, model.at_home, model.home_tab),
        ));
        out.push(ListRow::Blank);
        if section.empty_hint {
            out.push(ListRow::Hint(paint_empty_hint(geo.row_width)));
            continue;
        }
        let in_project_section =
            section.kind == SectionKind::OnDeck && section.project_label.is_some();
        if !section.thread_blocks.is_empty() {
            for block in &section.thread_blocks {
                out.push(ListRow::ThreadHeader(paint_thread_header(
                    &block.name,
                    block.open_count,
                    geo,
                )));
                for id in block.task_ids.iter().copied() {
                    push_task(
                        id,
                        &mut out,
                        &mut selected_idx,
                        &mut anchor_last_idx,
                        in_project_section,
                        true,
                    );
                }
                out.push(ListRow::Blank);
            }
            for id in section.loose_task_ids.iter().copied() {
                push_task(
                    id,
                    &mut out,
                    &mut selected_idx,
                    &mut anchor_last_idx,
                    in_project_section,
                    false,
                );
            }
        } else {
            for id in section.task_ids.iter().copied() {
                push_task(
                    id,
                    &mut out,
                    &mut selected_idx,
                    &mut anchor_last_idx,
                    in_project_section,
                    false,
                );
            }
        }
    }
    (out, anchor_last_idx, selected_idx)
}

/// Rail row(s) for one task: `  <mark> T<n> <title>`, wrapped at the rail width with a
/// four-cell continuation indent and a trailing pad cell so no glyph touches the rule.
/// The selected row paints the hollow `▹` marker; every other row keeps its status glyph.
fn paint_rail_row_lines(
    task: &Task,
    identifier: Option<&str>,
    selected: bool,
    row_width: u16,
    leading_indent: usize,
) -> Vec<TaskRowLine> {
    let row_w = row_width as usize;
    let mark = if selected {
        "▹"
    } else {
        status_glyph(task.status)
    };
    let prefix = format!("{}  {mark} ", " ".repeat(leading_indent));
    let identifier_width = identifier.map(display_width).unwrap_or(0);
    let identifier_gap = usize::from(identifier_width > 0);
    let prefix_cells = display_width(&prefix);
    let title_x = prefix_cells + identifier_width + identifier_gap;
    let room = row_w.saturating_sub(1).saturating_sub(title_x).max(1);
    let continuation_indent = leading_indent + 4;
    let continuation_room = row_w
        .saturating_sub(1)
        .saturating_sub(continuation_indent)
        .max(1);
    let segments: Vec<String> =
        crate::ui::edit::wrap_text(&task.title, room.min(continuation_room))
            .into_iter()
            .map(|wrapped| wrapped.text)
            .collect();
    let mut lines = Vec::with_capacity(segments.len());
    let mut spans = vec![Span::styled(prefix, style_dim())];
    if let Some(identifier) = identifier {
        spans.push(Span::styled(identifier.to_string(), style_dim()));
        spans.push(Span::styled(" ".to_string(), style_dim()));
    }
    let head = segments.first().cloned().unwrap_or_default();
    let head_cells = display_width(&head);
    spans.push(Span::styled(head, style_dim()));
    lines.push(TaskRowLine {
        line: bound_line(Line::from(spans), row_w),
        content_x: u16::try_from(prefix_cells).unwrap_or(u16::MAX),
        content_width: u16::try_from(identifier_width + identifier_gap + head_cells)
            .unwrap_or(u16::MAX)
            .max(1),
        identifier: identifier.map(|_| {
            (
                u16::try_from(prefix_cells).unwrap_or(u16::MAX),
                u16::try_from(identifier_width).unwrap_or(u16::MAX),
            )
        }),
    });
    for segment in segments.iter().skip(1) {
        let cells = display_width(segment);
        lines.push(TaskRowLine {
            line: bound_line(
                Line::from(Span::styled(
                    format!("{}{segment}", " ".repeat(continuation_indent)),
                    style_dim(),
                )),
                row_w,
            ),
            content_x: u16::try_from(continuation_indent).unwrap_or(u16::MAX),
            content_width: u16::try_from(cells).unwrap_or(u16::MAX).max(1),
            identifier: None,
        });
    }
    lines
}

/// Paint one scoped ON DECK thread block label. Headers are presentation-only: their
/// associated ids stay in the task-only queue stream, so neither keyboard selection nor
/// mouse hit testing can land on this row.
fn paint_thread_header(name: &str, open_count: usize, geo: &TierGeometry) -> Line<'static> {
    let text = match geo.tier {
        Tier::Standard => format!("  #{name} · {open_count} open"),
        Tier::Compact => format!("  #{name} {open_count}"),
    };
    paint_bounded_line(&text, geo.row_width, style_dim())
}

fn paint_project_group_header(
    section: &QueueSection,
    width: u16,
    collapsed: bool,
) -> Line<'static> {
    let title = section
        .project_label
        .as_deref()
        .map(short_project)
        .unwrap_or("project");
    paint_collapsible_header(
        if collapsed { "▸" } else { "▾" },
        title,
        section.count,
        width,
    )
}

fn paint_thread_group_header(section: &QueueSection, width: u16, collapsed: bool) -> Line<'static> {
    let title = section
        .thread_label
        .as_deref()
        .map(|name| format!("#{name}"))
        .unwrap_or_else(|| "#thread".to_string());
    paint_collapsible_header(
        if collapsed { "▸" } else { "▾" },
        &title,
        section.count,
        width,
    )
}

fn paint_thread_project_header(
    project_path: Option<&str>,
    count: usize,
    width: u16,
    geo: &TierGeometry,
) -> Line<'static> {
    let title = match project_path {
        Some(path) => short_project(path).to_string(),
        None => "desk".to_string(),
    };
    let text = match geo.tier {
        Tier::Standard => format!("  {title} · {count} open"),
        Tier::Compact => format!("  {title} {count}"),
    };
    paint_bounded_line(&text, width, style_dim())
}

fn paint_collapsible_header(chevron: &str, title: &str, count: usize, width: u16) -> Line<'static> {
    let left = present_line(&format!(" {chevron} {title} "), width as usize);
    let right_budget = (width as usize).saturating_sub(display_width(&left));
    let right = if right_budget == 0 {
        String::new()
    } else {
        present_line(&format!("{} ", count), right_budget)
    };
    let rule_w = (width as usize)
        .saturating_sub(display_width(&left))
        .saturating_sub(display_width(&right));
    let rule = "─".repeat(rule_w);
    bound_line(
        Line::from(vec![
            Span::styled(left, style_bold()),
            Span::styled(rule, style_dim()),
            Span::styled(right, style_dim()),
        ]),
        width as usize,
    )
}

fn paint_section_header(
    section: &QueueSection,
    width: u16,
    at_home: bool,
    home_tab: BoardTab,
) -> Line<'static> {
    let title = section_title(section, at_home, home_tab);
    let left = present_line(&format!(" {title} "), width as usize);
    let right_budget = (width as usize).saturating_sub(display_width(&left));
    let right = if right_budget == 0 {
        String::new()
    } else {
        present_line(&format!("{} ", section.count), right_budget)
    };
    let rule_w = (width as usize)
        .saturating_sub(display_width(&left))
        .saturating_sub(display_width(&right));
    let rule = "─".repeat(rule_w);
    bound_line(
        Line::from(vec![
            Span::styled(left, style_bold()),
            Span::styled(rule, style_dim()),
            Span::styled(right, style_dim()),
        ]),
        width as usize,
    )
}

fn section_title(section: &QueueSection, at_home: bool, home_tab: BoardTab) -> String {
    match section.kind {
        SectionKind::InMotion => "IN MOTION".to_string(),
        SectionKind::Done => "DONE".to_string(),
        SectionKind::OnDeck
            if at_home && home_tab == BoardTab::Desk && section.project_label.is_none() =>
        {
            "desk".to_string()
        }
        SectionKind::OnDeck => "ON DECK".to_string(),
    }
}

fn paint_empty_hint(width: u16) -> Line<'static> {
    // " no open tasks here — P rescope or + capture"
    let spans = vec![
        Span::styled("    no open tasks here — ".to_string(), style_dim()),
        Span::styled("P".to_string(), style_bold()),
        Span::styled(" rescope or ".to_string(), style_dim()),
        Span::styled("+".to_string(), style_bold()),
        Span::styled(" capture".to_string(), style_dim()),
    ];
    bound_line(Line::from(spans), width as usize)
}

fn paint_rule_row(width: u16) -> Line<'static> {
    let rule = "─".repeat(width as usize);
    Line::from(Span::styled(rule, style_dim()))
}

/// The status line, plus the `u Undo` control's (column, width) inside it when the
/// delete-recovery notice actually painted one that survived clipping (I2, Minor 1,
/// round 2).
///
/// `undo_offset` is the column the caller's own composition put [`DELETE_NOTICE_UNDO`] at
/// inside `message` (see [`QueueFrameModel::status_undo_offset`]), not a position this
/// function locates itself: `message` is partly user text (the deleted task's title), so a
/// `find` for the control's literal words here would resolve to whichever occurrence came
/// first in that text rather than the one actually painted -- Minor 1's regression. The one
/// thing this function still verifies is that the control survived the row's own width
/// clipping, exactly as the pre-fix code did for a control it had located by search.
/// Paint a shared bottom input: a bold prompt glyph, mono body, dim placeholder
/// while empty, and an optional refusal tail. The caller supplies already-windowed
/// text at `width - 2` cells, exactly matching the cursor budget below.
pub(crate) fn paint_bottom_input_slot(
    frame: &mut Frame<'_>,
    surface: Rect,
    row: u16,
    width: u16,
    input: &BottomInputSlot<'_>,
) {
    // Wrapped continuation rows stack upward from the input line, inside the
    // reserved blank rows; anything past the frame top is dropped rather than
    // overwriting chrome above them.
    let above_count = input.above_rows.len() as u16;
    for (index, above) in input.above_rows.iter().enumerate() {
        let offset = above_count - index as u16;
        let Some(y) = row.checked_sub(offset) else {
            break;
        };
        if y == 0 && offset > 0 {
            break;
        }
        put_line(
            frame,
            surface,
            y,
            width,
            bound_line(
                Line::from(Span::styled(format!("\u{258e} {above}"), style_bold())),
                width as usize,
            ),
        );
    }
    let body = &input.text;
    let placeholder = input.placeholder;
    let cursor_col = input.cursor_col;
    let refusal = input.refusal;
    let prefix = "▎ ";
    let prefix_width = display_width(prefix) as u16;
    let avail = width.saturating_sub(prefix_width) as usize;
    let empty = body.is_empty();
    let shown = present_line(if empty { placeholder } else { body }, avail);
    let style = if empty { style_dim() } else { style_bold() };
    let mut spans = vec![
        Span::styled(prefix, style_bold()),
        Span::styled(shown.clone(), style),
    ];
    if let Some(refusal) = refusal {
        // AC-13: the line owns its refusal. It paints as a dim tail on the input
        // line itself — the refusal only ever sets on a draft that is empty after
        // trim, so it never crowds out real text.
        let room = avail.saturating_sub(display_width(&shown) + 1);
        spans.push(Span::styled(
            format!(" {}", present_line(refusal, room)),
            style_dim(),
        ));
    }
    put_line(
        frame,
        surface,
        row,
        width,
        bound_line(Line::from(spans), width as usize),
    );
    let caret_row = row
        .checked_sub(input.cursor_row_offset.min(above_count))
        .unwrap_or(row);
    place_edit_cursor(
        frame,
        local_rect(
            surface,
            Rect::new(
                prefix_width.min(width.saturating_sub(1)),
                caret_row,
                width.saturating_sub(prefix_width),
                1,
            ),
        ),
        cursor_col,
    );
}

/// Paint a shared input-slot message in its reserved blank row, never over the cursor.
fn paint_bottom_input_message(
    frame: &mut Frame<'_>,
    surface: Rect,
    row: u16,
    width: u16,
    message: &str,
) {
    put_line(
        frame,
        surface,
        row,
        width,
        bound_line(
            Line::from(Span::styled(
                present_line(message, width as usize),
                style_reverse_bold(),
            )),
            width as usize,
        ),
    );
}

fn paint_quick_add_hint(
    frame: &mut Frame<'_>,
    surface: Rect,
    row: u16,
    width: u16,
    project_scope: bool,
    message: Option<&str>,
    tier: Tier,
) {
    let (text, style) = if let Some(message) = message {
        (message.to_string(), style_reverse_bold())
    } else {
        let text = match tier {
            Tier::Standard => format!(
                "Enter save · esc cancel · tab expand · scope: {}",
                if project_scope {
                    "this project"
                } else {
                    "desk"
                }
            ),
            Tier::Compact => format!(
                "⏎ save · esc · tab · {}",
                if project_scope { "proj" } else { "desk" }
            ),
        };
        (text, style_dim())
    };
    put_line(
        frame,
        surface,
        row,
        width,
        bound_line(
            Line::from(Span::styled(present_line(&text, width as usize), style)),
            width as usize,
        ),
    );
}

fn paint_status_line(
    message: Option<&str>,
    undo_offset: Option<usize>,
    counts: StatusCounts,
    width: u16,
    hint: Option<StatusHint<'_>>,
) -> (Line<'static>, Option<(u16, u16)>) {
    let (left, left_style, undo_hit) = if let Some(msg) = message {
        let text = format!(" {msg}");
        let shown = present_line(&text, width as usize);
        let control_w = display_width(crate::ui::board::DELETE_NOTICE_UNDO);
        // The leading space `text` adds ahead of `message` shifts the offset by one column.
        let undo_hit = undo_offset.and_then(|offset| {
            let x = offset.saturating_add(1);
            if x.saturating_add(control_w) <= display_width(&shown) {
                Some((x as u16, control_w as u16))
            } else {
                None
            }
        });
        (shown, strip_color(style_bold()), undo_hit)
    } else {
        // Idle status: done count only. In-motion is already on the section header.
        (
            present_line(&format!(" {} done", counts.done), width as usize),
            style_dim(),
            None,
        )
    };
    let left_w = display_width(&left);
    // The left text always wins the row. The hint drops its crumb first, then its keys.
    let right = hint.and_then(|hint| {
        let fits = |text: &str| left_w + 2 + display_width(text) < width as usize;
        let with_crumb = hint
            .crumb
            .map(|crumb| format!("{crumb}    {}", hint.keys))
            .filter(|text| fits(text));
        with_crumb.or_else(|| fits(hint.keys).then(|| hint.keys.to_string()))
    });
    let right_w = right.as_deref().map(display_width).unwrap_or(0);
    let trailing = usize::from(right.is_some());
    let pad_w = (width as usize)
        .saturating_sub(left_w)
        .saturating_sub(right_w)
        .saturating_sub(trailing);
    let mut spans = vec![
        Span::styled(left, left_style),
        Span::styled(" ".repeat(pad_w), style_plain()),
    ];
    if let Some(right) = right {
        spans.push(Span::styled(right, style_dim()));
        spans.push(Span::styled(" ", style_plain()));
    }
    (bound_line(Line::from(spans), width as usize), undo_hit)
}

/// Selector: home tabs left, project picker chip right.
fn paint_selector_row(
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
) -> (Line<'static>, Vec<(QueueHitTarget, u16, u16)>) {
    let width = geo.row_width as usize;
    let mut hits = Vec::new();
    if width == 0 {
        return (Line::from(""), hits);
    }

    let mut left_spans: Vec<Span<'static>> = Vec::new();
    let mut left_width = 0usize;

    if model.at_home {
        for (index, (tab, label)) in [
            (BoardTab::Desk, "desk"),
            (BoardTab::Projects, "projects"),
            (BoardTab::Threads, "threads"),
        ]
        .iter()
        .enumerate()
        {
            if index > 0 {
                left_spans.push(Span::styled(" · ".to_string(), style_dim()));
                left_width += 3;
            }
            let active = model.home_tab == *tab;
            let text = format!(" {label} ");
            let shown = present_line(&text, width.saturating_sub(left_width));
            let w = display_width(&shown);
            hits.push((
                QueueHitTarget::HomeTab(*tab),
                left_width as u16,
                w.max(1) as u16,
            ));
            left_spans.push(Span::styled(
                shown,
                if active {
                    style_reverse_bold()
                } else {
                    style_dim()
                },
            ));
            left_width += w;
        }
    }

    let mut spans = left_spans;
    if model.at_home {
        let pad = width.saturating_sub(left_width);
        if pad > 0 {
            spans.push(Span::styled(" ".repeat(pad), style_plain()));
        }
    } else {
        let chip_label = model.scope_label;
        let chip_budget = (geo.selector_chip_max as usize).min(width);
        let chip_raw = format!("{chip_label} ▾ ");
        let chip_text = present_line(&chip_raw, chip_budget);
        let chip_w = display_width(&chip_text);
        let spacer_w = width.saturating_sub(left_width).saturating_sub(chip_w);
        let chip_x = (left_width + spacer_w) as u16;
        spans.push(Span::styled(" ".repeat(spacer_w), style_plain()));
        spans.push(Span::styled(
            present_line(chip_label, chip_budget.saturating_sub(display_width(" ▾ "))),
            style_bold(),
        ));
        spans.push(Span::styled(
            present_line(" ▾ ", chip_budget.saturating_sub(display_width(chip_label))),
            style_dim(),
        ));
        hits.push((QueueHitTarget::ProjectChip, chip_x, chip_w.max(1) as u16));
    }

    (bound_line(Line::from(spans), width), hits)
}

/// Verb bar: ` key label · key label …`, trimmed to `budget` entries.
///
/// Returns hit regions as (verb_index, x, width).
fn mutating_verb_key(key: &str) -> bool {
    matches!(
        key,
        "s" | "d" | "o" | "b" | "x" | "a" | "e" | "u" | "n" | "q"
    )
}

fn paint_verb_bar(
    entries: &[VerbEntry<'_>],
    budget: usize,
    width: u16,
    prefix: bool,
) -> (Line<'static>, Vec<(usize, u16, u16)>) {
    let shown: Vec<&VerbEntry<'_>> = entries.iter().take(budget).collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut hits = Vec::new();
    let mut x = 0u16;

    spans.push(Span::styled(" ".to_string(), style_plain()));
    x = x.saturating_add(1);

    for (i, entry) in shown.iter().enumerate() {
        if i > 0 {
            let sep = " · ";
            spans.push(Span::styled(sep.to_string(), style_dim()));
            x = x.saturating_add(display_width(sep) as u16);
        }
        let key = if prefix && mutating_verb_key(entry.key) {
            format!("ctrl+{}", entry.key)
        } else {
            entry.key.to_string()
        };
        let key_w = display_width(&key) as u16;
        let start = x;
        spans.push(Span::styled(key, style_bold()));
        x = x.saturating_add(key_w);
        let label = format!(" {}", entry.label);
        let label_w = display_width(&label) as u16;
        spans.push(Span::styled(label, style_plain()));
        x = x.saturating_add(label_w);
        hits.push((i, start, key_w.saturating_add(label_w)));
    }

    let line = bound_line(Line::from(spans), width as usize);
    // Drop hits that fall entirely past the clipped width.
    hits.retain(|(_, start, w)| (*start as usize) < width as usize && *w > 0);
    (line, hits)
}

fn row_meta(task: &Task, now: SystemTime, in_project_section: bool) -> String {
    let age = format_age(now, task.updated_at);
    let project = if !in_project_section {
        match &task.scope {
            TaskScope::Project { path } => Some(short_project(path).to_string()),
            TaskScope::Global => None,
        }
    } else {
        None
    };
    fit_row_meta(project, age)
}

/// Keep the age intact and shrink the project basename inside the standard meta column.
fn fit_row_meta(project: Option<String>, age: String) -> String {
    const CONTENT: usize = 27;
    const SEP: &str = " · ";
    let project = project.and_then(|name| {
        let room = CONTENT.saturating_sub(display_width(&age).saturating_add(display_width(SEP)));
        (room > 0).then(|| present_line(&name, room))
    });
    [project, Some(age)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(SEP)
}

pub(crate) fn format_age(now: SystemTime, then: SystemTime) -> String {
    let secs = now.duration_since(then).unwrap_or_default().as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

pub(crate) fn short_project(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

fn put_line(frame: &mut Frame<'_>, area: Rect, row: u16, width: u16, line: Line<'static>) {
    put_line_at(frame, area, Rect::new(0, row, width, 1), line);
}

/// Paint `line` at an arbitrary local single-row `rect`, padding with plain spaces to
/// `rect.width` so whatever the surface painted underneath cannot bleed through.
fn put_line_at(frame: &mut Frame<'_>, area: Rect, rect: Rect, line: Line<'static>) {
    let rect = local_rect(area, rect);
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let w = rect.width as usize;
    if line.width() >= w {
        frame.render_widget(Paragraph::new(line), rect);
        return;
    }
    let pad = w - line.width();
    let mut spans: Vec<Span<'static>> = line.spans.into_iter().collect();
    spans.push(Span::styled(" ".repeat(pad), style_plain()));
    let padded = Line::from(spans);
    frame.render_widget(Paragraph::new(padded), rect);
}

pub(crate) fn display_width(s: &str) -> usize {
    Line::from(s).width()
}

fn bound_line(line: Line<'static>, max_width: usize) -> Line<'static> {
    if line.width() <= max_width {
        return line;
    }
    // Collapse to a single truncated plain span (should be rare).
    let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let style = line
        .spans
        .first()
        .map(|s| strip_color(s.style))
        .unwrap_or_else(style_plain);
    Line::from(Span::styled(present_line(&plain, max_width), style))
}

/// Find the first forbidden color SGR sequence in `text`, if any.
fn find_color_sgr(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            let params_at = i;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_alphabetic() || b == b'~' {
                    let cmd = b;
                    let params = &text[params_at..i];
                    i += 1;
                    if cmd == b'm' && sgr_params_contain_color(params) {
                        return Some(text[start..i].to_string());
                    }
                    break;
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

fn sgr_params_contain_color(params: &str) -> bool {
    if params.is_empty() || params.chars().all(|c| c == ';') {
        return false; // ESC[m / empty segments == reset
    }
    for raw in params.split(';').filter(|p| !p.is_empty()) {
        let code: u32 = match raw.parse() {
            Ok(n) => n,
            Err(_) => return true, // unparseable: treat as unsafe
        };
        match code {
            // reset / mono intensity / underline / reverse and their offs,
            // including resets to default fg/bg/underline color.
            0 | 1 | 2 | 4 | 7 | 22 | 24 | 27 | 39 | 49 | 59 => {}
            // classic/bright fg/bg and extended color selectors
            30..=38 | 40..=48 | 58 | 90..=107 => return true,
            // italic / blink / hidden / strike etc. are outside the mono set
            _ => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tier::{self, Tier};
    use ratatui::backend::TestBackend;
    use ratatui::{widgets::Paragraph, Terminal};

    fn plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    fn paint_to_buffer(line: Line<'static>, width: u16) -> Buffer {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new(line), frame.area());
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn steps_start_two_rows_after_the_notes_block() {
        let layout = page_content_layout(1, 3, 16);
        assert_eq!(layout.steps_start, 3);

        let layout = page_content_layout(5, 3, 16);
        assert_eq!(layout.steps_start, 7);
    }

    #[test]
    fn present_row_title_and_meta_never_overlap_and_truncation_uses_visible_omission_mark() {
        let geo = tier::resolve(80, 24);
        assert_eq!(geo.tier, Tier::Standard);
        assert!(geo.meta_column_width > 0);

        let long_title = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(4); // 104 chars
        let long_meta = "meta-meta-meta-meta-meta-meta-meta"; // > 28

        let line = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
                identifier: None,
                title: &long_title,
                meta: long_meta,
                selected: false,
                title_bold: false,
            },
            &geo,
        );

        assert!(
            line.width() <= geo.row_width as usize,
            "row width {} exceeds budget {}",
            line.width(),
            geo.row_width
        );

        let text = plain(&line);
        assert!(
            text.contains('…'),
            "truncation must use a visible omission mark, got {text:?}"
        );

        // Title lives in the left budget; meta in the trailing meta column.
        let title_budget = geo.title_width as usize;
        let meta_budget = geo.meta_column_width as usize;
        let left_zone: String = text.chars().take(title_budget).collect();
        let right_zone: String = text
            .chars()
            .skip(text.chars().count().saturating_sub(meta_budget))
            .collect();

        // Meta content (pre-truncation marker of the meta string) must not appear in the title zone.
        assert!(
            !left_zone.contains("meta-meta-meta"),
            "meta leaked into title zone: left={left_zone:?}"
        );
        // Title alphabet run must not appear in the meta zone.
        assert!(
            !right_zone.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            "title leaked into meta zone: right={right_zone:?}"
        );

        // Both sides truncated independently when over budget.
        let title_only = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
                identifier: None,
                title: &long_title,
                meta: "1h",
                selected: false,
                title_bold: false,
            },
            &geo,
        );
        assert!(
            plain(&title_only).contains('…'),
            "long title must truncate with …"
        );

        let meta_only = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
                identifier: None,
                title: "short",
                meta: long_meta,
                selected: false,
                title_bold: false,
            },
            &geo,
        );
        let meta_text = plain(&meta_only);
        assert!(
            meta_text.contains('…'),
            "long meta must truncate with …, got {meta_text:?}"
        );
        assert!(
            meta_text.contains("short"),
            "short title must remain intact when only meta overflows"
        );
    }

    #[test]
    fn rendered_lines_are_display_width_bounded_for_standard_and_compact_budgets() {
        let cases = [
            tier::resolve(80, 24),  // standard
            tier::resolve(120, 30), // standard (wide not returned in M1)
            tier::resolve(77, 24),  // compact
            tier::resolve(48, 18),  // compact
            tier::resolve(40, 10),  // compact
            tier::resolve(1, 1),    // tiny
        ];

        let titles = [
            "",
            "ok",
            "a title with spaces",
            &"很长的标题需要截断处理".repeat(3),
            &"x".repeat(200),
            "safe\u{1b}]52;clipboard\u{7}payload",
        ];
        let metas = ["", "1h", "tsk · 2d", &"m".repeat(80)];

        for geo in cases {
            for title in titles {
                for meta in metas {
                    for selected in [false, true] {
                        let line = paint_task_row(
                            &TaskRowPaint {
                                glyph: "◓",
                                identifier: None,
                                title,
                                meta,
                                selected,
                                title_bold: selected,
                            },
                            &geo,
                        );
                        assert!(
                            line.width() <= geo.row_width as usize,
                            "tier={:?} {}x{} width {} > budget {} title={title:?} meta={meta:?}",
                            geo.tier,
                            geo.width,
                            geo.height,
                            line.width(),
                            geo.row_width
                        );

                        // Compact never paints trailing meta.
                        if geo.meta_column_width == 0 && !meta.is_empty() {
                            let text = plain(&line);
                            // Meta body should not appear; empty meta budget drops it.
                            if display_width(meta) > 2 {
                                assert!(
                                    !text.contains(meta),
                                    "compact row must drop meta, got {text:?}"
                                );
                            }
                        }

                        let bounded = paint_bounded_line(title, geo.row_width, style_bold());
                        assert!(
                            bounded.width() <= geo.row_width as usize,
                            "bounded line exceeded row width on {}x{}",
                            geo.width,
                            geo.height
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn task_identifier_is_dimmed_without_underline() {
        let geo = tier::resolve(80, 24);
        let line = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
                identifier: Some("T30"),
                title: "copy this",
                meta: "1m",
                selected: false,
                title_bold: false,
            },
            &geo,
        );
        let identifier = line
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "T30")
            .expect("identifier span");
        assert!(identifier.style.add_modifier.contains(Modifier::DIM));
        assert!(!identifier.style.add_modifier.contains(Modifier::UNDERLINED));

        let selected = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
                identifier: Some("T30"),
                title: "copy this",
                meta: "1m",
                selected: true,
                title_bold: false,
            },
            &geo,
        );
        let identifier = selected
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "T30")
            .expect("selected identifier span");
        assert!(identifier.style.add_modifier.contains(Modifier::DIM));
        assert!(identifier.style.add_modifier.contains(Modifier::REVERSED));
        assert!(!identifier.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn frame_escape_scan_finds_no_sgr_color_codes_only_bold_dim_underline_reverse() {
        // Scanner accepts mono SGR and rejects color SGR.
        assert_no_color_sgr("plain text");
        assert_no_color_sgr("\u{1b}[1mbold\u{1b}[0m \u{1b}[2mdim\u{1b}[0m");
        assert_no_color_sgr("\u{1b}[4munder\u{1b}[0m \u{1b}[7mrev\u{1b}[0m");
        assert_no_color_sgr("\u{1b}[1;2;4;7mcombo\u{1b}[22;24;27m");
        // Backend cleanup resets default fg, bg, and underline color after mono styles.
        assert_no_color_sgr("\u{1b}[1mbold\u{1b}[22m\u{1b}[2mdim\u{1b}[39m\u{1b}[49m\u{1b}[59m");

        let color_samples = [
            "\u{1b}[31mred\u{1b}[0m",
            "\u{1b}[42mgreen-bg\u{1b}[0m",
            "\u{1b}[38;5;196m256\u{1b}[0m",
            "\u{1b}[48;2;1;2;3mrgb-bg\u{1b}[0m",
            "\u{1b}[91mbright\u{1b}[0m",
            "\u{1b}[3mitalic-not-mono\u{1b}[0m",
        ];
        for sample in color_samples {
            assert!(
                find_color_sgr(sample).is_some(),
                "scanner must reject color/non-mono SGR in {sample:?}"
            );
        }

        // Painted rows use only mono modifiers and leave colors reset.
        let geo = tier::resolve(80, 24);
        let variants = [
            TaskRowPaint {
                glyph: "○",
                identifier: None,
                title: "plain row",
                meta: "1h",
                selected: false,
                title_bold: false,
            },
            TaskRowPaint {
                glyph: "▲",
                identifier: None,
                title: "bold title",
                meta: "tsk · 2m",
                selected: false,
                title_bold: true,
            },
            TaskRowPaint {
                glyph: "◓",
                identifier: None,
                title: "selected row",
                meta: "3d",
                selected: true,
                title_bold: false,
            },
        ];

        for row in variants {
            let line = paint_task_row(&row, &geo);
            for span in &line.spans {
                assert!(
                    span.style.fg.is_none(),
                    "span fg must be unset, got {:?}",
                    span.style.fg
                );
                assert!(
                    span.style.bg.is_none(),
                    "span bg must be unset, got {:?}",
                    span.style.bg
                );
                let extra = span.style.add_modifier.difference(MONO_MODIFIERS);
                assert!(extra.is_empty(), "span has non-mono modifiers {extra:?}");
            }

            let buffer = paint_to_buffer(line, geo.row_width);
            assert_buffer_mono(&buffer);

            // Bounded lines with each mono style also stay color-free.
            for style in [
                style_plain(),
                style_bold(),
                style_dim(),
                style_underline(),
                style_reverse(),
                style_reverse_dim(),
                style_reverse_bold(),
            ] {
                let bounded = paint_bounded_line("chrome · label", geo.row_width, style);
                let buf = paint_to_buffer(bounded, geo.row_width);
                assert_buffer_mono(&buf);
            }
        }

        // strip_color must erase an incoming colored style.
        let cleaned = strip_color(
            Style::default()
                .fg(Color::Red)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
        );
        assert!(cleaned.fg.is_none());
        assert!(cleaned.bg.is_none());
        assert_eq!(cleaned.add_modifier, Modifier::BOLD);
    }
}
