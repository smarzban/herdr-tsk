//! Board intent reducer.

use std::path::PathBuf;
use std::time::Instant;

use uuid::Uuid;

use crate::config::{default_config_dir, SettingsRecord};
use crate::context::InvocationSnapshot;
use crate::domain::{normalize_thread, DomainError, DomainState, HumanStatus, TaskScope};
use crate::ui::capture::{CaptureField, TITLE_REQUIRED_MESSAGE};
use crate::ui::edit::{flatten_line_breaks, EditBuffer};
use crate::ui::input::BoardIntent;
use crate::ui::mouse::BoardPopup;
use crate::ui::queue::ThreadProjectCollapseKey;

use super::commands::{resolve_board_command, CommandSurface};
use super::model::{
    BoardForm, BoardInputMode, BoardLocation, BoardModel, IntentOutcome, ProjectPickerState,
    ProjectScopeOption, StepEditor, StepEditorSave, TaskEditSave,
};

/// What the row says when an action that aims at the selection is asked for on a board that
/// has none. One wording, so the same refusal always reads the same way.
const NO_SELECTION: &str = "select a task first";

/// What the row says when an Undo is refused because its target moved on.
///
/// The words are the domain's, taken from [`DomainError::StaleUndo`] rather than restated
/// here, so this cannot drift from the refusal it presents. Only the *order* is this
/// layer's: the domain leads with the identifier, which is right for a log line or an error
/// chain, but the board has one chrome row that clips, and a 36-character uuid in front
/// spends the whole budget on the part the reader cannot act on. Reading the same words
/// reason-first means a clipped row loses the uuid instead of the meaning.
///
/// If the domain ever stops leading with the identifier, there is nothing to move and its
/// line is shown exactly as stated.
fn stale_undo_message(id: Uuid) -> String {
    let stated = DomainError::StaleUndo(id).to_string();
    let identifier = format!("task {id}");
    match stated.strip_prefix(&identifier).map(str::trim_start) {
        Some(reason) if !reason.is_empty() => format!("{reason} · {identifier}"),
        _ => stated,
    }
}

/// A **mutating board action**: an intent this reducer may report as persisting domain
/// state, and so the one predicate every rule about mutating actions reads.
///
/// The board loop uses it to decide whether a save baseline has to be loaded before the
/// intent is applied, and [`apply_intent`] uses it to time the delete recovery notice
///. Both read this list rather than keeping one of their own, so the pinned
/// definition and the classification cannot drift apart.
pub fn board_intent_may_persist(intent: &BoardIntent) -> bool {
    matches!(
        intent,
        BoardIntent::ConfirmEdit
            | BoardIntent::ConfirmEditNext
            | BoardIntent::SetStatus(_)
            | BoardIntent::Complete
            | BoardIntent::Reopen
            | BoardIntent::SoftDelete
            | BoardIntent::Undo
            | BoardIntent::PrimaryVerb
            | BoardIntent::ToggleBlock
            | BoardIntent::QuickAddSave
            | BoardIntent::QuickAddSaveNext
    )
}

/// The chrome row's lifetime rule, in one place.
///
/// The row carries two different things with two different lifetimes: the **message**, which
/// is feedback about the action the user just took, and the **delete recovery notice**, which
/// outlives its action on purpose so the way back stays on screen. Every event:
///
/// - **an edit opens** — the message is cleared (a refusal left by an earlier action would
///   read as a refusal of an edit nobody has attempted yet); the notice is kept, because
///   opening an edit is not a mutating action and says the notice survives one.
///   Applied by the two `BeginEdit…` arms, which know whether a field actually opened.
/// - **an edit is cancelled** — the message is cleared, for the same reason in reverse: the
///   refusal explained a field that is now gone; the notice is kept. Applied by the
///   `CancelEdit` arm.
/// - **a mutating board action** — *both* are cleared, and only here. The notice because
///   says so; the message because it was feedback about an earlier action, and a
///   message left standing would be read as this action's answer. Whatever the action then
///   reports (a confirmation, a refusal, the stale-undo message) owns the row on its own,
///   because this clearing happens before the intent is applied. The exception is an action
///   that **refused**: clears the notice for a change, and a refusal is not one, so
///   [`apply_intent`] puts the notice back whenever the intent it ran reported nothing to
///   persist: a refused Undo or Done keeps the way back it never spent. That is one rule at one
///   place rather than a per-arm exception, and it reads off the same outcome the caller
///   persists on.
/// - **any other action** — both are left exactly as they are.
///
/// Two events outside this reducer touch the notice. A failed save
/// ([`BoardModel::begin_save_recovery`]) *suspends* it, because a deletion that did not
/// reach disk has nothing to undo, and a successful Retry
/// ([`BoardModel::end_save_recovery`]) puts it back. An open command surface *covers* the
/// notice for as long as it is open
/// ([`BoardModel::visible_delete_notice`]): the notice is not taken down, but for that
/// duration it is genuinely not painted, which is the cost of the row never advertising a
/// click the modal surface would swallow.
///
/// When both channels are set the row carries **both**, notice first, message second, legend
/// last (see [`fit_chrome_row`]). Nothing on this row wins by taking another thing off it.
fn apply_chrome_row_lifetime(model: &mut BoardModel, intent: &BoardIntent) {
    if board_intent_may_persist(intent) {
        model.clear_delete_notice();
        model.clear_message();
    }
}

/// Apply a board intent to domain + model.
///
/// Mutating intents call Task Domain only. Caller persists with Task Store when outcome is
/// [`IntentOutcome::Persist`]. The capture snapshot is retained by the form.
pub fn apply_intent(
    domain: &mut DomainState,
    model: &mut BoardModel,
    intent: BoardIntent,
    snapshot: Option<&InvocationSnapshot>,
) -> Result<IntentOutcome, DomainError> {
    // A successful quick add remains emphasized only until the next input intent.
    model.clear_saved_task();
    // Mark-then-confirm (AC-11): any intent other than the delete verb's own
    // confirmation routes clears an armed steps delete mark. Command confirmations
    // are excluded here because they recurse below as the intent they resolved to, which
    // then faces this same rule as itself.
    if model
        .form
        .as_ref()
        .is_some_and(|form| form.steps.delete_mark.is_some())
        && !matches!(
            intent,
            BoardIntent::SoftDelete | BoardIntent::ConfirmCommand | BoardIntent::SelectCommand(_)
        )
    {
        if let Some(form) = model.form.as_mut() {
            form.steps.delete_mark = None;
        }
        // The press-again hint lives exactly as long as the mark it explains
        // (AC-23): the intervening intent that disarms the mark takes the footer
        // message down with it, before whatever the intent itself has to report.
        model.clear_message();
    }
    let notice_before = model.delete_notice().map(str::to_string);
    let mutating = board_intent_may_persist(&intent);
    let result = apply_board_intent(domain, model, intent, snapshot);
    // A command confirmation carries no lifetime of its own: it recurses with the command it
    // resolved to, and that intent is classified on the way through, so it is the recursion
    // that restores.
    if mutating && !matches!(result, Ok(IntentOutcome::Persist)) {
        model.delete_notice = notice_before;
    }
    result
}

fn apply_board_intent(
    domain: &mut DomainState,
    model: &mut BoardModel,
    intent: BoardIntent,
    snapshot: Option<&InvocationSnapshot>,
) -> Result<IntentOutcome, DomainError> {
    // While the task page owns input, its verbs act on the page's own task: keep the
    // selection pinned to the bound id, whatever deck visibility did to it meanwhile
    // (a completed task leaves the list, but the page and its verbs stay on it).
    //
    // Closing the page ENDS that contract, so the closing intents are excluded. Pinning
    // through the close left `selection_id` on a task that had just left the deck (complete
    // the page's task with `d`, then Esc): the board then painted no highlighted row while
    // the pin still named the hidden task, and the next `space` or `d` mutated something the
    // user could not see. Excluding them lets `reanchor_selection` move the pin to a visible
    // row, which is what it already does for every other way a task leaves the deck.
    let closes_the_page = matches!(intent, BoardIntent::CloseLayer | BoardIntent::OpenTaskPage);
    if !closes_the_page
        && matches!(
            model.input_mode,
            BoardInputMode::TaskPage
                | BoardInputMode::EditTitle
                | BoardInputMode::EditNotes
                | BoardInputMode::EditScope
                | BoardInputMode::FormScopeDropdown
        )
    {
        if let Some(bound) = model
            .form
            .as_ref()
            .filter(|form| form.is_task())
            .and_then(BoardForm::task_id)
        {
            model.selection_id = Some(bound);
        }
    }
    // What this intent does to the chrome row, decided once, before it is applied.
    apply_chrome_row_lifetime(model, &intent);

    // A command surface is a dispatch surface: any real intent closes it first, so the
    // reducer below runs exactly as it does for the direct keyboard or chip route.
    // CloseLayer owns the progressive dismiss order, including the command
    // surface as its first layer, so it must not be pre-cleared here.
    if !matches!(
        intent,
        BoardIntent::OpenCommandPalette
            | BoardIntent::CommandNext
            | BoardIntent::CommandPrev
            | BoardIntent::CommandQueryInsert(_)
            | BoardIntent::CommandQueryInsertText(_)
            | BoardIntent::CommandQueryBackspace
            | BoardIntent::ConfirmCommand
            | BoardIntent::SelectCommand(_)
            | BoardIntent::CloseLayer
    ) {
        model.close_command_surface();
    }

    match intent {
        BoardIntent::OpenCommandPalette => {
            if model.project_picker.is_some() {
                return Ok(IntentOutcome::None);
            }
            model.open_command_surface(CommandSurface::Palette);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CommandNext => {
            model.move_command_selection(true);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CommandPrev => {
            model.move_command_selection(false);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CommandQueryInsert(character) => {
            if model.surface == CommandSurface::Palette {
                model.command_query.push(character);
                model.command_selected = 0;
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CommandQueryInsertText(text) => {
            if model.surface == CommandSurface::Palette {
                // The query is a single search line, so each pasted break becomes one space
                // rather than gluing the words on either side of it together (as Title does).
                model.command_query.push_str(&flatten_line_breaks(&text));
                model.command_selected = 0;
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CommandQueryBackspace => {
            if model.surface == CommandSurface::Palette {
                model.command_query.pop();
                model.command_selected = 0;
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CloseCommandSurface => return Ok(IntentOutcome::None),
        // The app loop performs the OSC 52 write so pure reducer tests stay terminal-free.
        BoardIntent::CopyTaskNumber(_) => return Ok(IntentOutcome::None),
        BoardIntent::OpenWalkthrough => return Ok(IntentOutcome::None),
        BoardIntent::ConfirmCommand | BoardIntent::SelectCommand(_) => {
            // Resolve to the existing intent, then run that exact route. `SelectCommand`
            // reaches here only defensively: every real caller -- the key, paste and
            // mouse routes, and `apply_board_intent_with_save_recovery` -- already runs it
            // through `resolve_board_command` before an intent gets this far, exactly as
            // they already do for `ConfirmCommand`; this arm just keeps a direct
            // `apply_intent` call (as tests make) from skipping that resolution.
            let Some(resolved) = resolve_board_command(model, intent) else {
                return Ok(IntentOutcome::None);
            };
            return apply_intent(domain, model, resolved, snapshot);
        }
        BoardIntent::Quit => {
            // Esc closes an open menu first; second Esc quits.
            if model.popup != BoardPopup::None {
                model.close_popup();
                return Ok(IntentOutcome::None);
            }
            return Ok(IntentOutcome::Quit);
        }
        BoardIntent::OpenCapture => {
            model.close_popup();
            model.form = None;
            // A non-All board scope wins for this quick-add draft. All projects retains the
            // invocation default.
            let scope = model
                .quick_add_scope()
                .or_else(|| snapshot.map(crate::ui::capture::CaptureModel::default_scope))
                .unwrap_or(TaskScope::Global);
            model.quick_add = Some(super::model::QuickAddState::new(snapshot.cloned(), scope));
            model.quick_add_save = None;
            model.input_mode = BoardInputMode::QuickAdd;
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddInsert(character) => {
            model.invalidate_quick_add_stash();
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.insert_char(character);
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddInsertText(text) => {
            model.invalidate_quick_add_stash();
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.insert_text(&flatten_line_breaks(&text));
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddBackspace => {
            model.invalidate_quick_add_stash();
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.backspace();
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddDeleteForward => {
            model.invalidate_quick_add_stash();
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.delete_forward();
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveLeft => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_left();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveRight => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_right();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveLineStart => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_line_start();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveLineEnd => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_line_end();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveWordLeft => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_word_left();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddMoveWordRight => {
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title.move_word_right();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ExpandQuickAdd => {
            let Some(quick_add) = model.quick_add.as_ref() else {
                return Ok(IntentOutcome::None);
            };
            if model.form.as_ref().is_some_and(|form| !form.is_task()) {
                // The task page is view-first for persisted tasks. A capture draft has nothing
                // to view, so quick-add deliberately opens its page in Notes edit mode.
                model.focus_form_field(CaptureField::Notes);
                return Ok(IntentOutcome::None);
            }
            let lifted = match lift_quick_add_tokens(
                quick_add.title.value(),
                domain,
                quick_add.snapshot.as_ref().as_ref(),
            ) {
                Ok(lifted) => lifted,
                Err(message) => {
                    model.set_message(message);
                    return Ok(IntentOutcome::None);
                }
            };
            let scope = lifted.scope.unwrap_or_else(|| quick_add.scope.clone());
            let snapshot = quick_add.snapshot.as_ref().clone();
            let mut form = BoardForm::capture(snapshot, model.this_repo.as_deref(), &model.tasks);
            form.title = crate::ui::edit::seeded_draft(&lifted.title);
            form.scope = scope;
            form.thread =
                crate::ui::edit::seeded_draft(lifted.thread.as_deref().unwrap_or_default());
            form.focus = CaptureField::Notes;
            form.select_current_scope();
            model.form = Some(form);
            model.input_mode = BoardInputMode::EditNotes;
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CancelQuickAdd => {
            model.discard_quick_add();
            // A save+next acknowledgement has no free status row while the line is open.
            // Keep it for the restored board row when the user finally closes quick-add.
            return Ok(IntentOutcome::None);
        }
        BoardIntent::QuickAddSelectIndex(index) => {
            model.discard_quick_add();
            return apply_board_intent(domain, model, BoardIntent::SelectIndex(index), snapshot);
        }
        BoardIntent::QuickAddSave | BoardIntent::QuickAddSaveNext => {
            return quick_add_save(
                domain,
                model,
                matches!(intent, BoardIntent::QuickAddSaveNext),
            );
        }
        BoardIntent::FormFocusNext => {
            if model.input_mode == BoardInputMode::TaskPage {
                // The view names no field: entering from the page lands on the form's own
                // field (Title at open) rather than advancing past it.
                model.enter_page_field_focus();
            } else if model.form.is_some() && model.input_mode != BoardInputMode::FormScopeDropdown
            {
                model.move_form_focus(true);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FormFocusPrev => {
            if model.input_mode == BoardInputMode::TaskPage {
                model.enter_page_field_focus();
            } else if model.form.is_some() && model.input_mode != BoardInputMode::FormScopeDropdown
            {
                model.move_form_focus(false);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FocusFormField(field) => {
            model.focus_form_field(field);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FormCycleScope => {
            if model
                .form
                .as_ref()
                .is_some_and(|form| form.focus == CaptureField::Scope)
                && model.input_mode != BoardInputMode::FormScopeDropdown
            {
                model.cycle_form_scope();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::OpenFormScopeDropdown => {
            model.open_form_scope_dropdown();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FormScopeNext => {
            model.move_form_scope_dropdown(true);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FormScopePrev => {
            model.move_form_scope_dropdown(false);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ConfirmFormScopeDropdown => {
            if model.input_mode == BoardInputMode::FormScopeDropdown {
                model.close_form_scope_dropdown(true);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CancelFormScopeDropdown => {
            if model.input_mode == BoardInputMode::FormScopeDropdown {
                model.close_form_scope_dropdown(false);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectFormScopeOption(index) => {
            model.select_form_scope_option(index);
            return Ok(IntentOutcome::None);
        }

        BoardIntent::SelectNext => {
            model.select_next();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectPrev => {
            model.select_prev();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectIndex(idx) => {
            model.select_index(idx);
            // A row click also expands that row's peek; a second click on the same row
            // inside the double-click window opens the task page instead.
            if let Some(id) = model.selected_id() {
                let now = Instant::now();
                let is_double = model.last_row_click.is_some_and(|(at, last)| {
                    last == id && now.duration_since(at) <= ROW_DOUBLE_CLICK_WINDOW
                });
                if is_double {
                    model.last_row_click = None;
                    open_task_page_on(domain, model, id);
                } else {
                    model.last_row_click = Some((now, id));
                    if model.detail_open == Some(id) {
                        model.detail_open = None;
                    } else {
                        model.detail_open = Some(id);
                    }
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ListScrollTo(offset) => {
            model.set_list_scroll(offset);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::BeginAddStep => {
            model.close_popup();
            // The step input lives on the task page's footer row; from any other
            // surface there is no footer line to paint it on, so the verb is inert.
            if model.form.as_ref().is_some_and(|form| form.is_task()) {
                open_step_editor(model, "", None);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::BeginEditTitle | BoardIntent::BeginEditNotes | BoardIntent::BeginEditScope => {
            model.close_popup();
            let focus = match intent {
                BoardIntent::BeginEditTitle => CaptureField::Title,
                BoardIntent::BeginEditNotes => CaptureField::Notes,
                BoardIntent::BeginEditScope => CaptureField::Scope,
                _ => unreachable!("matched task-form entry intent"),
            };
            // Contextual rename (AC-10): on the page with the step cursor active, `e`
            // opens the footer's one-line step input seeded with the highlighted
            // step instead of the title field.
            if intent == BoardIntent::BeginEditTitle {
                if let Some((task_id, step_id)) = cursor_step(domain, model) {
                    let text = domain.get(task_id).and_then(|task| {
                        task.steps
                            .iter()
                            .find(|step| step.id == step_id)
                            .map(|step| step.text.clone())
                    });
                    if let Some(text) = text {
                        open_step_editor(model, &text, Some(step_id));
                        return Ok(IntentOutcome::None);
                    }
                }
            }
            // The page already open: move focus into the asked field, keep every draft.
            if model.form.as_ref().is_some_and(BoardForm::is_task) {
                model.focus_form_field(focus);
                return Ok(IntentOutcome::None);
            }
            if let Some(id) = model.selected_id() {
                if let Some(task) = domain.get(id) {
                    // One immutable id and three independent drafts are captured at open.
                    // `sync_from_domain` deliberately never writes this form, so background
                    // refresh can reanchor selection without redirecting its later save.
                    let form =
                        BoardForm::task(task, model.this_repo.as_deref(), &model.tasks, focus);
                    model.input_mode = form.parent_mode();
                    model.form = Some(form);
                    model.clear_message();
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditInsert(c) => {
            edit_draft(model, |draft| draft.insert_char(c));
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditInsertText(text) => {
            // Title stays one line in either form; Notes preserves pasted line breaks.
            // The step editor is one line by construction, so it flattens like Title.
            let single_line = model.input_mode == BoardInputMode::EditStep
                || model
                    .form
                    .as_ref()
                    .is_some_and(|form| form.focus == CaptureField::Title);
            edit_draft(model, |draft| {
                if single_line {
                    draft.insert_text(&flatten_line_breaks(&text));
                } else {
                    draft.insert_text(&text);
                }
            });
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditInsertLineBreak => {
            if model
                .form
                .as_ref()
                .is_some_and(|form| form.focus == CaptureField::Notes)
            {
                edit_draft(model, |draft| draft.insert_char('\n'));
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditBackspace => {
            edit_draft(model, EditBuffer::backspace);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditDeleteForward => {
            edit_draft(model, EditBuffer::delete_forward);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveLeft => {
            edit_draft(model, EditBuffer::move_left);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveRight => {
            edit_draft(model, EditBuffer::move_right);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveUp | BoardIntent::EditMoveDown => {
            // Vertical movement wraps at the painted notes width the renderer
            // recorded; until a frame has been drawn (width 0) it stays inert.
            let delta: isize = if matches!(intent, BoardIntent::EditMoveDown) {
                1
            } else {
                -1
            };
            let target = model
                .form
                .as_ref()
                .filter(|form| {
                    form.focus == CaptureField::Notes
                        && model.input_mode == BoardInputMode::EditNotes
                })
                .and_then(|form| {
                    crate::ui::edit::wrapped_vertical_move(
                        &form.notes,
                        form.notes_width.get(),
                        delta,
                    )
                });
            if let Some(target) = target {
                edit_draft(model, |draft| draft.set_cursor(target));
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveLineStart => {
            edit_draft(model, EditBuffer::move_line_start);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveLineEnd => {
            edit_draft(model, EditBuffer::move_line_end);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveWordLeft => {
            edit_draft(model, EditBuffer::move_word_left);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::EditMoveWordRight => {
            edit_draft(model, EditBuffer::move_word_right);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CancelEdit => {
            // The step line editor cancels to page view: draft discarded, no mutation,
            // the page and its step cursor state untouched.
            if model.input_mode == BoardInputMode::EditStep
                && model.form.as_ref().is_some_and(BoardForm::is_task)
            {
                close_step_editor(model);
                return Ok(IntentOutcome::None);
            }
            // Field edit on the task page: Esc cancels the field being edited (its draft
            // resets to the saved value) and steps back to view mode. Drafts on other
            // fields survive; the second Esc closes the page.
            if matches!(
                model.input_mode,
                BoardInputMode::EditTitle
                    | BoardInputMode::EditNotes
                    | BoardInputMode::EditThread
                    | BoardInputMode::EditScope
            ) && model.form.as_ref().is_some_and(BoardForm::is_task)
            {
                let field = model
                    .form
                    .as_ref()
                    .map(|form| form.focus)
                    .unwrap_or(CaptureField::Title);
                let saved = model
                    .form
                    .as_ref()
                    .and_then(|form| form.task_id())
                    .and_then(|id| model.tasks.iter().find(|task| task.id == id))
                    .cloned();
                if let (Some(form), Some(task)) = (model.form.as_mut(), saved) {
                    form.reset_field_to_saved(field, &task);
                }
                model.input_mode = BoardInputMode::TaskPage;
                model.clear_message();
                return Ok(IntentOutcome::None);
            }
            // Esc from an expanded capture returns to its retained quick-add line. Keep the
            // complete form as a stash so Tab can restore its notes and scope.
            if model.quick_add.is_some() && model.form.as_ref().is_some_and(|form| !form.is_task())
            {
                model.input_mode = BoardInputMode::QuickAdd;
                model.clear_message();
                return Ok(IntentOutcome::None);
            }
            // All other complete forms discard as before. Dropdown Esc has its own intent.
            if model.form.take().is_some() {
                model.input_mode = if model.quick_add.is_some() {
                    BoardInputMode::QuickAdd
                } else {
                    BoardInputMode::Normal
                };
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ConfirmEditNext => {
            // Ctrl+Enter in the step line editor (AC-12): add mode saves and reopens
            // the line empty — the rapid-capture loop; rename mode downgrades to a
            // plain save, decided inside `confirm_step_editor` from the editor's
            // own mode. No other surface maps the key, so anywhere else it is the
            // plain confirm.
            if model.input_mode == BoardInputMode::EditStep {
                return confirm_step_editor(domain, model, true);
            }
            return apply_intent(domain, model, BoardIntent::ConfirmEdit, snapshot);
        }
        BoardIntent::ConfirmEdit => {
            // The step line editor applies its own domain command (add or rename) and
            // returns to page view; it never saves the task form's title/notes drafts.
            if model.input_mode == BoardInputMode::EditStep {
                return confirm_step_editor(domain, model, false);
            }
            if model.form.as_ref().is_some_and(|form| !form.is_task()) {
                // Capture keeps its immutable invocation snapshot in the shared form. Without
                // it there is nothing to save against, so the draft remains visible and intact.
                let form = model.form.as_ref().expect("capture form checked above");
                let Some(snap) = form.snapshot().cloned() else {
                    model.set_message("capture context unavailable; press Esc and try again");
                    return Ok(IntentOutcome::None);
                };
                let title = form.title.value().to_string();
                let notes =
                    (!form.notes.value().trim().is_empty()).then(|| form.notes.value().to_string());
                let scope_override = Some(form.scope.clone());
                let thread = match normalize_optional_thread(form.thread.value()) {
                    Ok(thread) => thread,
                    Err(()) => {
                        if let Some(form) = model.form.as_mut() {
                            form.thread_refusal = Some("invalid thread name".into());
                        }
                        return Ok(IntentOutcome::None);
                    }
                };
                return match crate::capture::capture_save(
                    domain,
                    None,
                    &snap,
                    title,
                    notes,
                    scope_override,
                    thread,
                ) {
                    Ok(id) => {
                        let expanded_quick_add = model.quick_add.is_some();
                        if expanded_quick_add {
                            // The app save boundary still owns this create. Retain the expanded
                            // form until it succeeds so recovery Cancel can return to a complete
                            // quick-add stash instead of an edit mode with no form.
                            model.begin_quick_add_save(id, false);
                        } else {
                            model.input_mode = BoardInputMode::Normal;
                            model.clear_message();
                        }
                        Ok(IntentOutcome::Persist)
                    }
                    Err(crate::capture::CaptureError::Domain(DomainError::EmptyTitle)) => {
                        model.set_message(TITLE_REQUIRED_MESSAGE);
                        Ok(IntentOutcome::None)
                    }
                    Err(error) => {
                        model.set_message(error.to_string());
                        Ok(IntentOutcome::None)
                    }
                };
            }
            let outcome = confirm_edit(domain, model)?;
            model.sync_from_domain(domain);
            return Ok(outcome);
        }

        BoardIntent::OpenProjectSelector => {
            // the project-scope chip opens this dropdown from any board surface.
            // Modals that own their own decision (save failure or dispatch recovery) keep
            // that decision rather than being dismissed by the chip.
            if matches!(model.popup, BoardPopup::SaveRecovery) {
                return Ok(IntentOutcome::None);
            }
            let options = model.project_options();
            // Highlight the option that matches the current deck scope (session filter).
            let selected = match &model.board_location {
                BoardLocation::Home { .. } => 0,
                BoardLocation::Project(path) => options
                    .iter()
                    .position(|option| option == &ProjectScopeOption::Project(path.clone()))
                    .unwrap_or(0),
            };
            model.close_popup();
            model.close_command_surface();
            model.close_help();
            model.project_picker = Some(ProjectPickerState { options, selected });
            model.popup = BoardPopup::ProjectPicker;
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ProjectPickerNext => {
            model.move_project_picker(true);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ProjectPickerPrev => {
            model.move_project_picker(false);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ConfirmProjectChoice => {
            // Session-only navigation: nothing durable is touched, so no outcome persists.
            let Some(picker) = model.project_picker.take() else {
                return Ok(IntentOutcome::None);
            };
            let chosen = picker.options.get(picker.selected).cloned();
            model.set_board_scope(chosen.unwrap_or(ProjectScopeOption::Home));
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CancelProjectPicker => {
            if model.project_picker.is_some() {
                model.close_popup();
                model.clear_message();
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectProjectOption(index) => {
            // Mouse-only jump: a dropdown row click chooses its option directly, the way
            // `SelectIndex` chooses a task row directly, instead of stepping
            // `ProjectPickerNext`/`Prev` to it first. Session-only
            // navigation: nothing durable is touched, so no outcome persists.
            let Some(picker) = model.project_picker.take() else {
                return Ok(IntentOutcome::None);
            };
            let Some(chosen) = picker.options.get(index).cloned() else {
                // Out of range against the picker this click actually opened: put it back
                // rather than silently discard an open selection.
                model.project_picker = Some(picker);
                return Ok(IntentOutcome::None);
            };
            model.set_board_scope(chosen);
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectHomeTab(tab) => {
            model.set_home_tab(tab);
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectSectionProject(index) => {
            let Some(path) = model
                .queue_view()
                .sections
                .get(index)
                .and_then(|section| section.project_label.clone())
            else {
                return Ok(IntentOutcome::None);
            };
            let path_buf = PathBuf::from(path.clone());
            let now = Instant::now();
            let is_double = model
                .last_project_header_click
                .as_ref()
                .is_some_and(|(at, last)| {
                    last == &path_buf && now.duration_since(*at) <= ROW_DOUBLE_CLICK_WINDOW
                });
            if is_double {
                model.last_project_header_click = None;
                model.set_board_scope(ProjectScopeOption::Project(path_buf));
            } else {
                let previous_visible = model.visible_ids();
                let previous = model.selection_id;
                model.toggle_project_collapsed(&path);
                model.last_project_header_click = Some((now, path_buf));
                model.reanchor_selection(previous, &previous_visible);
            }
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectSectionThread(index) => {
            let Some(thread) = model
                .queue_view()
                .sections
                .get(index)
                .and_then(|section| section.thread_label.clone())
            else {
                return Ok(IntentOutcome::None);
            };
            let previous_visible = model.visible_ids();
            let previous = model.selection_id;
            model.toggle_thread_collapsed(&thread);
            model.reanchor_selection(previous, &previous_visible);
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectSectionThreadProject {
            section_idx,
            subgroup_idx,
        } => {
            let view = model.queue_view();
            let Some(section) = view.sections.get(section_idx) else {
                return Ok(IntentOutcome::None);
            };
            let Some(subgroup) = section.thread_subgroups.get(subgroup_idx) else {
                return Ok(IntentOutcome::None);
            };
            let Some(path) = subgroup.project_path.clone().map(PathBuf::from) else {
                let previous_visible = model.visible_ids();
                let previous = model.selection_id;
                let key = ThreadProjectCollapseKey {
                    thread: section.thread_label.clone().unwrap_or_default(),
                    project_path: None,
                };
                model.toggle_thread_project_collapsed(key);
                model.reanchor_selection(previous, &previous_visible);
                model.clear_message();
                return Ok(IntentOutcome::None);
            };
            let now = Instant::now();
            let is_double = model
                .last_project_header_click
                .as_ref()
                .is_some_and(|(at, last)| {
                    last == &path && now.duration_since(*at) <= ROW_DOUBLE_CLICK_WINDOW
                });
            if is_double {
                model.last_project_header_click = None;
                model.set_board_scope(ProjectScopeOption::Project(path));
            } else {
                let previous_visible = model.visible_ids();
                let previous = model.selection_id;
                let key = ThreadProjectCollapseKey {
                    thread: section.thread_label.clone().unwrap_or_default(),
                    project_path: Some(path.to_string_lossy().into_owned()),
                };
                model.toggle_thread_project_collapsed(key);
                model.last_project_header_click = Some((now, path));
                model.reanchor_selection(previous, &previous_visible);
            }
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        // The app-level save-recovery boundary handles these while unresolved.
        BoardIntent::RetrySave | BoardIntent::CancelSave => return Ok(IntentOutcome::None),
        BoardIntent::PrimaryVerb => {
            model.close_popup();
            // With the page's step cursor active, `space` toggles the highlighted
            // steps step (never the task's status); every other context keeps the
            // state-mapped status verb.
            if let Some((task_id, step_id)) = cursor_step(domain, model) {
                domain.toggle_step(task_id, step_id)?;
            } else {
                let Some(id) = model.selected_id() else {
                    model.set_message(NO_SELECTION);
                    return Ok(IntentOutcome::None);
                };
                let Some(task) = domain.get(id) else {
                    model.set_message("that task is no longer here");
                    return Ok(IntentOutcome::None);
                };
                match task.status {
                    HumanStatus::Ready => {
                        domain.set_status(id, HumanStatus::Started)?;
                    }
                    HumanStatus::Done => {
                        domain.reopen(id)?;
                    }
                    HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {
                        return Ok(IntentOutcome::None);
                    }
                }
            }
        }
        BoardIntent::ToggleBlock => {
            model.close_popup();
            let Some(id) = model.selected_id() else {
                model.set_message(NO_SELECTION);
                return Ok(IntentOutcome::None);
            };
            let Some(task) = domain.get(id) else {
                model.set_message("that task is no longer here");
                return Ok(IntentOutcome::None);
            };
            match task.status {
                HumanStatus::Blocked => {
                    domain.set_status(id, HumanStatus::Ready)?;
                }
                HumanStatus::Ready | HumanStatus::Started | HumanStatus::Review => {
                    domain.set_status(id, HumanStatus::Blocked)?;
                }
                HumanStatus::Done => {
                    model.set_message("completed tasks cannot be blocked");
                    return Ok(IntentOutcome::None);
                }
            }
        }
        BoardIntent::OpenTaskPage => {
            // Toggle: on the page itself Enter closes it; from the board it opens the
            // selected task's page in view mode (no field focused).
            if model.form.as_ref().is_some_and(BoardForm::is_task) {
                model.form = None;
                model.input_mode = BoardInputMode::Normal;
                model.clear_message();
                return Ok(IntentOutcome::None);
            }
            let Some(id) = model.selected_id() else {
                return Ok(IntentOutcome::None);
            };
            open_task_page_on(domain, model, id);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectStep(index) => {
            // AC-21: a click on an step row moves the step cursor onto that step,
            // scrolling the window to reveal it if hidden. A click only selects —
            // no toggle, no editor, no delete mark, nothing persisted — and any
            // armed mark was already cleared as an intervening intent above
            // (AC-11). The mouse map produces this intent only for the page in
            // view mode; anywhere else it stays inert.
            if model.input_mode == BoardInputMode::TaskPage {
                if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                    let steps = form
                        .task_id()
                        .and_then(|id| domain.get(id))
                        .map(|task| task.steps.len())
                        .unwrap_or(0);
                    if index < steps {
                        form.steps.cursor = Some(index);
                        steps_scroll_to_cursor(form, index);
                    }
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageScrollTo(offset) => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    let horizon = form.notes_max_scroll.get();
                    form.notes_scroll = offset.min(horizon);
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageWheelScrollUp | BoardIntent::PageWheelScrollDown => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    let horizon = form.notes_max_scroll.get();
                    form.notes_scroll = match intent {
                        BoardIntent::PageWheelScrollUp => form.notes_scroll.saturating_sub(1),
                        BoardIntent::PageWheelScrollDown => {
                            form.notes_scroll.saturating_add(1).min(horizon)
                        }
                        _ => unreachable!("wheel intents matched above"),
                    };
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageScrollUp => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    // Keyboard arrows own an active step cursor before they move the
                    // shared stream. Wheel intents above remain the explicit reading
                    // route, so an active cursor does not become inert just because
                    // notes overflow (AC-18, AC-26).
                    match form.steps.cursor {
                        Some(0) => form.steps.cursor = None,
                        Some(index) => {
                            let cursor = index - 1;
                            form.steps.cursor = Some(cursor);
                            steps_scroll_to_cursor(form, cursor);
                        }
                        None => form.notes_scroll = form.notes_scroll.saturating_sub(1),
                    }
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageScrollDown => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    let steps = form
                        .task_id()
                        .and_then(|id| domain.get(id))
                        .map(|task| task.steps.len())
                        .unwrap_or(0);
                    match form.steps.cursor {
                        // A bare Down on a task with steps activates the cursor on
                        // the first step instead of scrolling, including after an
                        // earlier Up-deactivation (AC-17, AC-18).
                        None if steps > 0 => {
                            form.steps.cursor = Some(0);
                            steps_scroll_to_cursor(form, 0);
                        }
                        Some(index) if steps > 0 => {
                            let cursor = (index + 1).min(steps - 1);
                            form.steps.cursor = Some(cursor);
                            steps_scroll_to_cursor(form, cursor);
                        }
                        _ => {
                            let horizon = form.notes_max_scroll.get();
                            form.notes_scroll = form.notes_scroll.saturating_add(1).min(horizon);
                        }
                    }
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PeekDetail => {
            let Some(id) = model.selected_id() else {
                return Ok(IntentOutcome::None);
            };
            model.detail_open = Some(id);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CollapseDetail => {
            model.detail_open = None;
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ToggleDoneDrawer => {
            let previous_visible = model.visible_ids();
            let previous = model.selection_id;
            model.drawer_open = !model.drawer_open;
            model.reanchor_selection(previous, &previous_visible);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::OpenHelp => {
            if model.project_picker.is_some() {
                return Ok(IntentOutcome::None);
            }
            model.close_command_surface();
            model.close_popup();
            model.input_mode = BoardInputMode::Help;
            return Ok(IntentOutcome::None);
        }
        BoardIntent::CloseLayer => {
            // Progressive close: transient surface (palette/help/dropdown) →
            // open detail → quit. SaveRecovery is not dismissible here; the save-recovery
            // gate owns Retry/Cancel.
            if model.surface != CommandSurface::None {
                model.close_command_surface();
                return Ok(IntentOutcome::None);
            }
            if model.input_mode == BoardInputMode::Help {
                model.input_mode = BoardInputMode::Normal;
                return Ok(IntentOutcome::None);
            }
            if model.input_mode == BoardInputMode::FormScopeDropdown {
                model.close_form_scope_dropdown(false);
                return Ok(IntentOutcome::None);
            }
            if model.form.take().is_some() {
                model.input_mode = if model.quick_add.is_some() {
                    BoardInputMode::QuickAdd
                } else {
                    BoardInputMode::Normal
                };
                model.clear_message();
                return Ok(IntentOutcome::None);
            }
            if model.quick_add.take().is_some() {
                model.input_mode = BoardInputMode::Normal;
                model.clear_message();
                return Ok(IntentOutcome::None);
            }
            let before = model.popup;
            model.close_popup();
            if model.popup != before {
                return Ok(IntentOutcome::None);
            }
            if model.detail_open.is_some() {
                model.detail_open = None;
                return Ok(IntentOutcome::None);
            }
            return Ok(IntentOutcome::Quit);
        }
        // The five intents below aim at the selection, and an empty board has none. Each
        // says so rather than returning to a row that has just been cleared for an action
        // that then did nothing: a silent no-op is the failure the row exists to prevent.
        BoardIntent::SetStatus(status) => {
            let Some(id) = model.selected_id() else {
                model.set_message(NO_SELECTION);
                return Ok(IntentOutcome::None);
            };
            domain.set_status(id, status)?;
            model.close_popup();
        }
        BoardIntent::Complete => {
            model.close_popup();
            let Some(id) = model.selected_id() else {
                model.set_message(NO_SELECTION);
                return Ok(IntentOutcome::None);
            };
            domain.complete(id)?;
        }
        BoardIntent::Reopen => {
            model.close_popup();
            let Some(id) = model.selected_id() else {
                model.set_message(NO_SELECTION);
                return Ok(IntentOutcome::None);
            };
            domain.reopen(id)?;
        }
        BoardIntent::SoftDelete => {
            model.close_popup();
            // On the page with the step cursor active, the delete verb is the
            // steps's mark-then-confirm: the first press visibly marks the
            // highlighted step, a second press removes it, and the task-level soft
            // delete below never runs.
            match page_step_delete(domain, model)? {
                PageStepDelete::Marked => return Ok(IntentOutcome::None),
                PageStepDelete::Removed => {}
                PageStepDelete::NotApplicable => {
                    let Some(id) = model.selected_id() else {
                        model.set_message(NO_SELECTION);
                        return Ok(IntentOutcome::None);
                    };
                    // Read the title before the delete, and only arm the notice once the delete
                    // itself succeeded: a refused delete has nothing to recover from.
                    let title = domain.get(id).map(|task| task.title.clone());
                    domain.soft_delete(id)?;
                    if let Some(title) = title {
                        model.arm_delete_notice(&title);
                    }
                    // Deleting from the page deletes the page's own task: the surface closes and
                    // the undo route back to it lives on the board row, same as the notice says.
                    if model
                        .form
                        .as_ref()
                        .filter(|form| form.is_task())
                        .and_then(BoardForm::task_id)
                        == Some(id)
                    {
                        model.form = None;
                        model.input_mode = BoardInputMode::Normal;
                    }
                }
            }
        }
        BoardIntent::Undo => {
            model.close_popup();
            if let Err(error) = domain.undo() {
                if let DomainError::StaleUndo(id) = error {
                    model.sync_from_domain(domain);
                    // Nothing moved, so this reports nothing to persist -- which is what
                    // puts the way back on the row beside the refusal that explains why
                    // this attempt did not take. The refusal is stated the
                    // way this row can be read when it clips, not the way the domain logs
                    // it; the error itself is unchanged.
                    model.set_message(stale_undo_message(id));
                    return Ok(IntentOutcome::None);
                }
                return Err(error);
            }
        }
        BoardIntent::ToggleVerbModifier => {
            model.verb_modifier = model.verb_modifier.toggled();
            model.close_command_surface();
            if let Err(error) =
                SettingsRecord::new(default_config_dir()).set_verb_modifier(model.verb_modifier)
            {
                model.set_message(format!("could not save verb keys: {error}"));
            }
            return Ok(IntentOutcome::None);
        }
    }

    model.sync_from_domain(domain);
    Ok(IntentOutcome::Persist)
}

/// Apply one draft operation, but only while a field edit is actually open.
///
/// Every editing intent is inert in every other mode, exactly as the insert and backspace
/// intents already were before the cursor arrived.
fn edit_draft(model: &mut BoardModel, operation: impl FnOnce(&mut EditBuffer)) {
    if model.input_mode == BoardInputMode::FormScopeDropdown {
        return;
    }
    // The steps step editor owns the keyboard in its mode: its draft is the page
    // form's steps editor buffer, not the task form's title/notes fields. Any
    // edit-draft intent takes the line's refusal down (AC-13) — including a cursor
    // move that changes no text — the same lifetime quick-add's message follows.
    if model.input_mode == BoardInputMode::EditStep {
        if let Some(editor) = model
            .form
            .as_mut()
            .and_then(|form| form.steps.editor.as_mut())
        {
            editor.refusal = None;
            operation(&mut editor.buffer);
        }
        return;
    }
    let Some(form) = model.form.as_mut() else {
        return;
    };
    match form.focus {
        CaptureField::Title => operation(&mut form.title),
        CaptureField::Notes => operation(&mut form.notes),
        CaptureField::Thread => {
            form.thread_refusal = None;
            operation(&mut form.thread);
        }
        CaptureField::Scope => {}
    }
}

/// Open the task page in view mode on `id`, replacing whatever surface held input. Shared
/// by the keyboard route (`Enter`) and the mouse route (a row double-click).
fn open_task_page_on(domain: &DomainState, model: &mut BoardModel, id: Uuid) {
    let Some(task) = domain.get(id) else {
        return;
    };
    model.close_popup();
    model.close_command_surface();
    model.close_help();
    // The page replaces the peek: both would otherwise describe the same task twice.
    model.detail_open = None;
    let form = BoardForm::task(
        task,
        model.this_repo.as_deref(),
        &model.tasks,
        CaptureField::Title,
    );
    model.form = Some(form);
    model.input_mode = BoardInputMode::TaskPage;
    model.clear_message();
}

/// The row double-click window: a second click on the same row within it opens the page.
const ROW_DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(400);

/// Save the line through the same capture pipeline the expanded form uses.
fn quick_add_save(
    domain: &mut DomainState,
    model: &mut BoardModel,
    keep_open: bool,
) -> Result<IntentOutcome, DomainError> {
    let Some(quick_add) = model.quick_add.as_ref() else {
        return Ok(IntentOutcome::None);
    };
    let Some(snapshot) = quick_add.snapshot.as_ref().clone() else {
        model.set_message("capture context unavailable; press Esc and try again");
        return Ok(IntentOutcome::None);
    };
    let lifted = match lift_quick_add_tokens(
        quick_add.title.value(),
        domain,
        quick_add.snapshot.as_ref().as_ref(),
    ) {
        Ok(lifted) => lifted,
        Err(message) => {
            model.set_message(message);
            return Ok(IntentOutcome::None);
        }
    };
    let scope = lifted.scope.unwrap_or_else(|| quick_add.scope.clone());
    match crate::capture::capture_save(
        domain,
        None,
        &snapshot,
        lifted.title,
        None,
        Some(scope.clone()),
        lifted.thread,
    ) {
        Ok(id) => {
            // Do not discard the draft until the app save boundary confirms persistence. A
            // failed save keeps this exact state behind SaveRecovery for retry or cancel.
            model.form = None;
            model.begin_quick_add_save(id, keep_open);
            Ok(IntentOutcome::Persist)
        }
        Err(crate::capture::CaptureError::Domain(DomainError::EmptyTitle)) => {
            model.set_message(TITLE_REQUIRED_MESSAGE);
            Ok(IntentOutcome::None)
        }
        Err(error) => {
            model.set_message(error.to_string());
            Ok(IntentOutcome::None)
        }
    }
}

/// Directives lifted from a quick-add title before capture.
///
/// This parser is deliberately private to quick-add. Title, notes, and checklist editors retain
/// their literal text, while the status-row capture can apply scope and thread together.
struct QuickAddTokens {
    title: String,
    scope: Option<TaskScope>,
    thread: Option<String>,
}

/// Lift whitespace-delimited `!p` and `!t` directives in either order.
///
/// A directive consumes only its immediate non-directive argument. Parsing completes before any
/// value is returned, so a malformed thread cannot partially apply a preceding scope override.
fn lift_quick_add_tokens(
    value: &str,
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> Result<QuickAddTokens, String> {
    let words: Vec<&str> = value.split_whitespace().collect();
    let mut title = Vec::new();
    let mut scope = None;
    let mut thread = None;
    let mut index = 0;

    while let Some(word) = words.get(index) {
        match *word {
            "!p" => {
                let argument = quick_add_token_argument(&words, index);
                scope = Some(match argument {
                    Some(path) => TaskScope::Project {
                        path: crate::scope::resolve_project_path(path, domain, snapshot),
                    },
                    None => TaskScope::Global,
                });
                index += usize::from(argument.is_some()) + 1;
            }
            "!t" => {
                let argument = quick_add_token_argument(&words, index);
                thread = match argument {
                    Some(name) => Some(
                        normalize_thread(name).map_err(|_| "invalid thread name".to_string())?,
                    ),
                    None => None,
                };
                index += usize::from(argument.is_some()) + 1;
            }
            _ => {
                title.push(*word);
                index += 1;
            }
        }
    }

    Ok(QuickAddTokens {
        title: title.join(" "),
        scope,
        thread,
    })
}

/// A directive consumes one argument only when the next word is neither another directive nor
/// a literal `#` title word.
fn quick_add_token_argument<'a>(words: &'a [&str], index: usize) -> Option<&'a str> {
    words
        .get(index + 1)
        .copied()
        .filter(|word| *word != "!p" && *word != "!t" && !word.starts_with('#'))
}

fn normalize_optional_thread(value: &str) -> Result<Option<String>, ()> {
    let value = value.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        normalize_thread(value).map(Some).map_err(|_| ())
    }
}

fn confirm_edit(
    domain: &mut DomainState,
    model: &mut BoardModel,
) -> Result<IntentOutcome, DomainError> {
    // The task bound at open, not `model.selected_id()`: refresh may move the visible pin, but
    // it never changes the form's immutable id or any of its three drafts.
    let Some(form) = model.form.as_ref().filter(|form| form.is_task()) else {
        return Ok(IntentOutcome::None);
    };
    let id = form.task_id().expect("task form has immutable id");
    let title = form.title.value().trim().to_string();
    let notes = (!form.notes.value().trim().is_empty()).then(|| form.notes.value().to_string());
    let scope = form.scope.clone();
    let thread = match normalize_optional_thread(form.thread.value()) {
        Ok(thread) => thread,
        Err(()) => {
            if let Some(form) = model.form.as_mut() {
                form.thread_refusal = Some("invalid thread name".into());
            }
            return Ok(IntentOutcome::None);
        }
    };
    let task = domain.get(id).ok_or(DomainError::UnknownId(id))?.clone();
    // `DomainState::edit` does not reject a soft-deleted task itself. Refuse before touching
    // the form so's bound-task and draft-recovery guarantees remain intact.
    if task.soft_deleted {
        return Err(DomainError::SoftDeleted(id));
    }

    // One existing DomainState::edit call updates title, Notes, scope, and thread together.
    domain.edit(id, &title, notes.clone(), scope.clone(), thread.clone())?;

    // Retain the complete form and mode until the persistence boundary confirms this exact
    // atomic edit. A failed save can then Retry or Cancel without orphaning the input state.
    model.task_edit_save = Some(TaskEditSave {
        id,
        title,
        notes,
        scope,
        thread,
    });
    Ok(IntentOutcome::Persist)
}

// ---------------------------------------------------------------------------
// Steps step cursor, verbs, and one-line editor (T-3)
// ---------------------------------------------------------------------------

/// The step the page's cursor highlights, as (task id, step id), when the page is in
/// view mode with a task form open, the step cursor active, and the highlighted index
/// still naming a live step. `None` in every other case — including a cursor left past
/// the end of a steps another actor shrank — so verbs degrade to their inactive
/// behavior instead of acting on a stale index.
fn cursor_step(domain: &DomainState, model: &BoardModel) -> Option<(Uuid, Uuid)> {
    if model.input_mode != BoardInputMode::TaskPage {
        return None;
    }
    let form = model.form.as_ref().filter(|form| form.is_task())?;
    let task_id = form.task_id()?;
    let index = form.steps.cursor?;
    let step_id = domain.get(task_id)?.steps.get(index).map(|step| step.id)?;
    Some((task_id, step_id))
}

/// Keep the selected step in the renderer-recorded shared content viewport.
fn steps_scroll_to_cursor(form: &mut BoardForm, cursor: usize) {
    let target = form
        .steps
        .content_start
        .get()
        .saturating_add(1)
        .saturating_add(cursor);
    let rows = form.steps.window_rows.get().max(1);
    if target < form.notes_scroll {
        form.notes_scroll = target;
    } else if target >= form.notes_scroll.saturating_add(rows) {
        form.notes_scroll = target + 1 - rows;
    }
    form.notes_scroll = form.notes_scroll.min(form.notes_max_scroll.get());
}

/// Outcome of routing the delete verb through the page's step cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageStepDelete {
    /// The cursor is not active on the page: the verb is the task soft delete.
    NotApplicable,
    /// First press: the cursor's step is visibly marked; nothing was removed.
    Marked,
    /// Second press: the marked step was removed through the domain command.
    Removed,
}

/// Mark-then-confirm delete for the step under the page's cursor (AC-11). Any
/// intervening intent already cleared the mark in [`apply_intent`], so a mark found
/// here equal to the cursor's step can only be this verb's own first press.
fn page_step_delete(
    domain: &mut DomainState,
    model: &mut BoardModel,
) -> Result<PageStepDelete, DomainError> {
    let Some((task_id, step_id)) = cursor_step(domain, model) else {
        return Ok(PageStepDelete::NotApplicable);
    };
    let form = model
        .form
        .as_ref()
        .filter(|form| form.is_task())
        .expect("cursor_step checked a task form");
    let index = form
        .steps
        .cursor
        .expect("cursor_step checked an active cursor");
    if form.steps.delete_mark != Some(index) {
        model
            .form
            .as_mut()
            .expect("task form checked above")
            .steps
            .delete_mark = Some(index);
        // AC-23: the footer's message slot carries the press-again hint for exactly
        // as long as the mark is armed — the removal press and every intervening
        // intent clear it with the mark. The verb's own modifier names the key, so
        // the hint stays truthful when the palette flips the chord.
        model.set_message(format!(
            "press {}x again to remove",
            model.verb_modifier.prefix()
        ));
        return Ok(PageStepDelete::Marked);
    }
    domain.remove_step(task_id, step_id)?;
    let len = domain
        .get(task_id)
        .map(|task| task.steps.len())
        .unwrap_or(0);
    let form = model
        .form
        .as_mut()
        .filter(|form| form.is_task())
        .expect("task form checked above");
    form.steps.delete_mark = None;
    if len == 0 {
        form.steps.cursor = None;
    } else {
        let cursor = form.steps.cursor.unwrap_or(0).min(len - 1);
        form.steps.cursor = Some(cursor);
        steps_scroll_to_cursor(form, cursor);
    }
    Ok(PageStepDelete::Removed)
}

/// Open the page footer's one-line step input: seeded with `text`, renaming
/// `step` when given, adding when `None`.
fn open_step_editor(model: &mut BoardModel, text: &str, rename: Option<Uuid>) {
    if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
        form.steps.editor = Some(StepEditor {
            buffer: crate::ui::edit::seeded_draft(text),
            rename,
            refusal: None,
        });
        model.input_mode = BoardInputMode::EditStep;
        model.clear_message();
    }
}

/// Close the step editor back to page view, discarding its draft. Any pending editor
/// save goes with it: a closed line has nothing left for the save boundary to
/// release (the only path that can be here with one pending is a defensive direct
/// intent, never the keyboard).
fn close_step_editor(model: &mut BoardModel) {
    if let Some(form) = model.form.as_mut() {
        form.steps.pending_save = None;
        form.steps.editor = None;
    }
    model.input_mode = BoardInputMode::TaskPage;
    model.clear_message();
}

/// What the step line paints when its draft is empty after trim (AC-13): a short dim
/// refusal on the line itself, never the board status row.
const STEP_TEXT_REQUIRED: &str = "text required";

/// Apply the step editor's draft through the domain command (add or rename).
///
/// The editor and its input mode OUTLIVE the save call (AC-14): a successful apply
/// records the touched step on the page's pending-save slot and leaves the line
/// exactly as the user left it. Only the persistence boundary's confirmed sync —
/// `BoardModel::sync_from_domain` → `finish_step_editor_save` — releases it:
/// closing for a plain Enter, reopening empty for Ctrl+Enter in add mode
/// (`keep_open`, downgraded to a close in rename mode). A failed save therefore
/// holds the line behind SaveRecovery until Retry/Cancel resolve it, and a Cancelled
/// resolution unwinds it to page view with no orphan edit mode.
///
/// An empty-after-trim draft is the line's own refusal (AC-13), painted on the line
/// and cleared when it closes or its buffer changes; it never reaches the board
/// message. Every other domain refusal — an step another actor removed — propagates
/// before anything is cleared, exactly as the task form's edit does.
fn confirm_step_editor(
    domain: &mut DomainState,
    model: &mut BoardModel,
    keep_open: bool,
) -> Result<IntentOutcome, DomainError> {
    let Some(form) = model.form.as_ref().filter(|form| form.is_task()) else {
        return Ok(IntentOutcome::None);
    };
    let Some(task_id) = form.task_id() else {
        return Ok(IntentOutcome::None);
    };
    let Some(editor) = form.steps.editor.as_ref() else {
        return Ok(IntentOutcome::None);
    };
    let text = editor.buffer.value().to_string();
    let rename = editor.rename;
    let touched = match rename {
        Some(step_id) => domain
            .rename_step(task_id, step_id, &text)
            .map(|()| step_id),
        None => domain.add_step(task_id, &text),
    };
    let touched = match touched {
        Ok(step) => step,
        Err(DomainError::EmptyStepText) => {
            if let Some(editor) = model
                .form
                .as_mut()
                .and_then(|form| form.steps.editor.as_mut())
            {
                editor.refusal = Some(STEP_TEXT_REQUIRED.to_string());
            }
            return Ok(IntentOutcome::None);
        }
        Err(other) => return Err(other),
    };
    let form = model.form.as_mut().expect("task form checked above");
    form.steps.pending_save = Some(StepEditorSave {
        step: touched,
        text: text.trim().to_string(),
        reopen: keep_open && rename.is_none(),
    });
    Ok(IntentOutcome::Persist)
}
