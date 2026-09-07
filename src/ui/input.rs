//! Board and capture keyboard maps: key events → intents.
//!
//! Host open-board is not an in-app key.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::HumanStatus;

use super::board::{BoardInputMode, PickerTab};
use super::capture::{CaptureField, CaptureScopeChoice};
use super::tier::{ResponsivePresentation, WideStage};

/// Board primary actions reachable by keyboard inside the board.
///
/// Excludes host-level open-board and capture save/cancel (those are capture-mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimaryBoardAction {
    SelectTask,
    OpenCapture,
    EditTitle,
    Complete,
    Reopen,
    SoftDelete,
    Undo,
}

/// Capture-mode primary actions reachable by keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimaryCaptureAction {
    SaveCapture,
    CancelCapture,
    /// Title/notes typing and field focus (Tab).
    EditField,
    ChangeScope,
}

/// Every capture primary action (for table coverage tests).
pub const PRIMARY_CAPTURE_ACTIONS: &[PrimaryCaptureAction] = &[
    PrimaryCaptureAction::SaveCapture,
    PrimaryCaptureAction::CancelCapture,
    PrimaryCaptureAction::EditField,
    PrimaryCaptureAction::ChangeScope,
];

/// Intents produced by the capture key map and mouse map.
///
/// Not `Copy`: [`CaptureIntent::InsertText`] carries a pasted run, so one typed intent
/// stays the single route into the editor rather than a side channel beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureIntent {
    Insert(char),
    /// Insert a pasted run at the cursor.
    InsertText(String),
    /// Split the focused draft at the cursor.
    InsertLineBreak,
    Backspace,
    /// Remove the character at the cursor.
    DeleteForward,
    /// One character toward the start / end of the draft.
    MoveLeft,
    MoveRight,
    /// One WRAPPED row up / down in the multiline Notes draft.
    MoveUp,
    MoveDown,
    /// Either end of the cursor's own line.
    MoveLineStart,
    MoveLineEnd,
    /// The nearest word boundary in the asked direction.
    MoveWordLeft,
    MoveWordRight,
    FocusNext,
    FocusPrev,
    /// Direct field focus (mouse click on field row).
    FocusField(CaptureField),
    /// Cycle Global ↔ this_repo project scope.
    CycleScope,
    /// Choose one explicit scope control: This project / Global / Other….
    SelectScope(CaptureScopeChoice),
    /// Begin typing an arbitrary project path when scope is focused.
    BeginScopePathEdit,
    Save,
    Cancel,
    /// Retry the exact submitted draft after a persistence failure.
    RetrySave,
    /// Abandon the submitted draft and restore the pre-create baseline.
    CancelSave,
}

/// Bottom chrome: compact key legend for capture form.
pub const CAPTURE_HELP_LINE: &str =
    "Tab fields  ·  1–3 scope  ·  3 other path  ·  Enter save  ·  Esc cancel";
/// Bottom chrome while Notes has focus: Enter opens a line there, so both save encodings stay
/// visible. Alt+Enter works when a legacy terminal cannot distinguish modified Enter keys.
pub const CAPTURE_NOTES_HELP_LINE: &str =
    "Tab fields  ·  Ctrl+Enter/Alt+Enter save  ·  Enter newline  ·  Esc cancel";
/// Compact legend shown while a Capture save is unresolved.
pub const CAPTURE_SAVE_RECOVERY_HELP_LINE: &str = "r retry  ·  c cancel";

/// Board primary actions reachable by an the normal-mode key (table coverage).
pub const PRIMARY_BOARD_ACTIONS: &[PrimaryBoardAction] = &[
    PrimaryBoardAction::SelectTask,
    PrimaryBoardAction::OpenCapture,
    PrimaryBoardAction::EditTitle,
    PrimaryBoardAction::Complete,
    PrimaryBoardAction::Reopen,
    PrimaryBoardAction::SoftDelete,
    PrimaryBoardAction::Undo,
];

/// Intents produced by the board key map and mouse map.
///
/// Not `Copy`: [`BoardIntent::EditInsertText`] carries a pasted run, so one typed intent
/// stays the single route into the editor rather than a side channel beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardIntent {
    Quit,
    SelectNext,
    SelectPrev,
    /// Select visible list row by index (mouse row click).
    SelectIndex(usize),
    /// Select a wide-split board row and return board focus without peek or double-click.
    FocusBoardAndSelectIndex(usize),
    /// Copy the presentation-only `T<number>` identifier for one persisted task.
    CopyTaskNumber(uuid::Uuid),
    /// Jump the list viewport to a content offset without changing selection or peek
    /// (list scrollbar track click / thumb drag).
    ListScrollTo(usize),
    /// Jump the task-page body to a content offset (page scrollbar track click / drag).
    PageScrollTo(usize),
    SetStatus(HumanStatus),
    Complete,
    Reopen,
    SoftDelete,
    Undo,
    BeginEditTitle,
    BeginEditNotes,
    /// Open the same bound task form as `e`, focused on Scope (palette Change scope).
    BeginEditScope,
    /// Open the steps section's one-line editor empty to add an step (page `a`).
    BeginAddStep,
    /// Move focus through the shared capture/task form fields.
    FormFocusNext,
    FormFocusPrev,
    /// Focus one painted field of the already-open shared form (mouse).
    FocusFormField(CaptureField),
    /// Enter or a second click opens or closes the selected task-page Thread text editor.
    /// It never commits or leaves the enclosing task edit session.
    ToggleThreadEditing,
    /// Cycle the shared form's chosen task scope directly from its Scope field.
    FormCycleScope,
    /// Open the shared form's keyboard scope dropdown, move its pending selection, apply it,
    /// or return to the parent form without applying it.
    OpenFormScopeDropdown,
    FormScopeNext,
    FormScopePrev,
    ConfirmFormScopeDropdown,
    CancelFormScopeDropdown,
    /// Choose a form dropdown option by its painted source index, apply only its parent
    /// draft, and return to that form (mouse).
    SelectFormScopeOption(usize),
    EditInsert(char),
    /// Insert a pasted run at the cursor.
    EditInsertText(String),
    /// Split the draft at the cursor.
    EditInsertLineBreak,
    EditBackspace,
    /// Remove the character at the cursor.
    EditDeleteForward,
    /// One character toward the start / end of the draft.
    EditMoveLeft,
    EditMoveRight,
    EditMoveUp,
    EditMoveDown,
    /// Either end of the cursor's own line.
    EditMoveLineStart,
    EditMoveLineEnd,
    /// The nearest word boundary in the asked direction.
    EditMoveWordLeft,
    EditMoveWordRight,
    ConfirmEdit,
    /// Shift+Enter in the inline step editor: save and continue. Add mode reopens the
    /// empty next row, while rename mode downgrades to a plain save-and-close (decided by
    /// the reducer from the editor's own mode).
    ConfirmEditNext,
    CancelEdit,
    /// Status-row quick-add edits and actions.
    QuickAddInsert(char),
    QuickAddInsertText(String),
    QuickAddBackspace,
    QuickAddDeleteForward,
    QuickAddMoveLeft,
    QuickAddMoveRight,
    QuickAddMoveLineStart,
    QuickAddMoveLineEnd,
    QuickAddMoveWordLeft,
    QuickAddMoveWordRight,
    QuickAddSave,
    QuickAddSaveNext,
    ExpandQuickAdd,
    CancelQuickAdd,
    /// A list click while quick-add is open discards the draft, then selects its row.
    QuickAddSelectIndex(usize),
    OpenCapture,
    /// Open the session project selector for the Projects lens (`P` or the chip).
    OpenProjectSelector,
    /// Move the project selector's option.
    ProjectPickerNext,
    ProjectPickerPrev,
    /// Adopt the highlighted project for this board session only.
    ConfirmProjectChoice,
    /// Close the project selector without changing the selection.
    CancelProjectPicker,
    /// Select project selector option by index and adopt it in one step (mouse click on a
    /// dropdown row;,,). The mouse-only counterpart of `SelectIndex` for
    /// the task list: it runs the same effect `ConfirmProjectChoice` does after enough
    /// `ProjectPickerNext`/`ProjectPickerPrev` presses reached this option.
    SelectProjectOption(usize),
    /// Switch to a persistent navigation tab (`1` desk · `2` selected project · `3`
    /// projects). Tab 2 with no project opens the picker instead.
    SelectNavTab(crate::ui::queue::NavTab),
    /// Open the project board's searchable thread filter picker (bare `t`).
    OpenThreadFilterPicker,
    /// Open the projects index's searchable View picker (bare `v`).
    OpenProjectsViewPicker,
    /// Focus the visible projects-index search field (`/` or a click).
    FocusProjectsSearch,
    /// Move the open list picker's selection.
    ListPickerNext,
    ListPickerPrev,
    /// Apply the highlighted list-picker option.
    ConfirmListPicker,
    /// Close the list picker without applying anything.
    CancelListPicker,
    /// Choose a visible list-picker row by index and apply it in one step (mouse).
    SelectListOption(usize),
    /// Type into the open list picker's search query.
    ListPickerQueryInsert(char),
    /// Paste into the open list picker's search query.
    ListPickerQueryInsertText(String),
    ListPickerQueryBackspace,
    /// Type into the projects index's search.
    ProjectsQueryInsert(char),
    /// Paste into the projects index's search.
    ProjectsQueryInsertText(String),
    ProjectsQueryBackspace,
    /// Mouse route onto a projects index row: select it and open its project in slot 2.
    SelectProjectRow(usize),
    /// Move the task page's step cursor onto one steps step by its painted absolute
    /// index (mouse click on an step row; AC-21). A click selects — it never toggles the
    /// step, opens the editor, or arms the delete mark; no key produces it.
    SelectStep(usize),
    /// Retry the exact failed board persistence state.
    RetrySave,
    /// Restore the last persisted board state and abandon the failed mutation.
    CancelSave,
    /// Open the searchable command palette (`:`).
    OpenCommandPalette,
    /// Move the command-surface selection.
    CommandNext,
    CommandPrev,
    /// Invoke the selected command through its existing intent route.
    ConfirmCommand,
    /// Select command-surface row by index and confirm it in one step (mouse click on a
    /// palette/action-sheet row; G-7,,,). The mouse-only counterpart of
    /// `SelectIndex` for the task list and `SelectProjectOption` for the project dropdown:
    /// `map_board_mouse` takes `&BoardModel`, so a click cannot set `command_selected` itself,
    /// and a click names a row directly rather than stepping `CommandNext`/`CommandPrev` to it
    /// first. Resolved by `resolve_board_command` exactly the way `ConfirmCommand` is, so a
    /// click tears the surface down the same way Enter does rather than bypassing that
    /// teardown to hand back the row's intent directly.
    SelectCommand(usize),
    /// Close the open action sheet or palette without any domain action.
    CloseCommandSurface,
    /// Narrow the palette query.
    CommandQueryInsert(char),
    /// Narrow the palette query by a pasted run.
    ///
    /// The query is one search line, so the reducer folds each pasted break to a space.
    CommandQueryInsertText(String),
    CommandQueryBackspace,
    /// Retained no-op seam for the non-live launch walkthrough helper.
    OpenWalkthrough,
    /// `ctrl+s` — state-mapped primary verb. Reducer lands in.
    PrimaryVerb,
    /// `ctrl+b` — toggle blocked ↔ ready. Reducer lands in.
    ToggleBlock,
    /// `ctrl+r` — toggle review ↔ ready. Reducer lands in.
    ToggleReview,
    /// Help card: scroll its key list one row.
    HelpScrollUp,
    HelpScrollDown,
    /// `Enter` on a stored task-page step: flip it between done and ready.
    ToggleStep,
    /// `Enter` opens the selected task as a full-page view in single-pane presentation.
    OpenTaskPage,
    /// `→` at wide widths: move the stage slider one step towards the task (0 → A → G → F).
    StageRight,
    /// `←` at wide widths: move the stage slider one step towards the board (F → G → A → 0).
    StageLeft,
    /// `→` — expand the selected row's inline peek (notes preview under the row).
    PeekDetail,
    /// `←` — collapse the inline peek when one is open; a no-op otherwise.
    CollapseDetail,
    /// Task page view mode: scroll the notes body one wrapped row up.
    PageScrollUp,
    /// Task page view mode: scroll the notes body one wrapped row down.
    PageScrollDown,
    /// Mouse-wheel scrolling is content-only: it never enters step-cursor navigation
    /// when the shared page body has reached an edge.
    PageWheelScrollUp,
    PageWheelScrollDown,
    /// `z` — open/close the done drawer. Reducer lands in.
    ToggleDoneDrawer,
    /// `ctrl+f` — the file verb: toggle the task's archived flag (picker: archive/
    /// unarchive the selected project). No undo entry. Reducer lands in.
    File,
    /// Picker `Tab` / `←` / `→`: flip the project selector between its main and
    /// archived tabs. Reducer lands in.
    ProjectPickerSwitchTab,
    /// Launch card: unarchive the archived project the default pointed at.
    LaunchUnarchive,
    /// Launch card: keep the project archived; quick-add goes to the desk this session.
    LaunchKeepArchived,
    /// Mouse route onto the picker's painted tab row.
    SelectPickerTab(PickerTab),
    /// Expand/collapse the done drawer's archived group (Enter or click on its header,
    /// which the intent also selects). Session-only. Reducer lands in.
    ToggleArchivedGroup,
    /// `?` — open the help card. Surface wiring lands in.
    OpenHelp,
    /// `Esc` — layered close. Full layer order lands in.
    CloseLayer,
    /// Toggle all group headers on the active home tab (`Ctrl+G`).
    ToggleAllGroups,
}

/// Bottom chrome: compact key legend for primary board actions.
pub const BOARD_HELP_LINE: &str = "↑↓/jk  ·  ctrl+s start  ·  enter open  ·  → peek  ·  ctrl+d done  ·  ctrl+o reopen  ·  ctrl+b block  ·  ctrl+r review  ·  + add  ·  ctrl+e title  ·  ctrl+x del  ·  ctrl+u undo  ·  ctrl+f archive  ·  d drawer  ·  g archived  ·  p projects  ·  : palette  ·  ? help  ·  ctrl+q quit";
/// Compact legend shown while the action sheet or command palette is open.
pub const COMMAND_SURFACE_HELP_LINE: &str =
    "↑↓ select  ·  type to filter  ·  Enter run  ·  Esc close";
/// Compact legend shown while the help card is open.
pub const HELP_SURFACE_HELP_LINE: &str = "↑↓ scroll  ·  esc close";
/// Compact legend shown while a failed board save is unresolved.
pub const LAUNCH_CARD_HELP_LINE: &str = "y unarchive · n keep archived";
pub const SAVE_RECOVERY_HELP_LINE: &str = "↑↓  ·  r retry  ·  c cancel";
/// Compact legend while the projects index search field owns input.
pub const PROJECTS_SEARCH_HELP_LINE: &str = "/ search  ·  type  ·  enter open  ·  esc clear";
/// Compact legend shown while the first-use walkthrough is open.
pub const WALKTHROUGH_HELP_LINE: &str = "Enter next  ·  Esc skip";

/// One normal-mode key → intent entry. Help text and `map_normal` share this table.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalKeyEntry {
    code: KeyCode,
    intent: BoardIntent,
    help_chord: &'static str,
    help_label: &'static str,
    /// True when this binding requires Ctrl.
    verb: bool,
}

/// Canonical the normal-mode map. Single source for keys and help.
const NORMAL_KEYMAP: &[NormalKeyEntry] = &[
    NormalKeyEntry {
        code: KeyCode::Char('j'),
        intent: BoardIntent::SelectNext,
        help_chord: "↑↓ / jk",
        help_label: "move",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Down,
        intent: BoardIntent::SelectNext,
        help_chord: "↑↓ / jk",
        help_label: "move",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char('k'),
        intent: BoardIntent::SelectPrev,
        help_chord: "↑↓ / jk",
        help_label: "move",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Up,
        intent: BoardIntent::SelectPrev,
        help_chord: "↑↓ / jk",
        help_label: "move",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Enter,
        intent: BoardIntent::OpenTaskPage,
        help_chord: "enter",
        help_label: "open",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Right,
        intent: BoardIntent::PeekDetail,
        help_chord: "→ / ←",
        help_label: "peek",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Left,
        intent: BoardIntent::CollapseDetail,
        help_chord: "→ / ←",
        help_label: "peek",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Esc,
        intent: BoardIntent::CloseLayer,
        help_chord: "esc",
        help_label: "close",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char('s'),
        intent: BoardIntent::PrimaryVerb,
        help_chord: "s",
        help_label: "start / reopen",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('d'),
        intent: BoardIntent::Complete,
        help_chord: "d",
        help_label: "done",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('o'),
        intent: BoardIntent::Reopen,
        help_chord: "o",
        help_label: "reopen",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('b'),
        intent: BoardIntent::ToggleBlock,
        help_chord: "b",
        help_label: "block",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('r'),
        intent: BoardIntent::ToggleReview,
        help_chord: "r",
        help_label: "review",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('e'),
        intent: BoardIntent::BeginEditTitle,
        help_chord: "e",
        help_label: "edit title",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('x'),
        intent: BoardIntent::SoftDelete,
        help_chord: "x / delete",
        help_label: "delete",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Delete,
        intent: BoardIntent::SoftDelete,
        help_chord: "x / delete",
        help_label: "delete",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('u'),
        intent: BoardIntent::Undo,
        help_chord: "u",
        help_label: "undo",
        verb: true,
    },
    NormalKeyEntry {
        code: KeyCode::Char('f'),
        intent: BoardIntent::File,
        help_chord: "f",
        help_label: "archive",
        verb: true,
    },
    // `+` opens an input surface, like bare `:` palette, `z` drawer, and `?` help. It is
    // not a task-mutating verb, so it does not take the configured verb modifier.
    NormalKeyEntry {
        code: KeyCode::Char('+'),
        intent: BoardIntent::OpenCapture,
        help_chord: "+",
        help_label: "add",
        verb: false,
    },
    // Bare `d` opens the done drawer; the same letter with Ctrl is the done verb. The
    // map keys on (code, modifier class), so both live here.
    NormalKeyEntry {
        code: KeyCode::Char('d'),
        intent: BoardIntent::ToggleDoneDrawer,
        help_chord: "d",
        help_label: "done drawer",
        verb: false,
    },
    // Bare `g` folds the drawer's archived group; inert while the drawer is closed.
    NormalKeyEntry {
        code: KeyCode::Char('g'),
        intent: BoardIntent::ToggleAllGroups,
        help_chord: "g",
        help_label: "archived group",
        verb: false,
    },
    // The project slot is mouse-clickable; `p` gives the keyboard the same route.
    NormalKeyEntry {
        code: KeyCode::Char('p'),
        intent: BoardIntent::OpenProjectSelector,
        help_chord: "p",
        help_label: "projects",
        verb: false,
    },
    // `t` opens the project board's thread filter; `v` the projects index's View
    // selector. Both are navigation (bare, not verbs); the reducer gates each to its
    // own destination, so they are inert everywhere else.
    NormalKeyEntry {
        code: KeyCode::Char('t'),
        intent: BoardIntent::OpenThreadFilterPicker,
        help_chord: "t",
        help_label: "threads",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char('v'),
        intent: BoardIntent::OpenProjectsViewPicker,
        help_chord: "v",
        help_label: "views",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char(':'),
        intent: BoardIntent::OpenCommandPalette,
        help_chord: ":",
        help_label: "palette",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char('?'),
        intent: BoardIntent::OpenHelp,
        help_chord: "?",
        help_label: "help",
        verb: false,
    },
    NormalKeyEntry {
        code: KeyCode::Char('q'),
        intent: BoardIntent::Quit,
        help_chord: "q",
        help_label: "quit",
        verb: true,
    },
];

/// Look up a normal-mode key's help label by its literal key code text (e.g. `"d"`,
/// `"b"`, `":"`, `"?"`), i.e. the same word [`NORMAL_KEYMAP`] uses to describe it on the
/// help card. Min-5: the verb bar's static entries (the ones that are not context-dependent
/// per row) borrow their word from here instead of duplicating a literal that can drift out
/// of sync with the keymap, the way `: more` drifted from the palette's real name (`:` opens
/// the command palette, not a "more" menu -- that word belonged to a retired classic surface).
///
/// `space` and `enter` are deliberately not looked up here: `PrimaryVerb` and `OpenTaskPage`
/// are context-dependent per selected row, so the verb bar computes their word from
/// task state rather than the keymap's one generic word (`primary` / `detail`) for them.
pub fn keymap_help_label(chord: &str) -> Option<&'static str> {
    NORMAL_KEYMAP
        .iter()
        .find(|entry| entry.help_chord == chord)
        .map(|entry| entry.help_label)
}

/// Every active-tier help binding (chord, label), derived from the key map.
/// The complete unmodified normal-mode table, for documentation and regression guards.
pub fn normal_mode_keymap() -> Vec<(KeyCode, BoardIntent)> {
    NORMAL_KEYMAP
        .iter()
        .map(|entry| (entry.code, entry.intent.clone()))
        .collect()
}

pub fn normal_help_bindings() -> Vec<(&'static str, &'static str)> {
    let mut bindings = Vec::new();
    for entry in NORMAL_KEYMAP {
        let binding = (entry.help_chord, entry.help_label);
        if !bindings.contains(&binding) {
            bindings.push(binding);
        }
    }
    bindings
}

/// Whether a bare character is unbound in normal mode, and so types into destination
/// search lines (the projects index). Bound keys — nav, verbs, pickers — stay routes.
pub fn is_unbound_normal_char(character: char) -> bool {
    !NORMAL_KEYMAP
        .iter()
        .any(|entry| entry.code == KeyCode::Char(character))
}

/// The board section of the help card: every normal-mode binding, Ctrl chords spelled
/// out, plus the bare routes the keymap table does not own (`1`/`2`/`3`, `ctrl+c`).
fn board_help_bindings() -> Vec<(String, &'static str)> {
    let mut seen: Vec<(String, &'static str)> = Vec::new();
    for entry in NORMAL_KEYMAP {
        let chord = if entry.verb {
            format!("ctrl+{}", entry.help_chord)
        } else {
            entry.help_chord.to_string()
        };
        let binding = (chord, entry.help_label);
        if !seen.contains(&binding) {
            seen.push(binding);
        }
    }
    seen.push(("1 · 2 · 3".to_string(), "desk · project · projects"));
    seen.push(("/".to_string(), "search (projects)"));
    seen.push(("ctrl+c".to_string(), "quit"));
    seen
}

/// Task-page chords the help card lists under the board bindings. The page has no
/// `?` of its own, so the board's card is the one place a user can read them.
pub fn task_page_help_bindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("ctrl+e", "edit title"),
        ("ctrl+n", "edit notes"),
        ("ctrl+a", "add step"),
        ("tab / ↓", "select steps"),
        ("enter", "toggle step"),
        ("ctrl+s · ctrl+d · ctrl+b · ctrl+r", "task status"),
        ("ctrl+o", "reopen task"),
        ("ctrl+x", "delete step or task"),
        ("ctrl+f", "archive"),
        ("→ / ←", "wide stage"),
        ("esc", "close"),
    ]
}

/// Editing chords, shared by the task page's field editors and the quick-add draft.
fn editing_help_bindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("enter (title)", "next field"),
        ("enter (notes)", "new line"),
        ("enter (step)", "save step, next row"),
        ("shift+enter", "save edit"),
        ("tab / shift+tab", "next / previous field"),
        ("space (scope)", "cycle scope"),
        ("esc", "cancel"),
    ]
}

/// Quick-add bar chords and capture tokens.
fn quick_add_help_bindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("+", "open quick-add"),
        ("enter", "save"),
        ("shift+enter", "save, keep open"),
        ("tab", "expand to task page"),
        ("!p name · !p", "project · desk"),
        ("!t name · !t", "thread · none"),
    ]
}

/// Picker, palette, and help chords.
fn picker_help_bindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑↓ / jk", "move"),
        ("type", "filter"),
        ("enter", "choose"),
        ("ctrl+f (projects)", "archive project"),
        ("ctrl+u (projects)", "unarchive project"),
        ("tab (projects)", "main · archived"),
        ("esc", "close"),
    ]
}

/// Help-card body lines painted by the renderer: one section per surface, each a
/// two-column key list. Complete by construction for the board (it walks the keymap);
/// the other sections are the surfaces' own maps, kept in step by the keymap tests.
///
/// No heading or trailing close instruction: the shared modal card's own title (`help`)
/// and footer legend already say both.
pub fn help_card_lines() -> Vec<String> {
    let owned = |bindings: Vec<(&'static str, &'static str)>| -> Vec<(String, &'static str)> {
        bindings
            .into_iter()
            .map(|(chord, label)| (chord.to_string(), label))
            .collect()
    };
    let mut lines = Vec::new();
    lines.push(" board".to_string());
    lines.extend(paired_help_lines(&board_help_bindings()));
    for (title, bindings) in [
        (" task page", owned(task_page_help_bindings())),
        (" editing", owned(editing_help_bindings())),
        (" quick-add", owned(quick_add_help_bindings())),
        (" pickers · palette", owned(picker_help_bindings())),
    ] {
        lines.push(String::new());
        lines.push(title.to_string());
        lines.extend(paired_help_lines(&bindings));
    }
    lines
}

fn paired_help_lines(bindings: &[(String, &str)]) -> Vec<String> {
    bindings
        .chunks(2)
        .map(|pair| match pair {
            // Compact's 40-column minimum needs both bindings on one line.
            [(c1, l1), (c2, l2)] => format!(" {c1} {l1} | {c2} {l2}"),
            [(c1, l1)] => format!(" {c1} {l1}"),
            _ => String::new(),
        })
        .collect()
}

/// Map a key event to a board intent for the current input mode.
///
/// Only press (and repeat) events produce intents. Unknown keys → `None`.
pub fn map_key(mode: BoardInputMode, key: KeyEvent) -> Option<BoardIntent> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    match mode {
        BoardInputMode::Normal => map_normal(key),
        BoardInputMode::TaskPage => map_task_page(key),
        BoardInputMode::ProjectPicker => map_project_picker(key),
        BoardInputMode::ListPicker => map_list_picker(key),
        BoardInputMode::ProjectsSearch => map_projects_search(key),
        BoardInputMode::SaveRecovery => map_save_recovery(key),
        BoardInputMode::LaunchCard => map_launch_card(key),
        BoardInputMode::Palette => map_palette(key),
        BoardInputMode::Help => map_help(key),
        BoardInputMode::QuickAdd => map_quick_add_key(key),
        BoardInputMode::FormScopeDropdown => map_board_form_key(CaptureField::Scope, true, key),
        BoardInputMode::EditScope => map_board_form_key(CaptureField::Scope, false, key),
        BoardInputMode::SelectThread => map_selected_thread_key(key),
        BoardInputMode::EditThread => map_thread_edit_key(key),
        BoardInputMode::EditTitle | BoardInputMode::EditNotes => map_edit(mode, key),
        // A step edit belongs to the retained task form, not a modal editor. Enter saves one
        // independent add; Shift+Enter saves the session. Alt+Enter is not a save route on
        // the board.
        BoardInputMode::EditStep => {
            let shift_save = key.modifiers.contains(KeyModifiers::SHIFT)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
            if key.code == KeyCode::Enter && shift_save {
                Some(BoardIntent::ConfirmEditNext)
            } else if key.code == KeyCode::Enter && key.modifiers.is_empty() {
                Some(BoardIntent::ConfirmEdit)
            } else if matches!(key.code, KeyCode::Up | KeyCode::Down) && key.modifiers.is_empty() {
                Some(if key.code == KeyCode::Up {
                    BoardIntent::PageScrollUp
                } else {
                    BoardIntent::PageScrollDown
                })
            } else if key.modifiers.contains(KeyModifiers::CONTROL)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
            {
                match key.code {
                    KeyCode::Char('a') => Some(BoardIntent::BeginAddStep),
                    KeyCode::Char('d') => Some(BoardIntent::Complete),
                    KeyCode::Char('o') => Some(BoardIntent::Reopen),
                    KeyCode::Char('x') | KeyCode::Delete => Some(BoardIntent::SoftDelete),
                    _ => {
                        map_form_edit_key(CaptureField::Title, FormEditNavigation::Form, true, key)
                    }
                }
            } else {
                map_form_edit_key(CaptureField::Title, FormEditNavigation::Form, true, key)
            }
        }
    }
}

/// How one key routes at the responsive boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsiveKeyRoute {
    /// A stage-slider intent (or a stage key the current surface already maps).
    Intent(BoardIntent),
    /// A stage key that does nothing in this stage (`←` in 0, `→` in F).
    Inert,
    /// Not a stage key: the surface's own map owns it.
    Surface,
}

/// Route the stage-slider keys of the wide layout.
///
/// Only bare `→` / `←` in the Normal and TaskPage view modes are stage keys, and only while
/// the frame is wide. The mode check does the scoping: `Normal` slides the board-owned stages
/// and `TaskPage` the task-owned ones, so every field editor (its own mode, never these two)
/// keeps its existing arrow semantics without being named here. A task edit session parked in
/// view mode (a dirty draft with no editor open) still slides: the pane stays bound, so
/// nothing is lost. `Enter` and `Esc` keep their surface meaning (`OpenTaskPage` records the
/// origin stage; `CloseLayer` restores it), and `Tab` is never a stage key. Below the wide
/// threshold nothing here fires.
pub fn route_responsive_key(
    mode: BoardInputMode,
    stage: WideStage,
    presentation: ResponsivePresentation,
    key: KeyEvent,
) -> ResponsiveKeyRoute {
    if presentation != ResponsivePresentation::WideSplit
        || !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        || !key.modifiers.is_empty()
    {
        return ResponsiveKeyRoute::Surface;
    }
    let board_owned =
        mode == BoardInputMode::Normal && matches!(stage, WideStage::FullBoard | WideStage::Split);
    let task_owned =
        mode == BoardInputMode::TaskPage && matches!(stage, WideStage::Rail | WideStage::FullTask);
    if !board_owned && !task_owned {
        return ResponsiveKeyRoute::Surface;
    }
    match (key.code, stage) {
        (KeyCode::Right, WideStage::FullTask) | (KeyCode::Left, WideStage::FullBoard) => {
            ResponsiveKeyRoute::Inert
        }
        (KeyCode::Right, _) => ResponsiveKeyRoute::Intent(BoardIntent::StageRight),
        (KeyCode::Left, _) => ResponsiveKeyRoute::Intent(BoardIntent::StageLeft),
        _ => ResponsiveKeyRoute::Surface,
    }
}

/// Map the selected task-page Thread footer. It is a navigation target until Enter or a
/// second click deliberately opens the text cursor.
fn map_selected_thread_key(key: KeyEvent) -> Option<BoardIntent> {
    if key.code == KeyCode::Enter && key.modifiers.is_empty() {
        return Some(BoardIntent::ToggleThreadEditing);
    }
    // A selected footer is navigation, not an invisible editor. Retain only session controls
    // from the shared field map, so printable input cannot mutate a draft without a cursor.
    match map_form_edit_key(CaptureField::Thread, FormEditNavigation::Form, true, key) {
        intent @ Some(
            BoardIntent::BeginAddStep
            | BoardIntent::ConfirmEdit
            | BoardIntent::CancelEdit
            | BoardIntent::FormFocusNext
            | BoardIntent::FormFocusPrev,
        ) => intent,
        _ => None,
    }
}

/// Map an active task-page Thread editor. Plain Enter returns to the selected footer, retaining
/// its draft and the surrounding task session; Shift+Enter remains the sole save route.
fn map_thread_edit_key(key: KeyEvent) -> Option<BoardIntent> {
    if key.code == KeyCode::Enter && key.modifiers.is_empty() {
        return Some(BoardIntent::ToggleThreadEditing);
    }
    map_form_edit_key(CaptureField::Thread, FormEditNavigation::Form, true, key)
}

/// Focus-navigation intents layered over the shared field editor map.
///
/// A real board form uses `Form`; single-field edits have no focus navigation. Compatibility
/// wrappers may translate the form focus intent after this shared mapper returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormEditNavigation {
    None,
    Form,
}

/// Map one key for either kind of board form.
///
/// Unlike standalone quick capture, board capture and task editing emit [`BoardIntent`]s. They
/// differ only in the immutable value held by the form, so this is a thin form-navigation wrapper
/// over [`map_form_edit_key`], the sole field-edit implementation.
/// Map the one-line quick-add status input. Its editing chords intentionally match Title.
pub fn map_quick_add_key(key: KeyEvent) -> Option<BoardIntent> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Enter if shift && !ctrl && !alt => return Some(BoardIntent::QuickAddSaveNext),
        KeyCode::Tab if mods.is_empty() => return Some(BoardIntent::ExpandQuickAdd),
        KeyCode::Char('a') if ctrl => return Some(BoardIntent::QuickAddMoveLineStart),
        KeyCode::Char('e') if ctrl => return Some(BoardIntent::QuickAddMoveLineEnd),
        KeyCode::Left if ctrl => return Some(BoardIntent::QuickAddMoveWordLeft),
        KeyCode::Right if ctrl => return Some(BoardIntent::QuickAddMoveWordRight),
        _ => {}
    }
    if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    match key.code {
        KeyCode::Enter => Some(BoardIntent::QuickAddSave),
        KeyCode::Esc => Some(BoardIntent::CancelQuickAdd),
        KeyCode::Backspace => Some(BoardIntent::QuickAddBackspace),
        KeyCode::Delete => Some(BoardIntent::QuickAddDeleteForward),
        KeyCode::Left => Some(BoardIntent::QuickAddMoveLeft),
        KeyCode::Right => Some(BoardIntent::QuickAddMoveRight),
        KeyCode::Home => Some(BoardIntent::QuickAddMoveLineStart),
        KeyCode::End => Some(BoardIntent::QuickAddMoveLineEnd),
        KeyCode::Char(character) if !character.is_control() => {
            Some(BoardIntent::QuickAddInsert(character))
        }
        _ => None,
    }
}

pub fn map_board_form_key(
    focused: CaptureField,
    dropdown_open: bool,
    key: KeyEvent,
) -> Option<BoardIntent> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    if dropdown_open {
        return map_form_scope_dropdown_key(key);
    }
    map_form_edit_key(focused, FormEditNavigation::Form, false, key)
}

/// Map a retained task form, where Ctrl+A owns inline step capture.
pub fn map_task_form_key(
    focused: CaptureField,
    dropdown_open: bool,
    key: KeyEvent,
) -> Option<BoardIntent> {
    if dropdown_open {
        map_form_scope_dropdown_key(key)
    } else {
        map_form_edit_key(focused, FormEditNavigation::Form, true, key)
    }
}

/// Scope dropdown keys are the one form-mode extra: it owns its temporary selection rather than
/// an editable field.
fn map_form_scope_dropdown_key(key: KeyEvent) -> Option<BoardIntent> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(BoardIntent::CancelFormScopeDropdown),
        KeyCode::Enter => Some(BoardIntent::ConfirmFormScopeDropdown),
        KeyCode::Up | KeyCode::Char('k') => Some(BoardIntent::FormScopePrev),
        KeyCode::Down | KeyCode::Char('j') => Some(BoardIntent::FormScopeNext),
        _ => None,
    }
}

/// Shared field-edit key map for board forms and the legacy mode-only edit entry point.
///
/// The chord table runs before modified-key rejection so bound Ctrl presses cannot fall
/// through to printable insertion.
fn map_form_edit_key(
    focused: CaptureField,
    navigation: FormEditNavigation,
    task_steps: bool,
    key: KeyEvent,
) -> Option<BoardIntent> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let super_key = mods.contains(KeyModifiers::SUPER);

    match key.code {
        // Shift+Enter is the one task-session save chord. Alt+Enter is not a fallback here
        // (standalone Capture keeps its own); a modified Enter that is not Shift is inert.
        KeyCode::Enter if shift && !alt && !ctrl && !super_key => {
            return Some(BoardIntent::ConfirmEdit)
        }
        KeyCode::Tab => match navigation {
            FormEditNavigation::None => {}
            FormEditNavigation::Form => return Some(BoardIntent::FormFocusNext),
        },
        KeyCode::BackTab => match navigation {
            FormEditNavigation::None => {}
            FormEditNavigation::Form => return Some(BoardIntent::FormFocusPrev),
        },
        KeyCode::Char('c') if ctrl => return Some(BoardIntent::CancelEdit),
        KeyCode::Char('a') if ctrl && task_steps => return Some(BoardIntent::BeginAddStep),
        KeyCode::Char('a') if ctrl && focused != CaptureField::Scope => {
            return Some(BoardIntent::EditMoveLineStart)
        }
        KeyCode::Char('e') if ctrl && focused != CaptureField::Scope => {
            return Some(BoardIntent::EditMoveLineEnd)
        }
        KeyCode::Left if ctrl && focused != CaptureField::Scope => {
            return Some(BoardIntent::EditMoveWordLeft)
        }
        KeyCode::Right if ctrl && focused != CaptureField::Scope => {
            return Some(BoardIntent::EditMoveWordRight)
        }
        _ => {}
    }

    // Shift is how a capital letter arrives, so it is not treated as a chord here.
    if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }

    match focused {
        CaptureField::Scope => match key.code {
            KeyCode::Esc => Some(BoardIntent::CancelEdit),
            KeyCode::Enter => Some(BoardIntent::OpenFormScopeDropdown),
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                Some(BoardIntent::FormCycleScope)
            }
            _ => None,
        },
        CaptureField::Title | CaptureField::Notes | CaptureField::Thread => match key.code {
            KeyCode::Enter if focused == CaptureField::Notes => {
                Some(BoardIntent::EditInsertLineBreak)
            }
            // A one-line Title is finished by Enter: focus moves on to Notes. Saving stays
            // Shift+Enter (matched above).
            KeyCode::Enter
                if focused == CaptureField::Title && navigation == FormEditNavigation::Form =>
            {
                Some(BoardIntent::FormFocusNext)
            }
            // A plain Enter must neither save a thread nor close an inline step editor.
            KeyCode::Enter => None,
            KeyCode::Esc => Some(BoardIntent::CancelEdit),
            KeyCode::Backspace => Some(BoardIntent::EditBackspace),
            KeyCode::Delete => Some(BoardIntent::EditDeleteForward),
            KeyCode::Left => Some(BoardIntent::EditMoveLeft),
            KeyCode::Right => Some(BoardIntent::EditMoveRight),
            // Vertical arrows navigate WRAPPED rows in the multiline Notes draft;
            // Title and Thread are one line, so they stay inert there.
            KeyCode::Up if focused == CaptureField::Notes => Some(BoardIntent::EditMoveUp),
            KeyCode::Down if focused == CaptureField::Notes => Some(BoardIntent::EditMoveDown),
            KeyCode::Home => Some(BoardIntent::EditMoveLineStart),
            KeyCode::End => Some(BoardIntent::EditMoveLineEnd),
            KeyCode::Char(character) if !character.is_control() => {
                Some(BoardIntent::EditInsert(character))
            }
            _ => None,
        },
    }
}

fn map_projects_search(key: KeyEvent) -> Option<BoardIntent> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(BoardIntent::CloseLayer),
        KeyCode::Enter => Some(BoardIntent::OpenTaskPage),
        KeyCode::Backspace => Some(BoardIntent::ProjectsQueryBackspace),
        KeyCode::Char(character) if !character.is_control() => {
            Some(BoardIntent::ProjectsQueryInsert(character))
        }
        _ => None,
    }
}

/// Map a bracketed-paste payload to the intent that inserts it.
///
/// A paste arrives as `Event::Paste`, never as a key press, so it cannot go through
/// [`map_key`]. The two edit modes, the palette, and the projects search consume one.
pub fn map_edit_paste(mode: BoardInputMode, text: &str) -> Option<BoardIntent> {
    match mode {
        BoardInputMode::QuickAdd => Some(BoardIntent::QuickAddInsertText(text.to_string())),
        BoardInputMode::EditTitle
        | BoardInputMode::EditNotes
        | BoardInputMode::EditThread
        | BoardInputMode::EditStep => Some(BoardIntent::EditInsertText(text.to_string())),
        BoardInputMode::SelectThread
        | BoardInputMode::EditScope
        | BoardInputMode::FormScopeDropdown
        | BoardInputMode::LaunchCard
        | BoardInputMode::TaskPage => None,
        BoardInputMode::ListPicker => {
            Some(BoardIntent::ListPickerQueryInsertText(text.to_string()))
        }
        BoardInputMode::ProjectsSearch => {
            Some(BoardIntent::ProjectsQueryInsertText(text.to_string()))
        }
        BoardInputMode::Palette => Some(BoardIntent::CommandQueryInsertText(text.to_string())),
        BoardInputMode::Normal
        | BoardInputMode::ProjectPicker
        | BoardInputMode::SaveRecovery
        | BoardInputMode::Help => None,
    }
}

/// Which primary board action an intent advances, if any.
pub fn intent_primary_action(intent: &BoardIntent) -> Option<PrimaryBoardAction> {
    match intent {
        BoardIntent::SelectNext
        | BoardIntent::SelectPrev
        | BoardIntent::SelectIndex(_)
        | BoardIntent::FocusBoardAndSelectIndex(_) => Some(PrimaryBoardAction::SelectTask),
        BoardIntent::Complete => Some(PrimaryBoardAction::Complete),
        BoardIntent::Reopen => Some(PrimaryBoardAction::Reopen),
        BoardIntent::SoftDelete => Some(PrimaryBoardAction::SoftDelete),
        BoardIntent::Undo => Some(PrimaryBoardAction::Undo),
        BoardIntent::BeginEditTitle => Some(PrimaryBoardAction::EditTitle),
        BoardIntent::OpenCapture => Some(PrimaryBoardAction::OpenCapture),
        BoardIntent::SetStatus(_)
        | BoardIntent::BeginEditNotes
        | BoardIntent::BeginEditScope
        | BoardIntent::BeginAddStep
        | BoardIntent::Quit
        | BoardIntent::EditInsert(_)
        | BoardIntent::EditInsertText(_)
        | BoardIntent::EditInsertLineBreak
        | BoardIntent::EditBackspace
        | BoardIntent::EditDeleteForward
        | BoardIntent::EditMoveLeft
        | BoardIntent::EditMoveRight
        | BoardIntent::EditMoveUp
        | BoardIntent::EditMoveDown
        | BoardIntent::EditMoveLineStart
        | BoardIntent::EditMoveLineEnd
        | BoardIntent::EditMoveWordLeft
        | BoardIntent::EditMoveWordRight
        | BoardIntent::ConfirmEdit
        | BoardIntent::ConfirmEditNext
        | BoardIntent::CancelEdit
        | BoardIntent::QuickAddInsert(_)
        | BoardIntent::QuickAddInsertText(_)
        | BoardIntent::QuickAddBackspace
        | BoardIntent::QuickAddDeleteForward
        | BoardIntent::QuickAddMoveLeft
        | BoardIntent::QuickAddMoveRight
        | BoardIntent::QuickAddMoveLineStart
        | BoardIntent::QuickAddMoveLineEnd
        | BoardIntent::QuickAddMoveWordLeft
        | BoardIntent::QuickAddMoveWordRight
        | BoardIntent::QuickAddSave
        | BoardIntent::QuickAddSaveNext
        | BoardIntent::ExpandQuickAdd
        | BoardIntent::CancelQuickAdd
        | BoardIntent::QuickAddSelectIndex(_)
        | BoardIntent::CopyTaskNumber(_)
        | BoardIntent::FormFocusNext
        | BoardIntent::FormFocusPrev
        | BoardIntent::FocusFormField(_)
        | BoardIntent::ToggleThreadEditing
        | BoardIntent::FormCycleScope
        | BoardIntent::OpenFormScopeDropdown
        | BoardIntent::FormScopeNext
        | BoardIntent::FormScopePrev
        | BoardIntent::ConfirmFormScopeDropdown
        | BoardIntent::CancelFormScopeDropdown
        | BoardIntent::SelectFormScopeOption(_)
        | BoardIntent::OpenProjectSelector
        | BoardIntent::ProjectPickerNext
        | BoardIntent::ProjectPickerPrev
        | BoardIntent::ConfirmProjectChoice
        | BoardIntent::CancelProjectPicker
        | BoardIntent::SelectProjectOption(_)
        | BoardIntent::SelectNavTab(_)
        | BoardIntent::OpenThreadFilterPicker
        | BoardIntent::OpenProjectsViewPicker
        | BoardIntent::ListPickerNext
        | BoardIntent::ListPickerPrev
        | BoardIntent::ConfirmListPicker
        | BoardIntent::CancelListPicker
        | BoardIntent::SelectListOption(_)
        | BoardIntent::ListPickerQueryInsert(_)
        | BoardIntent::ListPickerQueryInsertText(_)
        | BoardIntent::ListPickerQueryBackspace
        | BoardIntent::ProjectsQueryInsert(_)
        | BoardIntent::ProjectsQueryInsertText(_)
        | BoardIntent::ProjectsQueryBackspace
        | BoardIntent::FocusProjectsSearch
        | BoardIntent::SelectProjectRow(_)
        | BoardIntent::SelectStep(_)
        | BoardIntent::RetrySave
        | BoardIntent::CancelSave
        | BoardIntent::OpenCommandPalette
        | BoardIntent::CommandNext
        | BoardIntent::CommandPrev
        | BoardIntent::ConfirmCommand
        | BoardIntent::SelectCommand(_)
        | BoardIntent::CloseCommandSurface
        | BoardIntent::CommandQueryInsert(_)
        | BoardIntent::CommandQueryInsertText(_)
        | BoardIntent::CommandQueryBackspace
        | BoardIntent::OpenWalkthrough
        | BoardIntent::PrimaryVerb
        | BoardIntent::ToggleBlock
        | BoardIntent::ToggleReview
        | BoardIntent::HelpScrollUp
        | BoardIntent::HelpScrollDown
        | BoardIntent::ToggleStep
        | BoardIntent::OpenTaskPage
        | BoardIntent::StageRight
        | BoardIntent::StageLeft
        | BoardIntent::PeekDetail
        | BoardIntent::CollapseDetail
        | BoardIntent::PageScrollUp
        | BoardIntent::PageScrollDown
        | BoardIntent::PageWheelScrollUp
        | BoardIntent::PageWheelScrollDown
        | BoardIntent::PageScrollTo(_)
        | BoardIntent::ListScrollTo(_)
        | BoardIntent::ToggleDoneDrawer
        | BoardIntent::ToggleArchivedGroup
        | BoardIntent::ProjectPickerSwitchTab
        | BoardIntent::SelectPickerTab(_)
        | BoardIntent::LaunchUnarchive
        | BoardIntent::LaunchKeepArchived
        | BoardIntent::File
        | BoardIntent::OpenHelp
        | BoardIntent::CloseLayer
        | BoardIntent::ToggleAllGroups => None,
    }
}

/// Representative normal-mode binding for each the primary board action (table test source).
pub fn primary_action_sample_key(action: PrimaryBoardAction) -> KeyEvent {
    let (code, mods) = match action {
        PrimaryBoardAction::SelectTask => (KeyCode::Char('j'), KeyModifiers::NONE),
        PrimaryBoardAction::OpenCapture => (KeyCode::Char('+'), KeyModifiers::NONE),
        PrimaryBoardAction::EditTitle => (KeyCode::Char('e'), KeyModifiers::CONTROL),
        PrimaryBoardAction::Complete => (KeyCode::Char('d'), KeyModifiers::CONTROL),
        PrimaryBoardAction::Reopen => (KeyCode::Char('o'), KeyModifiers::CONTROL),
        PrimaryBoardAction::SoftDelete => (KeyCode::Char('x'), KeyModifiers::CONTROL),
        PrimaryBoardAction::Undo => (KeyCode::Char('u'), KeyModifiers::CONTROL),
    };
    KeyEvent::new(code, mods)
}

/// the normal-mode map. Walks [`NORMAL_KEYMAP`], the same table help uses.
fn verb_mod_held(mods: KeyModifiers) -> bool {
    mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT)
}

fn map_normal(key: KeyEvent) -> Option<BoardIntent> {
    let mods = key.modifiers;
    if key.code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        return Some(BoardIntent::Quit);
    }
    // One letter can carry a bare route and a Ctrl verb (`d` drawer / `ctrl+d` done), so
    // the lookup keys on the modifier class as well as the code.
    let verb = verb_mod_held(mods);
    let entry = NORMAL_KEYMAP
        .iter()
        .find(|entry| entry.code == key.code && entry.verb == verb)?;
    if entry.verb {
        return Some(entry.intent.clone());
    }
    // Bare navigation / chrome: reject extra modifiers. Shift is how `:` / `?` arrive.
    if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    Some(entry.intent.clone())
}

/// Task page view mode: the page is a focused single-task surface. Ctrl verbs act on the
/// page's task. Ctrl+E/Ctrl+N enter its edit session, while Tab selects and cycles steps in
/// task view. Bare arrows scroll unless a step has been selected. Esc closes.
///
/// The step verbs reuse this map's existing intents: Ctrl+S, Ctrl+E, and Ctrl+X act on a
/// selected step, otherwise on the task. The reducer disambiguates using the model cursor.
fn map_task_page(key: KeyEvent) -> Option<BoardIntent> {
    let mods = key.modifiers;
    if key.code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        return Some(BoardIntent::Quit);
    }
    let verb = verb_mod_held(mods);
    let extra = mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    match key.code {
        KeyCode::Esc if !extra => Some(BoardIntent::CloseLayer),
        KeyCode::Char('q') if verb => Some(BoardIntent::CloseLayer),
        KeyCode::Enter if !extra => Some(BoardIntent::OpenTaskPage),
        KeyCode::Char('s') if verb => Some(BoardIntent::PrimaryVerb),
        KeyCode::Char('a') if verb => Some(BoardIntent::BeginAddStep),
        KeyCode::Char('d') if verb => Some(BoardIntent::Complete),
        KeyCode::Char('o') if verb => Some(BoardIntent::Reopen),
        KeyCode::Char('b') if verb => Some(BoardIntent::ToggleBlock),
        KeyCode::Char('r') if verb => Some(BoardIntent::ToggleReview),
        KeyCode::Char('x') | KeyCode::Delete if verb => Some(BoardIntent::SoftDelete),
        KeyCode::Char('u') if verb => Some(BoardIntent::Undo),
        KeyCode::Char('f') if verb => Some(BoardIntent::File),
        KeyCode::Char('e') if verb => Some(BoardIntent::BeginEditTitle),
        KeyCode::Char('n') if verb => Some(BoardIntent::BeginEditNotes),
        KeyCode::Tab if !extra => Some(BoardIntent::FormFocusNext),
        KeyCode::BackTab
            if !mods
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            Some(BoardIntent::FormFocusPrev)
        }
        KeyCode::Up | KeyCode::Char('k') if !extra => Some(BoardIntent::PageScrollUp),
        KeyCode::Down | KeyCode::Char('j') if !extra => Some(BoardIntent::PageScrollDown),
        _ => None,
    }
}

/// Launch card: `y` unarchives, `n`/`Esc` keep archived. No `Enter` default (gate F-1):
/// the choice must be explicit.
fn map_launch_card(key: KeyEvent) -> Option<BoardIntent> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(BoardIntent::Quit);
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Char('y') => Some(BoardIntent::LaunchUnarchive),
        KeyCode::Char('n') | KeyCode::Esc => Some(BoardIntent::LaunchKeepArchived),
        _ => None,
    }
}

/// Help card: arrows, `j`/`k`, and page keys scroll the list; `Esc`, `?`, or `q` close.
/// Other keys are inert so a stray press cannot dismiss what you were reading.
fn map_help(key: KeyEvent) -> Option<BoardIntent> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(BoardIntent::Quit);
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => Some(BoardIntent::CloseLayer),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => Some(BoardIntent::HelpScrollUp),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => Some(BoardIntent::HelpScrollDown),
        _ => None,
    }
}

/// Project selector modal.
fn map_project_picker(key: KeyEvent) -> Option<BoardIntent> {
    let mods = key.modifiers;
    if mods.contains(KeyModifiers::CONTROL)
        && !mods.intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return match key.code {
            KeyCode::Char('f') => Some(BoardIntent::File),
            KeyCode::Char('u') => Some(BoardIntent::Undo),
            _ => None,
        };
    }
    if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(BoardIntent::CancelProjectPicker),
        KeyCode::Enter => Some(BoardIntent::ConfirmProjectChoice),
        KeyCode::Char('j') | KeyCode::Down => Some(BoardIntent::ProjectPickerNext),
        KeyCode::Char('k') | KeyCode::Up => Some(BoardIntent::ProjectPickerPrev),
        KeyCode::Tab | KeyCode::Left | KeyCode::Right => Some(BoardIntent::ProjectPickerSwitchTab),
        _ => None,
    }
}

/// Searchable list picker (thread filter / projects View). Printable keys narrow the
/// query, arrows move, Enter applies, Esc cancels — the palette's contract with counts.
fn map_list_picker(key: KeyEvent) -> Option<BoardIntent> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(BoardIntent::CancelListPicker),
        KeyCode::Enter => Some(BoardIntent::ConfirmListPicker),
        KeyCode::Down => Some(BoardIntent::ListPickerNext),
        KeyCode::Up => Some(BoardIntent::ListPickerPrev),
        KeyCode::Backspace => Some(BoardIntent::ListPickerQueryBackspace),
        KeyCode::Tab => Some(BoardIntent::ListPickerNext),
        KeyCode::BackTab => Some(BoardIntent::ListPickerPrev),
        KeyCode::Char(character) if !character.is_control() => {
            Some(BoardIntent::ListPickerQueryInsert(character))
        }
        _ => None,
    }
}

fn map_save_recovery(key: KeyEvent) -> Option<BoardIntent> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Char('r') | KeyCode::Enter => Some(BoardIntent::RetrySave),
        KeyCode::Char('c') | KeyCode::Esc => Some(BoardIntent::CancelSave),
        KeyCode::Char('j') | KeyCode::Down => Some(BoardIntent::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(BoardIntent::SelectPrev),
        _ => None,
    }
}

/// Palette: printable keys type the query, so movement stays on arrows and Tab.
fn map_palette(key: KeyEvent) -> Option<BoardIntent> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(BoardIntent::CloseCommandSurface),
        KeyCode::Enter => Some(BoardIntent::ConfirmCommand),
        KeyCode::Down | KeyCode::Tab => Some(BoardIntent::CommandNext),
        KeyCode::Up | KeyCode::BackTab => Some(BoardIntent::CommandPrev),
        KeyCode::Backspace => Some(BoardIntent::CommandQueryBackspace),
        KeyCode::Char(character) if !character.is_control() => {
            Some(BoardIntent::CommandQueryInsert(character))
        }
        _ => None,
    }
}

/// Thin mode-only wrapper for the title and notes task editors.
fn map_edit(mode: BoardInputMode, key: KeyEvent) -> Option<BoardIntent> {
    let focused = match mode {
        BoardInputMode::EditTitle => CaptureField::Title,
        BoardInputMode::EditNotes => CaptureField::Notes,
        _ => return None,
    };
    map_form_edit_key(focused, FormEditNavigation::None, true, key)
}

/// Map a key event to a capture form intent for the focused field.
///
/// Only press (and repeat) events produce intents. Unknown keys → `None`.
/// Printable characters insert when title/notes are focused so letters like `s`
/// remain typeable; scope override keys apply when the scope row is focused.
///
/// When `path_editing` is true (scope path buffer active), printable keys insert
/// into the path buffer instead of cycling scope.
pub fn map_capture_key(focused: CaptureField, key: KeyEvent) -> Option<CaptureIntent> {
    map_capture_key_state(focused, false, false, key)
}

/// Like [`map_capture_key`], with scope path-edit and save-recovery awareness.
pub fn map_capture_key_state(
    focused: CaptureField,
    path_editing: bool,
    save_recovery: bool,
    key: KeyEvent,
) -> Option<CaptureIntent> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    let mods = key.modifiers;
    if save_recovery {
        if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
            return None;
        }
        return match key.code {
            KeyCode::Char('r') | KeyCode::Enter => Some(CaptureIntent::RetrySave),
            KeyCode::Char('c') | KeyCode::Esc => Some(CaptureIntent::CancelSave),
            _ => None,
        };
    }
    if key.code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        return Some(CaptureIntent::Cancel);
    }
    // Shift-Tab is BackTab; allow SHIFT for BackTab only.
    if key.code == KeyCode::BackTab {
        return Some(CaptureIntent::FocusPrev);
    }
    // The two text fields carry the board's editing chords, so their table runs before the
    // modified-chord rejection below; the scope row is deliberately not part of it.
    if matches!(
        focused,
        CaptureField::Title | CaptureField::Notes | CaptureField::Thread
    ) {
        if let Some(intent) = map_capture_edit_chord(key) {
            return Some(intent);
        }
    }

    if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }

    // Global (focus-independent) keys. Cancel also aborts a path edit when the model has a
    // path buffer (handled in apply_capture_intent); Enter is per-field, because it opens a
    // line in Notes instead of saving.
    match key.code {
        KeyCode::Esc => return Some(CaptureIntent::Cancel),
        KeyCode::Tab => return Some(CaptureIntent::FocusNext),
        _ => {}
    }

    match focused {
        // Title is one line, so Enter saves the draft; Notes is multi-line, so Enter opens
        // a line and the chord pair above saves instead (ADR 0006).
        CaptureField::Title | CaptureField::Notes | CaptureField::Thread => match key.code {
            KeyCode::Enter => Some(match focused {
                CaptureField::Notes => CaptureIntent::InsertLineBreak,
                _ => CaptureIntent::Save,
            }),
            KeyCode::Backspace => Some(CaptureIntent::Backspace),
            KeyCode::Delete => Some(CaptureIntent::DeleteForward),
            KeyCode::Left => Some(CaptureIntent::MoveLeft),
            KeyCode::Right => Some(CaptureIntent::MoveRight),
            // Vertical arrows navigate WRAPPED rows in the multiline Notes draft;
            // Title and Thread are one line, so they stay inert there.
            KeyCode::Up if focused == CaptureField::Notes => Some(CaptureIntent::MoveUp),
            KeyCode::Down if focused == CaptureField::Notes => Some(CaptureIntent::MoveDown),
            KeyCode::Home => Some(CaptureIntent::MoveLineStart),
            KeyCode::End => Some(CaptureIntent::MoveLineEnd),
            KeyCode::Char(c) if !c.is_control() => Some(CaptureIntent::Insert(c)),
            _ => None,
        },
        // Scope keeps its append-and-backspace path buffer, and Enter confirms the path.
        CaptureField::Scope if path_editing => match key.code {
            KeyCode::Enter => Some(CaptureIntent::Save),
            KeyCode::Backspace => Some(CaptureIntent::Backspace),
            KeyCode::Char(c) if !c.is_control() => Some(CaptureIntent::Insert(c)),
            _ => None,
        },
        CaptureField::Scope => match key.code {
            KeyCode::Enter => Some(CaptureIntent::Save),
            // Explicit scope controls.
            KeyCode::Char('1') => Some(CaptureIntent::SelectScope(CaptureScopeChoice::ThisProject)),
            KeyCode::Char('2') => Some(CaptureIntent::SelectScope(CaptureScopeChoice::Global)),
            KeyCode::Char('3') => Some(CaptureIntent::SelectScope(CaptureScopeChoice::Other)),
            // Scope cycle Global ↔ this_repo.
            KeyCode::Char('s') | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                Some(CaptureIntent::CycleScope)
            }
            // Begin arbitrary project path edit.
            KeyCode::Char('p') | KeyCode::Char('e') => Some(CaptureIntent::BeginScopePathEdit),
            _ => None,
        },
    }
}

/// The Capture text fields' modified-chord table.
///
/// A pure function of the key event: it never queries a terminal capability. It runs before
/// Capture's modified-chord rejection so a bound Ctrl or Alt press cannot fall through to the
/// printable-character route. `Ctrl+C` is not listed: Capture already cancels on it first.
fn map_capture_edit_chord(key: KeyEvent) -> Option<CaptureIntent> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let super_key = key.modifiers.contains(KeyModifiers::SUPER);

    match key.code {
        KeyCode::Enter if ctrl != alt && !shift && !super_key => Some(CaptureIntent::Save),
        KeyCode::Char('a') if ctrl => Some(CaptureIntent::MoveLineStart),
        KeyCode::Char('e') if ctrl => Some(CaptureIntent::MoveLineEnd),
        KeyCode::Left if ctrl => Some(CaptureIntent::MoveWordLeft),
        KeyCode::Right if ctrl => Some(CaptureIntent::MoveWordRight),
        _ => None,
    }
}

/// Map a bracketed-paste payload to the capture intent that inserts it.
///
/// A paste arrives as `Event::Paste`, never as a key press, so it cannot go through
/// [`map_capture_key`]. Only the two text fields consume one; the scope row does not, unless
/// its path editor is open — pasting a project path into the "other project path" row is the
/// likeliest paste in the product, and it worked before bracketed paste routed the payload
/// away from the key path, so the open path editor accepts a paste exactly as it accepts
/// typed characters.
pub fn map_capture_paste_state(
    focused: CaptureField,
    path_editing: bool,
    text: &str,
) -> Option<CaptureIntent> {
    match focused {
        CaptureField::Title | CaptureField::Notes | CaptureField::Thread => {
            Some(CaptureIntent::InsertText(text.to_string()))
        }
        CaptureField::Scope if path_editing => Some(CaptureIntent::InsertText(text.to_string())),
        CaptureField::Scope => None,
    }
}

/// Which primary capture action an intent advances, if any.
pub fn intent_primary_capture_action(intent: &CaptureIntent) -> Option<PrimaryCaptureAction> {
    match intent {
        CaptureIntent::Save | CaptureIntent::RetrySave => Some(PrimaryCaptureAction::SaveCapture),
        CaptureIntent::Cancel | CaptureIntent::CancelSave => {
            Some(PrimaryCaptureAction::CancelCapture)
        }
        CaptureIntent::Insert(_)
        | CaptureIntent::InsertText(_)
        | CaptureIntent::InsertLineBreak
        | CaptureIntent::Backspace
        | CaptureIntent::DeleteForward
        | CaptureIntent::MoveLeft
        | CaptureIntent::MoveRight
        | CaptureIntent::MoveUp
        | CaptureIntent::MoveDown
        | CaptureIntent::MoveLineStart
        | CaptureIntent::MoveLineEnd
        | CaptureIntent::MoveWordLeft
        | CaptureIntent::MoveWordRight
        | CaptureIntent::FocusNext
        | CaptureIntent::FocusPrev
        | CaptureIntent::FocusField(
            CaptureField::Title | CaptureField::Notes | CaptureField::Thread,
        ) => Some(PrimaryCaptureAction::EditField),
        // Scope row click cycles; path edit and focusing scope are scope interaction.
        CaptureIntent::FocusField(CaptureField::Scope)
        | CaptureIntent::CycleScope
        | CaptureIntent::SelectScope(_)
        | CaptureIntent::BeginScopePathEdit => Some(PrimaryCaptureAction::ChangeScope),
    }
}

/// Representative binding for each primary capture action (table test source).
///
/// ChangeScope sample assumes scope field focus; EditField assumes title/notes focus.
pub fn primary_capture_action_sample_key(action: PrimaryCaptureAction) -> KeyEvent {
    let (code, mods) = match action {
        PrimaryCaptureAction::SaveCapture => (KeyCode::Enter, KeyModifiers::NONE),
        PrimaryCaptureAction::CancelCapture => (KeyCode::Esc, KeyModifiers::NONE),
        PrimaryCaptureAction::EditField => (KeyCode::Char('a'), KeyModifiers::NONE),
        PrimaryCaptureAction::ChangeScope => (KeyCode::Char('s'), KeyModifiers::NONE),
    };
    KeyEvent::new(code, mods)
}

/// Focus assumed when sampling a primary capture action key (for table tests).
pub fn primary_capture_action_sample_focus(action: PrimaryCaptureAction) -> CaptureField {
    match action {
        PrimaryCaptureAction::ChangeScope => CaptureField::Scope,
        PrimaryCaptureAction::SaveCapture
        | PrimaryCaptureAction::CancelCapture
        | PrimaryCaptureAction::EditField => CaptureField::Title,
    }
}
