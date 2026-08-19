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
    self, editor_row_budget, FormScopeDropdown, PaletteCommandRow, QueueFrameModel, QueueOverlay,
    VerbEntry,
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
        let mut items = Vec::with_capacity(5);
        items.push(VerbEntry {
            key: "e",
            label: "edit",
        });
        match task.status {
            HumanStatus::Ready => items.push(VerbEntry {
                key: "space",
                label: "start",
            }),
            HumanStatus::Done => items.push(VerbEntry {
                key: "space",
                label: "reopen",
            }),
            HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {}
        }
        if task.status == HumanStatus::Done {
            items.push(VerbEntry {
                key: "o",
                label: help("o", "reopen"),
            });
        } else {
            items.push(VerbEntry {
                key: "d",
                label: help("d", "done"),
            });
            items.push(VerbEntry {
                key: "b",
                label: if task.status == HumanStatus::Blocked {
                    "unblock"
                } else {
                    help("b", "block")
                },
            });
        }
        items.push(VerbEntry {
            key: "esc",
            label: "close",
        });
        return items;
    }

    let mut items = Vec::with_capacity(6);
    let selected_task = model
        .selected_id()
        .and_then(|id| model.tasks.iter().find(|t| t.id == id));

    if let Some(task) = selected_task {
        match task.status {
            HumanStatus::Ready => {
                items.push(VerbEntry {
                    key: "space",
                    label: "start",
                });
            }
            HumanStatus::Done => {
                items.push(VerbEntry {
                    key: "space",
                    label: "reopen",
                });
            }
            // `PrimaryVerb` does not act on Doing/Blocked/Review yet ("resume not
            // available yet"): omit the entry rather than advertise a no-op.
            HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {}
        }
        items.push(VerbEntry {
            key: "enter",
            label: "open",
        });
        if task.status == HumanStatus::Done {
            items.push(VerbEntry {
                key: "o",
                label: help("o", "reopen"),
            });
        } else {
            items.push(VerbEntry {
                key: "d",
                label: help("d", "done"),
            });
            items.push(VerbEntry {
                key: "b",
                label: if task.status == HumanStatus::Blocked {
                    "unblock"
                } else {
                    help("b", "block")
                },
            });
        }
    }
    items.push(VerbEntry {
        key: ":",
        label: help(":", "palette"),
    });
    items.push(VerbEntry {
        key: "?",
        label: help("?", "help"),
    });
    items
}

/// Build the task page's paint payload from the open task form. View mode wraps the notes
/// draft and windows it by the page scroll; field edits reuse the form's cursor windowing.
fn build_task_page_overlay<'a>(
    model: &BoardModel,
    form: &BoardForm,
    geo: &tier::TierGeometry,
    scope_dropdown: Option<FormScopeDropdown<'a>>,
) -> QueueOverlay<'a> {
    let width = geo.row_width as usize;
    let lay = render::task_page_layout(geo);
    let bound_task = form
        .task_id()
        .and_then(|id| model.tasks.iter().find(|task| task.id == id));
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

    // Notes body: wrapped + scrolled in view mode, cursor-windowed while editing.
    let notes_width = width.saturating_sub(3);
    let want = lay.notes_rows as usize;
    let (notes_rows, notes_cursor, more_lines) = if model.input_mode() == BoardInputMode::EditNotes
    {
        let (rows, row, col) = escaped_draft_rows(&form.notes, notes_width, want);
        (rows, Some((row, col)), 0)
    } else if form.notes.value().trim().is_empty() {
        // The painter names this case ("no notes yet"); hand it no rows.
        (Vec::new(), None, 0)
    } else {
        let all = wrapped_draft_rows(&form.notes, notes_width);
        let total = all.len();
        // Record what this frame could actually show, so the next scroll intent is bounded by
        // rendered rows rather than logical lines (see `BoardForm::notes_max_scroll`).
        let max_scroll = total.saturating_sub(want);
        form.notes_max_scroll.set(max_scroll);
        let scroll = form.notes_scroll.min(max_scroll);
        let rows: Vec<String> = all.iter().skip(scroll).take(want).cloned().collect();
        let more = total.saturating_sub(scroll + rows.len());
        (rows, None, more)
    };

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
    let editing_on_page =
        model.open_field_edit().is_some() && model.form.as_ref().is_some_and(BoardForm::is_task);
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
    let overlay = if model.input_mode() == BoardInputMode::Help {
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
        if form.is_task() {
            build_task_page_overlay(model, form, &geo, scope_dropdown)
        } else {
            let field_width = render::capture_field_width(&geo);
            let notes_height = editor_row_budget(&geo).saturating_sub(2).clamp(1, 3) as usize;
            let (title, title_cursor) = escaped_line_window(&form.title, field_width);
            let (notes_rows, notes_cursor_row, notes_cursor_col) =
                escaped_draft_rows(&form.notes, field_width, notes_height);
            let scope_label = match &form.scope {
                TaskScope::Project { path } => path.clone(),
                TaskScope::Global => "global".into(),
            };
            QueueOverlay::Capture {
                title,
                title_cursor,
                notes_rows,
                notes_cursor_row,
                notes_cursor_col,
                scope_label,
                focus: form.focus,
                scope_dropdown,
            }
        }
    } else {
        QueueOverlay::None
    };

    let frame_model = QueueFrameModel {
        tasks: &model.tasks,
        view: &queue_view,
        selection_id: model.selection_id,
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

    // Inline capture / board-form edits still own the rule row. The task page keeps the
    // rule and puts `editing…` on the status line instead: the verb bar already names save.
    if model.open_field_edit().is_some() && !model.form.as_ref().is_some_and(BoardForm::is_task) {
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
