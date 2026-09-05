//! Session-only board state, forms, selection, and recovery presentation.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Position;
use uuid::Uuid;

use crate::context::InvocationSnapshot;
use crate::domain::{DomainState, HumanStatus, Task, TaskScope};
use crate::ui::capture::CaptureField;
use crate::ui::edit::{seeded_draft, EditBuffer};
use crate::ui::input::{
    BOARD_HELP_LINE, COMMAND_SURFACE_HELP_LINE, HELP_SURFACE_HELP_LINE, LAUNCH_CARD_HELP_LINE,
    SAVE_RECOVERY_HELP_LINE,
};
use crate::ui::mouse::BoardPopup;
pub use crate::ui::queue::BoardTab;
use crate::ui::queue::{
    self, visible_task_ids, BoardLens, QueueView, SectionKind, ThreadProjectCollapseKey,
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
    /// the help card is open.
    Help,
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

/// Session-only board location: home tabs or one focused project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BoardLocation {
    Home {
        tab: BoardTab,
    },
    Project(PathBuf),
    /// Read-only focus on an archived project, opened with Enter from the picker's
    /// archived tab (AC-41). Session-only: leaving it hides those tasks again.
    ArchivedProject(PathBuf),
}

impl BoardLocation {
    pub(super) fn lens(&self) -> BoardLens<'_> {
        match self {
            Self::Home { tab } => BoardLens::Home(*tab),
            Self::Project(path) => BoardLens::Project(path.as_path()),
            Self::ArchivedProject(path) => BoardLens::ArchivedProject(path.as_path()),
        }
    }

    pub(super) fn at_home(&self) -> bool {
        matches!(self, Self::Home { .. })
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
    /// Default from the selected single-project board scope or invocation snapshot, overridden
    /// by a parsed title token.
    pub(super) scope: TaskScope,
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
        TaskScope::Project { path } => archived.contains(path),
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
    /// Session board location (home tab or focused project). Not durable.
    pub(super) board_location: BoardLocation,
    /// Wide-slider stage. Focus and single-pane presentation derive from it. Session-only.
    pub(super) wide_stage: WideStage,
    /// Stage `Enter` (or a row double-click) left for the full task page; `Esc` returns there.
    pub(super) stage_origin: Option<WideStage>,
    /// Project-group headers collapsed on the Projects tab.
    pub(super) collapsed_projects: HashSet<String>,
    /// Thread-group headers collapsed on the Threads tab.
    pub(super) collapsed_threads: HashSet<String>,
    /// Project sub-headers collapsed under a thread on the Threads tab.
    pub(super) collapsed_thread_projects: HashSet<ThreadProjectCollapseKey>,
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
    /// Build a board model with queue-local session state only.
    pub fn from_tasks(tasks: Vec<Task>, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self {
            tasks,
            archived_projects: BTreeSet::new(),
            this_repo,
            board_location: BoardLocation::Home {
                tab: BoardTab::Desk,
            },
            wide_stage: WideStage::FullBoard,
            stage_origin: None,
            collapsed_projects: HashSet::new(),
            collapsed_threads: HashSet::new(),
            collapsed_thread_projects: HashSet::new(),
            drawer_open: false,
            archived_collapsed: true,
            launch_card: None,
            launch_card_shown: false,
            session_default_scope: None,
            detail_open: None,
            selection_id: None,
            last_row_click: None,
            last_project_header_click: None,
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
        model.ensure_home_tab_has_visible_tasks();
        model
    }

    /// When the default desk tab would show no rows, open on projects (then threads) instead.
    fn ensure_home_tab_has_visible_tasks(&mut self) {
        if !self.board_location.at_home() || !self.visible_ids().is_empty() {
            return;
        }
        let has_open = self.tasks.iter().any(|task| {
            !task.soft_deleted && !self.is_hidden(task) && task.status != HumanStatus::Done
        });
        if !has_open {
            return;
        }
        self.board_location = BoardLocation::Home {
            tab: BoardTab::Projects,
        };
        self.seed_selection();
        if self.visible_ids().is_empty() {
            self.board_location = BoardLocation::Home {
                tab: BoardTab::Threads,
            };
            self.seed_selection();
        }
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
    pub fn from_domain(state: &DomainState, this_repo: Option<PathBuf>) -> Self {
        let mut model = Self::from_tasks(state.tasks().to_vec(), this_repo);
        model.archived_projects = state.archived_projects();
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
            TaskScope::Project { path } => self.archived_projects.contains(path),
        }
    }

    /// Switch the home tab, when needed, so `id` would appear in [`Self::visible_ids`].
    pub(super) fn reveal_task_on_home(&mut self, id: Uuid) {
        if !self.board_location.at_home() || self.visible_ids().contains(&id) {
            return;
        }
        let Some(task) = self
            .tasks
            .iter()
            .find(|task| task.id == id && !task.soft_deleted && !self.is_hidden(task))
        else {
            return;
        };
        let tab = if task.thread.is_some() {
            BoardTab::Threads
        } else if matches!(task.scope, TaskScope::Project { .. }) {
            BoardTab::Projects
        } else {
            BoardTab::Desk
        };
        self.board_location = BoardLocation::Home { tab };
    }

    fn ensure_selection_visible(&mut self) {
        let Some(id) = self.selection_id else {
            return;
        };
        self.reveal_task_on_home(id);
    }

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
        // A merge (or this board's own picker verb) may archive the project this board is
        // focused on: reset the focus to home desk and name the project on the status row.
        // The quick-add default is guarded at `OpenCapture`, which never resolves to an
        // archived project, so the session default is left alone here.
        // A read-only focus whose project came back (picker, CLI, or a sibling process)
        // becomes an ordinary project focus: chip, dim rows and verbs all follow.
        if let BoardLocation::ArchivedProject(path) = &self.board_location {
            if !self
                .archived_projects
                .contains(path.to_string_lossy().as_ref())
            {
                self.board_location = BoardLocation::Project(path.clone());
            }
        }
        let focus_archived = match &self.board_location {
            BoardLocation::Project(path) => self
                .archived_projects
                .contains(path.to_string_lossy().as_ref()),
            // A read-only focus is deliberately on an archived project (AC-41): it is
            // not the accident this reset exists for.
            BoardLocation::ArchivedProject(_) | BoardLocation::Home { .. } => false,
        };
        if focus_archived {
            let name = match &self.board_location {
                BoardLocation::Project(path) => {
                    crate::ui::render::short_project(&path.to_string_lossy()).to_string()
                }
                _ => String::new(),
            };
            self.board_location = BoardLocation::Home {
                tab: BoardTab::Desk,
            };
            self.set_message(format!("project {name} is archived"));
        }
        let pinned_edit = self.task_edit_save.as_ref().map(|pending| pending.id);
        let pinned_quick_add = self.quick_add_save.as_ref().map(|pending| pending.id);
        self.finish_quick_add_save();
        self.finish_task_edit_save();
        self.finish_step_editor_save();
        if let Some(id) = pinned_edit.or(pinned_quick_add) {
            // A save this surface just made owns the selection, but only when the current
            // lens actually renders it (project focus never reveals across scopes).
            self.reveal_task_on_home(id);
            if self.visible_ids().contains(&id) {
                self.retarget_selection(Some(id), SelectionRetarget::Reanchor);
                self.follow_list.set(true);
            } else {
                // Anchor on the saved id's old position when it had one (an edit that
                // left this lens); otherwise fall back to the pre-sync selection so an
                // externally invisible save cannot jump the pin to the first row.
                let anchor = if previous_visible.contains(&id) {
                    Some(id)
                } else {
                    previous
                };
                self.reanchor_selection(anchor, &previous_visible);
            }
        } else {
            // Tasks merged in from disk are somebody else's work: never yank the home tab
            // or selection toward them. An otherwise-empty view may surface the first one,
            // so a board opened on nothing still lights up when captures arrive.
            if previous_visible.is_empty() {
                if let Some(id) = new_ids.first() {
                    self.reveal_task_on_home(*id);
                }
            }
            self.reanchor_selection(previous, &previous_visible);
            self.ensure_selection_visible();
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

    /// Whether the home tabs are visible (false in project focus).
    pub fn at_home(&self) -> bool {
        self.board_location.at_home()
    }

    /// Active home tab when at home; otherwise [`BoardTab::Desk`].
    pub fn home_tab(&self) -> BoardTab {
        match &self.board_location {
            BoardLocation::Home { tab } => *tab,
            BoardLocation::Project(_) | BoardLocation::ArchivedProject(_) => BoardTab::Desk,
        }
    }

    pub(super) fn set_home_tab(&mut self, tab: BoardTab) {
        // AC-45: `1`/`2`/`3` from the read-only archived focus go home, which hides that
        // project's tasks again.
        if self.focus_is_archived() {
            let previous_visible = self.visible_ids();
            self.board_location = BoardLocation::Home { tab };
            self.reanchor_selection(None, &previous_visible);
            self.seed_selection();
            return;
        }
        let BoardLocation::Home { tab: current } = self.board_location else {
            return;
        };
        if current == tab {
            return;
        }
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = BoardLocation::Home { tab };
        self.reanchor_selection(previous, &previous_visible);
    }

    /// The selected project scope, when the session is focused on one project.
    pub fn selected_project(&self) -> Option<&Path> {
        match &self.board_location {
            BoardLocation::Project(path) => Some(path.as_path()),
            BoardLocation::ArchivedProject(_) | BoardLocation::Home { .. } => None,
        }
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
        self.board_location = BoardLocation::Home {
            tab: BoardTab::Desk,
        };
        self.reanchor_selection(None, &previous_visible);
        self.seed_selection();
        self.clear_message();
    }

    /// Turn a read-only focus into the ordinary project focus on the same project
    /// (AC-43), keeping the selection where the user left it.
    pub(super) fn enter_project_focus(&mut self, path: PathBuf) {
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = BoardLocation::Project(path);
        self.reanchor_selection(previous, &previous_visible);
    }

    /// Open the read-only focus on `path` (AC-41). Session-only: nothing persists.
    pub(super) fn open_archived_focus(&mut self, path: PathBuf) {
        self.close_popup();
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = BoardLocation::ArchivedProject(path);
        self.reanchor_selection(previous, &previous_visible);
        if self.selection_id.is_none() {
            self.seed_selection();
        }
    }

    /// Home boards use the invocation default; project focus defaults to that project.
    pub(super) fn quick_add_scope(&self) -> Option<TaskScope> {
        match &self.board_location {
            BoardLocation::Home { .. } => self.session_default_scope.clone(),
            // Quick-add is refused outright in read-only focus, so its scope is moot.
            BoardLocation::Project(path) | BoardLocation::ArchivedProject(path) => {
                Some(TaskScope::Project {
                    path: path.to_string_lossy().into_owned(),
                })
            }
        }
    }

    /// Cancel an armed project-header double-click when another pointer target
    /// intervenes. Mouse-boundary state only, never persisted.
    pub(crate) fn cancel_project_header_double_click(&mut self) {
        self.last_project_header_click = None;
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
            let repo_path = repo.to_string_lossy().into_owned();
            if !self.archived_projects.contains(&repo_path) {
                push(repo, &mut paths);
            }
        }
        let mut from_tasks: Vec<PathBuf> = Vec::new();
        for task in &self.tasks {
            if task.soft_deleted {
                continue;
            }
            if let TaskScope::Project { path } = &task.scope {
                if self.archived_projects.contains(path) {
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

    /// Queue sections + counts for the current session location.
    pub fn queue_view(&self) -> QueueView {
        queue::query_board(
            &self.tasks,
            &self.archived_projects,
            self.this_repo.as_deref(),
            self.board_location.lens(),
            self.drawer_open,
        )
    }

    pub(super) fn toggle_project_collapsed(&mut self, path: &str) {
        if self.collapsed_projects.contains(path) {
            self.collapsed_projects.remove(path);
        } else {
            self.collapsed_projects.insert(path.to_string());
        }
    }

    pub(super) fn toggle_thread_collapsed(&mut self, name: &str) {
        if self.collapsed_threads.contains(name) {
            self.collapsed_threads.remove(name);
        } else {
            self.collapsed_threads.insert(name.to_string());
        }
    }

    pub(super) fn toggle_thread_project_collapsed(&mut self, key: ThreadProjectCollapseKey) {
        if self.collapsed_thread_projects.contains(&key) {
            self.collapsed_thread_projects.remove(&key);
        } else {
            self.collapsed_thread_projects.insert(key);
        }
    }

    /// Collapse every top-level group on the active home tab, or expand them when they are
    /// already all collapsed. Thread-project subgroups stay independent: Ctrl+G acts on the
    /// named thread groups the Threads tab presents at its top level.
    pub(super) fn toggle_all_groups(&mut self) -> bool {
        let view = self.queue_view();
        // AC-38: the archived group is one of the board's groups. While the drawer is
        // open it votes on the direction and folds with the rest; with the drawer shut
        // toggle-all never touches its state.
        let archived_joins = self.drawer_open
            && view
                .sections
                .iter()
                .any(|section| section.kind == SectionKind::Archived);
        let fold_archived = |model: &mut Self, collapsed: bool| {
            if archived_joins {
                model.archived_collapsed = collapsed;
            }
        };
        match self.board_location {
            BoardLocation::Home {
                tab: BoardTab::Projects,
            } => {
                let paths: Vec<String> = view
                    .sections
                    .iter()
                    .filter_map(|section| section.project_label.clone())
                    .collect();
                if paths.is_empty() && !archived_joins {
                    return false;
                }
                let all_collapsed = paths
                    .iter()
                    .all(|path| self.collapsed_projects.contains(path))
                    && (!archived_joins || self.archived_collapsed);
                if all_collapsed {
                    self.collapsed_projects.clear();
                } else {
                    self.collapsed_projects.extend(paths);
                }
                fold_archived(self, !all_collapsed);
                true
            }
            BoardLocation::Home {
                tab: BoardTab::Threads,
            } => {
                let threads: Vec<String> = view
                    .sections
                    .iter()
                    .filter_map(|section| section.thread_label.clone())
                    .collect();
                if threads.is_empty() && !archived_joins {
                    return false;
                }
                let all_collapsed = threads
                    .iter()
                    .all(|thread| self.collapsed_threads.contains(thread))
                    && (!archived_joins || self.archived_collapsed);
                if all_collapsed {
                    self.collapsed_threads.clear();
                } else {
                    self.collapsed_threads.extend(threads);
                }
                fold_archived(self, !all_collapsed);
                true
            }
            // Desk and project focus have no top-level groups of their own, but the open
            // drawer's archived group still answers the chord.
            _ => {
                if archived_joins {
                    let collapsed = self.archived_collapsed;
                    fold_archived(self, !collapsed);
                    true
                } else {
                    false
                }
            }
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
        visible_task_ids(
            &self.queue_view(),
            self.board_location.lens(),
            &self.collapsed_projects,
            &self.collapsed_threads,
            &self.collapsed_thread_projects,
            self.archived_collapsed,
        )
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
        self.retarget_selection(Some(pending.id), SelectionRetarget::Reanchor);
        self.reveal_task_on_home(pending.id);
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
        if pending.reopen {
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

    /// Park an existing-step draft before another task field becomes active. New-step add keeps
    /// its focused save-and-next workflow, so it deliberately cannot leave its inline row.
    pub(super) fn park_rename_step_draft(&mut self) -> bool {
        if self.input_mode != BoardInputMode::EditStep {
            return true;
        }
        let Some(form) = self.form.as_mut().filter(|form| form.is_task()) else {
            return false;
        };
        let Some(editor) = form.steps.editor.take() else {
            return true;
        };
        let Some(step_id) = editor.rename else {
            form.steps.editor = Some(editor);
            return false;
        };
        form.steps.drafts.insert(step_id, editor.buffer);
        true
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
    /// Seed selection on open: first visible IN MOTION id, else first visible ON DECK id.
    ///
    /// Honors collapse state via [`Self::visible_ids`]: a seeded row must be one the
    /// renderer painted, or verbs would mutate a task the user cannot see.
    pub(super) fn seed_selection(&mut self) {
        let visible: HashSet<Uuid> = self.visible_ids().into_iter().collect();
        let view = self.queue_view();
        for kind in [SectionKind::InMotion, SectionKind::OnDeck] {
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

    /// Set the session-only project focus. `None` returns home on the Desk tab.
    pub fn set_selected_project(&mut self, project: Option<PathBuf>) {
        self.set_board_scope(match project {
            None => ProjectScopeOption::Home,
            Some(path) => ProjectScopeOption::Project(path),
        });
    }

    pub(super) fn set_board_scope(&mut self, scope: ProjectScopeOption) {
        self.close_popup();
        let previous_visible = self.visible_ids();
        let previous = self.selection_id;
        self.board_location = match scope {
            ProjectScopeOption::Home => BoardLocation::Home {
                tab: BoardTab::Desk,
            },
            ProjectScopeOption::Project(path) => BoardLocation::Project(path),
        };
        self.reanchor_selection(previous, &previous_visible);
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
    fn toggle_all_groups_toggles_only_the_active_home_tabs_top_level_groups() {
        let mut domain = DomainState::new();
        let a = create(&mut domain, "task-a", project(REPO_A));
        let b = create(&mut domain, "task-b", project(REPO_B));
        domain
            .edit(
                a,
                "task-a",
                None,
                project(REPO_A),
                Some("release".to_string()),
            )
            .expect("thread a");
        domain
            .edit(
                b,
                "task-b",
                None,
                project(REPO_B),
                Some("release".to_string()),
            )
            .expect("thread b");

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.set_home_tab(BoardTab::Projects);
        assert!(model.toggle_all_groups());
        assert!(model.visible_ids().is_empty());
        assert!(model.toggle_all_groups());
        assert_eq!(model.visible_ids().len(), 2);

        model.set_home_tab(BoardTab::Threads);
        assert!(model.toggle_all_groups());
        assert!(model.visible_ids().is_empty());
        assert!(model.toggle_all_groups());
        assert_eq!(model.visible_ids().len(), 2);

        model.set_home_tab(BoardTab::Desk);
        assert!(!model.toggle_all_groups());
    }

    #[test]
    fn seed_selection_skips_rows_hidden_by_collapse() {
        let mut domain = DomainState::new();
        let in_a = create(&mut domain, "task-a", project(REPO_A));
        let in_b = create(&mut domain, "task-b", project(REPO_B));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Home {
            tab: BoardTab::Projects,
        };
        model.toggle_project_collapsed(REPO_A);
        model.selection_id = None;

        model.seed_selection();

        assert_eq!(
            model.selected_id(),
            Some(in_b),
            "seed must land on a painted row, never inside a collapsed group"
        );
        assert_ne!(model.selected_id(), Some(in_a));
    }

    #[test]
    fn sync_from_domain_reanchors_by_prior_order_not_new_order() {
        let mut domain = DomainState::new();
        let t1 = create(&mut domain, "task-1", project(REPO_A));
        let t2 = create(&mut domain, "task-2", project(REPO_A));
        let t3 = create(&mut domain, "task-3", project(REPO_A));
        let t4 = create(&mut domain, "task-4", project(REPO_A));

        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));
        model.board_location = BoardLocation::Home {
            tab: BoardTab::Projects,
        };
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
        assert_eq!(model.home_tab(), BoardTab::Desk);

        // Another process adds a project task between two syncs.
        create(&mut domain, "external task", project(REPO_B));
        model.sync_from_domain(&domain);

        assert_eq!(
            model.home_tab(),
            BoardTab::Desk,
            "a background merge must not move the user's home tab"
        );
        assert_eq!(model.selected_id(), Some(desk_task));
    }

    #[test]
    fn sync_from_domain_surfaces_externally_created_tasks_on_an_empty_board() {
        let domain = DomainState::new();
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO_A)));

        // Another process captures the first task while this board shows nothing.
        let mut domain = DomainState::new();
        let captured = create(&mut domain, "first capture", project(REPO_B));
        model.sync_from_domain(&domain);

        assert_eq!(
            model.home_tab(),
            BoardTab::Projects,
            "an otherwise-empty view follows the arriving task so it renders"
        );
        assert_eq!(model.selected_id(), Some(captured));
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
