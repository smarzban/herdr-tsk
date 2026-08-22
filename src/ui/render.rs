//! Queue Board Renderer: mono painters and full-frame queue chrome.
//!
//! Pure helpers that take geometry + row content → ratatui `Line`/`Span` using only
//! `Modifier::{BOLD, DIM, UNDERLINED, REVERSED}`, plus [`draw_queue_frame`] for the
//! the deck-only board skeleton.

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
use super::queue::{QueueSection, QueueView, SectionKind, StatusCounts};
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
    pub title: &'a str,
    /// Trailing meta (age / project). Ignored when geometry reserves no meta column.
    pub meta: &'a str,
    pub selected: bool,
    /// Bold the title when unselected (e.g. attention emphasis).
    pub title_bold: bool,
}

/// Paint one task row: glyph+title on the left, right-aligned meta in the meta budget.
///
/// Title and meta never share cells. Over-budget text is clipped through
/// [`present_line`] so truncation always shows `…`. Compact geometry
/// (`meta_column_width == 0`) drops meta and gives the title the full row.
pub fn paint_task_row(row: &TaskRowPaint<'_>, geo: &TierGeometry) -> Line<'static> {
    let row_w = geo.row_width as usize;
    let meta_budget = geo.meta_column_width as usize;
    let title_budget = if meta_budget == 0 {
        row_w
    } else {
        geo.title_width as usize
    };

    let glyph = super::terminal_text(row.glyph);
    let prefix = format!("  {glyph} ");
    let title_room = title_budget.saturating_sub(display_width(&prefix));
    let title = present_line(row.title, title_room);
    let left = fit_left(&format!("{prefix}{title}"), title_budget);
    let left_w = display_width(&left);

    // The prototype leaves one trailing column past the meta text (matched by section
    // headers' own trailing-space right budget); reserve it here so task rows agree with
    // the rest of the frame instead of running meta flush to the frame edge.
    let margin_w = if meta_budget == 0 { 0 } else { 1 };
    let meta_content_budget = meta_budget.saturating_sub(margin_w);

    // Compact (or empty meta): title zone only, padded out to the row width.
    let meta = if meta_budget == 0 || row.meta.is_empty() {
        String::new()
    } else {
        present_line(row.meta, meta_content_budget)
    };
    let meta_w = display_width(&meta);

    // Leader = pad title zone to `title_budget` + right-align pad inside the meta column.
    // When meta is absent the leader simply fills through the end of the row.
    let leader_w = if meta.is_empty() {
        row_w.saturating_sub(left_w)
    } else {
        let title_pad = title_budget.saturating_sub(left_w);
        let meta_pad = meta_content_budget.saturating_sub(meta_w);
        title_pad + meta_pad
    };
    let leader = " ".repeat(leader_w);

    let (left_style, leader_style, meta_style) = if row.selected {
        (style_reverse(), style_reverse_dim(), style_reverse_dim())
    } else {
        let title_style = if row.title_bold {
            style_bold()
        } else {
            style_plain()
        };
        (title_style, style_dim(), style_dim())
    };

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(3);
    spans.push(Span::styled(left, left_style));
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

/// One checklist item as the task page paints it: done flag + text, already extracted
/// from storage by the view model. The page payload consumes these views and never the
/// raw `Task.checklist`, so later surfaces swap consumers without touching storage.
#[derive(Debug, Clone)]
pub struct ChecklistItemView {
    pub done: bool,
    pub text: String,
}

/// The checklist item input as the page paints it on its FOOTER row (AC-25), on the
/// quick-add line pattern: the draft already windowed around its cursor at the
/// footer line's width (row width less the two-cell prompt), the terminal cursor
/// column inside that window, and the line's own empty-text refusal (AC-13) — a
/// short dim tail painted on the line itself, never the board status row.
#[derive(Debug, Clone)]
pub struct ChecklistEditorLine<'a> {
    pub text: String,
    pub cursor_col: u16,
    pub refusal: Option<&'a str>,
}

/// Transient overlay painted above the queue frame (palette, help, scope dropdown).
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
        title: String,
        title_cursor: u16,
        project_scope: bool,
        recovery: bool,
        message: Option<&'a str>,
    },
    /// The task page: a full-height, view-first takeover for one bound task. `focus` is
    /// `None` in view mode; field edits focus the same drafts the board form carries.
    TaskPage {
        /// Status glyph + space + the title draft, windowed to fit beside the status word.
        header: String,
        /// Terminal cursor column inside `header` while the title is being edited.
        title_cursor: Option<u16>,
        /// The task's status word, dim and right-aligned on the header row.
        status_word: &'static str,
        /// View mode: the wrapped notes windowed by the page scroll. Edit mode: the
        /// cursor-windowed draft rows.
        notes_rows: Vec<String>,
        /// Notes cursor (row, col) while the notes are being edited.
        notes_cursor: Option<(u16, u16)>,
        /// Wrapped note rows hidden below the window, named by the divider's tail.
        more_lines: usize,
        /// The extracted checklist item views, in storage order. Empty paints no
        /// checklist section at all: the page is identical to pre-feature for a task
        /// with no items.
        checklist_items: Vec<ChecklistItemView>,
        /// Absolute index of the item cursor's row, when active. The painter turns it
        /// into the row's `▸` gutter marker.
        checklist_cursor: Option<usize>,
        /// First item index the section's window shows (the cursor's scroll window).
        checklist_scroll: usize,
        /// Absolute index of the item the delete verb visibly marked, when armed.
        checklist_marked: Option<usize>,
        /// The footer's one-line add/rename item input, painted on the meta
        /// footer row (AC-25) on the quick-add line pattern.
        checklist_editor: Option<ChecklistEditorLine<'a>>,
        /// Footer: scope · created · updated.
        meta: String,
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
    /// Project chip label (`all projects` or a project name).
    pub scope_label: &'a str,
    /// Whether the session deck scope is structurally the all-projects scope. Kept
    /// separate from `scope_label`, which is display text and can collide with a real
    /// project name.
    pub all_projects_scope: bool,
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
    /// Prefix painted on mutating verb keys (`alt+` / `ctrl+`).
    pub verb_modifier: crate::config::VerbModifier,
    /// Clock for age labels (tests inject a fixed instant).
    pub now: SystemTime,
    /// Optional transient overlay (palette / help / scope dropdown).
    pub overlay: QueueOverlay<'a>,
    /// Session-only read-detail accordion/takeover target (Enter), independent of `overlay`.
    /// Standard: expands inline under the selected row. Compact: full-viewport takeover.
    /// Only painted while `overlay` is `QueueOverlay::None`.
    pub detail_open: Option<Uuid>,
}

/// Logical control under a painted rectangle (rebuilt every frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueHitTarget {
    ProjectChip,
    /// The quick-add input row. Clicking it keeps the already-focused line focused.
    QuickAddInput,
    Task(Uuid),
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
    /// One ON DECK project-group header in the all-projects view, indexed into
    /// `QueueFrameModel::view.sections`. A double-click narrows the session deck scope to
    /// that header's own project: the same session-only jump choosing it in the project
    /// selector dropdown performs. Never pushed for IN MOTION, DONE, the global group,
    /// or any header while the board is already scoped to one project.
    SectionProject(usize),
    /// The open help card: painted full-frame so any click inside it closes it, matching
    /// the keyboard's "any key closes".
    HelpDismiss,
    /// Shared-form title row. A click focuses Title without recreating the form.
    FormTitle,
    /// One painted shared-form Notes row. Any row focuses Notes; its index keeps the hit-map
    /// tied to the cursor-windowed row the renderer actually painted.
    FormNotes(usize),
    /// Shared-form scope row. A click opens the pending scope dropdown, never cycles scope.
    FormScope,
    /// One painted option in a shared form's scope dropdown, indexed into that form's own
    /// `TaskScope` choices. It cannot name the board selector's all-projects choice.
    FormScopeOption(usize),
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
}

impl QueueHitMap {
    fn push(&mut self, target: QueueHitTarget, area: Rect) {
        if area.width > 0 && area.height > 0 {
            self.regions.push(QueueHit { target, area });
        }
    }
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
/// Returns the hit-map recorded beside the paint. Never panics on tiny
/// geometries; every painted line is width-bounded to `geo.row_width`.
pub fn draw_queue_frame(
    frame: &mut Frame<'_>,
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
) -> QueueHitMap {
    let quick_add_geo = quick_add_geometry(*geo, &model.overlay);
    let geo = &quick_add_geo;
    let mut hits = QueueHitMap::default();
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return hits;
    }

    // Clear the frame so leftover cells never leak chrome between sizes.
    let full = Rect::new(0, 0, width, height);
    frame.render_widget(Paragraph::new(Line::from("")), full);

    // The task page owns the whole frame above the bottom chrome: the selector row stays
    // hidden while it is open (list navigation does not apply to a single-task surface).
    let page_active = matches!(model.overlay, QueueOverlay::TaskPage { .. });
    if let Some(row) = geo.selector_row {
        if !page_active {
            let (line, regions) = paint_selector_row(model, geo);
            put_line(frame, row, width, line);
            for (target, x, w) in regions {
                hits.push(target, Rect::new(x, row, w, 1));
            }
        }
    }

    // Task pages replace the viewport entirely. Accordion detail remains inline, so its task
    // rows stay interactive in both tiers.
    let base_list_interactive = true;

    if geo.viewport_height > 0 && !page_active {
        let (list_rows, anchor_last_idx, selected_idx) = build_list_rows(model, geo);
        let top = geo.viewport_top;
        let viewport_h = geo.viewport_height as usize;
        // An expanded accordion can land past the viewport on a long deck. Keep its complete
        // block visible, and otherwise follow the plain selection so mutating verbs never act
        // on an invisible row.
        let follow_idx = anchor_last_idx.or(selected_idx);
        let scroll = match follow_idx {
            Some(idx) if idx >= viewport_h => idx + 1 - viewport_h,
            _ => 0,
        };
        for (offset, list_row) in list_rows
            .into_iter()
            .skip(scroll)
            .take(viewport_h)
            .enumerate()
        {
            let y = top + offset as u16;
            match list_row {
                ListRow::Blank => put_line(frame, y, width, Line::from("")),
                ListRow::Header(kind, section_idx, line) => {
                    put_line(frame, y, width, line);
                    if kind == SectionKind::Done && base_list_interactive {
                        hits.push(QueueHitTarget::Drawer, Rect::new(0, y, width, 1));
                    }
                    // An all-projects ON DECK group header names the project it groups;
                    // offer the row as that project's own scope control. Scoped views
                    // title the section plain ON DECK, so no target is pushed there.
                    if kind == SectionKind::OnDeck
                        && base_list_interactive
                        && model.all_projects_scope
                        && model
                            .view
                            .sections
                            .get(section_idx)
                            .is_some_and(|section| section.project_label.is_some())
                    {
                        hits.push(
                            QueueHitTarget::SectionProject(section_idx),
                            Rect::new(0, y, width, 1),
                        );
                    }
                }
                ListRow::Hint(line) => put_line(frame, y, width, line),
                ListRow::Task { id, line } => {
                    put_line(frame, y, width, line);
                    if base_list_interactive {
                        hits.push(QueueHitTarget::Task(id), Rect::new(0, y, width, 1));
                    }
                }
                ListRow::Detail(line) => put_line(frame, y, width, line),
            }
        }
    }

    if let Some(row) = geo.rule_row {
        put_line(frame, row, width, paint_rule_row(width));
    }

    if let Some(row) = geo.status_row {
        if let QueueOverlay::QuickAdd {
            title,
            title_cursor,
            recovery,
            message,
            ..
        } = &model.overlay
        {
            // Quick-add reserves this blank row above its input at every operable geometry.
            // Validation and context refusals cannot use the ordinary status row because the
            // input owns it, while save recovery already owns the verb row.
            if !recovery {
                if let Some(message_row) = row.checked_sub(1).filter(|message_row| {
                    geo.rule_row.is_some_and(|rule_row| *message_row > rule_row)
                }) {
                    if let Some(message) = message {
                        paint_quick_add_message(frame, message_row, width, message);
                    }
                }
            }
            paint_quick_add_status(frame, row, width, title, *title_cursor);
            hits.push(QueueHitTarget::QuickAddInput, Rect::new(0, row, width, 1));
        } else {
            let (line, undo_hit) = paint_status_line(
                model.status_message,
                model.status_undo_offset,
                model.view.counts,
                width,
            );
            put_line(frame, row, width, line);
            if let Some((x, w)) = undo_hit {
                hits.push(QueueHitTarget::DeleteNoticeUndo, Rect::new(x, row, w, 1));
            }
        }
    }

    if let Some(row) = geo.verb_row {
        let budget = geo.verb_bar_entry_budget as usize;
        let verb_items = match &model.overlay {
            QueueOverlay::Palette { .. } => PALETTE_VERBS,
            QueueOverlay::Help { .. } => HELP_VERBS,
            QueueOverlay::ScopeDropdown { .. } => SCOPE_VERBS,
            QueueOverlay::QuickAdd { recovery, .. } if *recovery => &[],
            QueueOverlay::QuickAdd { .. } => QUICK_ADD_VERBS,
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
        )
        .then_some(model.verb_modifier);
        if let QueueOverlay::QuickAdd {
            project_scope,
            recovery: true,
            message,
            ..
        } = &model.overlay
        {
            paint_quick_add_hint(frame, row, width, *project_scope, *message, geo.tier);
        } else {
            let (line, verb_hits) = paint_verb_bar(verb_items, budget, width, prefix_verbs);
            put_line(frame, row, width, line);
            for (index, x, w) in verb_hits {
                hits.push(QueueHitTarget::Verb(index), Rect::new(x, row, w, 1));
            }
        }
    }

    paint_overlay(frame, model, geo, &mut hits);

    hits
}

/// Reserve breathing room around the quick-add input by taking two rows from the list.
///
/// At the 40×10 operating floor this leaves four list rows, so both blank rows remain. On
/// shorter frames with fewer than two viewport rows we retain the ordinary compact geometry:
/// functional chrome wins over decorative spacing.
fn quick_add_geometry(mut geo: TierGeometry, overlay: &QueueOverlay<'_>) -> TierGeometry {
    if !matches!(overlay, QueueOverlay::QuickAdd { .. }) || geo.viewport_height < 2 {
        return geo;
    }

    geo.viewport_height -= 2;
    geo.rule_row = geo.rule_row.map(|row| row.saturating_sub(2));
    geo.status_row = geo.status_row.map(|row| row.saturating_sub(1));
    geo
}

pub(crate) const QUICK_ADD_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "enter",
        label: "save",
    },
    VerbEntry {
        key: "ctrl+enter",
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
        key: "enter",
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
        key: "ctrl+enter",
        label: "save",
    },
    VerbEntry {
        key: "alt+enter",
        label: "save",
    },
    VerbEntry {
        key: "tab",
        label: "scope",
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
        CaptureField::Scope => FORM_SCOPE_VERBS,
    }
}

/// Verb bar for the Title editor: one field, so Tab has nothing to move focus
/// between and plain Enter saves.
const EDIT_TITLE_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "enter",
        label: "save",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];

/// Verb bar for the Notes editor: plain Enter opens a line (multi-line
/// field), so the real save chord is Ctrl+Enter -- and, per ADR 0006, Alt+Enter is an
/// *equal* save chord, not a fallback: Ctrl+Enter arrives bare (indistinguishable from a
/// plain Enter) on terminals without the disambiguating keyboard protocol, where the
/// advertised key would insert a line break instead of saving. Name both. Tab stays unbound.
const EDIT_NOTES_VERBS: &[VerbEntry<'static>] = &[
    VerbEntry {
        key: "ctrl+enter",
        label: "save",
    },
    VerbEntry {
        key: "alt+enter",
        label: "save",
    },
    VerbEntry {
        key: "esc",
        label: "cancel",
    },
];

const HELP_VERBS: &[VerbEntry<'static>] = &[VerbEntry {
    key: "any",
    label: "close",
}];

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
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
    hits: &mut QueueHitMap,
) {
    match &model.overlay {
        QueueOverlay::None => {}
        QueueOverlay::Palette { query, commands } => {
            paint_palette_overlay(frame, geo, query, commands, hits);
        }
        QueueOverlay::Help { lines } => {
            paint_help_overlay(frame, geo, lines, hits);
        }
        QueueOverlay::ScopeDropdown { options, selected } => {
            paint_scope_dropdown(
                frame,
                geo,
                options,
                *selected,
                QueueHitTarget::ProjectOption,
                hits,
            );
        }
        QueueOverlay::EditTitle {
            ref draft,
            cursor_col,
        } => {
            paint_edit_title_overlay(frame, geo, draft, *cursor_col);
        }
        QueueOverlay::EditNotes {
            ref rows,
            cursor_row,
            cursor_col,
        } => {
            paint_edit_notes_overlay(frame, geo, rows, *cursor_row, *cursor_col);
        }
        QueueOverlay::QuickAdd { .. } => {}
        QueueOverlay::TaskPage {
            ref header,
            title_cursor,
            status_word,
            ref notes_rows,
            notes_cursor,
            more_lines,
            ref checklist_items,
            checklist_cursor,
            checklist_scroll,
            checklist_marked,
            ref checklist_editor,
            ref meta,
            focus,
            scope_dropdown,
        } => {
            paint_task_page(
                frame,
                geo,
                header,
                *title_cursor,
                status_word,
                notes_rows,
                *notes_cursor,
                *more_lines,
                checklist_items,
                *checklist_cursor,
                *checklist_scroll,
                *checklist_marked,
                checklist_editor.as_ref(),
                meta,
                *focus,
                hits,
            );
            if let Some(dropdown) = scope_dropdown {
                paint_page_scope_dropdown(frame, geo, *dropdown, hits);
            }
        }
    }
}

fn clear_compact_takeover(frame: &mut Frame<'_>, geo: &TierGeometry) {
    // `Clear` resets every cell in the region to a blank default cell. A `Paragraph::new("")`
    // does not: it only sets style over the area and writes no symbols, so it never erases
    // whatever the base list already painted underneath it. Compact is a full-viewport
    // takeover, so the whole viewport is cleared before the editor paints its own rows.
    if geo.tier == Tier::Compact && geo.viewport_height > 0 {
        let area = Rect::new(0, geo.viewport_top, geo.row_width, geo.viewport_height);
        frame.render_widget(Clear, area);
    }
}

fn paint_edit_title_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    draft: &str,
    cursor_col: u16,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return;
    }
    clear_compact_takeover(frame, geo);
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
    place_edit_cursor(frame, region, cursor_col.min(avail as u16));
}

/// Paint full-width notes editor (standard or compact takeover).
fn paint_edit_notes_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    rows: &[String],
    cursor_row: u16,
    cursor_col: u16,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 {
        return;
    }
    clear_compact_takeover(frame, geo);
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
        place_edit_cursor_at(frame, region, cursor_row, cursor_col.min(avail as u16));
    }
}

fn paint_palette_overlay(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    query: &str,
    commands: &[PaletteCommandRow<'_>],
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height < 3 {
        return;
    }
    let max_rows = if geo.tier == Tier::Compact {
        // Compact: fill the list viewport when present, else as many rows as fit above chrome.
        geo.viewport_height.max(1) as usize
    } else {
        6usize
    };
    // Query sits on the status row when present; otherwise the last content row.
    let query_row = geo.status_row.unwrap_or(height.saturating_sub(2));
    // The dim rule row is base chrome every other the scene keeps: stop the command
    // list one row above it instead of one row above the query, which reached into the
    // rule row and overwrote it with command text.
    let list_bottom = geo
        .rule_row
        .map(|r| r.saturating_sub(1))
        .unwrap_or_else(|| query_row.saturating_sub(1));
    // Reserve one row for the `command` header directly above the list, and never paint
    // above `viewport_top`: `list_bottom` is a row *index*, not a row
    // *count*, so windowing straight off it let the header climb onto (or above) the
    // selector row on short/compact terminals once the command count passed the
    // available space. The rows actually available for header + list is the span between
    // `viewport_top` and `list_bottom` inclusive; the header eats one of them.
    let available = list_bottom.saturating_sub(geo.viewport_top) as usize;
    let rows = available.min(max_rows).min(commands.len());
    if rows > 0 {
        let selected = commands
            .iter()
            .position(|command| command.selected)
            .unwrap_or(0);
        let scroll = if selected < rows {
            0
        } else {
            (selected + 1 - rows).min(commands.len() - rows)
        };
        let first_cmd = list_bottom + 1 - rows as u16;
        let above = scroll > 0;
        let below = scroll + rows < commands.len();
        let marker = match (above, below) {
            (true, true) => " ▲▼",
            (true, false) => " ▲",
            (false, true) => " ▼",
            (false, false) => "",
        };
        let header_row = first_cmd.saturating_sub(1);
        put_line(
            frame,
            header_row,
            width,
            paint_bounded_line(&format!(" command{marker}"), width, style_underline()),
        );
        // the header row is the palette's own chrome, not a command
        // row and not outside the surface either -- a click here must neither run a
        // command (there is none to run) nor dismiss the surface, matching the
        // pre-rewrite behavior of a click on the palette's own furniture.
        hits.push(
            QueueHitTarget::CommandChrome,
            Rect::new(0, header_row, width, 1),
        );
        for (j, cmd) in commands.iter().skip(scroll).take(rows).enumerate() {
            let y = first_cmd.saturating_add(j as u16);
            let pre = if cmd.selected { " ▸ " } else { "   " };
            let text = format!("{pre}{}", cmd.label);
            let style = if cmd.selected {
                style_reverse()
            } else {
                style_plain()
            };
            put_line(frame, y, width, paint_bounded_line(&text, width, style));
            // Index into the full (unscrolled) command list, so a click resolves to the
            // same command `BoardModel::visible_commands()` would name at that position
            // regardless of which window is currently painted.
            hits.push(
                QueueHitTarget::Command(scroll + j),
                Rect::new(0, y, width, 1),
            );
        }
    }
    let q = format!(" :{query}");
    put_line(
        frame,
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
    lines: &[String],
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    let height = geo.height;
    if width == 0 || height == 0 || lines.is_empty() {
        return;
    }
    // Any click anywhere in the frame closes the card, matching the keyboard's "any key":
    // one full-frame region rather than one per painted line, so a click in the
    // padding around a short card still closes it.
    hits.push(QueueHitTarget::HelpDismiss, Rect::new(0, 0, width, height));
    frame.render_widget(Clear, Rect::new(0, 0, width, height));
    let body_w = (width.saturating_sub(2)).min(62) as usize;
    // Compact is a takeover: omit decorative blank rows so the title, every binding,
    // and the close instruction fit at the 40x10 minimum.
    let shown: Vec<&String> = if geo.tier == Tier::Compact {
        lines.iter().filter(|line| !line.is_empty()).collect()
    } else {
        // Keep standard card spacing, while sharing the loop below.
        lines.iter().collect()
    };
    let show = shown.len().min(height as usize);
    let y0 = if geo.tier == Tier::Compact {
        0
    } else {
        ((height as usize).saturating_sub(show) / 2).max(1) as u16
    };
    let x_pad = width.saturating_sub(body_w as u16) / 2;
    for (j, ln) in shown.iter().take(show).enumerate() {
        let y = y0.saturating_add(j as u16);
        if y >= height {
            break;
        }
        let body = present_line(ln, body_w);
        let pad = " ".repeat(x_pad as usize);
        if ln.contains("any key") {
            // Reverse only the words. Padding stays plain so the bar does not run to the left.
            let trail = (width as usize).saturating_sub(x_pad as usize + display_width(&body));
            put_line(
                frame,
                y,
                width,
                bound_line(
                    Line::from(vec![
                        Span::styled(pad, style_plain()),
                        Span::styled(body, style_reverse()),
                        Span::styled(" ".repeat(trail), style_plain()),
                    ]),
                    width as usize,
                ),
            );
        } else {
            let padded = format!("{pad}{body}");
            let style = if j == 0 { style_bold() } else { style_plain() };
            put_line(frame, y, width, paint_bounded_line(&padded, width, style));
        }
    }
}

/// The task page's full-height geometry: row 0 down to (not including) the lowest bottom
/// chrome row. Rows are budgeted title · divider · notes · checklist · meta footer,
/// dropping the divider and then the meta first as the pane shrinks. With items on the
/// page the content region halves (AC-24): notes own the top half, the checklist
/// section the bottom, and each half scrolls within its own window — the notes scroll
/// bound and the item cursor's scroll window both key off this layout. Zero required
/// rows (no items) reserves nothing: such pages keep the exact pre-checklist layout.
pub struct TaskPageLayout {
    /// First row the page must not paint (the lowest chrome row, or the frame height).
    pub bottom: u16,
    pub title_y: u16,
    pub divider_y: Option<u16>,
    pub notes_y: u16,
    pub notes_rows: u16,
    /// First row of the checklist block (its label row); the notes window ends here.
    pub checklist_y: u16,
    /// Rows the checklist block may paint (label + item rows). Zero when there is no
    /// section, in which case `checklist_y` sits at the notes end.
    pub checklist_rows: u16,
    pub meta_y: Option<u16>,
}

/// What the page's checklist section asks of the layout (AC-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecklistSection {
    /// No items: no section paints, notes keep the full content region. The item
    /// editor no longer reserves a section row — since T-7 it paints on the page
    /// footer, so an open line over an empty checklist is still no section.
    None,
    /// At least one item: the content region halves and the section owns the bottom
    /// half, however many items there are — the `+N more ↓` affordance names the
    /// tail the half cannot show.
    Items,
}

/// Classify the page's checklist section from its payload facts: items alone
/// decide it.
pub fn checklist_section(items: usize) -> ChecklistSection {
    if items > 0 {
        ChecklistSection::Items
    } else {
        ChecklistSection::None
    }
}

/// The window of checklist items the section's item rows show.
///
/// `avail` is the row count the section has for items (its block minus the label). When
/// items remain hidden below, the last row becomes the dim `+N more ↓` affordance —
/// unless that would leave no item row at all, the one degenerate window (a single item
/// row beside a long list) where the item wins and the affordance is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecklistWindow {
    /// First painted item's absolute index.
    pub first: usize,
    /// Item rows painted.
    pub count: usize,
    /// Items hidden below the window.
    pub hidden_after: usize,
    /// Whether the last section row paints the affordance instead of an item.
    pub affordance: bool,
}

pub fn checklist_window(total: usize, scroll: usize, avail: u16) -> ChecklistWindow {
    let avail = avail as usize;
    if total == 0 || avail == 0 {
        return ChecklistWindow {
            first: 0,
            count: 0,
            hidden_after: 0,
            affordance: false,
        };
    }
    // Never start past the last item, whatever a stale scroll offset claims.
    let first = scroll.min(total - 1);
    let fitting = avail.min(total - first);
    let hidden_after = total - first - fitting;
    let affordance = hidden_after > 0 && fitting >= 2;
    let count = if affordance { fitting - 1 } else { fitting };
    ChecklistWindow {
        first,
        count,
        hidden_after,
        affordance,
    }
}

/// Build the task page's row budget. `section` (see [`checklist_section`]) sizes the
/// checklist block: with items the block is the bottom half of the content region
/// (`content_rows / 2` rounded down, odd rows to the notes half, at least its label
/// row); `notes_floor` is the minimum number of notes rows the page must keep
/// visible. Callers pass 1 while a notes edit is active -- an edit that paints no row
/// is a blind edit -- and 0 otherwise, and both layout callers must agree, because the
/// payload builder windows the notes draft against the same budget the painter lays
/// out.
pub fn task_page_layout(
    geo: &TierGeometry,
    section: ChecklistSection,
    notes_floor: u16,
) -> TaskPageLayout {
    let height = geo.height;
    let bottom = [geo.rule_row, geo.status_row, geo.verb_row]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(height);
    // Blank row 0, title 1, divider 2, notes, checklist, meta last.
    let title_y: u16 = if bottom >= 2 { 1 } else { 0 };
    let meta_y = if bottom >= 4 { Some(bottom - 1) } else { None };
    let divider_y = if bottom >= 5 {
        Some(title_y.saturating_add(1))
    } else {
        None
    };
    let notes_y = if bottom >= 3 {
        divider_y
            .map(|y| y + 1)
            .unwrap_or(title_y.saturating_add(1))
    } else {
        bottom
    };
    let content_end = meta_y.unwrap_or(bottom);
    let content_rows = content_end.saturating_sub(notes_y);
    // `notes_floor` wins over the section (T-2 remediation): while a notes edit is
    // active the block caps around the reserved row instead of displacing it, so the
    // edit can never be scrolled or clamped out of the frame entirely.
    let cap = content_rows.saturating_sub(notes_floor.min(content_rows));
    let checklist_rows = match section {
        ChecklistSection::None => 0,
        ChecklistSection::Items => (content_rows / 2).max(1).min(cap),
    };
    let checklist_y = content_end.saturating_sub(checklist_rows);
    TaskPageLayout {
        bottom,
        title_y,
        divider_y,
        notes_y,
        notes_rows: content_rows.saturating_sub(checklist_rows),
        checklist_y,
        checklist_rows,
        meta_y,
    }
}

/// The task page: full-height takeover hiding the selector row, header + notes + meta
/// footer. Field clicks reuse the shared-form hit targets, so one mouse map serves the
/// page and the form alike.
#[allow(clippy::too_many_arguments)]
fn paint_task_page(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    header: &str,
    title_cursor: Option<u16>,
    status_word: &str,
    notes_rows: &[String],
    notes_cursor: Option<(u16, u16)>,
    more_lines: usize,
    checklist_items: &[ChecklistItemView],
    checklist_cursor: Option<usize>,
    checklist_scroll: usize,
    checklist_marked: Option<usize>,
    checklist_editor: Option<&ChecklistEditorLine>,
    meta: &str,
    focus: Option<CaptureField>,
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    if width == 0 || geo.height == 0 {
        return;
    }
    // `focus == Notes` arrives from the same frame's input mode the payload builder
    // used, so both sides of the payload/paint seam budget the same notes floor.
    let lay = task_page_layout(
        geo,
        checklist_section(checklist_items.len()),
        u16::from(focus == Some(CaptureField::Notes)),
    );
    if lay.bottom == 0 {
        return;
    }
    frame.render_widget(Clear, Rect::new(0, 0, width, lay.bottom));

    // Header: two-space gutter + glyph + title (bold), status word dim and right-aligned.
    let header = format!("  {header}");
    let mut line = Line::from(Span::styled(header.clone(), style_bold()));
    let used = display_width(&header);
    let word = display_width(status_word);
    if used + word < width as usize {
        line.spans
            .push(Span::raw(" ".repeat(width as usize - used - word)));
        line.spans
            .push(Span::styled(status_word.to_string(), style_dim()));
    }
    put_line(frame, lay.title_y, width, line);
    hits.push(
        QueueHitTarget::FormTitle,
        Rect::new(0, lay.title_y, width, 1),
    );
    if let Some(col) = title_cursor {
        place_edit_cursor(frame, Rect::new(0, lay.title_y, width, 1), col);
    }

    // Divider, its right end naming wrapped note rows the window does not show.
    if let Some(y) = lay.divider_y {
        let tail = if more_lines > 0 {
            format!(" {more_lines} more ↓ ")
        } else {
            String::new()
        };
        let tail_w = display_width(&tail);
        let dash_count = (width as usize).saturating_sub(2).saturating_sub(tail_w);
        let dashes = "─".repeat(dash_count);
        put_line(
            frame,
            y,
            width,
            Line::from(vec![
                Span::styled(format!("  {dashes}"), style_dim()),
                Span::styled(tail, style_dim()),
            ]),
        );
    }

    // Notes body: wrapped rows in view mode, cursor-windowed draft rows while editing.
    let notes_style = if focus == Some(CaptureField::Notes) {
        style_bold()
    } else {
        style_plain()
    };
    if notes_rows.is_empty() && lay.notes_rows > 0 {
        put_line(
            frame,
            lay.notes_y,
            width,
            paint_bounded_line("  no notes yet ", width, style_dim()),
        );
        hits.push(
            QueueHitTarget::FormNotes(0),
            Rect::new(0, lay.notes_y, width, 1),
        );
    }
    for (index, row) in notes_rows.iter().enumerate().take(lay.notes_rows as usize) {
        let y = lay.notes_y.saturating_add(index as u16);
        put_line(
            frame,
            y,
            width,
            paint_bounded_line(&format!("  {row} "), width, notes_style),
        );
        hits.push(QueueHitTarget::FormNotes(index), Rect::new(0, y, width, 1));
    }
    if let Some((row, col)) = notes_cursor {
        place_edit_cursor_at(
            frame,
            Rect::new(2, lay.notes_y, width.saturating_sub(3), lay.notes_rows),
            row.min(lay.notes_rows.saturating_sub(1)),
            col.min(width.saturating_sub(2)),
        );
    }

    // Checklist section: the bottom half of the content region, its label row the
    // divider between the halves (AC-24). The label names the done/total counts
    // (derived from the same item views, so the label cannot drift from the states
    // painted beside it). Items stack from the top of the half, directly under the
    // label (`checklist_scroll` windows them): the cursor's row is marked by a `▸`
    // gutter, a delete-marked row by `✗` in place of its state glyph, and hidden
    // items are named by a dim `+N more ↓` row on the half's last row, mirroring the
    // notes divider's style. Mono only. An empty checklist paints no block at all:
    // `checklist_rows` is zero and the layout above kept the pre-checklist page
    // whole — the item input lives on the footer row, not here (AC-25).
    if lay.checklist_rows > 0 {
        let done = checklist_items.iter().filter(|item| item.done).count();
        let label = format!("  checklist {}/{}", done, checklist_items.len());
        put_line(
            frame,
            lay.checklist_y,
            width,
            paint_bounded_line(&label, width, style_dim()),
        );
        let win = checklist_window(
            checklist_items.len(),
            checklist_scroll,
            lay.checklist_rows.saturating_sub(1),
        );
        for (i, item) in checklist_items
            .iter()
            .skip(win.first)
            .take(win.count)
            .enumerate()
        {
            let absolute = win.first + i;
            let y = lay.checklist_y.saturating_add(1 + i as u16);
            let gutter = if Some(absolute) == checklist_cursor {
                "▸ "
            } else {
                "  "
            };
            let glyph = if Some(absolute) == checklist_marked {
                "✗"
            } else if item.done {
                "✓"
            } else {
                "▪"
            };
            put_line(
                frame,
                y,
                width,
                paint_bounded_line(
                    &format!("{gutter}{glyph} {} ", item.text),
                    width,
                    style_plain(),
                ),
            );
        }
        if win.affordance {
            // AC-24: the affordance owns the half's last row. Items stack down from
            // the label, so the row after the last painted item is no longer the
            // block's foot; the half's own bottom row is.
            let y = lay
                .checklist_y
                .saturating_add(lay.checklist_rows)
                .saturating_sub(1);
            put_line(
                frame,
                y,
                width,
                paint_bounded_line(
                    &format!("  +{} more ↓ ", win.hidden_after),
                    width,
                    style_dim(),
                ),
            );
        }
    }

    // Meta footer: scope · created · updated. The whole row is the scope control —
    // except while the item input is open (AC-25): the footer row is then the
    // quick-add-pattern input line (prompt + mono line + cursor, its own refusal
    // tail), and the scope control is not painted, so it offers no click either.
    if let Some(y) = lay.meta_y {
        if let Some(editor) = checklist_editor {
            paint_prompt_input_line(
                frame,
                y,
                width,
                &editor.text,
                ITEM_INPUT_PLACEHOLDER,
                editor.cursor_col,
                editor.refusal,
            );
        } else {
            put_line(
                frame,
                y,
                width,
                paint_bounded_line(&format!("  {meta}"), width, style_dim()),
            );
            hits.push(QueueHitTarget::FormScope, Rect::new(0, y, width, 1));
        }
    }
}

/// What the footer item input paints while its draft is empty: the line's dim
/// placeholder, mirroring the quick-add bar's hint-on-empty convention.
const ITEM_INPUT_PLACEHOLDER: &str = "item…   enter save · ctrl+enter save+next · esc cancel";

/// The page's scope chooser stacks its options directly above the scope footer (left,
/// indented like the footer), never in the selector's corner: the footer is the control
/// being answered, so the chooser sits beside it.
fn paint_page_scope_dropdown(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    dropdown: FormScopeDropdown<'_>,
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    if width == 0 || dropdown.options.is_empty() {
        return;
    }
    // The dropdown anchors on the meta footer, which never moves with the checklist,
    // and never opens while a field edit owns the page.
    let lay = task_page_layout(geo, ChecklistSection::None, 0);
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
            y,
            width,
            paint_bounded_line(&format!("  {text}"), width, style_plain()),
        );
        hits.push(
            QueueHitTarget::FormScopeOption(j),
            Rect::new(0, y, width, 1),
        );
    }
}

fn paint_scope_dropdown(
    frame: &mut Frame<'_>,
    geo: &TierGeometry,
    options: &[String],
    selected: usize,
    option_target: impl Fn(usize) -> QueueHitTarget,
    hits: &mut QueueHitMap,
) {
    let width = geo.row_width;
    if width == 0 || options.is_empty() {
        return;
    }
    // Align under the project chip on the right of the selector row.
    let max_label = options
        .iter()
        .map(|o| display_width(o))
        .max()
        .unwrap_or(0)
        .max(display_width("all projects"));
    let col_w = (max_label + 4).min(geo.selector_chip_max as usize).max(8);
    let x0 = width.saturating_sub(col_w as u16);
    let top = geo.selector_row.map(|r| r.saturating_add(1)).unwrap_or(0);
    let max_n = if geo.tier == Tier::Compact {
        geo.viewport_height.max(1) as usize
    } else {
        options.len().min(12)
    };
    let rows = max_n.min(options.len());
    let selected = selected.min(options.len().saturating_sub(1));
    let scroll = if selected < rows {
        0
    } else {
        (selected + 1 - rows).min(options.len() - rows)
    };
    let panel_h = rows.min((geo.height.saturating_sub(top).saturating_sub(2)) as usize) as u16;
    if panel_h > 0 {
        frame.render_widget(Clear, Rect::new(x0, top, col_w as u16, panel_h));
    }
    for (j, opt) in options.iter().enumerate().skip(scroll).take(rows) {
        let y = top.saturating_add((j - scroll) as u16);
        if y >= geo.height.saturating_sub(2) {
            break;
        }
        let marker = if j == selected { "▸ " } else { "  " };
        let body = present_line(&format!("{marker}{opt}"), col_w);
        // Pad to col_w so the dropdown cell fully overwrites base row content under it.
        let w = col_w;
        let text = if display_width(&body) >= w {
            body
        } else {
            format!("{}{}", body, " ".repeat(w - display_width(&body)))
        };
        let style = if j == selected {
            style_reverse()
        } else {
            style_plain()
        };
        // Paint only the right-hand chip column.
        let area = Rect::new(x0, y, col_w as u16, 1);
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
        // `j` remains the source index after windowing, so a click selects the same option
        // Up/Down plus Enter would confirm rather than its position within this paint slice.
        hits.push(option_target(j), area);
    }
}

enum ListRow {
    Blank,
    /// Section header row, carrying its section kind plus its index into
    /// `QueueFrameModel::view.sections`, so the paint loop can offer the DONE header as
    /// the drawer's own toggle control and an all-projects group header as that
    /// project's own scope control.
    Header(SectionKind, usize, Line<'static>),
    Hint(Line<'static>),
    Task {
        id: Uuid,
        line: Line<'static>,
    },
    /// Read-only accordion content under a task, full-width and un-hit-tested.
    Detail(Line<'static>),
}

/// Read-only accordion body under an expanded task: notes preview,
/// scope, and created/updated age. the has no agent surfaces to show here.
///
/// The peek (`→`) shows up to [`PEEK_NOTES_LINE_LIMIT`] wrapped note lines; a dim
/// "… N more lines" tail names whatever did not fit. The full text lives behind `Enter`
/// on the task page.
const PEEK_NOTES_LINE_LIMIT: usize = 5;

fn detail_lines_for_task(task: &Task, now: SystemTime, width: u16) -> Vec<Line<'static>> {
    let indent = "    │ ";
    let mut lines: Vec<Line<'static>> = Vec::new();
    let notes_text = task.notes.as_deref().map(str::trim).unwrap_or_default();
    if notes_text.is_empty() {
        lines.push(paint_bounded_line(
            &format!("{indent}no notes yet"),
            width,
            style_dim(),
        ));
    } else {
        let mut shown = 0usize;
        let mut remaining = 0usize;
        for note_line in notes_text.lines() {
            if shown < PEEK_NOTES_LINE_LIMIT {
                lines.push(paint_bounded_line(
                    &format!("{indent}{note_line}"),
                    width,
                    style_dim(),
                ));
                shown += 1;
            } else {
                remaining += 1;
            }
        }
        if remaining > 0 {
            lines.push(paint_bounded_line(
                &format!("{indent}… {remaining} more lines"),
                width,
                style_dim(),
            ));
        }
    }
    let scope_text = match &task.scope {
        TaskScope::Project { path } => short_project(path).to_string(),
        TaskScope::Global => "global".to_string(),
    };
    let age_text = format!(
        "created {} ago · updated {} ago",
        format_age(now, task.created_at),
        format_age(now, task.updated_at)
    );
    lines.push(paint_bounded_line(
        &format!("{indent}scope {scope_text}"),
        width,
        style_dim(),
    ));
    lines.push(paint_bounded_line(
        &format!("{indent}{age_text}"),
        width,
        style_dim(),
    ));
    lines
}

/// Builds the list rows plus the index of an open accordion detail or selected task, which
/// must stay fully visible in the viewport.
fn build_list_rows(
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
) -> (Vec<ListRow>, Option<usize>, Option<usize>) {
    let mut out = Vec::new();
    let mut anchor_last_idx: Option<usize> = None;
    let mut selected_idx: Option<usize> = None;
    // Every section heading has one blank list row above it. This row remains ordinary list
    // content, so selection scrolling keeps the section and its first task reachable.
    out.push(ListRow::Blank);

    // The peek weaves inline under its row in BOTH tiers: rows keep their tier styling,
    // the peek body is ordinary list content that scrolls with the section.
    let detail_target = if matches!(model.overlay, QueueOverlay::None) {
        model.detail_open
    } else {
        None
    };

    for (section_idx, section) in model.view.sections.iter().enumerate() {
        // A preceding section with no content already ends in its required below-header blank
        // row, which doubles as this heading's above-header row. Otherwise add one list row.
        if !matches!(out.last(), Some(&ListRow::Blank)) {
            out.push(ListRow::Blank);
        }
        out.push(ListRow::Header(
            section.kind,
            section_idx,
            paint_section_header(section, geo.row_width, model.all_projects_scope),
        ));
        // every kind, in either tier, gets its below-header spacer before first
        // content. It is a `ListRow`, not chrome, and therefore scrolls normally.
        out.push(ListRow::Blank);
        if section.empty_hint {
            out.push(ListRow::Hint(paint_empty_hint(geo.row_width)));
            continue;
        }
        let in_project_section =
            section.kind == SectionKind::OnDeck && section.project_label.is_some();
        for id in &section.task_ids {
            let Some(task) = model.tasks.iter().find(|t| t.id == *id) else {
                // Stale id: skip without panicking (Selection Anchor's problem).
                continue;
            };
            let meta = row_meta(task, model.now, in_project_section);
            let line = paint_task_row(
                &TaskRowPaint {
                    glyph: status_glyph(task.status),
                    title: &task.title,
                    meta: &meta,
                    selected: model.selection_id == Some(task.id)
                        && !matches!(model.overlay, QueueOverlay::ScopeDropdown { .. }),
                    title_bold: false,
                },
                geo,
            );
            if model.selection_id == Some(task.id) {
                selected_idx = Some(out.len());
            }
            out.push(ListRow::Task { id: task.id, line });
            if detail_target == Some(task.id) {
                for line in detail_lines_for_task(task, model.now, geo.row_width) {
                    out.push(ListRow::Detail(line));
                }
                anchor_last_idx = Some(out.len() - 1);
            }
        }
    }
    (out, anchor_last_idx, selected_idx)
}

fn paint_section_header(section: &QueueSection, width: u16, all_projects: bool) -> Line<'static> {
    let title = section_title(section, all_projects);
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

fn section_title(section: &QueueSection, all_projects: bool) -> String {
    match section.kind {
        SectionKind::InMotion => "IN MOTION".to_string(),
        SectionKind::Done => "DONE".to_string(),
        SectionKind::OnDeck if all_projects => match section.project_label.as_deref() {
            Some(path) => short_project(path).to_string(),
            None => "global".to_string(),
        },
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
/// Paint the focused quick-add line in the row normally used for board feedback.
fn paint_quick_add_status(
    frame: &mut Frame<'_>,
    row: u16,
    width: u16,
    title: &str,
    title_cursor: u16,
) {
    paint_prompt_input_line(
        frame,
        row,
        width,
        title,
        "title…   !p global · !p name project · tab details",
        title_cursor,
        None,
    );
}

/// The one-line input pattern the board's quick-add bar established (AC-25 reuses
/// it for the task page's footer item input): a bold prompt glyph, the mono body —
/// dim placeholder while the draft is empty, bold text otherwise — the surface's
/// own dim refusal tail painted on the line itself, and the terminal cursor just
/// past the prompt. `body` must already be windowed around its cursor at
/// `width - 2` cells, exactly as the quick-add line hands its title over.
pub(crate) fn paint_prompt_input_line(
    frame: &mut Frame<'_>,
    row: u16,
    width: u16,
    body: &str,
    placeholder: &str,
    cursor_col: u16,
    refusal: Option<&str>,
) {
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
        row,
        width,
        bound_line(Line::from(spans), width as usize),
    );
    place_edit_cursor(
        frame,
        Rect::new(
            prefix_width.min(width.saturating_sub(1)),
            row,
            width.saturating_sub(prefix_width),
            1,
        ),
        cursor_col,
    );
}

/// Paint an inline quick-add refusal in its reserved blank row, never over the input cursor.
fn paint_quick_add_message(frame: &mut Frame<'_>, row: u16, width: u16, message: &str) {
    put_line(
        frame,
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
                    "global"
                }
            ),
            Tier::Compact => format!(
                "⏎ save · esc · tab · {}",
                if project_scope { "proj" } else { "glob" }
            ),
        };
        (text, style_dim())
    };
    put_line(
        frame,
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
) -> (Line<'static>, Option<(u16, u16)>) {
    if let Some(msg) = message {
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
        let style = strip_color(style_bold());
        let line = bound_line(Line::from(Span::styled(shown, style)), width as usize);
        return (line, undo_hit);
    }
    // Idle status: done count only. In-motion is already on the section header.
    let done = present_line(&format!(" {} done", counts.done), width as usize);
    let pad_w = (width as usize).saturating_sub(display_width(&done));
    let line = bound_line(
        Line::from(vec![
            Span::styled(done, style_dim()),
            Span::styled(" ".repeat(pad_w), style_plain()),
        ]),
        width as usize,
    );
    (line, None)
}

/// Selector: queue label left, project chip right.
///
/// Returns the line plus hit regions as (target, x, width).
fn paint_selector_row(
    model: &QueueFrameModel<'_>,
    geo: &TierGeometry,
) -> (Line<'static>, Vec<(QueueHitTarget, u16, u16)>) {
    let width = geo.row_width as usize;
    let mut hits = Vec::new();
    if width == 0 {
        return (Line::from(""), hits);
    }
    let chip_budget = (geo.selector_chip_max as usize).min(width);
    let chip_raw = format!("{} ▾ ", model.scope_label);
    let chip_text = present_line(&chip_raw, chip_budget);
    let chip_w = display_width(&chip_text);
    let label = String::new();
    let spacer_w = width.saturating_sub(chip_w);
    let chip_label = present_line(
        model.scope_label,
        chip_budget.saturating_sub(display_width(" ▾ ")),
    );
    let caret = present_line(
        " ▾ ",
        chip_budget.saturating_sub(display_width(&chip_label)),
    );
    let chip_x = (display_width(&label) + spacer_w) as u16;
    let line = Line::from(vec![
        Span::styled(label, style_bold()),
        Span::styled(" ".repeat(spacer_w), style_plain()),
        Span::styled(chip_label, style_bold()),
        Span::styled(caret, style_dim()),
    ]);
    hits.push((QueueHitTarget::ProjectChip, chip_x, chip_w.max(1) as u16));
    (bound_line(line, width), hits)
}

/// Verb bar: ` key label · key label …`, trimmed to `budget` entries.
///
/// Returns hit regions as (verb_index, x, width).
fn mutating_verb_key(key: &str) -> bool {
    matches!(
        key,
        "space" | "d" | "o" | "b" | "x" | "a" | "e" | "u" | "n" | "q"
    )
}

fn paint_verb_bar(
    items: &[VerbEntry<'_>],
    budget: usize,
    width: u16,
    prefix: Option<crate::config::VerbModifier>,
) -> (Line<'static>, Vec<(usize, u16, u16)>) {
    let shown: Vec<&VerbEntry<'_>> = items.iter().take(budget).collect();
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
        let key = match prefix {
            Some(modifier) if mutating_verb_key(entry.key) => {
                format!("{}{}", modifier.prefix(), entry.key)
            }
            _ => entry.key.to_string(),
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
    if in_project_section {
        return age;
    }
    match &task.scope {
        TaskScope::Project { path } => format!("{} · {age}", short_project(path)),
        TaskScope::Global => age,
    }
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

fn put_line(frame: &mut Frame<'_>, row: u16, width: u16, line: Line<'static>) {
    if width == 0 {
        return;
    }
    let w = width as usize;
    let area = Rect::new(0, row, width, 1);
    if line.width() >= w {
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    // Pad to full row width with plain spaces so base frame content cannot bleed through
    // (overlays paint short content; trailing cells must be overwritten).
    let pad = w - line.width();
    let mut spans: Vec<Span<'static>> = line.spans.into_iter().collect();
    spans.push(Span::styled(" ".repeat(pad), style_plain()));
    let padded = Line::from(spans);
    frame.render_widget(Paragraph::new(padded), area);
}

fn display_width(s: &str) -> usize {
    Line::from(s).width()
}

fn fit_left(raw: &str, budget: usize) -> String {
    if display_width(raw) <= budget {
        return raw.to_string();
    }
    // present_line already applied to the title; this only guards prefix edge cases.
    present_line(raw, budget)
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
    fn present_row_title_and_meta_never_overlap_and_truncation_uses_visible_omission_mark() {
        let geo = tier::resolve(80, 24);
        assert_eq!(geo.tier, Tier::Standard);
        assert!(geo.meta_column_width > 0);

        let long_title = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(4); // 104 chars
        let long_meta = "meta-meta-meta-meta-meta-meta-meta"; // > 28

        let line = paint_task_row(
            &TaskRowPaint {
                glyph: "○",
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
        let metas = ["", "1h", "herdr-tasks · 2d", &"m".repeat(80)];

        for geo in cases {
            for title in titles {
                for meta in metas {
                    for selected in [false, true] {
                        let line = paint_task_row(
                            &TaskRowPaint {
                                glyph: "◓",
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
                title: "plain row",
                meta: "1h",
                selected: false,
                title_bold: false,
            },
            TaskRowPaint {
                glyph: "▲",
                title: "bold title",
                meta: "herdr-tasks · 2m",
                selected: false,
                title_bold: true,
            },
            TaskRowPaint {
                glyph: "◓",
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
