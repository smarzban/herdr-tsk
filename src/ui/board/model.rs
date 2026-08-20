//! Session-only board state, forms, selection, and recovery presentation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use uuid::Uuid;

use crate::attention::{AttentionClassification, RefreshResult};
use crate::config::VerbModifier;
use crate::context::InvocationSnapshot;
use crate::domain::{
    is_linked, DispatchAttempt, DomainState, OwnedResourceReceipt, Task, TaskScope,
};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{seeded_draft, EditBuffer};
use crate::ui::input::{
    BOARD_HELP_LINE, COMMAND_SURFACE_HELP_LINE, HELP_SURFACE_HELP_LINE, SAVE_RECOVERY_HELP_LINE,
};
use crate::ui::mouse::BoardPopup;
use crate::ui::queue::{self, DeckScope, QueueView, SectionKind};
use crate::ui::selection;
use crate::ui::terminal_text;

use super::commands::CommandSurface;

/// Visible board title. Also the herdr pane title.
pub const BOARD_TITLE: &str = "Tasks";

/// Board chrome input mode (normal list vs field edit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardInputMode {
    #[default]
    Normal,
    EditTitle,
    EditNotes,
    /// The scope row of an open task form owns focus. The renderer adaptation remains
    /// deliberately thin until, but its keyboard state is a first-class form field.
    EditScope,
    /// A task/capture form's transient scope chooser. It returns to its parent form on Esc.
    FormScopeDropdown,
    /// The task page is open in view mode: the full-page surface shows the bound task and
    /// no field owns the cursor. Verbs act on the task; `e`/`n`/Tab enter field edits. A
    /// click does NOT: field regions are inert in this state, and only move focus once one
    /// of the edit states is already open (see the mouse mapper's form-field arms).
    TaskPage,
    /// Modal selection over the session project-scope options.
    ProjectPicker,
    /// Durable dispatch attempt recovery actions.
    Recovery,
    /// Explicit confirmation before cleanup touches recorded owned receipts.
    CleanupConfirm,
    /// Board persistence failed; only navigation plus Retry/Cancel are available.
    SaveRecovery,
    /// Searchable command palette is open.
    Palette,
    /// the help card is open.
    Help,
    /// Inline capture row/takeover from board `a`. Uses EditBuffer + snapshot; separate from standalone CaptureModel.
    Capture,
}

/// Result of applying a [`BoardIntent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentOutcome {
    /// No durable domain change.
    None,
    /// Domain mutated; caller should persist via Task Store and may refresh.
    Persist,
    /// Persistence already succeeded through the save-recovery boundary.
    Persisted,
    /// Leave the board UI.
    Quit,
    /// Resume an existing durable attempt on the worker.
    ResumeDispatch { attempt_id: Uuid },
    /// Clean an existing durable attempt after explicit confirmation.
    CleanupDispatch { attempt_id: Uuid },
}

/// Retained outcome vocabulary for the no-op walkthrough seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkthroughOutcome {
    Completed,
    Skipped,
    Unpresentable,
    Interrupted,
}

/// Open session project selector. Options are derived from the snapshot at open time.
#[derive(Debug, Clone)]
pub(super) struct ProjectPickerState {
    pub(super) options: Vec<ProjectScopeOption>,
    pub(super) selected: usize,
}

/// One session-only ON DECK scope offered by the project selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectScopeOption {
    All,
    Global,
    Project(PathBuf),
}

/// Session-owned ON DECK filter (owned form of [`DeckScope`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum OwnedDeckScope {
    #[default]
    All,
    Global,
    Project(PathBuf),
}

impl OwnedDeckScope {
    fn as_query(&self) -> DeckScope<'_> {
        match self {
            Self::All => DeckScope::All,
            Self::Global => DeckScope::Global,
            Self::Project(path) => DeckScope::Project(path.as_path()),
        }
    }
}
/// The immutable value a board form carries for its whole lifetime.
///
/// Keeping task identity and capture provenance mutually exclusive by construction means a
/// capture can never acquire a mutable selected-task target, and a task form can never reload
/// an invocation snapshot underneath its drafts.
#[derive(Debug, Clone)]
pub(super) enum BoardFormBinding {
    Task(Uuid),
    Capture(Box<Option<InvocationSnapshot>>),
}

/// One transient board form, shared by inline capture and task editing.
///
/// Both variants own two independent [`EditBuffer`]s, the selected task scope, field focus,
/// and a selection within the scope choices. A task form adds exactly one immutable task id;
/// a capture form instead retains exactly one immutable invocation snapshot. Keeping those two
/// mutually exclusive values inside the same form is what prevents title, Notes, and scope
/// from drifting into separately-bound edits.
#[derive(Debug, Clone)]
pub(super) struct BoardForm {
    pub(super) title: EditBuffer,
    pub(super) notes: EditBuffer,
    pub(super) scope: TaskScope,
    pub(super) focus: CaptureField,
    pub(super) scope_options: Vec<TaskScope>,
    pub(super) scope_selected: usize,
    pub(super) binding: BoardFormBinding,
    /// View-mode scroll of the task page's notes body (wrapped rows), never used by capture.
    pub(super) notes_scroll: usize,
    /// The furthest `notes_scroll` the LAST painted frame could actually show, in wrapped
    /// rows (`wrapped rows - visible rows`).
    ///
    /// The scroll bound depends on the wrap width, which only the renderer knows: the model
    /// is deliberately geometry-free and the render path takes `&BoardModel`. Bounding the
    /// intent by `notes.value().lines().count()` instead was wrong in the reachable
    /// direction, because wrapping only ever ADDS rows, so the tail of any wrapping note
    /// was unreachable while the divider still advertised it. A `Cell` lets the immutable
    /// renderer record what it actually laid out, without making the whole render path `&mut`
    /// or pushing geometry into every scroll intent.
    pub(super) notes_max_scroll: std::cell::Cell<usize>,
}

impl BoardForm {
    pub(super) fn task(
        task: &Task,
        this_repo: Option<&Path>,
        tasks: &[Task],
        focus: CaptureField,
    ) -> Self {
        Self::new(
            &task.title,
            task.notes.as_deref().unwrap_or_default(),
            task.scope.clone(),
            focus,
            BoardFormBinding::Task(task.id),
            this_repo,
            tasks,
        )
    }

    pub(super) fn capture(
        snapshot: Option<InvocationSnapshot>,
        this_repo: Option<&Path>,
        tasks: &[Task],
    ) -> Self {
        let initial_scope = snapshot
            .as_ref()
            .map(|snapshot| snapshot.default_scope.clone())
            .unwrap_or(TaskScope::Global);
        let title = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.title_prefill.as_deref())
            .unwrap_or_default()
            .to_string();
        Self::new(
            &title,
            "",
            initial_scope,
            CaptureField::Title,
            BoardFormBinding::Capture(Box::new(snapshot)),
            this_repo,
            tasks,
        )
    }

    pub(super) fn new(
        title: &str,
        notes: &str,
        scope: TaskScope,
        focus: CaptureField,
        binding: BoardFormBinding,
        this_repo: Option<&Path>,
        tasks: &[Task],
    ) -> Self {
        let scope_options = board_form_scope_options(&scope, this_repo, tasks);
        let scope_selected = scope_options
            .iter()
            .position(|option| option == &scope)
            .unwrap_or(0);
        Self {
            title: seeded_draft(title),
            notes: seeded_draft(notes),
            scope,
            focus,
            scope_options,
            scope_selected,
            binding,
            notes_scroll: 0,
            notes_max_scroll: std::cell::Cell::new(0),
        }
    }

    pub(super) fn is_task(&self) -> bool {
        matches!(&self.binding, BoardFormBinding::Task(_))
    }

    pub(super) fn task_id(&self) -> Option<Uuid> {
        match &self.binding {
            BoardFormBinding::Task(id) => Some(*id),
            BoardFormBinding::Capture(_) => None,
        }
    }

    pub(super) fn snapshot(&self) -> Option<&InvocationSnapshot> {
        match &self.binding {
            BoardFormBinding::Task(_) => None,
            BoardFormBinding::Capture(snapshot) => snapshot.as_ref().as_ref(),
        }
    }

    pub(super) fn parent_mode(&self) -> BoardInputMode {
        if self.is_task() {
            match self.focus {
                CaptureField::Title => BoardInputMode::EditTitle,
                CaptureField::Notes => BoardInputMode::EditNotes,
                CaptureField::Scope => BoardInputMode::EditScope,
            }
        } else {
            BoardInputMode::Capture
        }
    }

    /// Restore one field's draft to its bound task's saved value (Esc on the task page).
    pub(super) fn reset_field_to_saved(&mut self, field: CaptureField, task: &Task) {
        match field {
            CaptureField::Title => self.title = seeded_draft(&task.title),
            CaptureField::Notes => {
                self.notes = seeded_draft(task.notes.as_deref().unwrap_or_default());
            }
            CaptureField::Scope => {
                self.scope = task.scope.clone();
                self.select_current_scope();
            }
        }
    }

    pub(super) fn focus_next(&mut self) {
        self.focus = match self.focus {
            CaptureField::Title => CaptureField::Notes,
            CaptureField::Notes => CaptureField::Scope,
            CaptureField::Scope => CaptureField::Title,
        };
    }

    pub(super) fn focus_prev(&mut self) {
        self.focus = match self.focus {
            CaptureField::Title => CaptureField::Scope,
            CaptureField::Notes => CaptureField::Title,
            CaptureField::Scope => CaptureField::Notes,
        };
    }

    pub(super) fn cycle_scope(&mut self) {
        if self.scope_options.is_empty() {
            return;
        }
        let current = self
            .scope_options
            .iter()
            .position(|option| option == &self.scope)
            .unwrap_or(0);
        self.scope_selected = (current + 1) % self.scope_options.len();
        self.scope = self.scope_options[self.scope_selected].clone();
    }

    pub(super) fn move_scope_selection(&mut self, forward: bool) {
        if self.scope_options.is_empty() {
            return;
        }
        self.scope_selected = if forward {
            (self.scope_selected + 1) % self.scope_options.len()
        } else {
            self.scope_selected
                .checked_sub(1)
                .unwrap_or(self.scope_options.len() - 1)
        };
    }

    pub(super) fn select_current_scope(&mut self) {
        self.scope_selected = self
            .scope_options
            .iter()
            .position(|option| option == &self.scope)
            .unwrap_or(0);
    }

    pub(super) fn apply_scope_selection(&mut self) {
        if let Some(scope) = self.scope_options.get(self.scope_selected) {
            self.scope = scope.clone();
        }
    }
}

/// Scope choices shared by capture and task forms.
///
/// The order is intentional: the form's initial scope, the current repository, project scopes
/// represented by non-soft-deleted tasks, then Global. `TaskScope` has no all-projects value,
/// so an editor can never accidentally adopt the board selector's session-only `All` choice.
fn board_form_scope_options(
    initial_scope: &TaskScope,
    this_repo: Option<&Path>,
    tasks: &[Task],
) -> Vec<TaskScope> {
    let mut options = Vec::new();
    let push = |scope: TaskScope, options: &mut Vec<TaskScope>| {
        if !options.contains(&scope) {
            options.push(scope);
        }
    };
    push(initial_scope.clone(), &mut options);
    if let Some(repo) = this_repo {
        push(
            TaskScope::Project {
                path: repo.to_string_lossy().into_owned(),
            },
            &mut options,
        );
    }
    for task in tasks {
        if !task.soft_deleted {
            if let TaskScope::Project { path } = &task.scope {
                push(TaskScope::Project { path: path.clone() }, &mut options);
            }
        }
    }
    push(TaskScope::Global, &mut options);
    options
}

/// Pure board presentation state for one open session.
///
/// Session-only UI state lives here and is never written under the task store or
/// plugin config dirs. Visible rows come from [`queue::query`]; selection is
/// id-pinned and reanchored via [`selection::reanchor`].
#[derive(Debug, Clone)]
pub struct BoardModel {
    pub(super) tasks: Vec<Task>,
    pub(super) this_repo: Option<PathBuf>,
    /// Session ON DECK scope filter. Not durable.
    pub(super) deck_scope: OwnedDeckScope,
    /// Whether the done drawer lists completed tasks. Session-only.
    pub(super) drawer_open: bool,
    /// Accordion/takeover detail open on this task id, if any. Session-only.
    pub(super) detail_open: Option<Uuid>,
    /// Id-pinned selection into the queue-visible row set.
    pub(super) selection_id: Option<Uuid>,
    /// The last task-row click (time + id), kept only to detect a double-click that opens
    /// the task page. Presentation-only, never persisted.
    pub(super) last_row_click: Option<(Instant, Uuid)>,
    /// The last all-projects group-header click (time + project path), kept only to detect
    /// a double-click that narrows the board to that project. Presentation-only, never
    /// persisted.
    pub(super) last_project_header_click: Option<(Instant, PathBuf)>,
    pub(super) input_mode: BoardInputMode,
    /// The one active board form. It is present for inline capture and task editing alike;
    /// task identity or invocation context are held inside it and never rebound after open.
    pub(super) form: Option<BoardForm>,
    /// Last board action feedback or empty-selection chrome message.
    pub(super) message: Option<String>,
    /// Title of the task the last soft-delete removed, captured at delete time.
    ///
    /// Transient presentation only: it is never persisted, and it is the whole of the delete
    /// recovery notice's state. The title is captured rather than re-read,
    /// because by the time the row is painted the selection may have moved and the task
    /// itself may have been restored by the very Undo this notice offers.
    pub(super) delete_notice: Option<String>,
    /// A delete recovery notice a failed save took off the row, held until the failure is
    /// resolved.
    ///
    /// A failed save means the deletion is not durable, so the notice must not offer to undo
    /// it; but Retry can still make it durable, and then the way back has to come with it.
    /// See [`BoardModel::begin_save_recovery`] and [`BoardModel::end_save_recovery`].
    pub(super) suspended_delete_notice: Option<String>,
    /// Open recovery, cleanup, project-picker, or save-recovery presentation.
    pub(super) popup: BoardPopup,
    /// Presentation-only stale linked-session ids from the latest successful snapshot.
    pub(super) stale_links: HashSet<Uuid>,
    /// Open session project selector; never persisted.
    pub(super) project_picker: Option<ProjectPickerState>,
    /// Open palette (presentation only).
    pub(super) surface: CommandSurface,
    /// Palette query.
    pub(super) command_query: String,
    /// Selection into the currently visible command set.
    pub(super) command_selected: usize,
    /// No separate capture/title/notes state lives beside [`Self::form`]: one board form owns
    /// all field drafts and either its task binding or its immutable capture snapshot.
    /// Durable attempts hydrated from the store with the current domain snapshot.
    pub(super) attempts: Vec<DispatchAttempt>,
    /// Chord required for mutating verbs. Loaded from settings; flipped from the palette.
    pub verb_modifier: VerbModifier,
}

/// How an unresolved failed save ended.
///
/// The two outcomes differ in exactly one way the chrome row cares about: Retry makes the
/// working state durable, Cancel throws it away. See [`BoardModel::end_save_recovery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveResolution {
    /// Retry succeeded: what the failed save carried is now on disk.
    Retried,
    /// Cancel restored the pre-failure baseline: none of it happened.
    Cancelled,
}

impl BoardModel {
    /// Build a board model with queue-local session state only.
    pub fn from_tasks(tasks: Vec<Task>, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self {
            tasks,
            this_repo,
            deck_scope: OwnedDeckScope::All,
            drawer_open: false,
            detail_open: None,
            selection_id: None,
            last_row_click: None,
            last_project_header_click: None,
            input_mode: BoardInputMode::Normal,
            form: None,
            message: None,
            delete_notice: None,
            suspended_delete_notice: None,
            popup: BoardPopup::None,
            stale_links: HashSet::new(),
            project_picker: None,
            surface: CommandSurface::None,
            command_query: String::new(),
            command_selected: 0,
            attempts: Vec::new(),
            verb_modifier: VerbModifier::Alt,
        };
        model.seed_selection();
        model
    }

    /// Current detail popup (status picker or more menu).
    pub fn popup(&self) -> BoardPopup {
        self.popup
    }

    pub fn set_popup(&mut self, popup: BoardPopup) {
        self.close_help();
        self.popup = popup;
    }

    /// Dismiss the help layer when another surface must own input.
    pub fn close_help(&mut self) {
        if self.input_mode == BoardInputMode::Help {
            self.input_mode = BoardInputMode::Normal;
        }
    }

    pub fn close_popup(&mut self) {
        if self.popup != BoardPopup::SaveRecovery {
            self.popup = BoardPopup::None;
            self.project_picker = None;
        }
    }

    /// Present a failed board save without replacing its visible working state.
    ///
    /// A failed save *suspends* any delete recovery notice rather than taking it down for
    /// good: the deletion it offered to undo is not durable yet, so offering Undo would
    /// offer to undo nothing. Retry can still make it durable, and
    /// [`Self::end_save_recovery`] decides which of the two happened. Repeated failures
    /// keep the first suspended notice, so a second failed Retry cannot lose it.
    pub fn begin_save_recovery(&mut self, error: &str) {
        self.close_help();
        self.popup = BoardPopup::SaveRecovery;
        if let Some(notice) = self.delete_notice.take() {
            self.suspended_delete_notice = Some(notice);
        }
        self.set_message(format!("save failed: {error} · Retry or Cancel"));
    }

    /// End save recovery only after Retry succeeds or Cancel restores the baseline.
    ///
    /// A successful Retry makes the suspended deletion durable, so its recovery notice comes
    /// back: otherwise the user is left with a deleted task and no visible way back.
    /// Cancel restores the pre-failure baseline, so the deletion never happened and its
    /// notice is discarded.
    ///
    /// Nothing can be armed while a save failure is unresolved: the recovery gate routes
    /// every mutating intent back into [`Self::begin_save_recovery`], and only a mutating
    /// intent (a delete) arms a notice. That invariant is stated here by a resolution that is
    /// *total* rather than by an assertion, because the alternative to holding it is a panic
    /// inside a raw-mode TUI — a wedged terminal and a lost session, to report bookkeeping
    /// the user cannot see. Retry therefore prefers the suspended notice (the deletion it
    /// just made durable) and falls back to whatever was armed instead of wiping it, and both
    /// fields are emptied either way, so no path can lose the notice or leave a second copy
    /// behind to resurface at the next failure.
    pub fn end_save_recovery(&mut self, resolution: SaveResolution) {
        if self.popup == BoardPopup::SaveRecovery {
            self.popup = BoardPopup::None;
        }
        let armed = self.delete_notice.take();
        self.delete_notice = match resolution {
            SaveResolution::Retried => self.suspended_delete_notice.take().or(armed),
            SaveResolution::Cancelled => {
                self.suspended_delete_notice = None;
                None
            }
        };
    }

    /// Snapshot tasks from domain state (default agent kind; seed env at open).
    pub fn from_domain(state: &DomainState, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self::from_tasks(state.tasks().to_vec(), this_repo);
        model.attempts = state.active_attempts().to_vec();
        model.present_existing_dispatch_recovery();
        model
    }

    /// Replace task snapshot from domain (after mutation) and reanchor selection by id.
    pub fn sync_from_domain(&mut self, state: &DomainState) {
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.tasks = state.tasks().to_vec();
        self.stale_links.retain(|id| {
            self.tasks
                .iter()
                .any(|task| task.id == *id && !task.soft_deleted && is_linked(task))
        });
        self.attempts = state.active_attempts().to_vec();
        self.reanchor_selection(previous, &previous_visible);
        if self.attempts.is_empty()
            && matches!(
                self.popup,
                BoardPopup::Recovery | BoardPopup::CleanupConfirm
            )
        {
            self.close_popup();
        }
    }

    pub fn mark_stale(&mut self, id: Uuid) {
        self.stale_links.insert(id);
    }

    pub fn clear_stale(&mut self, id: Uuid) {
        self.stale_links.remove(&id);
    }

    pub fn is_stale(&self, id: Uuid) -> bool {
        self.stale_links.contains(&id)
    }

    pub fn apply_attention_result(&mut self, result: &RefreshResult) {
        for (id, classification) in &result.classifications {
            match classification {
                AttentionClassification::Matched => {
                    self.stale_links.remove(id);
                }
                AttentionClassification::Stale => {
                    self.stale_links.insert(*id);
                }
                AttentionClassification::Unavailable => {}
            }
        }
    }

    /// Presenter / pane title string.
    pub fn title(&self) -> &'static str {
        BOARD_TITLE
    }

    /// Resolved invocation repository used for scope-changing task edits.
    pub fn this_repo(&self) -> Option<&Path> {
        self.this_repo.as_deref()
    }

    /// The selected project scope, when the session filter is narrowed to one project.
    pub fn selected_project(&self) -> Option<&Path> {
        match &self.deck_scope {
            OwnedDeckScope::Project(path) => Some(path.as_path()),
            OwnedDeckScope::All | OwnedDeckScope::Global => None,
        }
    }

    /// Cancel an armed project-header double-click when another pointer target
    /// intervenes. Mouse-boundary state only, never persisted.
    pub(crate) fn cancel_project_header_double_click(&mut self) {
        self.last_project_header_click = None;
    }

    /// Options the session project selector offers, in presentation order.
    ///
    /// All projects and Global are always available. Project entries are paths that really
    /// carry a non-soft-deleted project-scoped task in the current snapshot, plus the
    /// resolved invocation repository and current selection: no path is ever invented.
    pub fn project_options(&self) -> Vec<ProjectScopeOption> {
        let mut paths: Vec<PathBuf> = Vec::new();
        let push = |path: PathBuf, paths: &mut Vec<PathBuf>| {
            if !paths.contains(&path) {
                paths.push(path);
            }
        };
        if let Some(repo) = self.this_repo.clone() {
            push(repo, &mut paths);
        }
        let mut from_tasks: Vec<PathBuf> = Vec::new();
        for task in &self.tasks {
            if task.soft_deleted {
                continue;
            }
            if let TaskScope::Project { path } = &task.scope {
                push(PathBuf::from(path), &mut from_tasks);
            }
        }
        from_tasks.sort();
        for path in from_tasks {
            push(path, &mut paths);
        }

        let mut options = Vec::with_capacity(paths.len() + 2);
        options.push(ProjectScopeOption::All);
        options.push(ProjectScopeOption::Global);
        options.extend(paths.into_iter().map(ProjectScopeOption::Project));
        options
    }

    /// Index into [`Self::project_options`] the open selector highlights.
    pub fn project_picker_index(&self) -> Option<usize> {
        self.project_picker.as_ref().map(|picker| picker.selected)
    }

    /// Queue sections + counts for the current session filters.
    pub fn queue_view(&self) -> QueueView {
        queue::query(
            &self.tasks,
            self.this_repo.as_deref(),
            self.deck_scope.as_query(),
            self.drawer_open,
        )
    }

    /// Whether the done drawer is open (session-only).
    pub fn drawer_open(&self) -> bool {
        self.drawer_open
    }

    /// Task id whose accordion/takeover detail is open, if any (session-only).
    pub fn detail_open(&self) -> Option<Uuid> {
        self.detail_open
    }

    /// Close the session-only detail layer.
    pub fn close_detail(&mut self) {
        self.detail_open = None;
    }

    /// Visible task ids from the queue section query (flat section order).
    pub fn visible_ids(&self) -> Vec<Uuid> {
        self.queue_view()
            .sections
            .iter()
            .flat_map(|section| section.task_ids.iter().copied())
            .collect()
    }

    /// Visible tasks in queue section order.
    pub fn visible_tasks(&self) -> Vec<&Task> {
        self.visible_ids()
            .into_iter()
            .filter_map(|id| self.tasks.iter().find(|t| t.id == id))
            .collect()
    }

    /// Selected index into the visible list (derived from the id pin).
    pub fn selected_index(&self) -> Option<usize> {
        let id = self.selection_id?;
        self.visible_ids().iter().position(|&row| row == id)
    }

    /// Selected task id, if any.
    pub fn selected_id(&self) -> Option<Uuid> {
        self.selection_id
    }
}

impl BoardModel {
    /// the retains this launch helper as a no-op seam for tests and later wiring.
    pub fn open_walkthrough(&mut self) {}

    /// The retired walkthrough has no presentation outcome to record.
    pub fn take_walkthrough_outcome(&mut self) -> Option<WalkthroughOutcome> {
        None
    }
    /// Current input mode (normal vs edit field).
    pub fn input_mode(&self) -> BoardInputMode {
        if self.surface == CommandSurface::Palette {
            return BoardInputMode::Palette;
        }
        match self.popup {
            BoardPopup::Recovery => BoardInputMode::Recovery,
            BoardPopup::CleanupConfirm => BoardInputMode::CleanupConfirm,
            BoardPopup::SaveRecovery => BoardInputMode::SaveRecovery,
            _ if self.project_picker.is_some() => BoardInputMode::ProjectPicker,
            _ => self.input_mode,
        }
    }

    /// Surface recovery for a persisted attempt without restoring any dispatch-start action.
    pub(super) fn present_existing_dispatch_recovery(&mut self) {
        let Some(attempt) = self.attempts.first() else {
            return;
        };
        let task_id = attempt.task_id();
        let message = format!(
            "dispatch recovery · {} · {}",
            attempt.last_error().unwrap_or("interrupted dispatch"),
            owned_resource_summary(attempt)
        );
        self.selection_id = Some(task_id);
        self.popup = BoardPopup::Recovery;
        self.set_message(message);
    }

    pub(super) fn selected_attempt(&self) -> Option<&DispatchAttempt> {
        let task_id = self.selected_id()?;
        self.attempts
            .iter()
            .find(|attempt| attempt.task_id() == task_id)
    }

    /// Edit buffer contents for the focused task-form text field.
    pub fn edit_buffer(&self) -> &str {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return "";
        };
        match form.focus {
            CaptureField::Title | CaptureField::Scope => form.title.value(),
            CaptureField::Notes => form.notes.value(),
        }
    }

    /// Cursor position in the focused task-form text buffer, counted in Unicode scalar values.
    pub fn edit_cursor(&self) -> usize {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return 0;
        };
        match form.focus {
            CaptureField::Title | CaptureField::Scope => form.title.cursor(),
            CaptureField::Notes => form.notes.cursor(),
        }
    }

    /// Whether a board form currently owns keyboard input. The app loop uses this one predicate
    /// to route Capture and task-form keys through the same mapper.
    pub fn board_form_open(&self) -> bool {
        self.form.is_some()
    }

    /// True when the open shared form is the task page's (not inline capture's).
    pub fn board_form_is_task(&self) -> bool {
        self.form.as_ref().is_some_and(BoardForm::is_task)
    }

    /// Focused field of the shared board form, if one is open.
    pub fn form_focus(&self) -> Option<CaptureField> {
        self.form.as_ref().map(|form| form.focus)
    }

    /// Chosen scope draft of the shared board form.
    pub fn form_scope(&self) -> Option<&TaskScope> {
        self.form.as_ref().map(|form| &form.scope)
    }

    /// The shared form's choices. It never includes the board selector's all-projects option.
    pub fn form_scope_options(&self) -> Vec<TaskScope> {
        self.form
            .as_ref()
            .map(|form| form.scope_options.clone())
            .unwrap_or_default()
    }

    /// The currently highlighted scope choice while the form dropdown is open.
    pub fn form_scope_dropdown_choice(&self) -> Option<&TaskScope> {
        let form = self.form.as_ref()?;
        form.scope_options.get(form.scope_selected)
    }

    /// Inline capture's title draft.
    pub fn capture_title_value(&self) -> &str {
        self.form
            .as_ref()
            .filter(|form| !form.is_task())
            .map(|form| form.title.value())
            .unwrap_or("")
    }

    /// Inline capture's notes draft.
    pub fn capture_notes_value(&self) -> &str {
        self.form
            .as_ref()
            .filter(|form| !form.is_task())
            .map(|form| form.notes.value())
            .unwrap_or("")
    }

    /// Which capture field currently has keyboard focus.
    pub fn capture_focus(&self) -> CaptureField {
        self.form
            .as_ref()
            .filter(|form| !form.is_task())
            .map(|form| form.focus)
            .unwrap_or(CaptureField::Title)
    }

    /// The scope this inline capture will create in, if its shared form is open.
    pub fn capture_scope(&self) -> Option<&TaskScope> {
        self.form
            .as_ref()
            .filter(|form| !form.is_task())
            .map(|form| &form.scope)
    }

    /// Project scopes available to the open inline capture, followed by Global.
    pub fn capture_scope_options(&self) -> Vec<TaskScope> {
        self.form
            .as_ref()
            .filter(|form| !form.is_task())
            .map(|form| form.scope_options.clone())
            .unwrap_or_default()
    }

    /// Enter field edit on the page's own focused field (Tab from view mode).
    pub(super) fn enter_page_field_focus(&mut self) {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return;
        };
        self.input_mode = form.parent_mode();
    }

    pub(super) fn move_form_focus(&mut self, forward: bool) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if forward {
            form.focus_next();
        } else {
            form.focus_prev();
        }
        self.input_mode = form.parent_mode();
    }

    /// Focus one already-painted field without reopening or rebinding the shared form.
    pub(super) fn focus_form_field(&mut self, focus: CaptureField) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if self.input_mode == BoardInputMode::FormScopeDropdown {
            return;
        }
        form.focus = focus;
        self.input_mode = form.parent_mode();
    }

    pub(super) fn cycle_form_scope(&mut self) {
        if let Some(form) = self.form.as_mut() {
            form.cycle_scope();
        }
    }

    pub(super) fn open_form_scope_dropdown(&mut self) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        // A Scope row click reaches this route directly. Keyboard only reaches it after focus
        // has moved to Scope, but both routes leave the parent form focused there.
        form.focus = CaptureField::Scope;
        // Each opening starts from the parent form's chosen scope. Esc therefore discards
        // only the dropdown's pending highlight, never a field draft or the whole form.
        form.select_current_scope();
        self.input_mode = BoardInputMode::FormScopeDropdown;
    }

    pub(super) fn close_form_scope_dropdown(&mut self, apply: bool) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if apply {
            form.apply_scope_selection();
        } else {
            form.select_current_scope();
        }
        self.input_mode = form.parent_mode();
    }

    pub(super) fn move_form_scope_dropdown(&mut self, forward: bool) {
        if self.input_mode == BoardInputMode::FormScopeDropdown {
            if let Some(form) = self.form.as_mut() {
                form.move_scope_selection(forward);
            }
        }
    }

    /// Apply a clicked source option exactly as keyboard navigation plus Enter would.
    pub(super) fn select_form_scope_option(&mut self, index: usize) {
        if self.input_mode != BoardInputMode::FormScopeDropdown {
            return;
        }
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if index >= form.scope_options.len() {
            return;
        }
        form.scope_selected = index;
        form.apply_scope_selection();
        self.input_mode = form.parent_mode();
    }

    /// Help line listing primary key bindings.
    pub fn help_line(&self) -> &'static str {
        match self.input_mode() {
            BoardInputMode::Palette => COMMAND_SURFACE_HELP_LINE,
            BoardInputMode::SaveRecovery => SAVE_RECOVERY_HELP_LINE,
            BoardInputMode::Help => HELP_SURFACE_HELP_LINE,
            _ => BOARD_HELP_LINE,
        }
    }

    /// Chrome status/message line feedback.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(terminal_text(&msg.into()));
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    /// Title carried by the visible delete recovery notice, if one is armed.
    pub fn delete_notice(&self) -> Option<&str> {
        self.delete_notice.as_deref()
    }

    /// Arm the notice for a task that has just been soft-deleted.
    pub(super) fn arm_delete_notice(&mut self, title: &str) {
        self.delete_notice = Some(terminal_text(title));
    }

    /// Take the notice down. See the chrome-row lifetime rule above [`apply_intent`].
    ///
    /// Private on purpose: the lifetime rule is one rule in one place, and a fourth caller
    /// clearing the notice on an event of its own is exactly the drift forbids.
    pub(super) fn clear_delete_notice(&mut self) {
        self.delete_notice = None;
    }

    /// The notice as far as the chrome row is concerned: armed, and not hidden under a modal
    /// command surface.
    ///
    /// An open action sheet or palette is modal and owns every click ([`super::mouse`]), so
    /// the row does not paint a control the click path would swallow: painted state and
    /// clickable state say the same thing.
    pub(super) fn visible_delete_notice(&self) -> Option<&str> {
        if self.surface != CommandSurface::None {
            return None;
        }
        self.delete_notice.as_deref()
    }

    /// True when the chrome row is the board's plain message row rather than the row an
    /// open field edit owns (which keeps its save chord and `Esc` beside whatever it says).
    ///
    /// The renderer and the hit test both read this, so the notice's Undo control can never
    /// be painted where a click would miss it.
    pub fn chrome_row_is_plain(&self) -> bool {
        self.form.is_none()
    }

    /// The field edit the board has open, if any.
    ///
    /// The one answer to "is a field being edited", read by the chrome row, by the compact
    /// Browse edit band ([`super::mouse::BoardLayout::edit_band`]) and by the renderer, so
    /// none of the three can decide differently from the others.
    /// The task an open field edit is bound to, or `None` outside an edit session.
    ///
    /// Read-only, and deliberately so: [`apply_board_intent_with_save_recovery`] needs the bound
    /// identity to judge availability against the durable record before a confirm mutates
    /// anything, and that judgment must never be able to *change* the
    /// binding. Open is still the only writer.
    ///
    /// [`apply_board_intent_with_save_recovery`]: crate::app::apply_board_intent_with_save_recovery
    pub fn edit_target(&self) -> Option<Uuid> {
        self.form.as_ref().and_then(BoardForm::task_id)
    }

    pub fn open_field_edit(&self) -> Option<BoardInputMode> {
        match self.input_mode {
            mode @ (BoardInputMode::EditTitle
            | BoardInputMode::EditNotes
            | BoardInputMode::EditScope) => Some(mode),
            _ => None,
        }
    }

    /// The chrome row an open field edit owns, composed for `width` columns.
    ///
    /// Same order as [`Self::chrome_row`], with two differences the mode forces: the edit's
    /// own legend is never dropped (it is the only statement of the save chord and `Esc` in
    /// Browse), and the notice is stated **without** its `u Undo` control, because `u` types
    /// a `u` into the draft here and there is no hit region on this row. The deletion stays
    /// visible; the route it names comes back with the row when the edit closes.
    /// Seed selection on open: first IN MOTION id, else first ON DECK id.
    pub(super) fn seed_selection(&mut self) {
        let view = self.queue_view();
        if let Some(id) = view
            .sections
            .iter()
            .find(|section| section.kind == SectionKind::InMotion)
            .and_then(|section| section.task_ids.first().copied())
        {
            self.selection_id = Some(id);
            return;
        }
        self.selection_id = view
            .sections
            .iter()
            .filter(|section| section.kind == SectionKind::OnDeck)
            .flat_map(|section| section.task_ids.iter().copied())
            .next();
    }

    /// Re-pin selection after the visible set changes (edit bind wins while still visible).
    pub(super) fn reanchor_selection(&mut self, previous: Option<Uuid>, previous_visible: &[Uuid]) {
        let new_visible = self.visible_ids();
        if self
            .detail_open
            .is_some_and(|id| !new_visible.contains(&id))
        {
            self.close_detail();
        }
        if let Some(bound) = self.edit_target() {
            if new_visible.contains(&bound) {
                self.selection_id = Some(bound);
                return;
            }
        }
        self.selection_id = selection::reanchor(previous, previous_visible, &new_visible);
        // `reanchor` defers when `previous` was `None`; if rows exist again (e.g. CancelSave
        // restored a completed task into the deck), seed rather than leave the pin empty.
        if self.selection_id.is_none() && !new_visible.is_empty() {
            self.seed_selection();
        }
    }

    pub(super) fn move_project_picker(&mut self, forward: bool) {
        let Some(picker) = self.project_picker.as_mut() else {
            return;
        };
        let len = picker.options.len();
        if len == 0 {
            return;
        }
        picker.selected = if forward {
            (picker.selected + 1) % len
        } else {
            picker.selected.checked_sub(1).unwrap_or(len - 1)
        };
    }

    pub(super) fn select_next(&mut self) {
        self.close_popup();
        let ids = self.visible_ids();
        if ids.is_empty() {
            self.selection_id = None;
            return;
        }
        let next = match self
            .selection_id
            .and_then(|id| ids.iter().position(|&row| row == id))
        {
            Some(i) => ids[(i + 1) % ids.len()],
            None => ids[0],
        };
        self.selection_id = Some(next);
    }

    pub(super) fn select_prev(&mut self) {
        self.close_popup();
        let ids = self.visible_ids();
        if ids.is_empty() {
            self.selection_id = None;
            return;
        }
        let prev = match self
            .selection_id
            .and_then(|id| ids.iter().position(|&row| row == id))
        {
            Some(0) | None => ids[ids.len() - 1],
            Some(i) => ids[i - 1],
        };
        self.selection_id = Some(prev);
    }

    pub(super) fn select_index(&mut self, idx: usize) {
        self.close_popup();
        let ids = self.visible_ids();
        if ids.is_empty() {
            self.selection_id = None;
            return;
        }
        if let Some(&id) = ids.get(idx) {
            self.selection_id = Some(id);
        }
    }

    /// Set the session-only project chip selection and narrow ON DECK scope.
    pub fn set_selected_project(&mut self, project: Option<PathBuf>) {
        self.set_deck_scope(match project {
            None => ProjectScopeOption::All,
            Some(path) => ProjectScopeOption::Project(path),
        });
    }

    pub(super) fn set_deck_scope(&mut self, scope: ProjectScopeOption) {
        self.close_popup();
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.deck_scope = match scope {
            ProjectScopeOption::All => OwnedDeckScope::All,
            ProjectScopeOption::Global => OwnedDeckScope::Global,
            ProjectScopeOption::Project(path) => OwnedDeckScope::Project(path),
        };
        self.reanchor_selection(previous, &previous_visible);
    }
}
/// Compact label for one project option; `None` is every project.
///
/// Prefers the last path segment so the Projects header stays readable on narrow panes.
pub fn project_option_label(option: Option<&Path>) -> String {
    match option {
        None => "all projects".to_string(),
        Some(path) => terminal_text(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&path.to_string_lossy()),
        ),
    }
}

pub(super) fn project_scope_option_label(option: &ProjectScopeOption) -> String {
    match option {
        ProjectScopeOption::All => "all projects".to_string(),
        ProjectScopeOption::Global => "global".to_string(),
        ProjectScopeOption::Project(path) => project_option_label(Some(path)),
    }
}

pub(super) fn owned_resource_label(receipt: &OwnedResourceReceipt) -> String {
    match receipt {
        OwnedResourceReceipt::Worktree(receipt) => {
            format!("worktree {}", terminal_text(&receipt.path))
        }
        OwnedResourceReceipt::Pane(receipt) => format!("pane {}", terminal_text(&receipt.pane_id)),
        OwnedResourceReceipt::Agent(receipt) => {
            format!("agent in pane {}", terminal_text(&receipt.pane_id))
        }
    }
}

pub(super) fn owned_resource_summary(attempt: &DispatchAttempt) -> String {
    if attempt.owned_resources().is_empty() {
        "no recorded resources".into()
    } else {
        attempt
            .owned_resources()
            .iter()
            .map(owned_resource_label)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}
