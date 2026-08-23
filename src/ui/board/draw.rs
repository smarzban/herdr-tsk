//! Queue chrome, overlays, verb bar, and frame drawing hooks.

use std::time::SystemTime;

use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::domain::{HumanStatus, TaskScope};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{escaped_draft_rows, escaped_line_window, wrapped_draft_rows};
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
    project_option_label, project_scope_option_label, BoardForm, BoardInputMode, BoardModel,
    OwnedDeckScope,
};

/// Verb bar for the base board list: labels follow the selected task.
///
/// `space` starts a ready task or reopens a done one. On started/blocked/review it is
/// omitted (`PrimaryVerb` does not act yet). `b` reads `unblock` only on a blocked task.
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
        if model
            .form
            .as_ref()
            .is_some_and(|form| form.steps.cursor.is_some())
        {
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
            // `PrimaryVerb` does not act on Doing/Blocked/Review yet ("resume not
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
    let step_editor = form.steps.editor.as_ref().map(|editor| {
        let avail = (geo.row_width as usize).saturating_sub(2);
        let (text, cursor_col) = escaped_line_window(&editor.buffer, avail);
        crate::ui::render::BottomInputSlot {
            text,
            cursor_col,
            placeholder: "step…   enter save · ctrl+enter save+next · esc cancel",
            refusal: editor.refusal.as_deref(),
            // The bottom input replaces the shared status row. Forward recovery
            // and record-refusal feedback to the surface that is actually visible.
            message: model.message(),
        }
    });
    // A notes edit always keeps one row: the layout reserves it (the section caps
    // around it), so an active edit can never be scrolled/clamped out of the frame
    // entirely. The step editor uses the shared bottom slot, so steps alone
    // classify the page section.
    let page_geo = render::bottom_input_geometry(*geo, step_editor.is_some());
    let lay = render::task_page_layout(
        &page_geo,
        render::steps_section(step_views.len()),
        u16::from(model.input_mode() == BoardInputMode::EditNotes),
    );
    // The renderer and input reducer share this viewport size for page scrolling.
    form.steps.window_rows.set(lay.notes_rows as usize);
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

    // Header: indent + glyph + title window + gap + right-aligned status word.
    let word_cells = status_word.chars().count() + 1;
    let title_avail = width.saturating_sub(4 + word_cells);
    let (header, title_cursor) = if model.input_mode() == BoardInputMode::EditTitle {
        let (window, col) = escaped_line_window(&form.title, title_avail);
        (format!("{glyph} {window}"), Some(4 + col))
    } else {
        (
            format!("{glyph} {}", present_line(form.title.value(), title_avail)),
            None,
        )
    };

    // View mode supplies every wrapped note row. The shared page painter combines
    // that stream with the steps, then windows it once against the fixed viewport.
    let notes_width = width.saturating_sub(3);
    let want = lay.notes_rows as usize;
    let (notes_rows, notes_cursor, more_lines) = if model.input_mode() == BoardInputMode::EditNotes
    {
        let (rows, row, col) = escaped_draft_rows(&form.notes, notes_width, want);
        (rows, Some((row, col)), 0)
    } else if form.notes.value().trim().is_empty() {
        (Vec::new(), None, 0)
    } else {
        (wrapped_draft_rows(&form.notes, notes_width), None, 0)
    };
    let content = render::page_content_layout(notes_rows.len(), step_views.len(), lay.notes_rows);
    form.notes_max_scroll.set(content.max_scroll);
    form.steps.content_start.set(content.steps_start);

    // Meta footer: scope · created · updated (ages only while the bound task is present).
    let mut meta = match &form.scope {
        TaskScope::Project { path } => render::short_project(path).to_string(),
        TaskScope::Global => "global".to_string(),
    };
    if let Some(task) = bound_task {
        let now = SystemTime::now();
        meta.push_str(&format!(
            " · created {} ago · updated {} ago",
            render::format_age(now, task.created_at),
            render::format_age(now, task.updated_at)
        ));
    }

    let focus = match model.input_mode() {
        BoardInputMode::EditTitle => Some(CaptureField::Title),
        BoardInputMode::EditNotes => Some(CaptureField::Notes),
        BoardInputMode::EditScope | BoardInputMode::FormScopeDropdown => Some(CaptureField::Scope),
        _ => None,
    };

    QueueOverlay::TaskPage {
        header,
        title_cursor,
        status_word,
        notes_rows,
        notes_cursor,
        more_lines,
        step_views,
        step_cursor: form.steps.cursor,
        step_scroll: form.notes_scroll,
        step_marked: form.steps.delete_mark,
        step_editor,
        meta,
        focus,
        scope_dropdown,
    }
}

/// Draw the board into any ratatui frame (live TTY or [`ratatui::backend::TestBackend`]).
///
/// the paints the queue frame via [`render::draw_queue_frame`]. Classic master-detail
/// chrome is retired; overlays that still need the classic layout (edit band, save
/// recovery banner via message) are layered lightly on top where session mode requires it.
pub fn draw_board(frame: &mut Frame, model: &BoardModel) {
    let _hits = draw_board_impl(frame, model);
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
    let scope_label = match &model.deck_scope {
        OwnedDeckScope::All => "all projects".to_string(),
        OwnedDeckScope::Global => "global".to_string(),
        OwnedDeckScope::Project(path) => project_option_label(Some(path.as_path())),
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
                TaskScope::Global => "global".to_string(),
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
        let (title, title_cursor) = escaped_line_window(&quick_add.title, input_width);
        QueueOverlay::QuickAdd {
            input: crate::ui::render::BottomInputSlot {
                text: title,
                cursor_col: title_cursor,
                placeholder: "title…   !p global · !p name project · tab details",
                refusal: None,
                // Save recovery owns the verb row; ordinary quick-add refusals
                // use the shared slot's reserved row above the cursor.
                message: (model.input_mode() != BoardInputMode::SaveRecovery)
                    .then(|| model.message())
                    .flatten(),
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
        scope_label: &scope_label,
        all_projects_scope: matches!(&model.deck_scope, OwnedDeckScope::All),
        status_message: status_owned.as_deref(),
        status_undo_offset,
        verb_items: &verbs,
        verb_modifier: model.verb_modifier,
        now: SystemTime::now(),
        overlay,
        detail_open: model.detail_open,
    };
    let hits = render::draw_queue_frame(frame, &frame_model, &geo);

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
