//! Queue chrome, overlays, verb bar, and frame drawing hooks.

use std::time::SystemTime;

use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::domain::{HumanStatus, TaskScope};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{escaped_line_window, wrap_text, wrapped_draft_rows, wrapped_edit_rows};
use crate::ui::input::{help_card_lines, keymap_help_label};
use crate::ui::mouse::BoardPopup;
use crate::ui::present_line;
use crate::ui::render::{
    self, FormScopeDropdown, PaletteCommandRow, QueueFrameModel, QueueOverlay, VerbEntry,
};
use crate::ui::tier;

use super::chrome::{notice_framed, row_width, DELETE_NOTICE_UNDO};
use super::commands::CommandSurface;
use super::model::{
    project_option_label, project_scope_option_label, BoardForm, BoardInputMode, BoardLocation,
    BoardModel,
};

/// Verb bar for the base board list: labels follow the selected task.
///
/// `space` starts a ready task or reopens a done one. On started/blocked/review it is
/// omitted (`PrimaryVerb` is inert on started/blocked/review). `b` reads `unblock` only on a blocked task.
/// Done tasks show `o reopen` instead of `d`/`b`. `:` / `?` take their word from the keymap.
pub fn board_verb_items(model: &BoardModel) -> Vec<VerbEntry<'static>> {
    let help = |chord: &str, fallback: &'static str| keymap_help_label(chord).unwrap_or(fallback);

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
        let mut entries = Vec::with_capacity(6);
        entries.push(VerbEntry {
            key: "e",
            label: "edit",
        });
        if model.has_live_step_cursor() {
            entries.push(VerbEntry {
                key: "space",
                label: "toggle step",
            });
        } else {
            match task.status {
                HumanStatus::Ready => entries.push(VerbEntry {
                    key: "space",
                    label: "start",
                }),
                HumanStatus::Done => entries.push(VerbEntry {
                    key: "space",
                    label: "reopen",
                }),
                HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {}
            }
        }
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
        entries.push(VerbEntry {
            key: "esc",
            label: "close",
        });
        // AC-22: the footer verb bar lists the step-add verb while the page's task
        // has at least one step. Last, like the board's capture entry, so the
        // compact budget keeps the established verbs; the bar's prefix convention
        // implies the modifier, exactly as for every other mutating key.
        if !task.steps.is_empty() {
            entries.push(VerbEntry {
                key: "a",
                label: "step",
            });
        }
        return entries;
    }

    let mut entries = Vec::with_capacity(7);
    let selected_task = model
        .selected_id()
        .and_then(|id| model.tasks.iter().find(|t| t.id == id));

    if let Some(task) = selected_task {
        match task.status {
            HumanStatus::Ready => {
                entries.push(VerbEntry {
                    key: "space",
                    label: "start",
                });
            }
            HumanStatus::Done => {
                entries.push(VerbEntry {
                    key: "space",
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

/// Build the task page's paint payload from the open task form. View mode wraps the notes
/// draft and windows it by the page scroll; field edits reuse the form's cursor windowing.
fn build_task_page_overlay<'a>(
    model: &'a BoardModel,
    form: &'a BoardForm,
    geo: &tier::TierGeometry,
    scope_dropdown: Option<FormScopeDropdown<'a>>,
) -> QueueOverlay<'a> {
    let width = geo.row_width as usize;
    let bound_task = form
        .task_id()
        .and_then(|id| model.tasks.iter().find(|task| task.id == id));
    // The section consumes the extracted step views, never the raw storage; with
    // steps the layout halves the content region (AC-24), so the notes window —
    // and with it the notes scroll bound recorded below — keys off the same halved
    // budget the painter lays out.
    let step_views = bound_task.map(super::model::step_views).unwrap_or_default();
    // A step draft is windowed for the shared bottom input slot: the row less
    // the two-cell `▎ ` prompt that owns the terminal cursor.
    let step_editor = form
        .steps
        .editor
        .as_ref()
        .map(|editor| {
            let avail = (geo.row_width as usize).saturating_sub(2);
            let (text, cursor_col) = escaped_line_window(&editor.buffer, avail);
            crate::ui::render::BottomInputSlot {
                text,
                cursor_col,
                placeholder: "step…   enter save · ctrl+enter save+next · esc cancel",
                refusal: editor.refusal.as_deref(),
                above_rows: Vec::new(),
                cursor_row_offset: 0,
                // The bottom input replaces the shared status row. Forward recovery
                // and record-refusal feedback to the surface that is actually visible.
                message: model.message(),
            }
        })
        .or_else(|| {
            (model.input_mode() == BoardInputMode::EditThread).then(|| {
                let avail = (geo.row_width as usize).saturating_sub(2);
                let (text, cursor_col) = escaped_line_window(&form.thread, avail);
                crate::ui::render::BottomInputSlot {
                    text,
                    cursor_col,
                    placeholder: "thread…   enter save · esc cancel",
                    refusal: None,
                    above_rows: Vec::new(),
                    cursor_row_offset: 0,
                    // The shared bottom slot owns this refusal while it is visible, so it
                    // never leaks through the status row and remains legible at 40x10.
                    message: form.thread_refusal.as_deref(),
                }
            })
        });
    // A notes edit always keeps one row: the layout reserves it (the section caps
    // around it), so an active edit can never be scrolled/clamped out of the frame
    // entirely. The step editor uses the shared bottom slot, so steps alone
    // classify the page section.
    let page_geo = render::bottom_input_geometry(*geo, step_editor.is_some());
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
    let word_cells = status_word.chars().count() + 1;
    let title_avail = width.saturating_sub(4 + word_cells);
    // The header may grow only inside the page body: it must stop one row short
    // of the lowest chrome row with one note row still living under it, or a
    // pathological title would eat the page (and the painter's chrome).
    let page_bottom = [page_geo.rule_row, page_geo.status_row, page_geo.verb_row]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(page_geo.height);
    let header_cap = page_bottom.saturating_sub(3).max(1) as usize;
    let editing_title = model.input_mode() == BoardInputMode::EditTitle;
    let mut header_rows: Vec<String> = Vec::new();
    let mut title_cursor = None;
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
    let lay = render::task_page_layout(
        &page_geo,
        render::steps_section(step_views.len()),
        u16::from(model.input_mode() == BoardInputMode::EditNotes),
        header_rows.len().max(1) as u16,
    );
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
        // Wheel and arrow scrolling are inert while the editor owns the page, so the
        // frame's scroll may follow the caret without fighting a reading position:
        // keep the minimal window that still shows the caret's wrapped row.
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
    let content = render::page_content_layout(notes_rows.len(), step_views.len(), lay.notes_rows);
    form.notes_max_scroll.set(content.max_scroll);
    form.steps.content_start.set(content.steps_start);
    form.notes_width.set(notes_width);

    // Meta footer: scope · thread · created · updated (ages only while the bound task is
    // present). Keep its clickable pieces separate from the painted string: a project basename
    // is user-controlled and may contain the same separator or label text.
    let meta_scope = match &form.scope {
        TaskScope::Project { path } => render::short_project(path).to_string(),
        TaskScope::Global => "desk".to_string(),
    };
    let meta_scope_width = u16::try_from(render::display_width(&meta_scope)).unwrap_or(u16::MAX);
    let mut meta = meta_scope.clone();
    let mut thread_slot = None;
    if let Some(task) = bound_task {
        thread_slot = if let Some(thread) = task.thread.as_deref() {
            Some(format!(" · #{thread}"))
        } else if matches!(
            model.input_mode(),
            BoardInputMode::EditTitle
                | BoardInputMode::EditNotes
                | BoardInputMode::EditThread
                | BoardInputMode::EditScope
                | BoardInputMode::FormScopeDropdown
        ) {
            // An empty thread still needs a visible field-sized footer target while the form
            // is editing, otherwise mouse users can only reach Thread after it already exists.
            Some(" · thread".to_string())
        } else {
            None
        };
        if let Some(slot) = &thread_slot {
            meta.push_str(slot);
        }
        let now = SystemTime::now();
        meta.push_str(&format!(
            " · created {} ago · updated {} ago",
            render::format_age(now, task.created_at),
            render::format_age(now, task.updated_at)
        ));
    }
    let thread_slot_width = thread_slot
        .as_deref()
        .map(render::display_width)
        .map(|width| u16::try_from(width).unwrap_or(u16::MAX));

    let focus = match model.input_mode() {
        BoardInputMode::EditTitle => Some(CaptureField::Title),
        BoardInputMode::EditNotes => Some(CaptureField::Notes),
        BoardInputMode::EditThread => Some(CaptureField::Thread),
        BoardInputMode::EditScope | BoardInputMode::FormScopeDropdown => Some(CaptureField::Scope),
        _ => None,
    };

    QueueOverlay::TaskPage {
        header_rows,
        title_cursor,
        status_word,
        notes_rows,
        notes_cursor,
        more_lines,
        step_views,
        step_cursor: form.steps.cursor,
        step_scroll: notes_scroll,
        step_marked: form.steps.delete_mark,
        step_editor,
        meta,
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

fn draw_board_impl(frame: &mut Frame, model: &BoardModel) -> render::QueueHitMap {
    let area = frame.area();
    let geo = tier::resolve(area.width, area.height);
    let queue_view = model.queue_view();
    let scope_label = match &model.board_location {
        BoardLocation::Home { .. } => String::new(),
        BoardLocation::Project(path) => project_option_label(path.as_path()),
    };
    // Status line surfaces delete recovery notice (title + u undo hint) when armed;
    // otherwise the last action message, otherwise counts.
    // When both channels are set (stale undo refusal after delete), compose notice first
    // then message so the refusal is visible. Use visible + notice_framed for
    // consistency with chrome row and hit test.
    let editing_on_page = model.open_field_edit().is_some() && model.form.is_some();
    let status_owned = match (model.visible_delete_notice(), model.message()) {
        (Some(title), Some(msg)) => Some(format!("{}  ·  {}", notice_framed(title, true), msg)),
        (Some(title), None) => Some(notice_framed(title, true)),
        (None, Some(msg)) => Some(msg.to_string()),
        (None, None) if editing_on_page => Some("editing…".to_string()),
        _ => None,
    };
    // Minor 1: the Undo control's offset inside `status_owned`,
    // computed from the notice's own composition rather than located by searching the
    // composed row for the literal `u Undo` text -- a title containing that literal (e.g.
    // a task titled `u Undo now`) would otherwise steal the region a `find` located, going
    // by which occurrence came first rather than which one the notice actually painted.
    // `notice_framed(title, true)` always ends in `DELETE_NOTICE_UNDO`, so the control's
    // start is exactly that rendering's width less the control's own width; this is `None`
    // whenever no notice (with its Undo control) is present at all.
    let status_undo_offset = model.visible_delete_notice().map(|title| {
        row_width(&notice_framed(title, true)).saturating_sub(row_width(DELETE_NOTICE_UNDO))
    });
    // the verb bar entries: computed from the selection and the open
    // surface so the label is true for the row it describes, and drawn from this one
    // function -- the same one the goldens call -- so a later edit to either side cannot
    // silently re-open the wording drift the round-2 review found.
    let verbs = board_verb_items(model);
    // Overlay payloads must outlive the frame_model borrow of their slices.
    let help_lines = if model.input_mode() == BoardInputMode::Help {
        help_card_lines(model.verb_modifier)
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
    let overlay = if let Some(quick_add) = model.quick_add.as_ref().filter(|_| {
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
        QueueOverlay::QuickAdd {
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
        }
    } else if model.input_mode() == BoardInputMode::Help {
        QueueOverlay::Help { lines: &help_lines }
    } else if model.command_surface() == CommandSurface::Palette {
        QueueOverlay::Palette {
            query: model.command_query(),
            commands: &palette_commands,
        }
    } else if model.popup() == BoardPopup::ProjectPicker {
        QueueOverlay::ScopeDropdown {
            options: &scope_options,
            selected: scope_selected,
        }
    } else if let Some(form) = model.form.as_ref() {
        let scope_dropdown = (model.input_mode() == BoardInputMode::FormScopeDropdown).then_some(
            FormScopeDropdown {
                options: &scope_options,
                selected: scope_selected,
            },
        );
        build_task_page_overlay(model, form, &geo, scope_dropdown)
    } else {
        QueueOverlay::None
    };

    let frame_model = QueueFrameModel {
        tasks: &model.tasks,
        view: &queue_view,
        selection_id: model.saved_task.or(model.selection_id),
        at_home: model.at_home(),
        home_tab: model.home_tab(),
        scope_label: &scope_label,
        collapsed_projects: &model.collapsed_projects,
        collapsed_threads: &model.collapsed_threads,
        collapsed_thread_projects: &model.collapsed_thread_projects,
        status_message: status_owned.as_deref(),
        status_undo_offset,
        verb_items: &verbs,
        verb_modifier: model.verb_modifier,
        now: SystemTime::now(),
        overlay,
        detail_open: model.detail_open,
        list_scroll: model.list_scroll.get(),
        follow_list: model.follow_list.get(),
        hover_id: model.hover_id,
    };
    let (hits, painted_list_scroll) = render::draw_queue_frame(frame, &frame_model, &geo);
    if let Some((scroll, max_scroll)) = painted_list_scroll {
        model.list_scroll.set(scroll);
        model.list_max_scroll.set(max_scroll);
    }

    // Board-form edits use the task page's status and verb rows rather than an inline rule row.
    if model.open_field_edit().is_some() && model.form.is_none() {
        if let Some(row) = geo.rule_row {
            let toast_area = ratatui::layout::Rect::new(0, row, area.width, 1);
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
    hits
}
