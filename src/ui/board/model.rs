//! Session-only board state, forms, selection, and recovery presentation.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Position;
use uuid::Uuid;

use crate::context::InvocationSnapshot;
use crate::domain::{DomainState, HumanStatus, Task, TaskScope};
use crate::scope::{archived_path_contains, paths_equivalent};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{seeded_draft, EditBuffer};
use crate::ui::input::{
    BOARD_HELP_LINE, COMMAND_SURFACE_HELP_LINE, HELP_SURFACE_HELP_LINE, LAUNCH_CARD_HELP_LINE,
    SAVE_RECOVERY_HELP_LINE,
};
use crate::ui::mouse::BoardPopup;
pub use crate::ui::queue::BoardTab;
use crate::ui::queue::{
    self, visible_task_ids, BoardLens, NavTab, ProjectRow, QueueView, SectionKind, ThreadFilter,
};
use crate::ui::selection;
use crate::ui::terminal_text;
use crate::ui::text_select::TextSelection;
use crate::ui::tier::{FocusedSurface, WideStage};

use super::commands::CommandSurface;

/// Visible board title. Also the herdr pane title.
pub const BOARD_TITLE: &str = "Tasks";

const DIRTY_TASK_SWITCH_REFUSAL: &str = "save or cancel edits before switching tasks";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionRetarget {
    Explicit,
    Reanchor,
}

/// Board chrome input mode (normal list vs field edit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardInputMode {
    #[default]
    Normal,
    EditTitle,
    EditNotes,
    /// The task page footer's optional thread name is selected, ready for Enter or a second
    /// click to enter its text editor without ending the enclosing task edit session.
    SelectThread,
    /// The task page footer's optional thread name owns its text cursor.
    EditThread,
    /// The scope row of an open task form owns focus. The renderer adaptation remains
    /// deliberately thin until, but its keyboard state is a first-class form field.
    EditScope,
    /// A task/capture form's transient scope chooser. It returns to its parent form on Esc.
    FormScopeDropdown,
    /// The launch card owns input: a two-choice modal raised at most once per session
    /// when the invocation default resolved to an archived project.
    LaunchCard,
    /// The task page is open in view mode: the full-page surface shows the bound task and
    /// no field owns the cursor. Verbs act on the task; `e`/`n`/Tab enter field edits. A
    /// click does NOT: field regions are inert in this state, and only move focus once one
    /// of the edit states is already open (see the mouse mapper's form-field arms).
    TaskPage,
    /// A one-line add/rename step draft owns input in its own row in the steps section. It is
    /// a Title-like [`crate::ui::edit::EditBuffer`] carried on the page form's steps state,
    /// not one of the three task-form fields: Enter applies the domain command, Shift+Enter
    /// adds and reopens an empty next row, and Esc cancels. The draft and this mode outlive
    /// the save call until confirmed sync closes or reopens it; every close returns to
    /// [`BoardInputMode::TaskPage`].
    EditStep,
    /// Modal selection over the session project-scope options.
    ProjectPicker,
    /// Board persistence failed; only navigation plus Retry/Cancel are available.
    SaveRecovery,
    /// Searchable command palette is open.
    Palette,
    /// The help card is open.
    Help,
    /// A searchable list picker owns input (project-board thread filter, projects
    /// index View selector). Query typing, movement, Enter applies, Esc cancels.
    ListPicker,
    /// The projects index search field owns text input until Enter or Esc.
    ProjectsSearch,
    /// Single-line status-row capture from board `+`.
    QuickAdd,
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
    /// Which tab the picker shows. Session-only.
    pub(super) tab: PickerTab,
    /// Archived tab entries (sorted scope paths), rebuilt from domain state.
    pub(super) archived: Vec<PathBuf>,
    pub(super) archived_selected: usize,
}

/// Which list the session project selector shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerTab {
    Main,
    Archived,
}

/// One session-only destination offered by the project selector (`P`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectScopeOption {
    /// Home board on the Desk tab.
    Home,
    Project(PathBuf),
}

/// Session-only board location: the three persistent destinations plus the read-only
/// archived focus. Tab digits never shift meaning; slot 2 carries the selected project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BoardLocation {
    /// Tab 1: the global overview plus the desk backlog.
    Desk,
    /// Tab 2: the selected project's board (the old project focus).
    Project(PathBuf),
    /// Tab 3: the projects index, or its cross-project thread View.
    Projects,
    /// Read-only focus on an archived project, opened with Enter from the picker's
    /// archived tab (AC-41). Session-only: leaving it hides those tasks again. It
    /// occupies slot 2 while open.
    ArchivedProject(PathBuf),
}

impl BoardLocation {
    pub(super) fn lens(&self) -> BoardLens<'_> {
        match self {
            Self::Desk => BoardLens::Desk,
            Self::Project(path) => BoardLens::Project(path.as_path()),
            Self::Projects => BoardLens::Projects,
            Self::ArchivedProject(path) => BoardLens::ArchivedProject(path.as_path()),
        }
    }

    pub(super) fn at_home(&self) -> bool {
        matches!(self, Self::Desk | Self::Projects)
    }
}

/// The projects index's View control: the project overview (default) or one
/// cross-project thread's flat task board.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProjectsView {
    #[default]
    Overview,
    Thread(String),
}

/// Which searchable list picker is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListPickerKind {
    /// The project board's thread filter (bare `t`).
    ThreadFilter,
    /// The projects index's View selector (bare `v`).
    ProjectsView,
}

/// One choice inside a searchable list picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPickerOption {
    pub label: String,
    pub count: Option<usize>,
    pub value: ListPickerValue,
}

/// What confirming a list-picker option applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListPickerValue {
    ThreadAll,
    ThreadNamed(String),
    ThreadWithout,
    ProjectsOverview,
    ProjectsThread(String),
}

/// An open searchable list picker (thread filter / projects view). Session-only.
#[derive(Debug, Clone)]
pub(super) struct ListPickerState {
    pub kind: ListPickerKind,
    pub options: Vec<ListPickerOption>,
    pub selected: usize,
    pub query: String,
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

/// One transient board form, shared by quick-add expansion and task editing.
///
/// Both variants own two independent [`EditBuffer`]s, the selected task scope, field focus,
/// and a selection within the scope choices. A task form adds exactly one immutable task id;
/// a quick-add form instead retains exactly one immutable invocation snapshot. Keeping those
/// two mutually exclusive values inside the same form prevents title, Notes, and scope from
/// drifting into separately-bound edits.
#[derive(Debug, Clone)]
pub(super) struct QuickAddState {
    pub(super) title: EditBuffer,
    /// The immutable invocation snapshot, retained for title prefill, token resolution, and
    /// capture provenance.
    pub(super) snapshot: Box<Option<InvocationSnapshot>>,
    /// Current visible destination: the default below, overridden while `!p` tokens are
    /// typed. The input row paints it (`Add to …`).
    pub(super) scope: TaskScope,
    /// The pre-token destination this draft saves to when no `!p` override applies.
    pub(super) default: TaskScope,
}

impl QuickAddState {
    pub(super) fn new(snapshot: Option<InvocationSnapshot>, scope: TaskScope) -> Self {
        let title = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.title_prefill.as_deref())
            .unwrap_or_default();
        Self {
            title: seeded_draft(title),
            snapshot: Box::new(snapshot),
            default: scope.clone(),
            scope,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct QuickAddSave {
    id: Uuid,
    keep_open: bool,
}

/// A task form mutation staged in the domain but not yet confirmed at the persistence boundary.
#[derive(Debug, Clone)]
pub(super) struct TaskEditSave {
    pub(super) id: Uuid,
    pub(super) title: String,
    pub(super) notes: Option<String>,
    pub(super) scope: TaskScope,
    pub(super) thread: Option<String>,
    /// Existing-step names staged alongside the ordinary task fields. They reach the
    /// domain only when the task session is confirmed with Shift+Enter.
    pub(super) step_renames: BTreeMap<Uuid, String>,
    /// Existing steps staged for removal by the task edit session.
    pub(super) step_removals: BTreeSet<Uuid>,
    /// Stable identity of the selected step when the save began. Source indices shift after
    /// removals, so the retained page cursor must reanchor through this id after sync.
    pub(super) selected_step: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub(super) struct BoardForm {
    pub(super) title: EditBuffer,
    pub(super) notes: EditBuffer,
    pub(super) scope: TaskScope,
    /// Optional thread draft shared by task-page and expanded-capture forms.
    pub(super) thread: EditBuffer,
    /// Field-local validation feedback, painted by the footer input rather than status chrome.
    pub(super) thread_refusal: Option<String>,
    /// A task page starts view-only. Entering any field makes its steps selectable and editable
    /// for the rest of that page session; closing the page drops the state with the form.
    pub(super) editing: bool,
    pub(super) focus: CaptureField,
    pub(super) scope_options: Vec<TaskScope>,
    pub(super) scope_selected: usize,
    pub(super) binding: BoardFormBinding,
    /// Editable values as they stood when this task session bound or last saved successfully.
    /// Dirty checks use this immutable baseline rather than the live domain snapshot, which can
    /// advance independently while the user is editing.
    pub(super) task_snapshot: Option<Box<Task>>,
    /// View-mode scroll of the task page's shared notes-and-steps body, never used by capture.
    pub(super) notes_scroll: usize,
    /// Page-session steps state (step cursor, window scroll, delete mark, step
    /// editor). Carried by the form so it lives exactly as long as the page does.
    pub(super) steps: StepsPageState,
    /// The furthest shared-content scroll offset the last painted frame can show.
    ///
    /// The scroll bound depends on the wrap width, which only the renderer knows: the model
    /// is deliberately geometry-free and the render path takes `&BoardModel`. Bounding the
    /// intent by `notes.value().lines().count()` instead was wrong in the reachable
    /// direction, because wrapping only ever ADDS rows, so the tail of any wrapping note
    /// was unreachable while the divider still advertised it. A `Cell` lets the immutable
    /// renderer record what it actually laid out, without making the whole render path `&mut`
    /// or pushing geometry into every scroll intent.
    pub(super) notes_max_scroll: std::cell::Cell<usize>,
    /// The notes wrap width the last painted frame used, recorded by the immutable
    /// renderer for the same reason as `notes_max_scroll`: vertical arrow movement
    /// wraps at the painted width, which only the renderer knows.
    pub(super) notes_width: std::cell::Cell<usize>,
}

impl BoardForm {
    pub(super) fn task(
        task: &Task,
        this_repo: Option<&Path>,
        tasks: &[Task],
        focus: CaptureField,
        archived: &BTreeSet<String>,
    ) -> Self {
        let mut form = Self::new(
            &task.title,
            task.notes.as_deref().unwrap_or_default(),
            task.scope.clone(),
            focus,
            BoardFormBinding::Task(task.id),
            this_repo,
            tasks,
            archived,
        );
        form.thread = seeded_draft(task.thread.as_deref().unwrap_or_default());
        form.task_snapshot = Some(Box::new(task.clone()));
        form
    }

    pub(super) fn capture(
        snapshot: Option<InvocationSnapshot>,
        this_repo: Option<&Path>,
        tasks: &[Task],
        archived: &BTreeSet<String>,
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
            archived,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        title: &str,
        notes: &str,
        scope: TaskScope,
        focus: CaptureField,
        binding: BoardFormBinding,
        this_repo: Option<&Path>,
        tasks: &[Task],
        archived: &BTreeSet<String>,
    ) -> Self {
        let scope_options = board_form_scope_options(&scope, this_repo, tasks, archived);
        let scope_selected = scope_options
            .iter()
            .position(|option| option == &scope)
            .unwrap_or(0);
        Self {
            title: seeded_draft(title),
            notes: seeded_draft(notes),
            scope,
            thread: seeded_draft(""),
            thread_refusal: None,
            editing: false,
            focus,
            scope_options,
            scope_selected,
            binding,
            task_snapshot: None,
            notes_scroll: 0,
            steps: StepsPageState::default(),
            notes_max_scroll: std::cell::Cell::new(0),
            notes_width: std::cell::Cell::new(0),
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
        match self.focus {
            CaptureField::Title => BoardInputMode::EditTitle,
            CaptureField::Notes => BoardInputMode::EditNotes,
            // Task-page Thread is first a selected footer control. Capture has no separate
            // selected state, so it continues directly into its text input.
            CaptureField::Thread if self.is_task() => BoardInputMode::SelectThread,
            CaptureField::Thread => BoardInputMode::EditThread,
            CaptureField::Scope => BoardInputMode::EditScope,
        }
    }

    /// Restore one field's draft to its bound task's saved value (Esc on the task page).
    pub(super) fn reset_field_to_saved(&mut self, field: CaptureField, task: &Task) {
        match field {
            CaptureField::Title => self.title = seeded_draft(&task.title),
            CaptureField::Notes => {
                self.notes = seeded_draft(task.notes.as_deref().unwrap_or_default());
            }
            CaptureField::Thread => {
                self.thread = seeded_draft(task.thread.as_deref().unwrap_or_default());
                self.thread_refusal = None;
            }
            CaptureField::Scope => {
                self.scope = task.scope.clone();
                self.select_current_scope();
            }
        }
        if let Some(snapshot) = self.task_snapshot.as_mut() {
            match field {
                CaptureField::Title => snapshot.title = task.title.clone(),
                CaptureField::Notes => snapshot.notes = task.notes.clone(),
                CaptureField::Thread => snapshot.thread = task.thread.clone(),
                CaptureField::Scope => snapshot.scope = task.scope.clone(),
            }
        }
    }

    pub(super) fn focus_next(&mut self) {
        self.focus = match self.focus {
            CaptureField::Title => CaptureField::Notes,
            CaptureField::Notes => CaptureField::Thread,
            CaptureField::Thread => CaptureField::Scope,
            CaptureField::Scope => CaptureField::Title,
        };
    }

    pub(super) fn focus_prev(&mut self) {
        self.focus = match self.focus {
            CaptureField::Title => CaptureField::Scope,
            CaptureField::Notes => CaptureField::Title,
            CaptureField::Thread => CaptureField::Notes,
            CaptureField::Scope => CaptureField::Thread,
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

/// The in-place add/rename step input in the task page's steps section (page-session only).
///
/// `rename` names the step being edited; `None` is an add (the buffer starts empty).
/// `refusal` is the line's own empty-text refusal (AC-13): painted on the line,
/// never the board status row, and cleared when the line closes or its buffer
/// changes.
#[derive(Debug, Clone)]
pub(super) struct StepEditor {
    pub(super) buffer: EditBuffer,
    pub(super) rename: Option<Uuid>,
    pub(super) refusal: Option<String>,
}

/// An step-editor apply waiting for the app save boundary to confirm persistence
/// (AC-14). `step` + `text` name the mutation that must land before the line may
/// close — or, for Shift+Enter in add mode, reopen empty.
#[derive(Debug, Clone)]
pub(super) struct StepEditorSave {
    pub(super) step: Uuid,
    pub(super) text: String,
    pub(super) reopen: bool,
    /// Leave the task-edit session after this add lands (Shift+Enter on a new step).
    pub(super) exit_editing: bool,
}

/// Page-session steps state for the task page, never persisted.
///
/// The step cursor lifecycle (spec: resolved decisions, amended 2026-08-22):
/// inactive when the page opens; a bare ↓ (re-)activates it on the first step —
/// including after an earlier ↑-deactivation, so activation is never one-shot;
/// ↑ from the first step deactivates it. While inactive ↑ scrolls the notes and
/// ↓ re-activates; a click on an step row moves the cursor onto that step
/// (AC-21). A task with no steps never activates a cursor.
#[derive(Debug, Clone, Default)]
pub(super) struct StepsPageState {
    /// Highlighted stored step index; `None` = inactive or the trailing add target.
    pub(super) cursor: Option<usize>,
    /// The trailing `+ step` target is selected instead of a stored step.
    pub(super) add_selected: bool,
    /// Absolute content row of the steps label, recorded by the renderer so cursor
    /// movement can keep its selected step inside the shared viewport.
    pub(super) content_start: std::cell::Cell<usize>,
    /// Step index visibly marked by the first press of the delete verb. Any intervening
    /// intent clears it; only the verb's second press removes.
    pub(super) delete_mark: Option<usize>,
    /// The open in-place add/rename editor, if any.
    pub(super) editor: Option<StepEditor>,
    /// Existing-step text staged by the task edit session. Leaving an inline rename parks its
    /// buffer here, so another field or step can receive the one terminal cursor without
    /// committing either change.
    pub(super) drafts: BTreeMap<Uuid, EditBuffer>,
    /// Existing steps removed only when the enclosing task form saves. Esc drops this set.
    pub(super) removals: BTreeSet<Uuid>,
    /// Capture-form steps staged until the create saves. Task pages persist adds immediately.
    pub(super) pending_adds: Vec<String>,
    /// An editor apply the save boundary has not confirmed yet (AC-14). While it is
    /// set, the editor and its input mode are held exactly as the user left them.
    pub(super) pending_save: Option<StepEditorSave>,
    /// Shared-content rows the last painted viewport showed (renderer-recorded).
    pub(super) window_rows: std::cell::Cell<usize>,
    /// Wrapped row count for each stored step at the last painted width. The renderer records
    /// it so keyboard movement scrolls to the selected step's first wrapped row.
    pub(super) row_counts: std::cell::RefCell<Vec<usize>>,
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
    archived: &BTreeSet<String>,
) -> Vec<TaskScope> {
    let mut options = Vec::new();
    let push = |scope: TaskScope, options: &mut Vec<TaskScope>| {
        if !options.contains(&scope) {
            options.push(scope);
        }
    };
    // AC-39: no dropdown offers an archived project. The form's own initial scope is
    // exempt: a task already sitting in an archived project keeps it as its value.
    let filed = |scope: &TaskScope| match scope {
        TaskScope::Project { path } => archived_path_contains(archived, path),
        TaskScope::Global => false,
    };
    push(initial_scope.clone(), &mut options);
    if let Some(repo) = this_repo {
        let scope = TaskScope::Project {
            path: repo.to_string_lossy().into_owned(),
        };
        if !filed(&scope) {
            push(scope, &mut options);
        }
    }
    for task in tasks {
        if !task.soft_deleted {
            if let TaskScope::Project { path } = &task.scope {
                let scope = TaskScope::Project { path: path.clone() };
                if !filed(&scope) {
                    push(scope, &mut options);
                }
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
    /// Scope paths of archived projects, carried from domain state so the lens
    /// query can filter hidden tasks without re-deriving from records.
    pub(super) archived_projects: BTreeSet<String>,
    pub(super) this_repo: Option<PathBuf>,
    /// Session board location (active surface). Not durable.
    pub(super) board_location: BoardLocation,
    /// Slot 2 identity, independent from the active surface (for example after switching to
    /// Desk or the Projects index). Not durable.
    pub(super) selected_project: Option<PathBuf>,
    /// Wide-slider stage. Focus and single-pane presentation derive from it. Session-only.
    pub(super) wide_stage: WideStage,
    /// Stage `Enter` (or a row double-click) left for the full task page; `Esc` returns there.
    pub(super) stage_origin: Option<WideStage>,
    /// The project board's session thread filter. It narrows every status section and
    /// is cleared whenever the selected project changes.
    pub(super) thread_filter: ThreadFilter,
    /// The projects index's View: the project overview, or one cross-project thread.
    pub(super) projects_view: ProjectsView,
    /// The projects index's search query. Session-only.
    pub(super) projects_query: String,
    /// The projects index's selected row cursor. Session-only.
    pub(super) projects_selected: usize,
    /// Open searchable list picker (thread filter / projects view). Session-only.
    pub(super) list_picker: Option<ListPickerState>,
    /// Whether the done drawer lists completed tasks. Session-only.
    pub(super) drawer_open: bool,
    /// The done drawer's archived group starts collapsed on every launch. Session-only.
    pub(super) archived_collapsed: bool,
    /// The archived project the launch card names, while the card is up.
    pub(super) launch_card: Option<PathBuf>,
    /// The launch card is shown at most once per session, whichever choice was made.
    pub(super) launch_card_shown: bool,
    /// Session quick-add default override (desk after "keep archived"). Session-only.
    pub(super) session_default_scope: Option<TaskScope>,
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
    /// The last projects-index row click (time + row path), kept only to detect a
    /// double-click that opens the project in slot 2. Presentation-only, never persisted.
    pub(super) last_project_row_click: Option<(Instant, PathBuf)>,
    /// First visible row of the help card's key list. Session-only, reset on open.
    pub(super) help_scroll: usize,
    /// Furthest help scroll the last painted card could show (renderer-recorded).
    pub(super) help_max_scroll: Cell<usize>,
    pub(super) input_mode: BoardInputMode,
    /// The one active board form. It is present for expanded quick-add and task editing alike;
    /// task identity or invocation context are held inside it and never rebound after open.
    pub(super) form: Option<BoardForm>,
    /// The status-row quick-add draft. It remains while its expanded form is open so Esc can
    /// return to the line without reconstructing its snapshot or cursor state.
    pub(super) quick_add: Option<QuickAddState>,
    /// Newly-created task briefly emphasized until the next input intent.
    pub(super) saved_task: Option<Uuid>,
    /// A quick-add create waiting for the app save boundary to confirm persistence.
    pub(super) quick_add_save: Option<QuickAddSave>,
    /// A task-form edit waiting for the app save boundary to confirm persistence.
    pub(super) task_edit_save: Option<TaskEditSave>,
    /// The app save boundary holds task-form release across its inner reducer sync.
    pub(super) hold_task_edit_save: bool,
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
    /// First ctrl+x on a task arms this id; a second press on the same task deletes it.
    pub(super) pending_delete: Option<Uuid>,
    /// Open project-picker or save-recovery presentation.
    pub(super) popup: BoardPopup,
    /// Open session project selector; never persisted.
    pub(super) project_picker: Option<ProjectPickerState>,
    /// Open palette (presentation only).
    pub(super) surface: CommandSurface,
    /// The last left-button press cell, held until release so a drag can grow a text
    /// selection out of it. Presentation-only; a plain click never reads it.
    pub(super) mouse_press: Option<Position>,
    /// `list_scroll` / notes scroll at the press, so the anchor stays on that content
    /// row while edge auto-scroll moves the viewport.
    pub(super) mouse_press_scroll: Option<usize>,
    /// List viewport offset. Scrollbar click/drag writes it; row click leaves it.
    pub(super) list_scroll: Cell<usize>,
    /// When true, the next paint nudges `list_scroll` so the selection is on screen
    /// (keyboard / reanchor). Row click and scrollbar leave it false.
    pub(super) follow_list: Cell<bool>,
    /// The live drag-selected screen region, cleared on the next press.
    pub(super) text_selection: Option<TextSelection>,
    /// Furthest list scroll the last painted frame could show (renderer-recorded).
    pub(super) list_max_scroll: Cell<usize>,
    /// When set, [`Self::message`] clears itself on the next idle tick after this instant.
    /// Sticky messages (errors, delete notices) leave this `None`.
    pub(super) message_expires_at: Option<Instant>,
    /// Sticky status restored when an ephemeral toast expires (e.g. save-recovery banner
    /// under a brief `copied` notice). Cleared by [`Self::set_message`] / [`Self::clear_message`].
    pub(super) message_restore: Option<String>,
    /// Palette query.
    pub(super) command_query: String,
    /// Selection into the currently visible command set.
    pub(super) command_selected: usize,
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
    /// Build a board model with queue-local session state only. The desk is the
    /// default destination; [`Self::from_domain`] applies directory-aware startup.
    pub fn from_tasks(tasks: Vec<Task>, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self {
            tasks,
            archived_projects: BTreeSet::new(),
            this_repo: this_repo.clone(),
            board_location: BoardLocation::Desk,
            selected_project: this_repo,
            wide_stage: WideStage::FullBoard,
            stage_origin: None,
            thread_filter: ThreadFilter::All,
            projects_view: ProjectsView::Overview,
            projects_query: String::new(),
            projects_selected: 0,
            list_picker: None,
            drawer_open: false,
            archived_collapsed: true,
            launch_card: None,
            launch_card_shown: false,
            session_default_scope: None,
            detail_open: None,
            selection_id: None,
            last_row_click: None,
            last_project_header_click: None,
            last_project_row_click: None,
            help_scroll: 0,
            help_max_scroll: Cell::new(usize::MAX),
            input_mode: BoardInputMode::Normal,
            form: None,
            quick_add: None,
            saved_task: None,
            quick_add_save: None,
            task_edit_save: None,
            hold_task_edit_save: false,
            message: None,
            delete_notice: None,
            suspended_delete_notice: None,
            pending_delete: None,
            popup: BoardPopup::None,
            project_picker: None,
            surface: CommandSurface::None,
            command_query: String::new(),
            command_selected: 0,
            mouse_press: None,
            mouse_press_scroll: None,
            list_scroll: Cell::new(0),
            follow_list: Cell::new(true),
            text_selection: None,
            list_max_scroll: Cell::new(0),
            message_expires_at: None,
            message_restore: None,
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
    pub fn end_save_recovery(&mut self, resolution: SaveResolution) -> bool {
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
        let cancelled_quick_add = resolution == SaveResolution::Cancelled
            && self.quick_add_save.take().is_some()
            && self.quick_add.is_some();
        if cancelled_quick_add {
            // The retained capture form is a stash, not an active page. Return its line to
            // normal keyboard routing and remove the failure chrome that line would cover.
            self.input_mode = BoardInputMode::QuickAdd;
            self.clear_message();
        }
        // The restored baseline discards the staged task edit, but its task-page form remains
        // open in view mode with every draft reset to the durable task.
        if resolution == SaveResolution::Cancelled {
            if let Some(pending) = self.task_edit_save.take() {
                let task = self
                    .tasks
                    .iter()
                    .find(|task| task.id == pending.id)
                    .cloned();
                if let (Some(form), Some(task)) = (self.form.as_mut(), task) {
                    form.title = seeded_draft(&task.title);
                    form.notes = seeded_draft(task.notes.as_deref().unwrap_or_default());
                    form.thread = seeded_draft(task.thread.as_deref().unwrap_or_default());
                    form.thread_refusal = None;
                    form.scope = task.scope.clone();
                    form.select_current_scope();
                    form.editing = false;
                    form.steps.editor = None;
                    form.steps.pending_save = None;
                    form.steps.drafts.clear();
                    form.steps.removals.clear();
                    form.steps.add_selected = false;
                    form.steps.delete_mark = None;
                    form.task_snapshot = Some(Box::new(task));
                }
                self.hold_task_edit_save = false;
                self.input_mode = BoardInputMode::TaskPage;
                self.clear_message();
            }
        }
        // A held inline step editor unwinds to page view on Cancel: the restored baseline
        // rolled its mutation back, so nothing is left to hold the row for, and an edit mode
        // whose editor is gone is the orphan no key can escape. The Cancel path's
        // `sync_from_domain` ran first and left
        // the pending save unresolved precisely because the mutation is not in the
        // baseline; Retried resolves it there instead and never reaches this branch.
        if resolution == SaveResolution::Cancelled
            && self
                .form
                .as_ref()
                .is_some_and(|form| form.steps.pending_save.is_some())
        {
            if let Some(form) = self.form.as_mut() {
                form.steps.pending_save = None;
                form.steps.editor = None;
            }
            self.input_mode = BoardInputMode::TaskPage;
            self.clear_message();
        }
        cancelled_quick_add
    }

    /// Snapshot tasks from domain state (default agent kind; seed env at open).
    ///
    /// Directory-aware startup: launching inside a live repository opens that
    /// project's board, empty or not (an empty board paints its own add hint).
    /// Launching anywhere else opens the desk. An archived invocation repository is
    /// handled by [`Self::offer_launch_card`] instead and stays on the desk.
    pub fn from_domain(state: &DomainState, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self::from_tasks(state.tasks().to_vec(), this_repo);
        model.archived_projects = state.archived_projects();
        if let Some(repo) = model.this_repo.clone() {
            if !model.is_archived_project_path(&repo) {
                let previous_visible = model.visible_ids();
                model.board_location = BoardLocation::Project(repo);
                model.reanchor_selection(None, &previous_visible);
                model.seed_selection();
            } else {
                model.selected_project = None;
            }
        }
        model
    }

    /// Raise the two-choice launch card when the invocation default resolves to an
    /// archived project, at most once per session. Returns whether the card was raised.
    pub fn offer_launch_card(
        &mut self,
        state: &DomainState,
        snapshot: &InvocationSnapshot,
    ) -> bool {
        if self.launch_card_shown {
            return false;
        }
        let TaskScope::Project { path } = &snapshot.default_scope else {
            return false;
        };
        if !state.is_project_archived(path) {
            return false;
        }
        self.launch_card_shown = true;
        self.launch_card = Some(PathBuf::from(path));
        self.popup = BoardPopup::LaunchCard;
        true
    }

    /// The one hidden predicate every working-lens surface filters on.
    fn is_hidden(&self, task: &Task) -> bool {
        if task.archived {
            return true;
        }
        match &task.scope {
            TaskScope::Global => false,
            TaskScope::Project { path } => archived_path_contains(&self.archived_projects, path),
        }
    }

    /// Navigation is never yanked by saves or background merges: a save reanchors the
    /// pin only when the current destination already renders the saved task, and no
    /// path switches the board to another project merely to reveal a row.
    ///
    /// Replace task snapshot from domain (after mutation) and reanchor selection by id.
    pub fn sync_from_domain(&mut self, state: &DomainState) {
        // Capture the prior visible order before the snapshot is replaced, so reanchoring
        // can still see where the selection used to live.
        let previous = self.selection_id;
        let previous_visible = self.visible_ids();
        let previous_id_set: HashSet<Uuid> = self.tasks.iter().map(|task| task.id).collect();
        self.tasks = state.tasks().to_vec();
        self.archived_projects = state.archived_projects();
        let new_ids: Vec<Uuid> = self
            .tasks
            .iter()
            .filter(|task| !previous_id_set.contains(&task.id))
            .map(|task| task.id)
            .collect();
        // A merge (or this board's own picker verb) may archive the project slot 2 is
        // showing: reset the destination to the desk and name the project on the status
        // row. The quick-add default is guarded at `OpenCapture`, which never resolves to
        // an archived project, so the session default is left alone here.
        // A read-only focus whose project came back (picker, CLI, or a sibling process)
        // becomes an ordinary project board: tab, dim rows and verbs all follow.
        if let BoardLocation::ArchivedProject(path) = &self.board_location {
            if !self.is_archived_project_path(path) {
                self.selected_project = Some(path.clone());
                self.board_location = BoardLocation::Project(path.clone());
            }
        }
        let focus_archived = match &self.board_location {
            BoardLocation::Project(path) => self.is_archived_project_path(path),
            // A read-only focus is deliberately on an archived project (AC-41): it is
            // not the accident this reset exists for.
            BoardLocation::ArchivedProject(_) | BoardLocation::Desk | BoardLocation::Projects => {
                false
            }
        };
        if focus_archived {
            let name = match &self.board_location {
                BoardLocation::Project(path) => {
                    crate::ui::render::short_project(&path.to_string_lossy()).to_string()
                }
                _ => String::new(),
            };
            self.board_location = BoardLocation::Desk;
            self.selected_project = None;
            self.set_message(format!("project {name} is archived"));
        }
        let pinned_edit = self.task_edit_save.as_ref().map(|pending| pending.id);
        let pinned_quick_add = self.quick_add_save.as_ref().map(|pending| pending.id);
        self.finish_quick_add_save();
        self.finish_task_edit_save();
        self.finish_step_editor_save();
        if let Some(id) = pinned_edit.or(pinned_quick_add) {
            // A save this surface just made owns the selection, but navigation never
            // follows it: the pin moves only when the current destination already
            // renders the saved task, otherwise it anchors on the saved id's old
            // position (or the prior pin) so nothing jumps to the first row.
            if self.visible_ids().contains(&id) {
                self.retarget_selection(Some(id), SelectionRetarget::Reanchor);
                self.follow_list.set(true);
            } else {
                let anchor = if previous_visible.contains(&id) {
                    Some(id)
                } else {
                    previous
                };
                self.reanchor_selection(anchor, &previous_visible);
            }
        } else {
            // Tasks merged in from disk are somebody else's work: never move the
            // board's destination or selection toward them. An otherwise-empty view
            // may surface the first one, but only when the current destination already
            // renders it — the desk's global lanes usually do.
            if previous_visible.is_empty() {
                if let Some(id) = new_ids.first() {
                    if self.visible_ids().contains(id) {
                        self.retarget_selection(Some(*id), SelectionRetarget::Reanchor);
                        self.follow_list.set(true);
                    }
                }
            }
            self.reanchor_selection(previous, &previous_visible);
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

    /// Whether the persistent tabs read as "home" surfaces (quick-add scope defaults).
    pub fn at_home(&self) -> bool {
        self.board_location.at_home()
    }

    /// Which navigation tab is active right now.
    pub fn nav_tab(&self) -> NavTab {
        match &self.board_location {
            BoardLocation::Desk => NavTab::Desk,
            BoardLocation::Project(_) | BoardLocation::ArchivedProject(_) => NavTab::ProjectBoard,
            BoardLocation::Projects => NavTab::Projects,
        }
    }

    /// Switch to a navigation tab. `NavTab::ProjectBoard` with no selected project is
    /// answered by the caller (the project picker opens instead — the slot never
    /// changes meaning). Returns whether the board moved.
    pub(super) fn select_nav_tab(&mut self, tab: NavTab) -> bool {
        let target = match tab {
            NavTab::Desk => BoardLocation::Desk,
            NavTab::Projects => BoardLocation::Projects,
            NavTab::ProjectBoard => {
                let Some(path) = self.selected_project.clone() else {
                    return false;
                };
                BoardLocation::Project(path)
            }
        };
        if self.board_location == target {
            return false;
        }
        // AC-45: `1`/`3` from the read-only archived focus leave it, which hides that
        // project's tasks again. (`2` stays put: the archived focus already occupies
        // slot 2.)
        if self.focus_is_archived() {
            let previous_visible = self.visible_ids();
            self.board_location = target;
            self.reanchor_selection(None, &previous_visible);
            self.seed_selection();
            return true;
        }
        self.switch_location(target);
        true
    }

    /// Move to `target` through the one shared guarded transition: reanchor by the
    /// previous visible order, clear a project-scoped thread filter when the project
    /// changes, and never touch open forms or save-recovery state.
    pub(super) fn switch_location(&mut self, target: BoardLocation) {
        if self.board_location == target {
            return;
        }
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        let same_project = matches!(
            (&self.board_location, &target),
            (BoardLocation::Project(a), BoardLocation::Project(b))
                if paths_equivalent(&a.to_string_lossy(), &b.to_string_lossy())
        );
        if same_project {
            // Equivalent spellings are the same stored project identity. Do not rewrite the
            // selected path merely because a host supplied an alias, but an explicit reopen
            // still clears its local thread filter.
            self.thread_filter = ThreadFilter::All;
            return;
        }
        self.thread_filter = ThreadFilter::All;
        self.board_location = target;
        self.reanchor_selection(previous, &previous_visible);
    }

    /// The selected project scope carried by slot 2, independent of the active surface.
    pub fn selected_project(&self) -> Option<&Path> {
        self.selected_project.as_deref()
    }

    pub fn active_project(&self) -> Option<&Path> {
        match &self.board_location {
            BoardLocation::Project(path) | BoardLocation::ArchivedProject(path) => {
                Some(path.as_path())
            }
            BoardLocation::Desk | BoardLocation::Projects => None,
        }
    }

    fn is_archived_project_path(&self, path: &Path) -> bool {
        let path = path.to_string_lossy();
        archived_path_contains(&self.archived_projects, &path)
    }

    /// Whether changing invocation context would discard or hide an unsaved draft.
    /// Reopen requests defer while any editor or non-empty quick-add owns the user's text.
    pub fn has_unsaved_work(&self) -> bool {
        if self.task_session_dirty()
            || self.quick_add_save.is_some()
            || self.task_edit_save.is_some()
        {
            return true;
        }
        if self.form.is_some() && self.input_mode != BoardInputMode::TaskPage {
            return true;
        }
        self.quick_add
            .as_ref()
            .is_some_and(|quick_add| !quick_add.title.value().is_empty())
    }

    /// Clear non-dirty presentation layers so a context switch lands on the target board,
    /// rather than leaving a clean task page, picker, search, or empty quick-add in front of it.
    fn dismiss_clean_surfaces_for_reopen(&mut self) {
        self.detail_open = None;
        self.wide_stage = WideStage::FullBoard;
        self.stage_origin = None;
        self.list_picker = None;
        self.project_picker = None;
        self.popup = BoardPopup::None;
        self.surface = CommandSurface::None;
        self.command_query.clear();
        self.command_selected = 0;
        self.projects_query.clear();
        self.projects_selected = 0;
        self.quick_add = None;
        self.form = None;
        self.input_mode = BoardInputMode::Normal;
        self.drawer_open = false;
        self.text_selection = None;
        self.last_row_click = None;
        self.last_project_header_click = None;
        self.last_project_row_click = None;
    }

    /// Apply an explicit reopen context without changing task ownership. A dirty editor is
    /// left untouched and the request remains pending for the caller to retry after save/cancel.
    /// Archived aliases are checked before switching so path spelling cannot bypass read-only
    /// project protections.
    pub fn apply_reopen_project(&mut self, project: Option<PathBuf>) -> bool {
        if self.has_unsaved_work() {
            self.set_message("save or cancel edits before reopening tsk");
            return false;
        }
        self.dismiss_clean_surfaces_for_reopen();
        let Some(project) = project else {
            self.this_repo = None;
            self.selected_project = None;
            self.session_default_scope = Some(TaskScope::Global);
            self.switch_location(BoardLocation::Desk);
            self.clear_message();
            return true;
        };
        let project_text = project.to_string_lossy();
        if self.is_archived_project_path(&project) {
            let name = crate::ui::render::short_project(&project_text).to_string();
            self.this_repo = None;
            self.selected_project = None;
            self.session_default_scope = Some(TaskScope::Global);
            self.switch_location(BoardLocation::Desk);
            self.set_message(format!("project {name} is archived"));
            return true;
        }
        let same = matches!(&self.board_location, BoardLocation::Project(current) if paths_equivalent(&current.to_string_lossy(), &project_text));
        self.this_repo = Some(project.clone());
        self.selected_project = Some(project.clone());
        self.session_default_scope = Some(TaskScope::Project {
            path: project.to_string_lossy().into_owned(),
        });
        if same {
            self.thread_filter = ThreadFilter::All;
        } else {
            self.switch_location(BoardLocation::Project(project));
        }
        self.clear_message();
        true
    }

    /// True while the board is in the read-only focus on an archived project (AC-41).
    pub fn focus_is_archived(&self) -> bool {
        matches!(self.board_location, BoardLocation::ArchivedProject(_))
    }

    /// The archived project this session is focused on, if any.
    pub fn archived_focus(&self) -> Option<&Path> {
        match &self.board_location {
            BoardLocation::ArchivedProject(path) => Some(path.as_path()),
            _ => None,
        }
    }

    /// The refusal every mutating verb paints in read-only focus (AC-42).
    pub(super) fn archived_focus_refusal(&self) -> Option<String> {
        let path = self.archived_focus()?;
        let path = path.to_string_lossy().into_owned();
        let name = crate::ui::render::short_project(&path);
        Some(format!(
            "project {name} is archived \u{b7} ctrl+u unarchive"
        ))
    }

    /// Leave the read-only archived focus for the desk (AC-45).
    pub(super) fn leave_archived_focus(&mut self) {
        let previous_visible = self.visible_ids();
        self.board_location = BoardLocation::Desk;
        self.reanchor_selection(None, &previous_visible);
        self.seed_selection();
        self.clear_message();
    }

    /// Turn a read-only focus into the ordinary project focus on the same project
    /// (AC-43), keeping the selection where the user left it.
    pub(super) fn enter_project_focus(&mut self, path: PathBuf) {
        self.selected_project = Some(path.clone());
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = BoardLocation::Project(path);
        self.reanchor_selection(previous, &previous_visible);
    }

    /// Open the read-only focus on `path` (AC-41). Session-only: nothing persists.
    pub(super) fn open_archived_focus(&mut self, path: PathBuf) {
        self.selected_project = None;
        self.close_popup();
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = BoardLocation::ArchivedProject(path);
        self.reanchor_selection(previous, &previous_visible);
        if self.selection_id.is_none() {
            self.seed_selection();
        }
    }

    /// The desk and the index use the invocation/session default; slot 2 defaults to
    /// its own project.
    pub(super) fn quick_add_scope(&self) -> Option<TaskScope> {
        match &self.board_location {
            BoardLocation::Desk | BoardLocation::Projects => self.session_default_scope.clone(),
            // Quick-add is refused outright in read-only focus, so its scope is moot.
            BoardLocation::Project(path) | BoardLocation::ArchivedProject(path) => {
                Some(TaskScope::Project {
                    path: path.to_string_lossy().into_owned(),
                })
            }
        }
    }

    /// The active destination's right-side control kind, when one is painted.
    pub fn nav_chip_kind(&self) -> Option<crate::ui::render::NavChipKind> {
        Some(match (&self.board_location, self.projects_view()) {
            (BoardLocation::Project(_), _) => crate::ui::render::NavChipKind::ThreadFilter,
            (BoardLocation::Projects, _) => crate::ui::render::NavChipKind::ProjectsView,
            _ => return None,
        })
    }

    /// The searchable list picker is open (thread filter / projects View).
    pub fn list_picker_open(&self) -> bool {
        self.list_picker.is_some()
    }

    /// The open list picker's kind, when one is open.
    pub fn list_picker_kind(&self) -> Option<ListPickerKind> {
        self.list_picker.as_ref().map(|picker| picker.kind)
    }

    pub fn list_picker_query(&self) -> Option<&str> {
        self.list_picker
            .as_ref()
            .map(|picker| picker.query.as_str())
    }

    /// Cancel an armed project-header or index-row double-click when another pointer
    /// target intervenes. Mouse-boundary state only, never persisted.
    pub(crate) fn cancel_project_header_double_click(&mut self) {
        self.last_project_header_click = None;
        self.last_project_row_click = None;
    }

    /// Record a left-button press cell and drop any finished selection's highlight.
    ///
    /// The press cell is what a following drag grows the selection from; a plain
    /// click never reads it. Called for every left press, whatever the input mode.
    pub fn begin_mouse_press(&mut self, position: Position) {
        self.mouse_press = Some(position);
        self.mouse_press_scroll = Some(self.content_scroll());
        self.text_selection = None;
    }

    fn content_scroll(&self) -> usize {
        if self.focused_surface() == FocusedSurface::Task {
            self.form
                .as_ref()
                .filter(|form| form.is_task())
                .map(|form| form.notes_scroll)
                .unwrap_or(0)
        } else {
            self.list_scroll.get()
        }
    }

    fn content_relative_anchor(&self, press: Position) -> Position {
        let Some(press_scroll) = self.mouse_press_scroll else {
            return press;
        };
        let dy = self.content_scroll() as i32 - press_scroll as i32;
        let y = if dy >= 0 {
            press.y.saturating_sub(dy as u16)
        } else {
            press.y.saturating_add(dy.unsigned_abs() as u16)
        };
        Position::new(press.x, y)
    }

    /// Extend the drag selection to `position`, anchored at the press cell.
    ///
    /// Inert without a live press (a drag that starts mid-gesture, e.g. before tsk
    /// saw the press), so it can never invent an anchor.
    pub fn drag_text_selection(&mut self, position: Position) {
        if let Some(press) = self.mouse_press {
            let anchor = self.content_relative_anchor(press);
            self.text_selection = Some(TextSelection::new(anchor, position));
        }
    }

    /// Recompute the live highlight after the viewport scrolled under a held drag.
    pub fn recompute_text_selection_head(&mut self, head: Position) {
        self.drag_text_selection(head);
    }

    /// Clear the press on release; the selection itself stays until copy clears it
    /// or the next press replaces it.
    pub fn end_mouse_press(&mut self) {
        self.mouse_press = None;
        self.mouse_press_scroll = None;
    }

    /// Drop a finished text selection highlight (after copy, Esc, or cancel).
    pub fn clear_text_selection(&mut self) {
        self.text_selection = None;
    }

    /// The live drag selection, if a drag is (or was) in progress.
    pub fn text_selection(&self) -> Option<TextSelection> {
        self.text_selection
    }

    /// Scroll the board list by `rows` without moving the pinned selection.
    ///
    /// Used by drag edge auto-scroll so a text drag near the viewport edge can
    /// reveal more of the deck. Clamped to the last painted max scroll.
    pub fn nudge_list_scroll(
        &self,
        direction: crate::ui::text_select::AutoScrollDirection,
        rows: u16,
    ) -> usize {
        use crate::ui::text_select::AutoScrollDirection;
        self.follow_list.set(false);
        let max = self.list_max_scroll.get();
        let cur = self.list_scroll.get();
        let next = match direction {
            AutoScrollDirection::Up => cur.saturating_sub(rows as usize),
            AutoScrollDirection::Down => cur.saturating_add(rows as usize).min(max),
        };
        self.list_scroll.set(next);
        cur.abs_diff(next)
    }

    /// Scroll the open task page's shared notes body by `rows` (view mode only).
    pub fn nudge_notes_scroll(
        &mut self,
        direction: crate::ui::text_select::AutoScrollDirection,
        rows: u16,
    ) -> usize {
        use crate::ui::text_select::AutoScrollDirection;
        let Some(form) = self.form.as_mut() else {
            return 0;
        };
        let horizon = form.notes_max_scroll.get();
        let cur = form.notes_scroll;
        form.notes_scroll = match direction {
            AutoScrollDirection::Up => form.notes_scroll.saturating_sub(rows as usize),
            AutoScrollDirection::Down => {
                form.notes_scroll.saturating_add(rows as usize).min(horizon)
            }
        };
        cur.abs_diff(form.notes_scroll)
    }

    /// Options the session project selector offers, in presentation order.
    ///
    /// Home and desk are always available. Project entries are paths that really
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
            if !self.is_archived_project_path(&repo) {
                push(repo, &mut paths);
            }
        }
        let mut from_tasks: Vec<PathBuf> = Vec::new();
        for task in &self.tasks {
            if task.soft_deleted {
                continue;
            }
            if let TaskScope::Project { path } = &task.scope {
                if self.is_archived_project_path(Path::new(path)) {
                    continue;
                }
                push(PathBuf::from(path), &mut from_tasks);
            }
        }
        from_tasks.sort();
        for path in from_tasks {
            push(path, &mut paths);
        }

        let mut options = Vec::with_capacity(paths.len() + 1);
        options.push(ProjectScopeOption::Home);
        options.extend(paths.into_iter().map(ProjectScopeOption::Project));
        options
    }

    /// Index into [`Self::project_options`] the open selector highlights.
    pub fn project_picker_index(&self) -> Option<usize> {
        let picker = self.project_picker.as_ref()?;
        Some(match picker.tab {
            PickerTab::Main => picker.selected,
            PickerTab::Archived => picker.archived_selected,
        })
    }

    /// Which tab the open selector shows.
    pub fn picker_tab(&self) -> Option<PickerTab> {
        self.project_picker.as_ref().map(|picker| picker.tab)
    }

    /// Archived tab entries: sorted scope paths of the archived projects.
    pub fn archived_project_options(&self) -> Vec<PathBuf> {
        self.archived_projects.iter().map(PathBuf::from).collect()
    }

    /// Rebuild the open selector's lists from the current snapshot and clamp both
    /// selections. No-op when the picker is closed.
    pub(super) fn refresh_project_picker(&mut self) {
        let Some(mut picker) = self.project_picker.take() else {
            return;
        };
        picker.options = self.project_options();
        picker.selected = picker.selected.min(picker.options.len().saturating_sub(1));
        picker.archived = self.archived_project_options();
        picker.archived_selected = picker
            .archived_selected
            .min(picker.archived.len().saturating_sub(1));
        self.project_picker = Some(picker);
    }

    /// Queue sections + counts for the current session destination.
    pub fn queue_view(&self) -> QueueView {
        let mut view = queue::query_board(
            &self.tasks,
            &self.archived_projects,
            self.this_repo.as_deref(),
            self.effective_lens(),
            self.drawer_open,
            &self.thread_filter,
        );
        if let BoardLocation::Projects = self.board_location {
            // The index's search narrows its own rows; task sections are untouched.
            let query = self.projects_query.trim().to_ascii_lowercase();
            if !query.is_empty() {
                view.projects.retain(|row| {
                    queue::short_project_name(&row.path)
                        .to_ascii_lowercase()
                        .contains(&query)
                        || row.path.to_ascii_lowercase().contains(&query)
                });
            }
        }
        view
    }

    /// The lens the current destination and its view control resolve to.
    pub(super) fn effective_lens(&self) -> BoardLens<'_> {
        match (&self.board_location, &self.projects_view) {
            (BoardLocation::Projects, ProjectsView::Thread(name)) => BoardLens::ThreadView(name),
            (location, _) => location.lens(),
        }
    }

    /// The thread filter active on slot 2's board.
    pub fn thread_filter(&self) -> &ThreadFilter {
        &self.thread_filter
    }

    /// The projects index's current View control.
    pub fn projects_view(&self) -> &ProjectsView {
        &self.projects_view
    }

    /// The help card's scroll offset.
    pub fn help_scroll(&self) -> usize {
        self.help_scroll
    }

    /// The index's search query.
    pub fn projects_query(&self) -> &str {
        &self.projects_query
    }

    /// The index rows the current query leaves visible (owned copy: rows are small).
    pub fn project_rows(&self) -> Vec<ProjectRow> {
        self.queue_view().projects
    }

    /// The index cursor, clamped to the rows the current query leaves visible.
    pub fn projects_cursor(&self) -> usize {
        let len = self.queue_view().projects.len();
        self.projects_selected.min(len.saturating_sub(1))
    }

    /// The index row the cursor rests on.
    pub fn selected_project_row(&self) -> Option<ProjectRow> {
        let projects = self.queue_view().projects;
        let cursor = self.projects_selected.min(projects.len().saturating_sub(1));
        projects.into_iter().nth(cursor)
    }

    pub(super) fn move_projects_cursor(&mut self, forward: bool) -> bool {
        let len = self.queue_view().projects.len();
        if len == 0 {
            self.projects_selected = 0;
            return false;
        }
        self.projects_selected = if forward {
            (self.projects_selected + 1) % len
        } else {
            self.projects_selected.checked_sub(1).unwrap_or(len - 1)
        };
        true
    }

    /// Open the project board's searchable thread filter picker (bare `t`).
    pub(super) fn open_thread_filter_picker(&mut self) {
        if !matches!(self.board_location, BoardLocation::Project(_)) {
            return;
        }
        let scope: Option<String> = match &self.board_location {
            BoardLocation::Project(path) => Some(path.to_string_lossy().into_owned()),
            _ => None,
        };
        let in_project = |task: &Task| matches!(&task.scope, TaskScope::Project { path } if scope.as_deref().is_some_and(|scope| paths_equivalent(path, scope)));
        let mut threads: BTreeMap<String, usize> = BTreeMap::new();
        let mut unthreaded = 0usize;
        let mut open = 0usize;
        for task in &self.tasks {
            if task.soft_deleted
                || task.archived
                || task.status == HumanStatus::Done
                || self.is_hidden(task)
                || !in_project(task)
            {
                continue;
            }
            open += 1;
            match task.thread.as_deref() {
                Some(name) => *threads.entry(name.to_ascii_lowercase()).or_default() += 1,
                None => unthreaded += 1,
            }
        }
        let mut options = vec![ListPickerOption {
            label: "All tasks".to_string(),
            count: Some(open),
            value: ListPickerValue::ThreadAll,
        }];
        let mut named: Vec<(String, usize)> = threads.into_iter().collect();
        named.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        options.extend(named.into_iter().map(|(name, count)| ListPickerOption {
            label: format!("#{name}"),
            count: Some(count),
            value: ListPickerValue::ThreadNamed(name),
        }));
        options.push(ListPickerOption {
            label: "Without a thread".to_string(),
            count: Some(unthreaded),
            value: ListPickerValue::ThreadWithout,
        });
        let selected = options
            .iter()
            .position(|option| match &option.value {
                ListPickerValue::ThreadAll => self.thread_filter == ThreadFilter::All,
                ListPickerValue::ThreadNamed(name) => {
                    self.thread_filter == ThreadFilter::Named(name.clone())
                }
                ListPickerValue::ThreadWithout => self.thread_filter == ThreadFilter::Without,
                _ => false,
            })
            .unwrap_or(0);
        self.list_picker = Some(ListPickerState {
            kind: ListPickerKind::ThreadFilter,
            options,
            selected,
            query: String::new(),
        });
    }

    /// Open the projects index's searchable View picker (bare `v`).
    pub(super) fn open_projects_view_picker(&mut self) {
        if !matches!(self.board_location, BoardLocation::Projects) {
            return;
        }
        let mut threads: BTreeMap<String, usize> = BTreeMap::new();
        for task in &self.tasks {
            if task.soft_deleted
                || task.archived
                || task.status == HumanStatus::Done
                || self.is_hidden(task)
            {
                continue;
            }
            if let Some(name) = task.thread.as_deref() {
                *threads.entry(name.to_ascii_lowercase()).or_default() += 1;
            }
        }
        let mut options = vec![ListPickerOption {
            label: "Overview".to_string(),
            count: None,
            value: ListPickerValue::ProjectsOverview,
        }];
        let mut named: Vec<(String, usize)> = threads.into_iter().collect();
        named.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        options.extend(named.into_iter().map(|(name, count)| ListPickerOption {
            label: format!("#{name}"),
            count: Some(count),
            value: ListPickerValue::ProjectsThread(name),
        }));
        let selected = options
            .iter()
            .position(|option| match (&option.value, &self.projects_view) {
                (ListPickerValue::ProjectsOverview, ProjectsView::Overview) => true,
                (ListPickerValue::ProjectsThread(name), ProjectsView::Thread(active)) => {
                    name == active
                }
                _ => false,
            })
            .unwrap_or(0);
        self.list_picker = Some(ListPickerState {
            kind: ListPickerKind::ProjectsView,
            options,
            selected,
            query: String::new(),
        });
    }

    /// Options the open picker shows after its search query, as (source index, option).
    pub fn visible_list_picker_options(&self) -> Vec<(usize, ListPickerOption)> {
        let Some(picker) = self.list_picker.as_ref() else {
            return Vec::new();
        };
        let query = picker.query.trim().to_ascii_lowercase();
        picker
            .options
            .iter()
            .enumerate()
            .filter(|(_, option)| {
                query.is_empty() || option.label.to_ascii_lowercase().contains(&query)
            })
            .map(|(index, option)| (index, option.clone()))
            .collect()
    }

    /// The picker row the cursor rests on, in the visible (filtered) order.
    pub fn list_picker_selected(&self) -> usize {
        self.list_picker
            .as_ref()
            .map(|picker| picker.selected)
            .unwrap_or(0)
    }

    pub fn selected_list_picker_option(&self) -> Option<(usize, ListPickerOption)> {
        let visible = self.visible_list_picker_options();
        let selected = self
            .list_picker
            .as_ref()
            .map(|picker| picker.selected.min(visible.len().saturating_sub(1)))
            .unwrap_or(0);
        visible.into_iter().nth(selected)
    }

    pub(super) fn move_list_picker(&mut self, forward: bool) {
        let len = self.visible_list_picker_options().len();
        if let Some(picker) = self.list_picker.as_mut() {
            picker.selected = if len == 0 {
                0
            } else if forward {
                (picker.selected + 1) % len
            } else {
                picker.selected.checked_sub(1).unwrap_or(len - 1)
            };
        }
    }

    pub(super) fn list_picker_query_insert(&mut self, character: char) {
        if let Some(picker) = self.list_picker.as_mut() {
            picker.query.push(character);
            picker.selected = 0;
        }
    }

    pub(super) fn list_picker_query_backspace(&mut self) {
        if let Some(picker) = self.list_picker.as_mut() {
            picker.query.pop();
            picker.selected = 0;
        }
    }

    pub(super) fn list_picker_query_insert_text(&mut self, text: &str) {
        if let Some(picker) = self.list_picker.as_mut() {
            picker.query.push_str(text);
            picker.selected = 0;
        }
    }

    /// Confirm the highlighted picker option: apply its value, close the picker, and
    /// report what was applied so the reducer can reanchor. `None` leaves the picker
    /// untouched (no visible option, or a kind/value mismatch).
    pub(super) fn confirm_list_picker(&mut self) -> Option<ListPickerValue> {
        let (_, option) = self.selected_list_picker_option()?;
        let value = option.value.clone();
        let kind = self.list_picker.as_ref()?.kind;
        let applied = match (kind, &value) {
            (ListPickerKind::ThreadFilter, ListPickerValue::ThreadAll) => {
                self.thread_filter = ThreadFilter::All;
                true
            }
            (ListPickerKind::ThreadFilter, ListPickerValue::ThreadNamed(name)) => {
                self.thread_filter = ThreadFilter::Named(name.clone());
                true
            }
            (ListPickerKind::ThreadFilter, ListPickerValue::ThreadWithout) => {
                self.thread_filter = ThreadFilter::Without;
                true
            }
            (ListPickerKind::ProjectsView, ListPickerValue::ProjectsOverview) => {
                self.projects_view = ProjectsView::Overview;
                self.projects_selected = 0;
                true
            }
            (ListPickerKind::ProjectsView, ListPickerValue::ProjectsThread(name)) => {
                self.projects_view = ProjectsView::Thread(name.clone());
                self.projects_selected = 0;
                true
            }
            _ => false,
        };
        if applied {
            self.list_picker = None;
            Some(value)
        } else {
            None
        }
    }

    pub(super) fn cancel_list_picker(&mut self) {
        self.list_picker = None;
    }

    /// Collapse (or reopen) the done drawer's archived group with Ctrl+G. The index
    /// and the project board have no collapsible task groups of their own; the drawer's
    /// archived group is the one group the chord still answers (AC-38).
    pub(super) fn toggle_all_groups(&mut self) -> bool {
        let view = self.queue_view();
        let archived_joins = self.drawer_open
            && view
                .sections
                .iter()
                .any(|section| section.kind == SectionKind::Archived);
        if archived_joins {
            self.archived_collapsed = !self.archived_collapsed;
            true
        } else {
            false
        }
    }

    /// Whether the done drawer is open (session-only).
    pub fn drawer_open(&self) -> bool {
        self.drawer_open
    }

    /// Toggle the archived group's collapse. Session-only; never persisted.
    pub(super) fn toggle_archived_collapsed(&mut self) {
        self.archived_collapsed = !self.archived_collapsed;
    }

    /// Whether the archived group's header row holds the selection.
    pub fn archived_header_selected(&self) -> bool {
        self.selection_id == Some(queue::ARCHIVED_HEADER_ROW_ID)
    }

    /// Pin the selection onto the archived header row (chrome, not a task).
    pub(super) fn select_archived_header(&mut self) -> bool {
        self.retarget_selection(
            Some(queue::ARCHIVED_HEADER_ROW_ID),
            SelectionRetarget::Explicit,
        )
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
        visible_task_ids(&self.queue_view(), self.archived_collapsed)
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

    /// Selected task id, if any. The archived header row is chrome, not a task:
    /// with it selected every task verb refuses with `select a task first`.
    pub fn selected_id(&self) -> Option<Uuid> {
        if self.selection_id == Some(queue::ARCHIVED_HEADER_ROW_ID) {
            None
        } else {
            self.selection_id
        }
    }

    /// Current list viewport offset.
    pub fn list_scroll(&self) -> usize {
        self.list_scroll.get()
    }

    /// Surface that currently owns keyboard and pointer routing, derived from the stage.
    pub fn focused_surface(&self) -> FocusedSurface {
        self.wide_stage.focused_surface()
    }

    /// Session-only wide-slider stage. The board always opens in `FullBoard`.
    pub fn wide_stage(&self) -> WideStage {
        self.wide_stage
    }

    /// Stage the full task page returns to on `Esc`, while one is remembered.
    pub fn stage_origin(&self) -> Option<WideStage> {
        self.stage_origin
    }

    /// Test surface. Retained task-page viewport offset.
    pub fn page_scroll(&self) -> usize {
        self.form
            .as_ref()
            .filter(|form| form.is_task())
            .map(|form| form.notes_scroll)
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn page_scroll_horizon(&self) -> usize {
        self.form
            .as_ref()
            .filter(|form| form.is_task())
            .map(|form| form.notes_max_scroll.get())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn set_page_scroll_horizon_for_test(&self, horizon: usize) {
        if let Some(form) = self.form.as_ref().filter(|form| form.is_task()) {
            form.notes_max_scroll.set(horizon);
        }
    }

    /// Test surface. Retained stored-step cursor, if active.
    pub fn step_cursor(&self) -> Option<usize> {
        self.form
            .as_ref()
            .filter(|form| form.is_task())
            .and_then(|form| form.steps.cursor)
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
            BoardPopup::SaveRecovery => BoardInputMode::SaveRecovery,
            BoardPopup::LaunchCard => BoardInputMode::LaunchCard,
            _ if self.project_picker.is_some() => BoardInputMode::ProjectPicker,
            _ if self.focused_surface() == FocusedSurface::Board
                && self.input_mode == BoardInputMode::TaskPage =>
            {
                BoardInputMode::Normal
            }
            _ => self.input_mode,
        }
    }

    /// Edit buffer contents for the focused board-page text field.
    pub fn edit_buffer(&self) -> &str {
        let Some(form) = self.form.as_ref() else {
            return "";
        };
        match form.focus {
            CaptureField::Title | CaptureField::Scope => form.title.value(),
            CaptureField::Notes => form.notes.value(),
            CaptureField::Thread => form.thread.value(),
        }
    }

    /// Cursor position in the focused board-page text buffer, counted in Unicode scalar values.
    pub fn edit_cursor(&self) -> usize {
        let Some(form) = self.form.as_ref() else {
            return 0;
        };
        match form.focus {
            CaptureField::Title | CaptureField::Scope => form.title.cursor(),
            CaptureField::Notes => form.notes.cursor(),
            CaptureField::Thread => form.thread.cursor(),
        }
    }

    /// Whether a board form currently owns keyboard input. The app loop uses this one predicate
    /// to route expanded quick-add and task-form keys through the same mapper.
    pub fn board_form_open(&self) -> bool {
        self.form.is_some()
    }

    /// Title draft displayed in the status-row quick-add line.
    pub fn quick_add_title_value(&self) -> &str {
        self.quick_add
            .as_ref()
            .map(|quick_add| quick_add.title.value())
            .unwrap_or("")
    }

    pub(super) fn clear_saved_task(&mut self) {
        self.saved_task = None;
    }

    pub(super) fn begin_quick_add_save(&mut self, id: Uuid, keep_open: bool) {
        self.quick_add_save = Some(QuickAddSave { id, keep_open });
    }

    pub(super) fn discard_quick_add(&mut self) {
        self.quick_add = None;
        self.quick_add_save = None;
        self.form = None;
        self.input_mode = BoardInputMode::Normal;
        self.clear_message();
    }

    /// Drop a retained expanded draft after the quick-add title changed.
    pub(super) fn invalidate_quick_add_stash(&mut self) {
        if self.form.as_ref().is_some_and(|form| !form.is_task()) {
            self.form = None;
        }
    }

    /// True when the current save-recovery result just completed a quick add.
    pub fn has_saved_task(&self) -> bool {
        self.saved_task.is_some()
    }

    fn finish_quick_add_save(&mut self) {
        let Some(pending) = self.quick_add_save.as_ref() else {
            return;
        };
        if !self.tasks.iter().any(|task| task.id == pending.id) {
            return;
        }
        let pending = self.quick_add_save.take().expect("checked quick-add save");
        self.saved_task = Some(pending.id);
        // Navigation never follows a save: the row flash is the feedback, and the pin
        // moves only when the current destination already renders the new task.
        if self.visible_ids().contains(&pending.id) {
            self.retarget_selection(Some(pending.id), SelectionRetarget::Reanchor);
            self.follow_list.set(true);
        } else if let Some(previous) = self.selection_id {
            let previous_visible = self.visible_ids();
            self.reanchor_selection(Some(previous), &previous_visible);
        }
        // The pending create is now durable. This is the only point an expanded quick-add
        // may release its complete form, so a failed save can still return to that stash.
        self.form = None;
        if pending.keep_open {
            if let Some(quick_add) = self.quick_add.as_mut() {
                quick_add.title = seeded_draft("");
            }
            self.input_mode = BoardInputMode::QuickAdd;
        } else {
            self.quick_add = None;
            self.input_mode = BoardInputMode::Normal;
        }
        self.clear_message();
    }

    fn finish_task_edit_save(&mut self) {
        if self.hold_task_edit_save {
            return;
        }
        let Some(pending) = self.task_edit_save.take() else {
            return;
        };
        let landed = self.tasks.iter().any(|task| {
            task.id == pending.id
                && task.title == pending.title
                && task.notes == pending.notes
                && task.scope == pending.scope
                && task.thread == pending.thread
                && pending.step_renames.iter().all(|(step_id, text)| {
                    task.steps
                        .iter()
                        .any(|step| step.id == *step_id && step.text == *text)
                })
                && pending
                    .step_removals
                    .iter()
                    .all(|step_id| !task.steps.iter().any(|step| step.id == *step_id))
        });
        if !landed {
            self.task_edit_save = Some(pending);
            return;
        }
        let saved_snapshot = self
            .tasks
            .iter()
            .find(|task| task.id == pending.id)
            .cloned();
        let selected_step_cursor = pending.selected_step.and_then(|selected_step| {
            saved_snapshot
                .as_ref()
                .and_then(|task| task.steps.iter().position(|step| step.id == selected_step))
        });
        if let Some(form) = self.form.as_mut().filter(|form| form.is_task()) {
            // Domain normalization (notably title trimming) is now durable. Refresh the
            // retained drafts so the page never paints a value that was not saved.
            form.title = seeded_draft(&pending.title);
            form.notes = seeded_draft(pending.notes.as_deref().unwrap_or_default());
            form.scope = pending.scope;
            form.scope_selected = form
                .scope_options
                .iter()
                .position(|option| option == &form.scope)
                .unwrap_or(0);
            form.thread = seeded_draft(pending.thread.as_deref().unwrap_or_default());
            form.thread_refusal = None;
            form.editing = false;
            form.steps.editor = None;
            form.steps.drafts.clear();
            form.steps.removals.clear();
            form.steps.cursor = selected_step_cursor;
            form.steps.add_selected = false;
            form.task_snapshot = saved_snapshot.map(Box::new);
        }
        self.input_mode = BoardInputMode::TaskPage;
        self.clear_message();
    }

    /// Hold a task form through the reducer's pre-persist sync.
    pub fn hold_task_edit_save(&mut self) {
        self.hold_task_edit_save = true;
    }

    /// Release a held form only after Retry or the initial persistence succeeds.
    pub fn release_task_edit_save(&mut self) {
        self.hold_task_edit_save = false;
    }

    /// Release a held inline step editor once persistence has confirmed its mutation
    /// (AC-14's release side).
    ///
    /// Mirrors [`Self::finish_quick_add_save`]'s identity check: the touched step
    /// must be present carrying its new text in the synced tasks before the row may
    /// close, or reopen empty for Shift+Enter. The Cancel path syncs the rolled-back
    /// baseline first, where the added step is absent, so the line stays held for
    /// [`Self::end_save_recovery`] to unwind to page view instead. The editor and its input mode outlive the save
    /// call precisely because nothing but this confirmed landing releases them.
    fn finish_step_editor_save(&mut self) {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return;
        };
        let Some(task_id) = form.task_id() else {
            return;
        };
        let Some(pending) = form.steps.pending_save.clone() else {
            return;
        };
        let landed = self.tasks.iter().any(|task| {
            task.id == task_id
                && task
                    .steps
                    .iter()
                    .any(|step| step.id == pending.step && step.text == pending.text)
        });
        if !landed {
            return;
        }
        let saved_snapshot = self.tasks.iter().find(|task| task.id == task_id).cloned();
        let form = self.form.as_mut().expect("task form checked above");
        form.steps.pending_save = None;
        form.task_snapshot = saved_snapshot.map(Box::new);
        if pending.exit_editing {
            form.editing = false;
            form.steps.editor = None;
            form.steps.add_selected = false;
            self.input_mode = BoardInputMode::TaskPage;
        } else if pending.reopen {
            // The rapid-capture loop: the in-place row reopens empty for the next step, its
            // mode never having left it.
            form.steps.add_selected = false;
            form.steps.editor = Some(StepEditor {
                buffer: seeded_draft(""),
                rename: None,
                refusal: None,
            });
        } else {
            form.steps.editor = None;
            form.steps.cursor = self
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.steps.iter().position(|step| step.id == pending.step));
            form.steps.add_selected = false;
            self.input_mode = BoardInputMode::TaskPage;
        }
        self.clear_message();
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

    /// Apply the shared cursor-window origin contract whenever form focus enters Notes.
    fn set_form_focus(form: &mut BoardForm, focus: CaptureField) {
        form.focus = focus;
        // Notes drafts are cursor-windowed rather than a full copy of the shared
        // content stream. Entering the editor therefore returns its window to the
        // visible origin, so a prior reading scroll cannot hide the draft or put
        // the terminal caret on a step row.
        if focus == CaptureField::Notes && form.is_task() {
            form.notes_scroll = 0;
        }
    }

    /// Enter field edit on the page's own focused field (Tab from view mode).
    pub(super) fn enter_page_field_focus(&mut self) {
        let Some(form) = self.form.as_mut().filter(|form| form.is_task()) else {
            return;
        };
        form.editing = true;
        Self::set_form_focus(form, form.focus);
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
        if form.is_task() {
            form.editing = true;
        }
        Self::set_form_focus(form, form.focus);
        self.input_mode = form.parent_mode();
    }

    /// Park an existing-step draft before another task field becomes active. An empty new-step
    /// add is discarded so a click or Tab can leave it. A typed add stays on its row.
    pub(super) fn park_rename_step_draft(&mut self) -> bool {
        if self.input_mode != BoardInputMode::EditStep {
            return true;
        }
        if self.empty_add_step_editor() {
            if let Some(form) = self.form.as_mut() {
                form.steps.editor = None;
            }
            self.input_mode = match self.form.as_ref() {
                Some(form) if form.is_task() => BoardInputMode::TaskPage,
                Some(form) => form.parent_mode(),
                None => BoardInputMode::TaskPage,
            };
            return true;
        }
        let Some(form) = self.form.as_mut() else {
            return false;
        };
        let Some(editor) = form.steps.editor.take() else {
            return true;
        };
        let Some(step_id) = editor.rename else {
            form.steps.editor = Some(editor);
            return false;
        };
        if !form.is_task() {
            form.steps.editor = Some(editor);
            return false;
        }
        form.steps.drafts.insert(step_id, editor.buffer);
        true
    }

    /// True when the inline add row is open and still empty after trim.
    pub fn empty_add_step_editor(&self) -> bool {
        self.input_mode == BoardInputMode::EditStep
            && self.form.as_ref().is_some_and(|form| {
                form.steps.editor.as_ref().is_some_and(|editor| {
                    editor.rename.is_none() && editor.buffer.value().trim().is_empty()
                })
            })
    }

    /// Copy the active existing-step buffer into the task session without releasing its row.
    /// The persistence boundary uses this for Shift+Enter so a failed save retains the editor.
    pub(super) fn stage_active_rename_draft(&mut self) -> bool {
        let Some(form) = self.form.as_mut().filter(|form| form.is_task()) else {
            return false;
        };
        let Some(editor) = form.steps.editor.as_ref() else {
            return false;
        };
        let Some(step_id) = editor.rename else {
            return false;
        };
        form.steps.drafts.insert(step_id, editor.buffer.clone());
        true
    }

    /// Whether the active inline row is an existing-step draft owned by the task session.
    pub fn has_active_step_rename(&self) -> bool {
        self.input_mode == BoardInputMode::EditStep
            && self
                .form
                .as_ref()
                .and_then(|form| form.steps.editor.as_ref())
                .is_some_and(|editor| editor.rename.is_some())
    }

    /// Focus one already-painted field without reopening or rebinding the shared form.
    pub(super) fn focus_form_field(&mut self, focus: CaptureField) {
        if !self.park_rename_step_draft() {
            return;
        }
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if self.input_mode == BoardInputMode::FormScopeDropdown {
            return;
        }
        if form.is_task() {
            form.editing = true;
        }
        Self::set_form_focus(form, focus);
        self.input_mode = form.parent_mode();
    }

    /// Toggle the selected task-page Thread control into or out of its text editor without
    /// closing the encompassing task edit session or discarding its draft.
    pub(super) fn toggle_thread_editing(&mut self) {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return;
        };
        if form.focus != CaptureField::Thread {
            return;
        }
        self.input_mode = match self.input_mode {
            BoardInputMode::SelectThread => BoardInputMode::EditThread,
            BoardInputMode::EditThread => BoardInputMode::SelectThread,
            _ => return,
        };
    }

    /// Whether the task page's cursor rests on a stored (not removed, not the `+ step`
    /// row) step. Enter toggles that step instead of acting on the page.
    pub fn stored_step_selected(&self) -> bool {
        if self.focused_surface() != FocusedSurface::Task
            || !matches!(
                self.input_mode,
                BoardInputMode::TaskPage | BoardInputMode::EditStep
            )
        {
            return false;
        }
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return false;
        };
        if form.steps.add_selected {
            return false;
        }
        let (Some(task_id), Some(index)) = (form.task_id(), form.steps.cursor) else {
            return false;
        };
        self.tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.steps.get(index))
            .is_some_and(|step| !form.steps.removals.contains(&step.id))
    }

    /// Whether the current task page has entered its edit session.
    pub fn task_editing(&self) -> bool {
        self.form
            .as_ref()
            .filter(|form| form.is_task())
            .is_some_and(|form| form.editing)
    }

    /// Whether the bound task session contains an unsaved user change.
    ///
    /// Geometry and editor presence are deliberately absent: opening an editor is clean. Only
    /// field values that differ from the bound snapshot, staged existing-step changes or removals,
    /// and a non-empty new-step draft make the session dirty.
    pub fn task_session_dirty(&self) -> bool {
        let Some(form) = self.form.as_ref().filter(|form| form.is_task()) else {
            return false;
        };
        let Some(snapshot) = form.task_snapshot.as_deref() else {
            return false;
        };
        if form.title.value() != snapshot.title
            || form.notes.value() != snapshot.notes.as_deref().unwrap_or_default()
            || form.thread.value() != snapshot.thread.as_deref().unwrap_or_default()
            || form.scope != snapshot.scope
            || !form.steps.removals.is_empty()
        {
            return true;
        }
        let changed_existing_step = |id: Uuid, value: &str| {
            snapshot
                .steps
                .iter()
                .find(|step| step.id == id)
                .is_none_or(|step| step.text != value)
        };
        if form
            .steps
            .drafts
            .iter()
            .any(|(id, draft)| changed_existing_step(*id, draft.value()))
        {
            return true;
        }
        form.steps.editor.as_ref().is_some_and(|editor| {
            editor.rename.map_or_else(
                || !editor.buffer.value().is_empty(),
                |id| changed_existing_step(id, editor.buffer.value()),
            )
        })
    }

    pub(super) fn cycle_form_scope(&mut self) {
        if let Some(form) = self.form.as_mut() {
            form.cycle_scope();
        }
    }

    pub(super) fn open_form_scope_dropdown(&mut self) {
        if !self.park_rename_step_draft() {
            return;
        }
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
            BoardInputMode::LaunchCard => LAUNCH_CARD_HELP_LINE,
            BoardInputMode::Help => HELP_SURFACE_HELP_LINE,
            BoardInputMode::ProjectsSearch => crate::ui::input::PROJECTS_SEARCH_HELP_LINE,
            _ => BOARD_HELP_LINE,
        }
    }

    /// Chrome status/message line feedback.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(terminal_text(&msg.into()));
        self.message_expires_at = None;
        self.message_restore = None;
    }

    /// Status feedback that clears itself after `ttl` (copy confirmation, brief notices).
    ///
    /// A sticky status already on the line (save-recovery banner, etc.) is stashed and
    /// restored when the toast expires, so a `copied` flash cannot erase it.
    pub fn set_ephemeral_message(&mut self, msg: impl Into<String>, ttl: std::time::Duration) {
        if self.message_expires_at.is_none() {
            self.message_restore = self.message.clone();
        }
        self.message = Some(terminal_text(&msg.into()));
        self.message_expires_at = Some(Instant::now() + ttl);
    }

    pub fn clear_message(&mut self) {
        self.message = None;
        self.message_expires_at = None;
        self.message_restore = None;
    }

    /// Drop an ephemeral status line whose TTL has elapsed. Sticky messages are untouched;
    /// a stashed sticky under a toast is restored.
    pub fn expire_ephemeral_message(&mut self) {
        if self
            .message_expires_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.message = self.message_restore.take();
            self.message_expires_at = None;
        }
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
            | BoardInputMode::SelectThread
            | BoardInputMode::EditThread
            | BoardInputMode::EditScope
            | BoardInputMode::EditStep) => Some(mode),
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
    /// Seed selection on open: first visible NEEDS YOU id, else IN MOTION, else ON DECK.
    ///
    /// Honors collapse state via [`Self::visible_ids`]: a seeded row must be one the
    /// renderer painted, or verbs would mutate a task the user cannot see.
    pub(super) fn seed_selection(&mut self) {
        let visible: HashSet<Uuid> = self.visible_ids().into_iter().collect();
        let view = self.queue_view();
        for kind in [
            SectionKind::NeedsYou,
            SectionKind::InMotion,
            SectionKind::OnDeck,
        ] {
            if let Some(id) = view
                .sections
                .iter()
                .filter(|section| section.kind == kind)
                .flat_map(|section| section.task_ids.iter().copied())
                .find(|id| visible.contains(id))
            {
                self.retarget_selection(Some(id), SelectionRetarget::Reanchor);
                self.follow_list.set(true);
                return;
            }
        }
        self.retarget_selection(None, SelectionRetarget::Reanchor);
        self.follow_list.set(true);
    }

    /// The only boundary that may replace the selected task while a task form is retained.
    /// A dirty form keeps both its selection and immutable binding on the same task.
    fn retarget_selection(&mut self, requested: Option<Uuid>, source: SelectionRetarget) -> bool {
        let bound = self.edit_target();
        let bound_is_visible = bound.is_some_and(|id| self.visible_ids().contains(&id));
        let must_keep_dirty_binding = source == SelectionRetarget::Explicit || bound_is_visible;
        if self.task_session_dirty() && must_keep_dirty_binding && requested != bound {
            if bound_is_visible {
                self.selection_id = bound;
            }
            if self.popup != BoardPopup::SaveRecovery {
                self.set_message(DIRTY_TASK_SWITCH_REFUSAL);
            }
            return false;
        }
        if source == SelectionRetarget::Explicit
            && requested != bound
            && self.form.as_ref().is_some_and(BoardForm::is_task)
        {
            let archived = self.archived_projects.clone();
            self.form = requested.and_then(|id| {
                self.tasks.iter().find(|task| task.id == id).map(|task| {
                    BoardForm::task(
                        task,
                        self.this_repo.as_deref(),
                        &self.tasks,
                        CaptureField::Title,
                        &archived,
                    )
                })
            });
            self.input_mode = if self.form.is_some() {
                BoardInputMode::TaskPage
            } else {
                BoardInputMode::Normal
            };
        }
        self.selection_id = requested;
        true
    }

    /// Re-pin selection after the visible set changes through the shared retarget boundary.
    pub(super) fn reanchor_selection(&mut self, previous: Option<Uuid>, previous_visible: &[Uuid]) {
        let new_visible = self.visible_ids();
        if self
            .detail_open
            .is_some_and(|id| !new_visible.contains(&id))
        {
            self.close_detail();
        }
        let requested = selection::reanchor(previous, previous_visible, &new_visible);
        if !self.retarget_selection(requested, SelectionRetarget::Reanchor) {
            return;
        }
        self.follow_list.set(true);
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
        // Movement stays within the active tab.
        let (selected, len) = match picker.tab {
            PickerTab::Main => (&mut picker.selected, picker.options.len()),
            PickerTab::Archived => (&mut picker.archived_selected, picker.archived.len()),
        };
        if len == 0 {
            return;
        }
        *selected = if forward {
            (*selected + 1) % len
        } else {
            selected.checked_sub(1).unwrap_or(len - 1)
        };
    }

    pub(super) fn select_next(&mut self) -> bool {
        let ids = self.visible_ids();
        let requested = if ids.is_empty() {
            None
        } else {
            Some(
                match self
                    .selection_id
                    .and_then(|id| ids.iter().position(|&row| row == id))
                {
                    Some(i) => ids[(i + 1) % ids.len()],
                    None => ids[0],
                },
            )
        };
        if !self.retarget_selection(requested, SelectionRetarget::Explicit) {
            return false;
        }
        self.close_popup();
        self.follow_list.set(true);
        true
    }

    pub(super) fn select_prev(&mut self) -> bool {
        let ids = self.visible_ids();
        let requested = if ids.is_empty() {
            None
        } else {
            Some(
                match self
                    .selection_id
                    .and_then(|id| ids.iter().position(|&row| row == id))
                {
                    Some(0) | None => ids[ids.len() - 1],
                    Some(i) => ids[i - 1],
                },
            )
        };
        if !self.retarget_selection(requested, SelectionRetarget::Explicit) {
            return false;
        }
        self.close_popup();
        self.follow_list.set(true);
        true
    }

    pub(super) fn select_index(&mut self, idx: usize) -> bool {
        let Some(&requested) = self.visible_ids().get(idx) else {
            return false;
        };
        if !self.retarget_selection(Some(requested), SelectionRetarget::Explicit) {
            return false;
        }
        self.close_popup();
        // Nudge only if this row is off-screen; a click on a visible row stays put.
        self.follow_list.set(true);
        true
    }

    pub(super) fn set_list_scroll(&mut self, offset: usize) {
        self.list_scroll.set(offset);
        self.follow_list.set(false);
    }

    /// Set the session destination through the picker. `None` (the desk choice) goes
    /// to tab 1, never to the index.
    pub fn set_selected_project(&mut self, project: Option<PathBuf>) {
        self.set_board_scope(match project {
            None => ProjectScopeOption::Home,
            Some(path) => ProjectScopeOption::Project(path),
        });
    }

    pub(super) fn set_board_scope(&mut self, scope: ProjectScopeOption) {
        self.close_popup();
        self.projects_query.clear();
        if self.input_mode == BoardInputMode::ProjectsSearch {
            self.input_mode = BoardInputMode::Normal;
        }
        match scope {
            ProjectScopeOption::Home => {
                self.selected_project = None;
                self.switch_location(BoardLocation::Desk);
            }
            ProjectScopeOption::Project(path) => {
                self.selected_project = Some(path.clone());
                self.switch_location(BoardLocation::Project(path));
            }
        }
    }
}
/// Compact label for one project path.
///
/// Prefers the last path segment so the Projects header stays readable on narrow panes.
pub fn project_option_label(path: &Path) -> String {
    terminal_text(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path.to_string_lossy()),
    )
}

pub(super) fn project_scope_option_label(option: &ProjectScopeOption) -> String {
    match option {
        ProjectScopeOption::Home => "desk".to_string(),
        ProjectScopeOption::Project(path) => project_option_label(path.as_path()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProvenanceOrigin;

    const REPO_A: &str = "/repos/a";
    const REPO_B: &str = "/repos/b";

    fn project(path: &str) -> TaskScope {
        TaskScope::Project {
            path: path.to_string(),
        }
    }

    fn create(domain: &mut DomainState, title: &str, scope: TaskScope) -> Uuid {
        domain
            .create(title, None, scope, ProvenanceOrigin::Manual, None)
            .expect("create")
    }

    #[test]
    fn ephemeral_status_clears_after_deadline() {
        let mut model = BoardModel::from_tasks(Vec::new(), None);
        model.set_ephemeral_message("copied", std::time::Duration::from_millis(1));
        assert_eq!(model.message(), Some("copied"));
        std::thread::sleep(std::time::Duration::from_millis(5));
        model.expire_ephemeral_message();
        assert!(model.message().is_none());
        assert!(model.message_expires_at.is_none());
    }

    #[test]
    fn ephemeral_toast_restores_sticky_save_recovery_banner() {
        let mut model = BoardModel::from_tasks(Vec::new(), None);
        model.set_message("save failed: disk full · Retry or Cancel");
        model.set_ephemeral_message("copied", std::time::Duration::from_millis(1));
        assert_eq!(model.message(), Some("copied"));
        std::thread::sleep(std::time::Duration::from_millis(5));
        model.expire_ephemeral_message();
        assert_eq!(
            model.message(),
            Some("save failed: disk full · Retry or Cancel"),
            "sticky banner must return after the toast, not vanish"
        );
        assert!(model.message_expires_at.is_none());
    }

    #[test]
    fn sticky_status_survives_expire_tick() {
        let mut model = BoardModel::from_tasks(Vec::new(), None);
        model.set_message("save failed");
        model.expire_ephemeral_message();
        assert_eq!(model.message(), Some("save failed"));
    }

    #[test]
    fn ctrl_g_folds_only_the_done_drawers_archived_group() {
        let mut domain = DomainState::new();
        create(&mut domain, "task-a", project(REPO_A));
        create(&mut domain, "task-b", project(REPO_B));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        // Drawer closed: no group answers the chord (the destinations have no
        // collapsible task groups anymore).
        assert!(!model.toggle_all_groups());

        // Drawer open with an archived group: the chord folds it.
        model.drawer_open = true;
        // No archived tasks here, so the archived section never paints.
        assert!(!model.toggle_all_groups());

        domain
            .archive_task(model.selected_id().expect("a selected task"))
            .expect("archive");
        model.sync_from_domain(&domain);
        model.drawer_open = true;
        // The archived group starts collapsed; the chord's first press expands it.
        assert!(model.toggle_all_groups());
        assert!(!model.archived_collapsed);
        assert!(model.toggle_all_groups());
        assert!(model.archived_collapsed);
    }

    #[test]
    fn nav_tabs_never_shift_meaning_and_slot_two_without_a_project_opens_nothing_here() {
        let mut domain = DomainState::new();
        create(&mut domain, "task-a", project(REPO_A));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        assert_eq!(
            model.nav_tab(),
            NavTab::ProjectBoard,
            "startup in a repo opens slot 2"
        );

        assert!(model.select_nav_tab(NavTab::Desk));
        assert_eq!(model.nav_tab(), NavTab::Desk);
        assert_eq!(model.selected_project(), Some(Path::new(REPO_A)));
        assert!(model.select_nav_tab(NavTab::Projects));
        assert_eq!(model.nav_tab(), NavTab::Projects);
        assert!(model.select_nav_tab(NavTab::ProjectBoard));
        assert_eq!(model.nav_tab(), NavTab::ProjectBoard);
        assert_eq!(model.selected_project(), Some(Path::new(REPO_A)));
        assert!(model.select_nav_tab(NavTab::Desk));
        assert!(model.select_nav_tab(NavTab::Projects));
        assert!(model.select_nav_tab(NavTab::ProjectBoard));
        assert_eq!(model.nav_tab(), NavTab::ProjectBoard);
        model.set_selected_project(Some(PathBuf::from(REPO_B)));
        model.select_nav_tab(NavTab::Desk);
        assert!(model.select_nav_tab(NavTab::ProjectBoard));
        assert_eq!(model.selected_project(), Some(Path::new(REPO_B)));
    }

    #[test]
    fn switching_projects_clears_the_local_thread_filter() {
        let mut domain = DomainState::new();
        create(&mut domain, "task-a", project(REPO_A));
        create(&mut domain, "task-b", project(REPO_B));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.thread_filter = ThreadFilter::Named("release".to_string());
        model.set_selected_project(Some(PathBuf::from(REPO_B)));
        assert_eq!(
            model.thread_filter(),
            &ThreadFilter::All,
            "the thread filter belongs to one project board; switching clears it"
        );

        // Same project again: the filter survives.
        model.thread_filter = ThreadFilter::Without;
        model.set_selected_project(Some(PathBuf::from(REPO_B)));
        assert_eq!(model.thread_filter(), &ThreadFilter::Without);
    }

    #[test]
    fn picker_desk_choice_lands_on_the_desk_tab_not_the_index() {
        let mut domain = DomainState::new();
        create(&mut domain, "task-a", project(REPO_A));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.set_selected_project(Some(PathBuf::from(REPO_B)));
        model.set_selected_project(None);
        assert_eq!(model.nav_tab(), NavTab::Desk);
    }

    #[test]
    fn projects_index_cursor_moves_within_the_visible_rows() {
        let mut domain = DomainState::new();
        create(&mut domain, "task-a", project("/repos/alpha"));
        create(&mut domain, "task-b", project("/repos/beta"));

        let mut model = BoardModel::from_domain(&domain, None);
        model.board_location = BoardLocation::Projects;
        assert_eq!(model.project_rows().len(), 2);
        assert!(model.move_projects_cursor(true));
        assert_eq!(model.projects_cursor(), 1);
        assert!(model.move_projects_cursor(true));
        assert_eq!(model.projects_cursor(), 0, "the index cursor wraps");
        assert!(model.move_projects_cursor(false));
        assert_eq!(model.projects_cursor(), 1);

        // Search narrows the rows and the cursor clamps.
        model.projects_query = "beta".to_string();
        assert_eq!(model.project_rows().len(), 1);
        assert_eq!(model.projects_cursor(), 0);
        assert_eq!(
            model.selected_project_row().expect("row").path,
            "/repos/beta"
        );
        model.projects_query = "zzz".to_string();
        assert!(model.project_rows().is_empty());
        assert!(model.selected_project_row().is_none());
    }

    #[test]
    fn thread_filter_picker_options_come_from_this_project_only() {
        let mut domain = DomainState::new();
        let a = create(&mut domain, "a1", project(REPO_A));
        domain
            .edit(a, "a1", None, project(REPO_A), Some("nav".to_string()))
            .expect("thread");
        let b = create(&mut domain, "b1", project(REPO_B));
        domain
            .edit(b, "b1", None, project(REPO_B), Some("other".to_string()))
            .expect("thread");

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.open_thread_filter_picker();
        let options = model.visible_list_picker_options();
        let labels: Vec<&str> = options
            .iter()
            .map(|(_, option)| option.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["All tasks", "#nav", "Without a thread"],
            "another project's thread must not offer itself here"
        );
        assert_eq!(model.list_picker_kind(), Some(ListPickerKind::ThreadFilter));
    }

    #[cfg(unix)]
    #[test]
    fn thread_filter_picker_matches_equivalent_project_aliases() {
        use std::fs;
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("tsk-thread-alias-{}", std::process::id()));
        let real = root.join("real");
        let alias = root.join("alias");
        fs::create_dir_all(&real).expect("real");
        symlink(&real, &alias).expect("alias");
        let mut domain = DomainState::new();
        let id = create(
            &mut domain,
            "aliased",
            project(alias.to_string_lossy().as_ref()),
        );
        domain
            .edit(
                id,
                "aliased",
                None,
                project(alias.to_string_lossy().as_ref()),
                Some("nav".into()),
            )
            .expect("thread");
        let mut model = BoardModel::from_domain(&domain, Some(real));
        model.open_thread_filter_picker();
        assert!(model
            .visible_list_picker_options()
            .iter()
            .any(|(_, option)| option.label == "#nav"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn confirming_thread_filter_narrows_and_clearing_restores() {
        let mut domain = DomainState::new();
        let a = create(&mut domain, "a1", project(REPO_A));
        domain
            .edit(a, "a1", None, project(REPO_A), Some("nav".to_string()))
            .expect("thread");
        create(&mut domain, "a2", project(REPO_A));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.open_thread_filter_picker();
        model.move_list_picker(true); // #nav
        let applied = model.confirm_list_picker();
        assert_eq!(
            applied,
            Some(ListPickerValue::ThreadNamed("nav".to_string()))
        );
        assert!(!model.list_picker_open());
        let visible = model.visible_ids();
        assert_eq!(
            visible,
            vec![a],
            "the filter narrows the board across statuses"
        );

        // Re-opening highlights the active filter; step back to All tasks and confirm.
        model.open_thread_filter_picker();
        model.move_list_picker(false);
        let applied = model.confirm_list_picker();
        assert_eq!(applied, Some(ListPickerValue::ThreadAll));
        assert_eq!(model.visible_ids().len(), 2);
    }

    #[test]
    fn projects_view_picker_lists_cross_project_threads() {
        let mut domain = DomainState::new();
        let a = create(&mut domain, "a1", project(REPO_A));
        domain
            .edit(a, "a1", None, project(REPO_A), Some("release".to_string()))
            .expect("thread");
        create(&mut domain, "b1", project(REPO_B));

        let mut model = BoardModel::from_domain(&domain, None);
        model.board_location = BoardLocation::Projects;
        model.open_projects_view_picker();
        let labels: Vec<String> = model
            .visible_list_picker_options()
            .iter()
            .map(|(_, option)| option.label.clone())
            .collect();
        assert_eq!(labels, vec!["Overview", "#release"]);

        model.move_list_picker(true);
        assert_eq!(
            model.confirm_list_picker(),
            Some(ListPickerValue::ProjectsThread("release".to_string()))
        );
        assert!(
            model
                .queue_view()
                .sections
                .iter()
                .any(|section| section.task_ids.contains(&a)),
            "the thread view shows the matching task from any project"
        );

        // Re-opening highlights the active view; step back to Overview and confirm.
        model.open_projects_view_picker();
        model.move_list_picker(false);
        assert_eq!(
            model.confirm_list_picker(),
            Some(ListPickerValue::ProjectsOverview)
        );
        assert_eq!(model.projects_view(), &ProjectsView::Overview);
        assert!(!model.project_rows().is_empty());
    }

    #[test]
    fn sync_from_domain_reanchors_by_prior_order_not_new_order() {
        let mut domain = DomainState::new();
        let t1 = create(&mut domain, "task-1", project(REPO_A));
        let t2 = create(&mut domain, "task-2", project(REPO_A));
        let t3 = create(&mut domain, "task-3", project(REPO_A));
        let t4 = create(&mut domain, "task-4", project(REPO_A));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.selection_id = Some(t3);

        // One sync carries both an external removal of the rows before the selection
        // (t1, t2 gone) and the local completion of the selection itself (t3 done).
        domain.soft_delete(t2).expect("soft delete");
        domain.soft_delete(t1).expect("soft delete");
        domain.set_status(t3, HumanStatus::Done).expect("status");
        model.sync_from_domain(&domain);

        assert_eq!(
            model.selected_id(),
            Some(t4),
            "selection must reanchor to the old-order neighbor, not the first new row"
        );
    }

    #[test]
    fn sync_from_domain_does_not_yank_home_tab_for_externally_created_tasks() {
        let mut domain = DomainState::new();
        let desk_task = create(&mut domain, "desk task", TaskScope::Global);

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Desk;
        model.seed_selection();
        assert_eq!(model.nav_tab(), NavTab::Desk);

        // Another process adds a project task between two syncs.
        create(&mut domain, "external task", project(REPO_B));
        model.sync_from_domain(&domain);

        assert_eq!(
            model.nav_tab(),
            NavTab::Desk,
            "a background merge must not move the user's destination"
        );
        assert_eq!(model.selected_id(), Some(desk_task));
    }

    #[test]
    fn sync_from_domain_surfaces_the_first_arriving_task_only_when_it_is_visible_here() {
        let domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Desk;
        assert!(model.visible_ids().is_empty());

        // Another process captures a desk-visible task while this board shows nothing:
        // the desk's global lanes render it, so the pin moves to it.
        let mut domain = DomainState::new();
        let started = create(&mut domain, "first capture", TaskScope::Global);
        domain
            .set_status(started, HumanStatus::Started)
            .expect("start");
        model.sync_from_domain(&domain);

        assert_eq!(
            model.nav_tab(),
            NavTab::Desk,
            "surfacing never moves the destination"
        );
        assert_eq!(model.selected_id(), Some(started));

        // A task the current destination does not render never drags the view there.
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Desk;
        model.selection_id = None;
        let mut domain = DomainState::new();
        create(&mut domain, "elsewhere", project(REPO_B));
        model.sync_from_domain(&domain);
        assert_eq!(model.nav_tab(), NavTab::Desk);
        assert_eq!(
            model.selected_id(),
            None,
            "an invisible arrival stays invisible rather than switching tabs"
        );
    }

    #[test]
    fn pinned_quick_add_save_does_not_pin_invisible_row_under_project_focus() {
        let mut domain = DomainState::new();
        let in_a = create(&mut domain, "focus task", project(REPO_A));
        let in_b = create(&mut domain, "other task", project(REPO_B));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Project(PathBuf::from(REPO_A));
        model.selection_id = Some(in_a);

        // A quick-add draft saved with a !p token for another project: the saved task
        // exists but the focused lens does not render it.
        model.begin_quick_add_save(in_b, false);
        model.sync_from_domain(&domain);

        assert_eq!(
            model.selected_id(),
            Some(in_a),
            "under project focus a save outside the scope must not pin an invisible row,              and must keep instead of jumping to the first row"
        );
    }
}
