//! Queue chrome, overlays, verb bar, and frame drawing hooks.

use std::time::SystemTime;

use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::domain::{HumanStatus, TaskScope};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{escaped_line_window, wrap_text, wrapped_draft_rows, wrapped_edit_rows};
use crate::ui::input::{help_card_lines, keymap_help_label};
use crate::ui::mouse::BoardPopup;
use crate::ui::render::{
    self, FormScopeDropdown, PaletteCommandRow, QueueFrameModel, QueueOverlay, VerbEntry,
};
use crate::ui::tier;
use crate::ui::{present_line, terminal_text};

use super::chrome::{notice_framed, row_width, DELETE_NOTICE_UNDO};
use super::commands::CommandSurface;
use super::model::{
    project_option_label, project_scope_option_label, BoardForm, BoardInputMode, BoardLocation,
    BoardModel,
};

/// Verb bar for the base board list: labels follow the selected task.
///
/// `s` starts a ready task or reopens a done one. On started/blocked/review it is
/// omitted (`PrimaryVerb` is inert on started/blocked/review). `b` reads `unblock` only on a blocked task.
/// Done tasks show `o reopen` instead of `d`/`b`. `:` / `?` take their word from the keymap.
pub fn board_verb_items(model: &BoardModel) -> Vec<VerbEntry<'static>> {
    let help = |chord: &str, fallback: &'static str| keymap_help_label(chord).unwrap_or(fallback);

    // An inline step editor saves only through Shift+Enter. Add mode reopens an empty row;
    // an existing-step rename commits the complete task edit session and exits it.
    if model.input_mode() == BoardInputMode::EditStep {
        return vec![
            VerbEntry {
                key: "shift+enter",
                label: "save",
            },
            VerbEntry {
                key: "esc",
                label: "cancel",
            },
        ];
    }

    // The task page's view mode: its own legend, true for the bound task.
    let page_task = if model.input_mode() == BoardInputMode::TaskPage {
        model
            .form
            .as_ref()
            .filter(|form| form.is_task())
            .and_then(|form| form.task_id())
            .and_then(|id| model.tasks.iter().find(|t| t.id == id))
    } else {
        None
    };
    if let Some(task) = page_task {
        return task_page_verb_items(model, task);
    }

    let mut entries = Vec::with_capacity(7);
    let selected_task = model
        .selected_id()
        .and_then(|id| model.tasks.iter().find(|t| t.id == id));

    if let Some(task) = selected_task {
        match task.status {
            HumanStatus::Ready => {
                entries.push(VerbEntry {
                    key: "s",
                    label: "start",
                });
            }
            HumanStatus::Done => {
                entries.push(VerbEntry {
                    key: "s",
                    label: "reopen",
                });
            }
            // `PrimaryVerb` is a no-op on Started/Blocked/Review ("nothing to do
            // available yet"): omit the entry rather than advertise a no-op.
            HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {}
        }
        entries.push(VerbEntry {
            key: "enter",
            label: "open",
        });
        if task.status == HumanStatus::Done {
            entries.push(VerbEntry {
                key: "o",
                label: help("o", "reopen"),
            });
        } else {
            entries.push(VerbEntry {
                key: "d",
                label: help("d", "done"),
            });
            entries.push(VerbEntry {
                key: "b",
                label: if task.status == HumanStatus::Blocked {
                    "unblock"
                } else {
                    help("b", "block")
                },
            });
        }
    }
    entries.push(VerbEntry {
        key: ":",
        label: help(":", "palette"),
    });
    entries.push(VerbEntry {
        key: "?",
        label: help("?", "help"),
    });
    // Capture is last so the compact budget preserves the established board verbs.
    entries.push(VerbEntry {
        key: "+",
        label: "capture",
    });
    entries
}

fn task_page_verb_items(model: &BoardModel, task: &crate::domain::Task) -> Vec<VerbEntry<'static>> {
    let help = |chord: &str, fallback: &'static str| keymap_help_label(chord).unwrap_or(fallback);
    let mut entries = Vec::with_capacity(6);
    entries.push(VerbEntry {
        key: "e",
        label: "edit",
    });
    let selected_step_done = model
        .form
        .as_ref()
        .filter(|form| form.task_id() == Some(task.id))
        .and_then(|form| form.steps.cursor)
        .and_then(|index| task.steps.get(index))
        .map(|step| step.done);
    if selected_step_done.is_some() {
        entries.push(VerbEntry {
            key: "s",
            label: "toggle step",
        });
    } else {
        match task.status {
            HumanStatus::Ready => entries.push(VerbEntry {
                key: "s",
                label: "start",
            }),
            HumanStatus::Done => entries.push(VerbEntry {
                key: "s",
                label: "reopen",
            }),
            HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {}
        }
    }
    if selected_step_done.unwrap_or(task.status == HumanStatus::Done) {
        entries.push(VerbEntry {
            key: "o",
            label: help("o", "reopen"),
        });
    } else {
        entries.push(VerbEntry {
            key: "d",
            label: help("d", "done"),
        });
        entries.push(VerbEntry {
            key: "b",
            label: if task.status == HumanStatus::Blocked {
                "unblock"
            } else {
                help("b", "block")
            },
        });
    }
    entries.push(VerbEntry {
        key: "esc",
        label: "close",
    });
    if model.task_editing() {
        entries.push(VerbEntry {
            key: "a",
            label: "step",
        });
    }
    entries
}

/// Build the task page's paint payload from the open task form. View mode wraps the notes
/// draft and windows it by the page scroll; field edits reuse the form's cursor windowing.
fn build_task_page_overlay<'a>(
    model: &'a BoardModel,
    form: &'a BoardForm,
    geo: &tier::TierGeometry,
    scope_dropdown: Option<FormScopeDropdown<'a>>,
    column: bool,
) -> QueueOverlay<'a> {
    let width = geo.row_width as usize;
    let bound_task = form
        .task_id()
        .and_then(|id| model.tasks.iter().find(|task| task.id == id));
    // The section consumes extracted, word-wrapped step views, never the raw storage. The
    // reserved width is the scrollbar-safe content width less the step glyph and trailing
    // pad, matching Notes' no-truncation behavior.
    let step_text_width = width.saturating_sub(8);
    // Task-edit removals are staged, not durable, but they immediately leave the rendered
    // list. Keep the task's source index only in the session state, then derive this compact
    // visible list so the renderer never receives a row it must hide conditionally.
    let mut step_views: Vec<render::StepView> = bound_task
        .map(|task| {
            task.steps
                .iter()
                .filter(|step| !form.steps.removals.contains(&step.id))
                .map(|step| render::StepView {
                    done: step.done,
                    rows: crate::ui::edit::wrap_text(&step.text, step_text_width)
                        .into_iter()
                        .map(|row| row.text)
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    let stored_step_count = step_views.len();
    // Existing-step edits are task-session drafts. Paint every parked draft first, then the
    // active row over it. Only the active EditStep mode receives cursor metadata, so moving to
    // Title, Notes, Thread, or Scope leaves the changed step visible without a second cursor.
    if let Some(task) = bound_task {
        for (visible, step) in task
            .steps
            .iter()
            .filter(|step| !form.steps.removals.contains(&step.id))
            .enumerate()
        {
            if let Some(draft) = form.steps.drafts.get(&step.id) {
                let (rows, _, _) = wrapped_edit_rows(draft, step_text_width);
                step_views[visible].rows = rows;
            }
        }
    }
    let inline_step_editor = form.steps.editor.as_ref().and_then(|editor| {
        let index = match editor.rename {
            Some(step_id) => bound_task?
                .steps
                .iter()
                .filter(|step| !form.steps.removals.contains(&step.id))
                .position(|step| step.id == step_id)?,
            None => step_views.len(),
        };
        let (rows, cursor_row, cursor_col) = wrapped_edit_rows(&editor.buffer, step_text_width);
        if index < step_views.len() {
            step_views[index].rows = rows;
        } else {
            step_views.push(render::StepView { done: false, rows });
        }
        (model.input_mode() == BoardInputMode::EditStep).then_some(render::InlineStepEditor {
            index,
            cursor_row: u16::try_from(cursor_row).unwrap_or(u16::MAX),
            cursor_col: u16::try_from(cursor_col).unwrap_or(u16::MAX),
            refusal: editor.refusal.as_deref(),
        })
    });
    // The page cursor stores source indices, while the overlay only carries visible rows.
    // Translate at the boundary, so a staged removal cannot make the selector jump to a
    // different stored step just because its former array slot disappeared.
    let visible_step_index = |source_index: usize| {
        bound_task.and_then(|task| {
            task.steps
                .iter()
                .enumerate()
                .filter(|(_, step)| !form.steps.removals.contains(&step.id))
                .position(|(index, _)| index == source_index)
        })
    };
    // Cursor state uses source indices, so keep the recorded row counts source-aligned too.
    // Staged removals occupy zero rows; the active add row is not addressable by this cursor.
    let mut visible_counts = step_views
        .iter()
        .take(stored_step_count)
        .map(|step| step.rows.len().max(1));
    form.steps.row_counts.replace(
        bound_task
            .map(|task| {
                task.steps
                    .iter()
                    .map(|step| {
                        if form.steps.removals.contains(&step.id) {
                            0
                        } else {
                            visible_counts.next().unwrap_or(1)
                        }
                    })
                    .collect()
            })
            .unwrap_or_default(),
    );
    let bottom_input = (model.input_mode() == BoardInputMode::EditThread).then(|| {
        let avail = (geo.row_width as usize).saturating_sub(2);
        let (text, cursor_col) = escaped_line_window(&form.thread, avail);
        crate::ui::render::BottomInputSlot {
            text,
            cursor_col,
            placeholder: "thread…   enter close · shift+enter save · esc cancel",
            refusal: None,
            above_rows: Vec::new(),
            cursor_row_offset: 0,
            message: form.thread_refusal.as_deref(),
        }
    });
    // A notes edit always keeps one row: the layout reserves it (the section caps
    // around it), so an active edit can never be scrolled/clamped out of the frame.
    // A wide column's height already reflects the shared footer's input slot.
    let page_geo = if column {
        *geo
    } else {
        render::bottom_input_geometry(*geo, bottom_input.is_some())
    };
    let status = bound_task
        .map(|task| task.status)
        .unwrap_or(HumanStatus::Ready);
    let status_word = match status {
        HumanStatus::Ready => "ready",
        HumanStatus::Started => "started",
        HumanStatus::Blocked => "blocked",
        HumanStatus::Review => "review",
        HumanStatus::Done => "done",
    };
    let glyph = render::status_glyph(status);

    // Header: indent + glyph + the WRAPPED title rows + right-aligned status word
    // on row 0. A long title wraps onto further bold rows indented under the
    // glyph instead of truncating; edit mode wraps the draft with its cursor.
    // The uniform budget keeps every row's wrap identical.
    let editing_title = model.input_mode() == BoardInputMode::EditTitle;
    let header_identifier = (!editing_title)
        .then(|| {
            bound_task
                .and_then(|task| task.number)
                .map(|number| format!("T{number}"))
        })
        .flatten();
    let mut header_rows: Vec<String> = Vec::new();
    let mut title_cursor = None;
    // The wide column paints its own two-row header (`draw_task_column`), so the in-page
    // header rows are only built for the single-pane page that actually consumes them.
    if !column {
        let word_cells = status_word.chars().count() + 1;
        let identifier_cells = header_identifier
            .as_deref()
            .map(render::display_width)
            .unwrap_or(0);
        let title_avail = width
            .saturating_sub(4 + word_cells + identifier_cells + usize::from(identifier_cells > 0));
        // The header may grow only inside the page body: it must stop one row short
        // of the lowest chrome row with one note row still living under it, or a
        // pathological title would eat the page (and the painter's chrome).
        let page_bottom = [page_geo.rule_row, page_geo.status_row, page_geo.verb_row]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(page_geo.height);
        let header_cap = page_bottom.saturating_sub(3).max(1) as usize;
        if editing_title {
            let (mut rows, cursor_row, cursor_col) = wrapped_edit_rows(&form.title, title_avail);
            let overflowed = rows.len() > header_cap;
            rows.truncate(header_cap);
            if overflowed {
                if let Some(last) = rows.last_mut() {
                    *last = present_line(last, title_avail.saturating_sub(1));
                }
            }
            for (offset, segment) in rows.iter().enumerate() {
                if offset == 0 {
                    header_rows.push(format!("{glyph} {segment}"));
                } else {
                    header_rows.push(segment.clone());
                }
            }
            // A caret hidden below the cap parks at the END of the last shown row:
            // its own hidden column would otherwise paint an unrelated position on
            // the ellipsis row.
            let shown_cursor_row = cursor_row.min(rows.len().saturating_sub(1));
            let shown_cursor_col = if cursor_row > shown_cursor_row {
                rows.last()
                    .map(|last| render::display_width(last))
                    .unwrap_or(0)
            } else {
                cursor_col
            };
            title_cursor = Some((
                u16::try_from(shown_cursor_row).unwrap_or(u16::MAX),
                u16::try_from(shown_cursor_col).unwrap_or(u16::MAX),
            ));
        } else {
            let mut rows: Vec<String> = wrap_text(form.title.value(), title_avail)
                .iter()
                .map(|row| row.text.clone())
                .collect();
            let overflowed = rows.len() > header_cap;
            rows.truncate(header_cap);
            if overflowed {
                if let Some(last) = rows.last_mut() {
                    *last = present_line(last, title_avail.saturating_sub(1));
                }
            }
            for (offset, row) in rows.iter().enumerate() {
                if offset == 0 {
                    header_rows.push(format!("{glyph} {row}"));
                } else {
                    header_rows.push(row.clone());
                }
            }
        }
    }
    let lay = if column {
        render::task_column_layout(&page_geo)
    } else {
        render::task_page_layout(
            &page_geo,
            render::steps_section(step_views.len()),
            u16::from(model.input_mode() == BoardInputMode::EditNotes),
            header_rows.len().max(1) as u16,
        )
    };
    // The renderer and input reducer share this viewport size for page scrolling.
    form.steps.window_rows.set(lay.notes_rows as usize);

    // View mode supplies every wrapped note row; Notes edit mode wraps too, with the
    // caret mapped into wrapped coordinates. The shared page painter combines that
    // stream with the steps, then windows it once against the fixed viewport.
    // Wrap at the width the painter can show WHOLE: the content region less its
    // two-cell gutter and one further reserved cell, taken in the scrollbar state
    // (content_width - 3 there), so an overflowing page never re-wraps rows that
    // were already painted -- and no wrapped row ever ends in the presenter's … .
    let notes_width = width.saturating_sub(6);
    let want = lay.notes_rows as usize;
    let editing_notes = model.input_mode() == BoardInputMode::EditNotes;
    let (notes_rows, notes_cursor, more_lines, notes_scroll) = if editing_notes {
        let (all_rows, cursor_row, cursor_column) = wrapped_edit_rows(&form.notes, notes_width);
        // The notes editor owns the page viewport, so wheel and page scrolling stay inert
        // while the caret is active. Keep the minimal window that shows the caret's wrapped
        // row; Esc returns to page view, where below-fold steps can be scrolled into view.
        let follow = form.notes_scroll.clamp(
            cursor_row.saturating_sub(want.saturating_sub(1)),
            cursor_row,
        );
        (
            all_rows,
            Some((
                u16::try_from(cursor_row).unwrap_or(u16::MAX),
                u16::try_from(cursor_column).unwrap_or(u16::MAX),
            )),
            0,
            follow,
        )
    } else if form.notes.value().trim().is_empty() {
        (Vec::new(), None, 0, form.notes_scroll)
    } else {
        (
            wrapped_draft_rows(&form.notes, notes_width),
            None,
            0,
            form.notes_scroll,
        )
    };
    let step_rows: usize = step_views.iter().map(|step| step.rows.len().max(1)).sum();
    // Match the painter's stream exactly: it always paints one notes row and a trailing
    // `+ step` row, even when both stored notes and stored steps are empty.
    let content =
        render::page_content_layout(notes_rows.len().max(1), step_rows + 1, lay.notes_rows);
    form.notes_max_scroll.set(content.max_scroll);
    form.steps.content_start.set(content.steps_start);
    form.notes_width.set(notes_width);

    // Meta footer: scope · thread · created · updated (ages only while the bound task is
    // present). The identifier belongs in the header, so it never competes with scope hits.
    // A wide column moves the project up into its header slot: the scope footer paints only
    // while the edit session can change it, so its control stays reachable by mouse.
    let editing_session = form.is_task() && form.editing || model.open_field_edit().is_some();
    let show_scope = !column || editing_session;
    let meta_scope = if show_scope {
        match &form.scope {
            TaskScope::Project { path } => render::short_project(path).to_string(),
            TaskScope::Global => "desk".to_string(),
        }
    } else {
        String::new()
    };
    let meta_scope_width = u16::try_from(render::display_width(&meta_scope)).unwrap_or(u16::MAX);
    let mut meta = String::new();
    let meta_scope_x = 0;
    meta.push_str(&meta_scope);
    let mut thread_slot = None;
    if let Some(task) = bound_task {
        let shown_thread = if form.is_task() && form.editing {
            Some(form.thread.value())
        } else {
            task.thread.as_deref()
        };
        // Without a scope ahead of it (a wide column outside an edit session) the thread
        // leads the footer and drops its separator.
        let separator = if meta.is_empty() { "" } else { " · " };
        thread_slot = if let Some(thread) = shown_thread.filter(|thread| !thread.is_empty()) {
            Some(format!("{separator}#{}", terminal_text(thread)))
        } else if (form.is_task() && form.editing)
            || matches!(
                model.input_mode(),
                BoardInputMode::EditTitle
                    | BoardInputMode::EditNotes
                    | BoardInputMode::SelectThread
                    | BoardInputMode::EditThread
                    | BoardInputMode::EditScope
                    | BoardInputMode::FormScopeDropdown
            )
        {
            // An empty thread still needs a visible field-sized footer target while the form
            // is editing, otherwise mouse users can only reach Thread after it already exists.
            Some(format!("{separator}thread"))
        } else {
            None
        };
        if let Some(slot) = &thread_slot {
            meta.push_str(slot);
        }
        let now = SystemTime::now();
        let ages = format!(
            "created {} ago · updated {} ago",
            render::format_age(now, task.created_at),
            render::format_age(now, task.updated_at)
        );
        if meta.is_empty() {
            meta.push_str(&ages);
        } else {
            meta.push_str(" · ");
            meta.push_str(&ages);
        }
    }
    let thread_slot_width = thread_slot
        .as_deref()
        .map(render::display_width)
        .map(|width| u16::try_from(width).unwrap_or(u16::MAX));

    let focus = match model.input_mode() {
        BoardInputMode::EditTitle => Some(CaptureField::Title),
        BoardInputMode::EditNotes => Some(CaptureField::Notes),
        BoardInputMode::SelectThread | BoardInputMode::EditThread => Some(CaptureField::Thread),
        BoardInputMode::EditScope | BoardInputMode::FormScopeDropdown => Some(CaptureField::Scope),
        _ => None,
    };

    QueueOverlay::TaskPage {
        header_rows,
        header_identifier,
        header_identifier_task: (!editing_title).then(|| form.task_id()).flatten(),
        title_cursor,
        status_word,
        notes_rows,
        notes_cursor,
        more_lines,
        step_views,
        stored_step_count,
        step_cursor: form.steps.cursor.and_then(visible_step_index),
        step_add_selected: form.steps.add_selected,
        step_scroll: notes_scroll,
        step_marked: form.steps.delete_mark.and_then(visible_step_index),
        inline_step_editor,
        bottom_input,
        meta,
        meta_scope_x,
        meta_scope_width,
        thread_slot_width,
        focus,
        scope_dropdown,
    }
}

/// Draw the board into any ratatui frame (live TTY or [`ratatui::backend::TestBackend`]).
///
/// the paints the queue frame via [`render::draw_queue_frame`]. Classic master-detail
/// chrome is retired; overlays that still need the classic layout (edit band, save
/// recovery banner via message) are layered lightly on top where session mode requires it.
pub fn draw_board(frame: &mut Frame, model: &BoardModel) -> render::QueueHitMap {
    let hits = draw_board_impl(frame, model);
    if let Some(selection) = model.text_selection() {
        crate::ui::text_select::paint_selection(frame, &selection, &hits.copyable);
    }
    hits
}

/// The mouse hit-map for one board frame, without a live terminal.
///
/// [`draw_board`] is the one painter every board size uses; this renders the identical
/// frame into a scratch buffer purely to recover the hit-map [`draw_board_impl`] builds
/// beside the paint, so the live mouse loop and the screen the user is looking at can
/// never disagree about where a control is -- there is no second, hand-maintained copy of
/// the geometry to drift out of step with a renderer change.
pub fn board_hit_map(area: ratatui::layout::Rect, model: &BoardModel) -> render::QueueHitMap {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let width = area.width.max(1);
    let height = area.height.max(1);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("scratch terminal");
    let mut hits = render::QueueHitMap::default();
    let _ = terminal.draw(|frame| {
        hits = draw_board_impl(frame, model);
    });
    hits
}

/// Board-level surfaces whose payloads must outlive the overlay borrowing them.
struct OverlayPayloads<'a> {
    help_lines: Vec<String>,
    palette_commands: Vec<PaletteCommandRow<'a>>,
    scope_options: Vec<String>,
    scope_selected: usize,
}

impl<'a> OverlayPayloads<'a> {
    fn collect(model: &'a BoardModel) -> Self {
        let help_lines = if model.input_mode() == BoardInputMode::Help {
            help_card_lines()
        } else {
            Vec::new()
        };
        let palette_commands: Vec<PaletteCommandRow<'_>> =
            if model.command_surface() == CommandSurface::Palette {
                let visible = model.visible_commands();
                let selected = model.command_selected();
                visible
                    .iter()
                    .enumerate()
                    .map(|(i, cmd)| PaletteCommandRow {
                        label: cmd.label,
                        selected: Some(i) == selected,
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let scope_options: Vec<String> = if model.popup() == BoardPopup::ProjectPicker {
            model
                .project_options()
                .iter()
                .map(project_scope_option_label)
                .collect()
        } else if model.input_mode() == BoardInputMode::FormScopeDropdown {
            // Short project names: the dropdown lists scopes, not filesystem paths.
            model
                .form_scope_options()
                .iter()
                .map(|scope| match scope {
                    TaskScope::Global => "desk".to_string(),
                    TaskScope::Project { path } => render::short_project(path).to_string(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let scope_selected = if model.popup() == BoardPopup::ProjectPicker {
            model.project_picker_index().unwrap_or(0)
        } else {
            model
                .form_scope_options()
                .iter()
                .position(|scope| Some(scope) == model.form_scope_dropdown_choice())
                .unwrap_or(0)
        };
        Self {
            help_lines,
            palette_commands,
            scope_options,
            scope_selected,
        }
    }

    /// The board-level modal or capture surface that outranks the list and the page, if any.
    fn modal(
        &'a self,
        model: &'a BoardModel,
        geo: &tier::TierGeometry,
    ) -> Option<QueueOverlay<'a>> {
        if let Some(quick_add) = model.quick_add.as_ref().filter(|_| {
            matches!(
                model.input_mode(),
                BoardInputMode::QuickAdd | BoardInputMode::SaveRecovery
            )
        }) {
            let input_width = (geo.row_width as usize).saturating_sub(2);
            // A long title wraps instead of scrolling sideways. Exactly ONE row above
            // the input is reserved (the message row: the shifted rule sits two up),
            // so the draft may span at most two painted rows, windowed by the
            // minimum that keeps the caret's wrapped row visible. Longer drafts
            // window vertically rather than touching chrome.
            const QUICK_ADD_MAX_ROWS: usize = 2;
            let (all_rows, cursor_row, cursor_column) =
                wrapped_edit_rows(&quick_add.title, input_width);
            let shown = all_rows.len().clamp(1, QUICK_ADD_MAX_ROWS);
            let start = cursor_row
                .min(all_rows.len().saturating_sub(1))
                .saturating_sub(shown - 1);
            let window: Vec<String> = all_rows[start..(start + shown).min(all_rows.len())].to_vec();
            let caret_index = cursor_row.saturating_sub(start);
            let title = window.last().cloned().unwrap_or_default();
            // Continuations paint top-first (the painter stacks them upward by index).
            let above_rows: Vec<String> = window[..window.len().saturating_sub(1)].to_vec();
            let multiline = window.len() > 1;
            return Some(QueueOverlay::QuickAdd {
                input: crate::ui::render::BottomInputSlot {
                    text: title,
                    cursor_col: u16::try_from(cursor_column).unwrap_or(u16::MAX),
                    placeholder: "title…   !p = desk · !p name = project · !t name = thread",
                    refusal: None,
                    // Save recovery owns the verb row; ordinary quick-add refusals
                    // use the shared slot's reserved row above the cursor. A wrapped
                    // draft owns those rows itself, so the message yields while it is
                    // up and returns when the draft is back under the cap.
                    message: (!multiline && model.input_mode() != BoardInputMode::SaveRecovery)
                        .then(|| model.message())
                        .flatten(),
                    above_rows,
                    cursor_row_offset: u16::try_from(
                        window.len().saturating_sub(1).saturating_sub(caret_index),
                    )
                    .unwrap_or(0),
                },
                project_scope: matches!(quick_add.scope, TaskScope::Project { .. }),
                recovery: model.input_mode() == BoardInputMode::SaveRecovery,
            });
        }
        if model.input_mode() == BoardInputMode::Help {
            return Some(QueueOverlay::Help {
                lines: &self.help_lines,
            });
        }
        if model.command_surface() == CommandSurface::Palette {
            return Some(QueueOverlay::Palette {
                query: model.command_query(),
                commands: &self.palette_commands,
            });
        }
        if model.popup() == BoardPopup::ProjectPicker {
            return Some(QueueOverlay::ScopeDropdown {
                options: &self.scope_options,
                selected: self.scope_selected,
            });
        }
        None
    }

    /// The open task page, painted from the retained form at `geo`.
    fn task_page(
        &'a self,
        model: &'a BoardModel,
        form: &'a BoardForm,
        geo: &tier::TierGeometry,
        column: bool,
    ) -> QueueOverlay<'a> {
        let scope_dropdown = (model.input_mode() == BoardInputMode::FormScopeDropdown).then_some(
            FormScopeDropdown {
                options: &self.scope_options,
                selected: self.scope_selected,
            },
        );
        build_task_page_overlay(model, form, geo, scope_dropdown, column)
    }
}

/// Status row content: delete recovery notice (title + u undo hint) when armed; otherwise
/// the last action message, otherwise counts. When both channels are set (stale undo
/// refusal after delete), compose notice first then message so the refusal is visible.
fn status_row_content(model: &BoardModel) -> (Option<String>, Option<usize>) {
    let editing_on_page = model.open_field_edit().is_some() && model.form.is_some();
    let status_owned = match (model.visible_delete_notice(), model.message()) {
        (Some(title), Some(msg)) => Some(format!("{}  ·  {}", notice_framed(title, true), msg)),
        (Some(title), None) => Some(notice_framed(title, true)),
        (None, Some(msg)) => Some(msg.to_string()),
        (None, None) if editing_on_page => Some("editing…".to_string()),
        _ => None,
    };
    // The Undo control's offset inside `status_owned`, computed from the notice's own
    // composition rather than located by searching the composed row for the literal
    // `u Undo` text -- a title containing that literal (e.g. a task titled `u Undo now`)
    // would otherwise steal the region a `find` located. `notice_framed(title, true)`
    // always ends in `DELETE_NOTICE_UNDO`, so the control's start is exactly that
    // rendering's width less the control's own width.
    let status_undo_offset = model.visible_delete_notice().map(|title| {
        row_width(&notice_framed(title, true)).saturating_sub(row_width(DELETE_NOTICE_UNDO))
    });
    (status_owned, status_undo_offset)
}

/// Field named in the task header's `editing <field>` state slot while an editor is active.
fn editing_field(model: &BoardModel) -> Option<&'static str> {
    match model.input_mode() {
        BoardInputMode::EditTitle => Some("title"),
        BoardInputMode::EditNotes => Some("notes"),
        BoardInputMode::SelectThread | BoardInputMode::EditThread => Some("thread"),
        BoardInputMode::EditScope | BoardInputMode::FormScopeDropdown => Some("scope"),
        BoardInputMode::EditStep => Some("step"),
        _ => None,
    }
}

/// Header state slot for the task column: `status · project`, `editing <field>`, or
/// `unsaved` while a dirty draft waits with no editor active.
fn task_header_state(model: &BoardModel, form: &BoardForm, task: &crate::domain::Task) -> String {
    if let Some(field) = editing_field(model).filter(|_| form.task_id() == Some(task.id)) {
        return format!("editing {field}");
    }
    let is_retained = model
        .form
        .as_ref()
        .is_some_and(|retained| retained.task_id() == Some(task.id));
    if is_retained && model.task_session_dirty() {
        return "unsaved".to_string();
    }
    let status = match task.status {
        HumanStatus::Ready => "ready",
        HumanStatus::Started => "started",
        HumanStatus::Blocked => "blocked",
        HumanStatus::Review => "review",
        HumanStatus::Done => "done",
    };
    let project = match &task.scope {
        TaskScope::Project { path } => render::short_project(path).to_string(),
        TaskScope::Global => "desk".to_string(),
    };
    format!("{status} · {project}")
}

/// The dim stage crumb and key hints for the wide status row.
fn wide_status_hint(model: &BoardModel) -> (Option<&'static str>, &'static str) {
    let editing = model.open_field_edit().is_some()
        || model.input_mode() == BoardInputMode::FormScopeDropdown
        || model.task_editing();
    match model.wide_stage() {
        tier::WideStage::FullBoard => (None, "→ pane · enter open"),
        tier::WideStage::Split => (Some("board ▸ task"), "→ task · ← close · enter open"),
        tier::WideStage::Rail if editing => (Some("board ◂ task"), "shift+enter save · esc cancel"),
        tier::WideStage::Rail => (Some("board ◂ task"), "← board · → full page"),
        tier::WideStage::FullTask if editing => (None, "shift+enter save · esc cancel"),
        tier::WideStage::FullTask => (None, "← rail · esc back"),
    }
}

fn draw_board_impl(frame: &mut Frame, model: &BoardModel) -> render::QueueHitMap {
    let area = frame.area();
    let responsive = tier::resolve_responsive(area.width, area.height, model.wide_stage());
    if responsive.presentation == tier::ResponsivePresentation::WideSplit {
        return draw_wide_board(frame, model, area, responsive);
    }
    let geo = tier::resolve(area.width, area.height);
    let queue_view = model.queue_view();
    let selection_id = model.saved_task.or(model.selection_id);
    let scope_label = match &model.board_location {
        BoardLocation::Home { .. } => String::new(),
        BoardLocation::Project(path) => project_option_label(path.as_path()),
    };
    let (status_owned, status_undo_offset) = status_row_content(model);
    // The verb bar entries: computed from the selection and the open surface so the label
    // is true for the row it describes, and drawn from this one function -- the same one
    // the goldens call -- so a later edit to either side cannot silently re-open wording
    // drift.
    let verbs = board_verb_items(model);
    let payloads = OverlayPayloads::collect(model);
    let overlay = match payloads.modal(model, &geo) {
        Some(modal) => modal,
        None => match model.form.as_ref() {
            // A parked task page is not painted while the board owns the frame.
            Some(form)
                if !(model.focused_surface() == tier::FocusedSurface::Board && form.is_task()) =>
            {
                payloads.task_page(model, form, &geo, false)
            }
            _ => QueueOverlay::None,
        },
    };
    let frame_model = QueueFrameModel {
        tasks: &model.tasks,
        view: &queue_view,
        selection_id,
        at_home: model.at_home(),
        home_tab: model.home_tab(),
        scope_label: &scope_label,
        collapsed_projects: &model.collapsed_projects,
        collapsed_threads: &model.collapsed_threads,
        collapsed_thread_projects: &model.collapsed_thread_projects,
        status_message: status_owned.as_deref(),
        status_undo_offset,
        verb_items: &verbs,
        now: SystemTime::now(),
        overlay,
        detail_open: model.detail_open,
        list_scroll: model.list_scroll.get(),
        follow_list: model.follow_list.get(),
    };
    let (hits, painted_list_scroll) = render::draw_queue_frame(frame, &frame_model, &geo, area);
    if let Some((scroll, max_scroll)) = painted_list_scroll {
        model.list_scroll.set(scroll);
        model.list_max_scroll.set(max_scroll);
    }
    paint_board_form_toast(frame, model, &geo, area);
    hits
}

/// Board-form edits use the task page's status and verb rows rather than an inline rule row.
fn paint_board_form_toast(
    frame: &mut Frame,
    model: &BoardModel,
    geo: &tier::TierGeometry,
    area: ratatui::layout::Rect,
) {
    if model.open_field_edit().is_some() && model.form.is_none() {
        if let Some(row) = geo.rule_row {
            let toast_area =
                ratatui::layout::Rect::new(area.x, area.y.saturating_add(row), area.width, 1);
            // Use the mono-only helpers so no product frame emits foreground or background
            // color SGR codes.
            frame.render_widget(
                Paragraph::new(present_line(
                    &model.edit_chrome_row(toast_area.width as usize),
                    toast_area.width as usize,
                ))
                .style(render::style_bold()),
                toast_area,
            );
        }
    }
}

/// The wide stage slider: unboxed columns, one rule column, one shared footer.
///
/// Stage 0 is the board at full width, A splits board and task page, G puts the dim rail
/// beside the page, F is the page at full width. Focus follows the stage, and the footer
/// (rule, status row with its stage crumb, verb bar) is painted once across the frame.
fn draw_wide_board(
    frame: &mut Frame,
    model: &BoardModel,
    area: ratatui::layout::Rect,
    responsive: tier::ResponsiveGeometry,
) -> render::QueueHitMap {
    let stage = model.wide_stage();
    let task_focus = model.focused_surface() == tier::FocusedSurface::Task;
    let density = responsive.density;
    let queue_view = model.queue_view();
    let selection_id = model.saved_task.or(model.selection_id);
    let selected_task = selection_id.and_then(|id| model.tasks.iter().find(|task| task.id == id));
    let scope_label = match &model.board_location {
        BoardLocation::Home { .. } => String::new(),
        BoardLocation::Project(path) => project_option_label(path.as_path()),
    };
    let (status_owned, status_undo_offset) = status_row_content(model);
    let verbs = board_verb_items(model);
    let payloads = OverlayPayloads::collect(model);
    let frame_geo = tier::resolve_density(area.width, area.height, density);
    let modal = payloads.modal(model, &frame_geo);

    // The footer's owner decides its rows: a bottom input, the palette query, or the verbs
    // of the focused surface. Column heights follow the footer's (possibly shifted) rule.
    let footer_needs_input = modal.as_ref().is_some_and(render::has_bottom_input)
        || (task_focus && model.input_mode() == BoardInputMode::EditThread);
    let footer_geo = render::bottom_input_geometry(frame_geo, footer_needs_input);
    let column_height = footer_geo.rule_row.unwrap_or(area.height);
    let column_geo = |width: u16| tier::resolve_column(width, column_height, area.height, density);
    let column_rect = |rect: ratatui::layout::Rect| {
        ratatui::layout::Rect::new(rect.x, rect.y, rect.width, column_height.min(rect.height))
    };

    // The task column paints the retained page when it is bound to the selection (or owns
    // focus), otherwise a fresh view of the selected task. Nothing here touches the model.
    let retained = model
        .form
        .as_ref()
        .filter(|form| form.is_task() && (task_focus || form.task_id() == selection_id));
    let preview_form = (!task_focus && retained.is_none())
        .then(|| {
            selected_task.map(|task| {
                BoardForm::task(
                    task,
                    model.this_repo.as_deref(),
                    &model.tasks,
                    CaptureField::Title,
                )
            })
        })
        .flatten();
    let task_form = retained.or(preview_form.as_ref());
    let task_area = responsive.task_content();
    let task_geo = (task_area.width > 0).then(|| column_geo(task_area.width));
    let task_overlay = match (task_geo.as_ref(), task_form) {
        (Some(geo), Some(form)) => payloads.task_page(model, form, geo, true),
        (Some(_), None) => QueueOverlay::None,
        (None, _) => QueueOverlay::None,
    };
    let header_task = task_form.and_then(|form| {
        form.task_id()
            .and_then(|id| model.tasks.iter().find(|task| task.id == id))
            .map(|task| (form, task))
    });
    let header_identifier = header_task
        .and_then(|(_, task)| task.number)
        .map(|number| format!("T{number}"));
    let header_state = header_task.map(|(form, task)| task_header_state(model, form, task));
    let editing_title = task_focus && model.input_mode() == BoardInputMode::EditTitle;
    let header_title: Option<(String, Option<u16>)> = header_task.map(|(form, task)| {
        if editing_title {
            let width = task_geo.map(|geo| geo.row_width as usize).unwrap_or(0);
            let glyph_w = render::display_width(render::status_glyph(task.status));
            let id_w = header_identifier
                .as_deref()
                .map(render::display_width)
                .unwrap_or(0);
            // The painted state slot keeps one trailing pad cell.
            let state_w = header_state
                .as_deref()
                .map(render::display_width)
                .unwrap_or(0)
                + 1;
            let room = render::task_header_title_room(width, glyph_w, id_w, state_w);
            let (text, cursor) = escaped_line_window(&form.title, room.max(1));
            (text, Some(cursor))
        } else {
            (form.title.value().to_string(), None)
        }
    });
    let header = header_title
        .as_ref()
        .map(|(title, cursor)| render::TaskColumnHeader {
            glyph: header_task
                .map(|(_, task)| render::status_glyph(task.status))
                .unwrap_or("○"),
            identifier: header_identifier.as_deref(),
            identifier_task: header_task.and_then(|(form, _)| form.task_id()),
            title,
            title_cursor_col: *cursor,
            state: header_state.as_deref().unwrap_or_default(),
            bold: task_focus,
        });

    let board_frame = QueueFrameModel {
        tasks: &model.tasks,
        view: &queue_view,
        selection_id,
        at_home: model.at_home(),
        home_tab: model.home_tab(),
        scope_label: &scope_label,
        collapsed_projects: &model.collapsed_projects,
        collapsed_threads: &model.collapsed_threads,
        collapsed_thread_projects: &model.collapsed_thread_projects,
        status_message: status_owned.as_deref(),
        status_undo_offset,
        verb_items: &verbs,
        now: SystemTime::now(),
        overlay: if task_focus {
            QueueOverlay::None
        } else {
            modal.clone().unwrap_or(QueueOverlay::None)
        },
        detail_open: None,
        list_scroll: model.list_scroll.get(),
        follow_list: model.follow_list.get(),
    };
    let task_frame = QueueFrameModel {
        overlay: task_overlay.clone(),
        at_home: false,
        scope_label: "",
        list_scroll: 0,
        follow_list: false,
        ..board_frame.clone()
    };
    let footer_frame = QueueFrameModel {
        overlay: match (&modal, task_focus) {
            (Some(modal), _) => modal.clone(),
            (None, true) => task_overlay.clone(),
            (None, false) => QueueOverlay::None,
        },
        ..board_frame.clone()
    };

    let mut hits = render::QueueHitMap::default();
    let board_area = responsive.board;
    if board_area.width > 0 {
        let board_geo = column_geo(board_area.width);
        if stage == tier::WideStage::Rail {
            let rail_frame = QueueFrameModel {
                follow_list: true,
                ..board_frame.clone()
            };
            let mut rail_hits =
                render::draw_rail_frame(frame, &rail_frame, &board_geo, column_rect(board_area));
            hits.regions.append(&mut rail_hits.regions);
            hits.copyable.append(&mut rail_hits.copyable);
        } else {
            let (mut board_hits, painted_list_scroll) =
                render::draw_queue_frame(frame, &board_frame, &board_geo, column_rect(board_area));
            hits.regions.append(&mut board_hits.regions);
            hits.copyable.append(&mut board_hits.copyable);
            if let Some((scroll, max_scroll)) = painted_list_scroll {
                model.list_scroll.set(scroll);
                model.list_max_scroll.set(max_scroll);
            }
        }
    }
    if responsive.rule.width > 0 {
        let rule = column_rect(responsive.rule);
        for y in rule.top()..rule.bottom() {
            frame.render_widget(
                Paragraph::new(render::paint_bounded_line("│", 1, render::style_dim())),
                ratatui::layout::Rect::new(rule.x, y, 1, 1),
            );
        }
    }
    if let Some(task_geo) = task_geo.as_ref() {
        let mut task_hits = render::draw_task_column(
            frame,
            &task_frame,
            task_geo,
            column_rect(task_area),
            header,
            modal.as_ref().filter(|_| task_focus),
        );
        // A board-owned preview keeps its control hits, but only the focus router reads
        // them: a click there moves the stage first, then dispatches against this frame.
        hits.regions.append(&mut task_hits.regions);
        hits.copyable.append(&mut task_hits.copyable);
    }
    let (crumb, keys) = wide_status_hint(model);
    let mut footer_hits = render::draw_queue_footer(
        frame,
        &footer_frame,
        &footer_geo,
        area,
        Some(render::StatusHint { crumb, keys }),
    );
    hits.regions.append(&mut footer_hits.regions);
    hits.copyable.append(&mut footer_hits.copyable);
    paint_board_form_toast(frame, model, &footer_geo, area);
    hits
}
