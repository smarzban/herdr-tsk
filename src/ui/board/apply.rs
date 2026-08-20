//! Board intent reducer and dispatch-recovery result application.

use std::collections::BTreeSet;
use std::time::Instant;

use uuid::Uuid;

use crate::config::{default_config_dir, SettingsRecord};
use crate::context::InvocationSnapshot;
use crate::dispatch::DispatchRecoveryResult;
use crate::domain::{DomainError, DomainState, HumanStatus, TaskScope};
use crate::host::HostPorts;
use crate::ui::capture::{CaptureField, TITLE_REQUIRED_MESSAGE};
use crate::ui::edit::{flatten_line_breaks, EditBuffer};
use crate::ui::input::BoardIntent;
use crate::ui::mouse::BoardPopup;

use super::commands::{resolve_board_command, CommandSurface};
use super::model::{
    owned_resource_summary, BoardForm, BoardInputMode, BoardModel, IntentOutcome, OwnedDeckScope,
    ProjectPickerState, ProjectScopeOption,
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
/// [`IntentOutcome::Persist`]. The capture snapshot is retained by the form; the host argument
/// stays as a compatibility seam for existing callers and dispatch recovery wiring.
pub fn apply_intent(
    domain: &mut DomainState,
    model: &mut BoardModel,
    intent: BoardIntent,
    snapshot: Option<&InvocationSnapshot>,
    host: Option<&dyn HostPorts>,
) -> Result<IntentOutcome, DomainError> {
    // A successful quick add remains emphasized only until the next input intent.
    model.clear_saved_task();
    let notice_before = model.delete_notice().map(str::to_string);
    let mutating = board_intent_may_persist(&intent);
    let result = apply_board_intent(domain, model, intent, snapshot, host);
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
    host: Option<&dyn HostPorts>,
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
            return apply_intent(domain, model, resolved, snapshot, host);
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
            // The invocation snapshot remains the default scope unless a title token says
            // otherwise, matching full capture semantics.
            model.quick_add = Some(super::model::QuickAddState::new(snapshot.cloned()));
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
            let (title, token_scope) = quick_add_title_and_scope(
                quick_add.title.value(),
                domain,
                quick_add.snapshot.as_ref().as_ref(),
            );
            let scope = token_scope.unwrap_or_else(|| quick_add.scope.clone());
            let snapshot = quick_add.snapshot.as_ref().clone();
            if let Some(quick_add) = model.quick_add.as_mut() {
                quick_add.title = crate::ui::edit::seeded_draft(&title);
                quick_add.scope = scope.clone();
            }
            let mut form = BoardForm::capture(snapshot, model.this_repo.as_deref(), &model.tasks);
            form.title = crate::ui::edit::seeded_draft(&title);
            form.scope = scope;
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
            return apply_board_intent(
                domain,
                model,
                BoardIntent::SelectIndex(index),
                snapshot,
                host,
            );
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
        BoardIntent::BeginEditTitle | BoardIntent::BeginEditNotes | BoardIntent::BeginEditScope => {
            model.close_popup();
            let focus = match intent {
                BoardIntent::BeginEditTitle => CaptureField::Title,
                BoardIntent::BeginEditNotes => CaptureField::Notes,
                BoardIntent::BeginEditScope => CaptureField::Scope,
                _ => unreachable!("matched task-form entry intent"),
            };
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
            let single_line = model
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
            // Field edit on the task page: Esc cancels the field being edited (its draft
            // resets to the saved value) and steps back to view mode. Drafts on other
            // fields survive; the second Esc closes the page.
            if matches!(
                model.input_mode,
                BoardInputMode::EditTitle | BoardInputMode::EditNotes | BoardInputMode::EditScope
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
        BoardIntent::ConfirmEdit => {
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
                return match crate::capture::capture_save(
                    domain,
                    None,
                    &snap,
                    title,
                    notes,
                    scope_override,
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
            if matches!(
                model.popup,
                BoardPopup::SaveRecovery | BoardPopup::Recovery | BoardPopup::CleanupConfirm
            ) {
                return Ok(IntentOutcome::None);
            }
            let options = model.project_options();
            // Highlight the option that matches the current deck scope (session filter).
            let selected = match &model.deck_scope {
                OwnedDeckScope::All => 0,
                OwnedDeckScope::Global => 1,
                OwnedDeckScope::Project(path) => options
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
            model.set_deck_scope(chosen.unwrap_or(ProjectScopeOption::All));
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
            model.set_deck_scope(chosen);
            model.clear_message();
            return Ok(IntentOutcome::None);
        }
        BoardIntent::SelectSectionProject(index) => {
            // Mouse-only jump: an all-projects ON DECK group header names its own project.
            // The first click only arms that path; a second click on the same header within
            // the task-row double-click window adopts it exactly the way choosing it in the
            // selector dropdown would. Session-only navigation: nothing durable is touched.
            // A stale section index or a header without a project remains inert.
            let Some(path) = model
                .queue_view()
                .sections
                .get(index)
                .and_then(|section| section.project_label.clone())
                .map(Into::into)
            else {
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
                model.set_deck_scope(ProjectScopeOption::Project(path));
                model.clear_message();
            } else {
                model.last_project_header_click = Some((now, path));
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::RecoveryResume => {
            let Some(attempt) = model.selected_attempt().cloned() else {
                model.close_popup();
                model.set_message("dispatch recovery is no longer available");
                return Ok(IntentOutcome::None);
            };
            model.close_popup();
            model.set_message("resuming dispatch recovery…");
            return Ok(IntentOutcome::ResumeDispatch {
                attempt_id: attempt.id(),
            });
        }
        BoardIntent::BeginCleanup => {
            if let Some(summary) = model.selected_attempt().map(owned_resource_summary) {
                model.popup = BoardPopup::CleanupConfirm;
                model.set_message(format!("confirm cleanup · {summary}"));
            } else {
                model.close_popup();
                model.set_message("dispatch recovery is no longer available");
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::ConfirmCleanup => {
            let Some(attempt) = model.selected_attempt().cloned() else {
                model.close_popup();
                model.set_message("dispatch recovery is no longer available");
                return Ok(IntentOutcome::None);
            };
            model.close_popup();
            model.set_message("cleaning recorded dispatch resources…");
            return Ok(IntentOutcome::CleanupDispatch {
                attempt_id: attempt.id(),
            });
        }
        BoardIntent::CancelRecovery => {
            model.popup = if model.popup == BoardPopup::CleanupConfirm {
                BoardPopup::Recovery
            } else {
                BoardPopup::None
            };
            return Ok(IntentOutcome::None);
        }
        // The app-level save-recovery boundary handles these while unresolved.
        BoardIntent::RetrySave | BoardIntent::CancelSave => return Ok(IntentOutcome::None),
        BoardIntent::PrimaryVerb => {
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
                HumanStatus::Ready => {
                    domain.set_status(id, HumanStatus::Started)?;
                }
                HumanStatus::Done => {
                    domain.reopen(id)?;
                }
                HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review => {
                    model.set_message("resume not available yet");
                    return Ok(IntentOutcome::None);
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
        BoardIntent::PageScrollUp => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    form.notes_scroll = form.notes_scroll.saturating_sub(1);
                }
            }
            return Ok(IntentOutcome::None);
        }
        BoardIntent::PageScrollDown => {
            if let Some(form) = model.form.as_mut().filter(|form| form.is_task()) {
                if model.input_mode == BoardInputMode::TaskPage {
                    // Bounded by the rows the last painted frame actually laid out, so the
                    // bottom of a wrapping note is reachable. Logical lines undercount every
                    // wrapped row, which stranded the tail of long notes.
                    let horizon = form.notes_max_scroll.get();
                    form.notes_scroll = form.notes_scroll.saturating_add(1).min(horizon);
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

/// Present a completed durable dispatch worker result after the caller reloads the store.
///
/// The loaded domain is the source of truth for owned receipts and whether recovery remains.
pub fn apply_dispatch_recovery_result(
    domain: &DomainState,
    model: &mut BoardModel,
    result: DispatchRecoveryResult,
) {
    model.sync_from_domain(domain);
    match result {
        DispatchRecoveryResult::Dispatched {
            pane_id,
            agent,
            prompt_warning,
        } => {
            model.close_popup();
            let mut message = match agent {
                Some(agent) => format!("dispatched · agent {agent} · pane {pane_id}"),
                None => format!("dispatched · pane {pane_id}"),
            };
            if let Some(warning) = prompt_warning {
                message.push_str(&format!(" (prompt failed: {warning})"));
            }
            model.set_message(message);
        }
        DispatchRecoveryResult::RecoveryRequired { attempt_id, .. }
        | DispatchRecoveryResult::Failed { attempt_id, .. }
        | DispatchRecoveryResult::PersistenceUncertain { attempt_id, .. } => {
            if let Some(attempt) = domain.active_attempt(attempt_id) {
                model.popup = BoardPopup::Recovery;
                model.set_message(format!(
                    "dispatch recovery · {} · {}",
                    attempt.last_error().unwrap_or("interrupted dispatch"),
                    owned_resource_summary(attempt)
                ));
            } else {
                model.close_popup();
                model.set_message("dispatch recovery state changed, reload before retrying");
            }
        }
        DispatchRecoveryResult::Cleaned { .. } => {
            model.close_popup();
            model.set_message("recorded dispatch resources cleaned");
        }
        DispatchRecoveryResult::Error { message } => {
            // The failed recovery attempt remains durable. Keep it presented because V1 has no
            // dispatch-start route that can reopen the recovery surface in this session.
            model.close_popup();
            model.present_existing_dispatch_recovery();
            model.set_message(format!("dispatch recovery failed: {message}"));
        }
    }
}

/// Apply one draft operation, but only while a field edit is actually open.
///
/// Every editing intent is inert in every other mode, exactly as the insert and backspace
/// intents already were before the cursor arrived.
fn edit_draft(model: &mut BoardModel, operation: impl FnOnce(&mut EditBuffer)) {
    if model.input_mode == BoardInputMode::FormScopeDropdown {
        return;
    }
    let Some(form) = model.form.as_mut() else {
        return;
    };
    match form.focus {
        CaptureField::Title => operation(&mut form.title),
        CaptureField::Notes => operation(&mut form.notes),
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
    let (title, token_scope) = quick_add_title_and_scope(
        quick_add.title.value(),
        domain,
        quick_add.snapshot.as_ref().as_ref(),
    );
    let scope = token_scope.unwrap_or_else(|| quick_add.scope.clone());
    match crate::capture::capture_save(domain, None, &snapshot, title, None, Some(scope.clone())) {
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

/// Parse whitespace-delimited quick-add scope directives before creating a task.
fn quick_add_title_and_scope(
    value: &str,
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> (String, Option<TaskScope>) {
    let words: Vec<&str> = value.split_whitespace().collect();
    if let Some(index) = words.iter().position(|word| *word == "!p") {
        let title = words[..index].join(" ");
        let path = words[index + 1..].join(" ");
        let scope = if path.is_empty() {
            TaskScope::Global
        } else {
            TaskScope::Project {
                path: resolve_quick_add_project_path(&path, domain, snapshot),
            }
        };
        return (title, Some(scope));
    }
    (words.join(" "), None)
}

/// Resolve a separator-free `!p` token against the projects available to this board.
///
/// Basename matching is case-sensitive, matching the domain's path equality. An absent or
/// ambiguous basename deliberately stays verbatim so a quick add never guesses a project.
fn resolve_quick_add_project_path(
    token: &str,
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> String {
    if token.contains('/') {
        return token.to_string();
    }

    let mut candidates = BTreeSet::new();
    for task in domain.tasks() {
        if let TaskScope::Project { path } = &task.scope {
            candidates.insert(path.clone());
        }
    }
    if let Some(snapshot) = snapshot {
        if let TaskScope::Project { path } = &snapshot.default_scope {
            candidates.insert(path.clone());
        }
        if let Some(this_repo) = snapshot.this_repo.as_deref() {
            candidates.insert(this_repo.to_string_lossy().into_owned());
        }
    }

    let mut matches = candidates.into_iter().filter(|path| {
        path.trim_end_matches('/')
            .rsplit('/')
            .find(|component| !component.is_empty())
            == Some(token)
    });
    match (matches.next(), matches.next()) {
        (Some(path), None) => path,
        _ => token.to_string(),
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
    let title = form.title.value().to_string();
    let notes = (!form.notes.value().trim().is_empty()).then(|| form.notes.value().to_string());
    let scope = form.scope.clone();
    let task = domain.get(id).ok_or(DomainError::UnknownId(id))?.clone();
    // `DomainState::edit` does not reject a soft-deleted task itself. Refuse before touching
    // the form so's bound-task and draft-recovery guarantees remain intact.
    if task.soft_deleted {
        return Err(DomainError::SoftDeleted(id));
    }

    // One existing DomainState::edit call updates title, Notes, and scope together.
    domain.edit(id, &title, notes, scope)?;

    // Only a successful atomic domain edit releases the form. Validation and stale-task
    // refusals leave its buffers, focus, scope choice, and immutable id untouched.
    model.input_mode = BoardInputMode::Normal;
    model.form = None;
    model.clear_message();
    Ok(IntentOutcome::Persist)
}
