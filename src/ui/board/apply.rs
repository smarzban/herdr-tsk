//! Board intent reducer.

use std::path::PathBuf;
use std::time::Instant;

use uuid::Uuid;

use crate::context::InvocationSnapshot;
use crate::domain::{normalize_thread, DomainError, DomainState, HumanStatus, TaskScope};
use crate::ui::capture::{CaptureField, TITLE_REQUIRED_MESSAGE};
use crate::ui::edit::{flatten_line_breaks, EditBuffer};
use crate::ui::input::BoardIntent;
use crate::ui::mouse::BoardPopup;
use crate::ui::queue::{ThreadProjectCollapseKey, ARCHIVED_HEADER_ROW_ID};
use crate::ui::tier::{FocusedSurface, WideStage};

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
            | BoardIntent::File
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
    if !model.task_editing()
        && model
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
    if model.focused_surface() == FocusedSurface::Task
        && model.input_mode == BoardInputMode::TaskPage
        && matches!(
            intent,
            BoardIntent::SelectNext | BoardIntent::SelectPrev | BoardIntent::SelectIndex(_)
        )
    {
        return Ok(IntentOutcome::None);
    }
    let closes_the_page = matches!(
        intent,
        BoardIntent::CloseLayer | BoardIntent::OpenTaskPage | BoardIntent::StageLeft
    );
    if model.focused_surface() == FocusedSurface::Task
        && !closes_the_page
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
            if model.input_mode == BoardInputMode::EditStep {
                if model.park_rename_step_draft() {
                    model.input_mode = BoardInputMode::TaskPage;
                    if !move_step_within_edit_group(model, true) {
                        select_add_step(model);
                    }
                }
            } else if model.input_mode == BoardInputMode::TaskPage {
                if model.task_editing() {
                    if model
                        .form
                        .as_ref()
                        .is_some_and(|form| form.steps.add_selected)
                    {
                        model.focus_form_field(CaptureField::Scope);
                    } else if !move_step_within_edit_group(model, true) {
                        select_add_step(model);
                    }
                } else if !move_step_with_tab(model, true) && !select_first_step_from_page(model) {
                    model.enter_page_field_focus();
                }
            } else if model.input_mode == BoardInputMode::EditNotes
                && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                select_step_from_tab(model, true);
            } else if model.input_mode == BoardInputMode::EditScope
                && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                model.focus_form_field(CaptureField::Thread);
            } else if matches!(
                model.input_mode,
                BoardInputMode::SelectThread | BoardInputMode::EditThread
            ) && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                model.focus_form_field(CaptureField::Title);
            } else if model.form.is_some() && model.input_mode != BoardInputMode::FormScopeDropdown
            {
                model.move_form_focus(true);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::FormFocusPrev => {
            if model.input_mode == BoardInputMode::EditStep {
                if model.park_rename_step_draft() {
                    model.input_mode = BoardInputMode::TaskPage;
                    if !move_step_within_edit_group(model, false) {
                        model.focus_form_field(CaptureField::Notes);
                    }
                }
                return Ok(IntentOutcome::None);
            } else if model.input_mode == BoardInputMode::TaskPage {
                if model.task_editing() {
                    if model
                        .form
                        .as_ref()
                        .is_some_and(|form| form.steps.add_selected)
                    {
                        select_last_step_for_edit(model);
                    } else if !move_step_within_edit_group(model, false) {
                        model.focus_form_field(CaptureField::Notes);
                    }
                } else if !move_step_with_tab(model, false) {
                    model.enter_page_field_focus();
                }
                return Ok(IntentOutcome::None);
            }
            if model.input_mode == BoardInputMode::EditTitle
                && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                model.focus_form_field(CaptureField::Thread);
            } else if matches!(
                model.input_mode,
                BoardInputMode::SelectThread | BoardInputMode::EditThread
            ) && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                model.focus_form_field(CaptureField::Scope);
            } else if model.input_mode == BoardInputMode::EditScope
                && model.form.as_ref().is_some_and(|form| form.is_task())
            {
                select_step_from_tab(model, false);
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
        BoardIntent::ToggleThreadEditing => {
            model.toggle_thread_editing();
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
            if !model.select_index(idx) {
                return Ok(IntentOutcome::None);
            }
            // A row click also expands that row's peek; a second click on the same row
            // inside the double-click window opens the task page instead.
            if let Some(id) = model.selected_id() {
                let now = Instant::now();
                let is_double = model.last_row_click.is_some_and(|(at, last)| {
                    last == id && now.duration_since(at) <= ROW_DOUBLE_CLICK_WINDOW
                });
                if is_double {
                    model.last_row_click = None;
                    open_full_task_page(domain, model, id);
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
        BoardIntent::FocusBoardAndSelectIndex(idx) => {
            if !model.select_index(idx) {
                return Ok(IntentOutcome::None);
            }
            // A left-side click takes focus left: a rail click in G selects the row and lands
            // the board in A. In 0 and A the click selects in place. A second click on the
            // same row inside the double-click window opens the full task page.
            model.detail_open = None;
            if model.wide_stage == WideStage::Rail {
                model.wide_stage = WideStage::Split;
            }
            if let Some(id) = model.selected_id() {
                let now = Instant::now();
                let is_double = model.last_row_click.is_some_and(|(at, last)| {
                    last == id && now.duration_since(at) <= ROW_DOUBLE_CLICK_WINDOW
                });
                if is_double {
                    model.last_row_click = None;
                    open_full_task_page(domain, model, id);
                } else {
                    model.last_row_click = Some((now, id));
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ListScrollTo(offset) => {
            if model.focused_surface() != FocusedSurface::Board {
                return Ok(IntentOutcome::None);
            }
            model.set_list_scroll(offset);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::BeginAddStep => {
            model.close_popup();
            // Ctrl+A and the trailing target both open the independent add row from task view
            // or an active task edit. Park a rename first, so Ctrl+A never drops an existing
            // staged rename while replacing its cursor with the add row.
            if model.form.as_ref().is_some_and(BoardForm::is_task) && model.park_rename_step_draft()
            {
                open_step_editor(model, "", None);
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::BeginEditTitle | BoardIntent::BeginEditNotes | BoardIntent::BeginEditScope => {
            model.close_popup();
            // Ctrl+E on an already-open inline row keeps that row focused. Field traversal is
            // explicit through Tab or clicks, so this never discards or redirects its draft.
            if intent == BoardIntent::BeginEditTitle && model.input_mode == BoardInputMode::EditStep
            {
                return Ok(IntentOutcome::None);
            }
            let focus = match intent {
                BoardIntent::BeginEditTitle => CaptureField::Title,
                BoardIntent::BeginEditNotes => CaptureField::Notes,
                BoardIntent::BeginEditScope => CaptureField::Scope,
                _ => unreachable!("matched task-form entry intent"),
            };
            // Ctrl+E on a selected step begins the whole task edit session and opens that
            // row's in-place editor, including when the selection was made in task view.
            if intent == BoardIntent::BeginEditTitle {
                if let Some((task_id, step_id)) = selected_step(domain, model) {
                    let text = domain.get(task_id).and_then(|task| {
                        task.steps
                            .iter()
                            .find(|step| step.id == step_id)
                            .map(|step| step.text.clone())
                    });
                    if let Some(text) = text {
                        if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                            form.editing = true;
                        }
                        open_step_editor(model, &text, Some(step_id));
                        return Ok(IntentOutcome::None);
                    }
                }
            }
            // The page already open: move focus into the asked field, keep every draft, and
            // transfer input ownership before the editor can accept a key.
            if model.form.as_ref().is_some_and(BoardForm::is_task) {
                enter_task_stage(model);
                model.focus_form_field(focus);
                return Ok(IntentOutcome::None);
            }
            if let Some(id) = model.selected_id() {
                if let Some(task) = domain.get(id) {
                    // One immutable id and three independent drafts are captured at open.
                    // `sync_from_domain` deliberately never writes this form, so background
                    // refresh can reanchor selection without redirecting its later save.
                    let mut form =
                        BoardForm::task(task, model.this_repo.as_deref(), &model.tasks, focus);
                    // A direct board edit is a real edit session too, so its confirmed task
                    // page keeps step interaction available after the field saves.
                    form.editing = true;
                    model.input_mode = form.parent_mode();
                    model.form = Some(form);
                    enter_task_stage(model);
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
            // Title and Thread stay one line in either form; Notes preserves pasted line
            // breaks. The step editor is one line by construction, so it flattens like Title.
            let single_line = model.input_mode == BoardInputMode::EditStep
                || model.form.as_ref().is_some_and(|form| {
                    matches!(form.focus, CaptureField::Title | CaptureField::Thread)
                });
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
            // The inline step editor cancels to page view: draft discarded, no mutation,
            // and the page's step cursor state stays intact.
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
            // Shift+Enter on an existing step commits the complete task edit session. Keep the
            // active row allocated until persistence confirms, while an add remains its own
            // save-and-next operation.
            if model.input_mode == BoardInputMode::EditStep {
                if model.stage_active_rename_draft() {
                    let outcome = confirm_edit(domain, model)?;
                    model.sync_from_domain(domain);
                    return Ok(outcome);
                }
                return confirm_step_editor(domain, model, true);
            }
            return apply_intent(domain, model, BoardIntent::ConfirmEdit, snapshot);
        }
        BoardIntent::ConfirmEdit => {
            if model.input_mode == BoardInputMode::EditStep {
                // Plain Enter closes an existing-step editor into the task session without
                // crossing the persistence boundary. New-step adds still save independently.
                if model.park_rename_step_draft() {
                    model.input_mode = BoardInputMode::TaskPage;
                    return Ok(IntentOutcome::None);
                }
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
            // A save-failure modal owns its decision rather than being dismissed by the chip.
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
        BoardIntent::ToggleAllGroups => {
            let previous_visible = model.visible_ids();
            let previous = model.selection_id;
            if model.toggle_all_groups() {
                model.reanchor_selection(previous, &previous_visible);
            }
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
            // A selected step owns the page primary verb. With no live step selection, retain
            // the task's start/reopen behavior.
            if let Some((task_id, step_id)) = selected_step(domain, model) {
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
        BoardIntent::StageRight => {
            stage_right(domain, model);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::StageLeft => {
            stage_left(model);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::OpenTaskPage => {
            // Enter on the archived header toggles the group instead of opening a page:
            // the header is chrome, never a task.
            if model.archived_header_selected() {
                let previous_visible = model.visible_ids();
                model.toggle_archived_collapsed();
                model.reanchor_selection(Some(ARCHIVED_HEADER_ROW_ID), &previous_visible);
                return Ok(IntentOutcome::None);
            }
            // Enter never opens inline step editing. A selected step remains selected in either
            // page state; Ctrl+E is the deliberate route into its editor.
            if selected_step(domain, model).is_some() {
                return Ok(IntentOutcome::None);
            }
            if model
                .form
                .as_ref()
                .is_some_and(|form| form.steps.add_selected)
            {
                open_step_editor(model, "", None);
                return Ok(IntentOutcome::None);
            }
            if model.form.as_ref().is_some_and(BoardForm::is_task) {
                // Enter beside the rail opens the full page; on the full page (or the
                // single-pane page) it toggles the page shut, as it always has.
                if model.focused_surface() == FocusedSurface::Board
                    || model.wide_stage == WideStage::Rail
                {
                    let Some(id) = model.selected_id() else {
                        return Ok(IntentOutcome::None);
                    };
                    open_full_task_page(domain, model, id);
                } else {
                    leave_task_page(model);
                }
                return Ok(IntentOutcome::None);
            }
            let Some(id) = model.selected_id() else {
                return Ok(IntentOutcome::None);
            };
            open_full_task_page(domain, model, id);
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectStep(index) => {
            // Existing steps belong to the enclosing task draft. Parking a rename before
            // selecting another row permits several step edits before one final Shift+Enter.
            // New-step add remains a focused save-and-next line and cannot be abandoned by a
            // row click.
            let editing = model.task_editing();
            if editing && !model.park_rename_step_draft() {
                return Ok(IntentOutcome::None);
            }
            let selected = model
                .form
                .as_ref()
                .filter(|form| form.is_task())
                .and_then(|form| {
                    form.task_id()
                        .map(|task_id| (task_id, &form.steps.removals))
                })
                .and_then(|(task_id, removals)| {
                    domain.get(task_id).and_then(|task| {
                        task.steps
                            .iter()
                            .enumerate()
                            .filter(|(_, step)| !removals.contains(&step.id))
                            .nth(index)
                            .map(|(source_index, step)| (source_index, step.id, step.text.clone()))
                    })
                });
            if let Some((source_index, step_id, text)) = selected {
                if let Some(form) = model.form.as_mut() {
                    form.steps.cursor = Some(source_index);
                    form.steps.add_selected = false;
                    steps_scroll_to_cursor(form, source_index);
                }
                if editing {
                    open_step_editor(model, &text, Some(step_id));
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageScrollTo(offset) => {
            if model.focused_surface() != FocusedSurface::Task {
                return Ok(IntentOutcome::None);
            }
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    let horizon = form.notes_max_scroll.get();
                    form.notes_scroll = offset.min(horizon);
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageWheelScrollUp | BoardIntent::PageWheelScrollDown => {
            if model.focused_surface() != FocusedSurface::Task {
                return Ok(IntentOutcome::None);
            }
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if matches!(
                    model.input_mode,
                    BoardInputMode::TaskPage | BoardInputMode::EditStep
                ) {
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
            if model.focused_surface() != FocusedSurface::Task {
                return Ok(IntentOutcome::None);
            }
            // A fresh add is independent of the surrounding task edit session, so it may be
            // open directly from task view. It has no rename draft to park: keep its cursor
            // alive and scroll the same shared body that task view uses.
            if model.input_mode == BoardInputMode::EditStep && !model.park_rename_step_draft() {
                if let Some(form) = model.form.as_mut() {
                    form.notes_scroll = form.notes_scroll.saturating_sub(1);
                }
                return Ok(IntentOutcome::None);
            }
            if model.task_editing() {
                model.input_mode = BoardInputMode::TaskPage;
                if model
                    .form
                    .as_ref()
                    .is_some_and(|form| form.steps.add_selected)
                {
                    select_last_step_for_edit(model);
                } else if !move_step_within_edit_group(model, false) {
                    model.focus_form_field(CaptureField::Notes);
                }
                return Ok(IntentOutcome::None);
            }
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
            if model.focused_surface() != FocusedSurface::Task {
                return Ok(IntentOutcome::None);
            }
            // See Up above. This must precede `task_editing()`, because Ctrl+A and the trailing
            // add target also open a fresh editor directly from task view.
            if model.input_mode == BoardInputMode::EditStep && !model.park_rename_step_draft() {
                if let Some(form) = model.form.as_mut() {
                    let horizon = form.notes_max_scroll.get();
                    form.notes_scroll = form.notes_scroll.saturating_add(1).min(horizon);
                }
                return Ok(IntentOutcome::None);
            }
            if model.task_editing() {
                model.input_mode = BoardInputMode::TaskPage;
                if !model
                    .form
                    .as_ref()
                    .is_some_and(|form| form.steps.add_selected)
                {
                    let _ = move_step_within_edit_group(model, true);
                }
                return Ok(IntentOutcome::None);
            }
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    let steps = form
                        .task_id()
                        .and_then(|id| domain.get(id))
                        .map(|task| task.steps.len())
                        .unwrap_or(0);
                    match form.steps.cursor {
                        Some(index) if steps > 0 => {
                            let cursor = (index + 1).min(steps - 1);
                            form.steps.cursor = Some(cursor);
                            steps_scroll_to_cursor(form, cursor);
                        }
                        None if steps > 0 && !form.steps.add_selected => {
                            form.steps.cursor = Some(0);
                            steps_scroll_to_cursor(form, 0);
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
        BoardIntent::ToggleArchivedGroup => {
            // Select the header row, then flip the group. The header stays in the
            // visible set collapsed or expanded, so reanchoring keeps it selected.
            let previous_visible = model.visible_ids();
            model.select_archived_header();
            model.toggle_archived_collapsed();
            model.reanchor_selection(Some(ARCHIVED_HEADER_ROW_ID), &previous_visible);
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
            if model.task_editing() {
                let saved = model
                    .form
                    .as_ref()
                    .and_then(BoardForm::task_id)
                    .and_then(|id| model.tasks.iter().find(|task| task.id == id))
                    .cloned();
                if let (Some(form), Some(task)) = (model.form.as_mut(), saved) {
                    *form = BoardForm::task(
                        &task,
                        model.this_repo.as_deref(),
                        &model.tasks,
                        CaptureField::Title,
                    );
                    model.input_mode = BoardInputMode::TaskPage;
                    model.clear_message();
                    return Ok(IntentOutcome::None);
                }
            }
            if model.form.as_ref().is_some_and(BoardForm::is_task)
                && model.focused_surface() == FocusedSurface::Task
            {
                leave_task_page(model);
                return Ok(IntentOutcome::None);
            }
            if model.focused_surface() == FocusedSurface::Board
                && model.form.as_ref().is_some_and(BoardForm::is_task)
            {
                // A parked page is not a visible layer on the board: Esc falls through to
                // the board's own close order instead of silently dropping the session.
            } else if model.form.take().is_some() {
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
            if let Some((task_id, step_id)) = selected_step(domain, model) {
                let done = domain
                    .get(task_id)
                    .and_then(|task| task.steps.iter().find(|step| step.id == step_id))
                    .is_some_and(|step| step.done);
                if !done {
                    domain.toggle_step(task_id, step_id)?;
                }
            } else {
                let Some(id) = model.selected_id() else {
                    model.set_message(NO_SELECTION);
                    return Ok(IntentOutcome::None);
                };
                domain.complete(id)?;
            }
        }
        BoardIntent::Reopen => {
            model.close_popup();
            if let Some((task_id, step_id)) = selected_step(domain, model) {
                let done = domain
                    .get(task_id)
                    .and_then(|task| task.steps.iter().find(|step| step.id == step_id))
                    .is_some_and(|step| step.done);
                if done {
                    domain.toggle_step(task_id, step_id)?;
                }
            } else {
                let Some(id) = model.selected_id() else {
                    model.set_message(NO_SELECTION);
                    return Ok(IntentOutcome::None);
                };
                domain.reopen(id)?;
            }
        }
        BoardIntent::SoftDelete => {
            model.close_popup();
            // On the page with the step cursor active, the delete verb is the
            // steps's mark-then-confirm: the first press visibly marks the
            // highlighted step, a second press removes it, and the task-level soft
            // delete below never runs.
            match page_step_delete(domain, model)? {
                PageStepDelete::Marked | PageStepDelete::Staged => return Ok(IntentOutcome::None),
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
                    // A task-owned stage (G or F) hands the slider back to the board the same
                    // way `Esc` would, so the arrows keep answering on the row with the undo.
                    if model
                        .form
                        .as_ref()
                        .filter(|form| form.is_task())
                        .and_then(BoardForm::task_id)
                        == Some(id)
                    {
                        model.form = None;
                        model.input_mode = BoardInputMode::Normal;
                        if matches!(model.wide_stage, WideStage::Rail | WideStage::FullTask) {
                            // Normal mode is board-owned, so the stage must be too: back to
                            // the full board when F was entered from it, otherwise to A.
                            model.wide_stage = match model.stage_origin.take() {
                                Some(WideStage::FullBoard) => WideStage::FullBoard,
                                _ => WideStage::Split,
                            };
                        }
                    }
                }
            }
        }
        BoardIntent::File => {
            // The picker handles File itself (T-8); for now it stays inert.
            if model.project_picker.is_some() {
                return Ok(IntentOutcome::None);
            }
            model.close_popup();
            let Some(id) = model.selected_id() else {
                model.set_message(NO_SELECTION);
                return Ok(IntentOutcome::None);
            };
            let Some(task) = domain.get(id) else {
                model.set_message("that task is no longer here");
                return Ok(IntentOutcome::None);
            };
            if task.archived {
                domain.unarchive_task(id)?;
            } else {
                domain.archive_task(id)?;
            }
            // Success has no message: the row's disappearance (or return) is the feedback.
        }
        BoardIntent::Undo => {
            // The picker handles Undo itself (T-8); for now it stays inert.
            if model.project_picker.is_some() {
                return Ok(IntentOutcome::None);
            }
            model.close_popup();
            // ctrl+u on an archived selection is the unarchive route, never an undo:
            // the stack is not popped (AC-3).
            if let Some(id) = model.selected_id() {
                if domain.get(id).is_some_and(|task| task.archived) {
                    domain.unarchive_task(id)?;
                    model.sync_from_domain(domain);
                    return Ok(IntentOutcome::Persist);
                }
            }
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

/// Move the slider one stage to the right. Stages A, G and F need a selected task; the
/// pane binds (or rebinds) its page to the selection on the way into G.
fn stage_right(domain: &DomainState, model: &mut BoardModel) {
    match model.wide_stage {
        WideStage::FullBoard => {
            if model.selected_id().is_some() {
                model.wide_stage = WideStage::Split;
            }
        }
        WideStage::Split => {
            if model.selected_id().is_some() {
                bind_selected_task_page(domain, model);
                model.wide_stage = WideStage::Rail;
            }
        }
        WideStage::Rail => {
            // A disk merge can remove the task G is showing; the parked form still pins it
            // as the selection, so ask the domain, not the pin. F needs a live task.
            if model
                .selected_id()
                .is_some_and(|id| domain.get(id).is_some())
            {
                model.stage_origin = Some(WideStage::Rail);
                model.wide_stage = WideStage::FullTask;
            }
        }
        WideStage::FullTask => {}
    }
}

/// Move the slider one stage to the left. The page session is parked, never dropped: G → A
/// keeps the pane bound to the same task (a dirty draft included), and F always returns to G.
fn stage_left(model: &mut BoardModel) {
    match model.wide_stage {
        WideStage::FullBoard => {}
        WideStage::Split => model.wide_stage = WideStage::FullBoard,
        WideStage::Rail => model.wide_stage = WideStage::Split,
        WideStage::FullTask => {
            model.stage_origin = None;
            model.wide_stage = WideStage::Rail;
        }
    }
}

/// Enter the task-owned stage nearest to the current board-owned one, when a task-side
/// action (a field edit from the board) needs the task to own input.
fn enter_task_stage(model: &mut BoardModel) {
    match model.wide_stage {
        WideStage::FullBoard => {
            model.stage_origin = Some(WideStage::FullBoard);
            model.wide_stage = WideStage::FullTask;
        }
        WideStage::Split => model.wide_stage = WideStage::Rail,
        WideStage::Rail | WideStage::FullTask => {}
    }
}

/// Close the task page from a task-owned stage. F returns to the stage `Enter` left; G
/// returns to A. A page that returns to the bare board is closed, one that returns to a
/// pane-bearing stage stays parked so its scroll and step cursor survive.
fn leave_task_page(model: &mut BoardModel) {
    let target = match model.wide_stage {
        WideStage::FullTask => model.stage_origin.take().unwrap_or(WideStage::FullBoard),
        WideStage::Rail => WideStage::Split,
        stage @ (WideStage::Split | WideStage::FullBoard) => stage,
    };
    model.stage_origin = None;
    model.wide_stage = target;
    if target == WideStage::FullBoard {
        model.form = None;
        model.input_mode = if model.quick_add.is_some() {
            BoardInputMode::QuickAdd
        } else {
            BoardInputMode::Normal
        };
    }
    model.clear_message();
}

/// Make sure the task page is bound to the selected task before a task-owned stage paints it.
fn bind_selected_task_page(domain: &DomainState, model: &mut BoardModel) {
    // `BoardModel::input_mode()` reports Normal while a task form is parked, so every
    // refocus route must restore the raw TaskPage mode before task input can dispatch.
    let Some(id) = model.selected_id() else {
        return;
    };
    if model.edit_target() != Some(id) || model.input_mode != BoardInputMode::TaskPage {
        open_task_page_on(domain, model, id);
    }
}

/// Open the full task page (stage F) on `id`, remembering the stage it left. Shared by the
/// keyboard route (`Enter`) and the mouse route (a row double-click).
fn open_full_task_page(domain: &DomainState, model: &mut BoardModel, id: Uuid) {
    if domain.get(id).is_none() {
        return;
    }
    if model.wide_stage != WideStage::FullTask {
        model.stage_origin = Some(model.wide_stage);
    }
    if model.edit_target() != Some(id) || model.input_mode != BoardInputMode::TaskPage {
        open_task_page_on(domain, model, id);
    }
    model.wide_stage = WideStage::FullTask;
}

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
    let step_renames = form
        .steps
        .drafts
        .iter()
        .filter(|(step_id, _)| !form.steps.removals.contains(step_id))
        .map(|(step_id, draft)| (*step_id, draft.value().trim().to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let step_removals = form.steps.removals.clone();
    if step_renames.values().any(String::is_empty) {
        model.set_message(STEP_TEXT_REQUIRED);
        return Ok(IntentOutcome::None);
    }
    let task = domain.get(id).ok_or(DomainError::UnknownId(id))?.clone();
    let selected_step = form
        .steps
        .cursor
        .and_then(|source_index| task.steps.get(source_index))
        .map(|step| step.id);
    // `DomainState::edit` does not reject a soft-deleted task itself. Refuse before touching
    // the form so's bound-task and draft-recovery guarantees remain intact.
    if task.soft_deleted {
        return Err(DomainError::SoftDeleted(id));
    }

    // The full page session changes under one revision, then the app persists exactly once.
    // A chained `edit` plus `rename_step` sequence would advance the merge base after each
    // draft and make the store correctly refuse the later revision as a conflicting writer.
    let step_rename_list = step_renames
        .iter()
        .map(|(step_id, text)| (*step_id, text.clone()))
        .collect::<Vec<_>>();
    domain.edit_with_step_changes(
        id,
        &title,
        notes.clone(),
        scope.clone(),
        thread.clone(),
        &step_rename_list,
        &step_removals.iter().copied().collect::<Vec<_>>(),
    )?;

    // Retain the complete form and mode until the persistence boundary confirms this exact
    // task-session save. A failed save can then Retry or Cancel without orphaning drafts.
    model.task_edit_save = Some(TaskEditSave {
        id,
        title,
        notes,
        scope,
        thread,
        step_renames,
        step_removals,
        selected_step,
    });
    Ok(IntentOutcome::Persist)
}

// ---------------------------------------------------------------------------
// Steps step cursor, verbs, and one-line editor (T-3)
// ---------------------------------------------------------------------------

/// Move from the form's end fields into its existing-step group or trailing add target.
fn select_step_from_tab(model: &mut BoardModel, forward: bool) {
    let target = model.form.as_ref().and_then(|form| {
        if !form.is_task()
            || !form.editing
            || (forward && form.focus != CaptureField::Notes)
            || (!forward && form.focus != CaptureField::Scope)
        {
            None
        } else {
            let task_id = form.task_id()?;
            let visible: Vec<usize> = model
                .tasks
                .iter()
                .find(|task| task.id == task_id)?
                .steps
                .iter()
                .enumerate()
                .filter(|(_, step)| !form.steps.removals.contains(&step.id))
                .map(|(source_index, _)| source_index)
                .collect();
            Some(if forward {
                visible.first().copied()
            } else {
                visible.last().copied()
            })
        }
    });
    let Some(target) = target else {
        return;
    };
    let Some(index) = target else {
        select_add_step(model);
        return;
    };
    let text = model.form.as_ref().and_then(|form| {
        form.task_id().and_then(|task_id| {
            model
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| {
                    task.steps
                        .get(index)
                        .map(|step| (step.id, step.text.clone()))
                })
        })
    });
    if let Some((step_id, text)) = text {
        if let Some(form) = model.form.as_mut() {
            form.steps.cursor = Some(index);
            form.steps.add_selected = false;
            steps_scroll_to_cursor(form, index);
        }
        open_step_editor(model, &text, Some(step_id));
    }
}

/// Start a task-page Tab cycle on its first stored step, or its trailing add target when
/// the checklist is empty, without entering the task edit session.
fn select_first_step_from_page(model: &mut BoardModel) -> bool {
    let count = model.form.as_ref().and_then(|form| {
        let task_id = form.task_id()?;
        model
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .map(|task| task.steps.len())
    });
    let Some(count) = count else {
        return false;
    };
    if count == 0 {
        select_add_step(model);
    } else if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
        form.steps.cursor = Some(0);
        form.steps.add_selected = false;
        steps_scroll_to_cursor(form, 0);
    }
    true
}

/// Cycle view-mode Tab through every stored step and then its trailing add target.
fn move_step_with_tab(model: &mut BoardModel, forward: bool) -> bool {
    let state = model.form.as_ref().and_then(|form| {
        if !form.is_task() {
            return None;
        }
        let task_id = form.task_id()?;
        let count = model
            .tasks
            .iter()
            .find(|task| task.id == task_id)?
            .steps
            .len();
        Some((form.steps.cursor, form.steps.add_selected, count))
    });
    let Some((cursor, add_selected, count)) = state else {
        return false;
    };
    if add_selected {
        if count == 0 {
            return true;
        }
        let index = if forward { 0 } else { count - 1 };
        let form = model.form.as_mut().expect("task form remains open");
        form.steps.cursor = Some(index);
        form.steps.add_selected = false;
        steps_scroll_to_cursor(form, index);
        return true;
    }
    let Some(index) = cursor else {
        return false;
    };
    if (forward && index + 1 == count) || (!forward && index == 0) {
        select_add_step(model);
    } else {
        let next = if forward { index + 1 } else { index - 1 };
        let form = model.form.as_mut().expect("task form remains open");
        form.steps.cursor = Some(next);
        form.steps.add_selected = false;
        steps_scroll_to_cursor(form, next);
    }
    true
}

/// Move within stored task-edit steps without wrapping, opening the reached row inline.
fn move_step_within_edit_group(model: &mut BoardModel, forward: bool) -> bool {
    let target = model.form.as_ref().and_then(|form| {
        if !form.is_task() || !form.editing {
            return None;
        }
        let index = form.steps.cursor?;
        let task_id = form.task_id()?;
        let task = model.tasks.iter().find(|task| task.id == task_id)?;
        let visible: Vec<usize> = task
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| !form.steps.removals.contains(&step.id))
            .map(|(source_index, _)| source_index)
            .collect();
        let position = visible
            .iter()
            .position(|source_index| *source_index == index)?;
        let target = if forward {
            *visible.get(position + 1)?
        } else {
            *visible.get(position.checked_sub(1)?)?
        };
        task.steps
            .get(target)
            .map(|step| (target, step.id, step.text.clone()))
    });
    let Some((target, step_id, text)) = target else {
        return false;
    };
    if let Some(form) = model.form.as_mut() {
        form.steps.cursor = Some(target);
        form.steps.add_selected = false;
        steps_scroll_to_cursor(form, target);
    }
    open_step_editor(model, &text, Some(step_id));
    true
}

fn select_add_step(model: &mut BoardModel) {
    if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
        form.steps.cursor = None;
        form.steps.add_selected = true;
        model.input_mode = BoardInputMode::TaskPage;
    }
}

fn select_last_step_for_edit(model: &mut BoardModel) {
    let target = model.form.as_ref().and_then(|form| {
        let task = model
            .tasks
            .iter()
            .find(|task| Some(task.id) == form.task_id())?;
        task.steps
            .iter()
            .enumerate()
            .rev()
            .find(|(_, step)| !form.steps.removals.contains(&step.id))
            .map(|(source_index, step)| (source_index, step.id, step.text.clone()))
    });
    let Some((index, step_id, text)) = target else {
        model.focus_form_field(CaptureField::Notes);
        return;
    };
    if let Some(form) = model.form.as_mut() {
        form.steps.cursor = Some(index);
        form.steps.add_selected = false;
        steps_scroll_to_cursor(form, index);
    }
    open_step_editor(model, &text, Some(step_id));
}

/// Resolve the page's selected step to live task and step ids, declining a stale index after
/// a domain change. Task view and task edit sessions share this selection.
fn selected_step(domain: &DomainState, model: &BoardModel) -> Option<(Uuid, Uuid)> {
    if model.focused_surface() != FocusedSurface::Task
        || !matches!(
            model.input_mode,
            BoardInputMode::TaskPage | BoardInputMode::EditStep
        )
    {
        return None;
    }
    let form = model.form.as_ref().filter(|form| form.is_task())?;
    if form.steps.add_selected {
        return None;
    }
    let task_id = form.task_id()?;
    let index = form.steps.cursor?;
    let step_id = domain.get(task_id)?.steps.get(index).map(|step| step.id)?;
    (!form.steps.removals.contains(&step_id)).then_some((task_id, step_id))
}

/// Keep the selected step in the renderer-recorded shared content viewport.
fn steps_scroll_to_cursor(form: &mut BoardForm, cursor: usize) {
    let counts = form.steps.row_counts.borrow();
    let wrapped_before: usize = if counts.len() >= cursor {
        counts.iter().take(cursor).copied().sum()
    } else {
        cursor
    };
    let target = form
        .steps
        .content_start
        .get()
        .saturating_add(1)
        .saturating_add(wrapped_before);
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
    /// The edit session owns this removal until its final save.
    Staged,
    /// First view-mode press: the cursor's step is visibly marked; nothing was removed.
    Marked,
    /// Second view-mode press: the marked step was removed through the domain command.
    Removed,
}

/// Mark-then-confirm delete for the step under the page's cursor (AC-11). Any
/// intervening intent already cleared the mark in [`apply_intent`], so a mark found
/// here equal to the cursor's step can only be this verb's own first press.
fn page_step_delete(
    domain: &mut DomainState,
    model: &mut BoardModel,
) -> Result<PageStepDelete, DomainError> {
    let Some((task_id, step_id)) = selected_step(domain, model) else {
        return Ok(PageStepDelete::NotApplicable);
    };
    let form = model
        .form
        .as_ref()
        .filter(|form| form.is_task())
        .expect("selected_step checked a task form");
    let index = form
        .steps
        .cursor
        .expect("selected_step checked an active cursor");
    if model.task_editing() {
        let task = domain
            .get(task_id)
            .expect("selected_step checked a live task");
        let form = model.form.as_mut().expect("task form checked above");
        form.steps.removals.insert(step_id);
        form.steps.drafts.remove(&step_id);
        form.steps.editor = None;
        form.steps.delete_mark = None;
        // Keep the selector on the nearest remaining stored row. Its source index remains
        // stable while the renderer filters staged removals, so it cannot point at the row
        // that just vanished or silently select a different draft.
        let remaining: Vec<usize> = task
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| !form.steps.removals.contains(&step.id))
            .map(|(source_index, _)| source_index)
            .collect();
        form.steps.cursor = remaining
            .iter()
            .copied()
            .find(|source_index| *source_index >= index)
            .or_else(|| remaining.last().copied());
        form.steps.add_selected = false;
        if let Some(cursor) = form.steps.cursor {
            steps_scroll_to_cursor(form, cursor);
        }
        model.input_mode = BoardInputMode::TaskPage;
        return Ok(PageStepDelete::Staged);
    }
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
        model.set_message("press ctrl+x again to remove");
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

/// Open an in-place step row seeded with `text`, renaming `step` when given and adding
/// a transient row when `None`.
fn open_step_editor(model: &mut BoardModel, text: &str, rename: Option<Uuid>) {
    if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
        form.steps.add_selected = false;
        if form
            .steps
            .editor
            .as_ref()
            .is_some_and(|editor| editor.rename == rename)
        {
            model.input_mode = BoardInputMode::EditStep;
            model.clear_message();
            return;
        }
        let buffer = rename
            .and_then(|step_id| form.steps.drafts.remove(&step_id))
            .unwrap_or_else(|| crate::ui::edit::seeded_draft(text));
        form.steps.editor = Some(StepEditor {
            buffer,
            rename,
            refusal: None,
        });
        model.input_mode = BoardInputMode::EditStep;
        model.clear_message();
    }
}

/// Close the inline step editor back to page view, discarding its draft. Any pending editor
/// save goes with it: a closed row has nothing left for the save boundary to release (the only
/// path that can be here with one pending is a defensive direct
/// intent, never the keyboard).
fn close_step_editor(model: &mut BoardModel) {
    if let Some(form) = model.form.as_mut() {
        form.steps.pending_save = None;
        form.steps.editor = None;
    }
    model.input_mode = BoardInputMode::TaskPage;
    model.clear_message();
}

/// What the inline step row paints when its draft is empty after trim: a short dim refusal
/// on the row itself, never the board status row.
const STEP_TEXT_REQUIRED: &str = "text required";

/// Persist a new-step editor draft through the domain command.
///
/// Existing-step editors are parked into the enclosing task session before this function is
/// reached. A successful add records the touched step on the page's pending-save slot and
/// leaves the line exactly as the user left it. Only the persistence boundary's confirmed
/// sync releases it, closing for Enter or reopening empty for Shift+Enter. A failed save
/// therefore holds the line behind SaveRecovery until Retry/Cancel resolve it.
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
    debug_assert!(
        editor.rename.is_none(),
        "existing-step drafts are staged by the task session"
    );
    let touched = match domain.add_step(task_id, &text) {
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
        reopen: keep_open,
    });
    Ok(IntentOutcome::Persist)
}
