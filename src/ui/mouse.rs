//! Mouse hit-testing for the queue board and standalone capture.

use std::io::{self, stdout, Write};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::Block;

use super::board::{board_verb_items, BoardInputMode, BoardModel};
use super::capture::{
    CaptureField, CaptureModel, CaptureScopeChoice, CAPTURE_FIELD_LABEL_WIDTH,
    CAPTURE_SCOPE_CONTROLS,
};
use super::input::{BoardIntent, CaptureIntent, PrimaryCaptureAction, PRIMARY_CAPTURE_ACTIONS};
use super::render::{
    form_verb_items, QueueHitMap, QueueHitTarget, PALETTE_VERBS, QUICK_ADD_VERBS, SCOPE_VERBS,
};

/// Transient presentation that still exists on the V1 queue board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardPopup {
    #[default]
    None,
    /// Session-only project scope selector.
    ProjectPicker,
    /// Durable dispatch attempt recovery actions for old persisted attempts.
    Recovery,
    /// Explicit cleanup confirmation for recorded owned receipts.
    CleanupConfirm,
    /// Board persistence failed; Retry or Cancel must resolve it before another mutation.
    SaveRecovery,
}

/// Labeled capture hit region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip<T> {
    pub rect: Rect,
    pub label: &'static str,
    pub value: T,
}

/// Capture form field and button regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureLayout {
    pub area: Rect,
    pub inner: Rect,
    pub save_recovery: bool,
    pub subtitle_area: Rect,
    pub title_area: Rect,
    pub notes_area: Rect,
    pub scope_area: Rect,
    pub scope_chips: Vec<Chip<CaptureScopeChoice>>,
    pub this_project_available: bool,
    pub scope_path_area: Rect,
    pub message_area: Rect,
    pub save_chip: Chip<()>,
    pub cancel_chip: Chip<()>,
    pub help_area: Rect,
}

/// One terminal input capability an event loop asks the terminal for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalInputCapability {
    MouseCapture,
    BracketedPaste,
    KeyboardProtocol,
}

pub(crate) fn requested_terminal_input(
    keyboard_enhancement_supported: bool,
) -> Vec<TerminalInputCapability> {
    let mut requested = vec![
        TerminalInputCapability::MouseCapture,
        TerminalInputCapability::BracketedPaste,
    ];
    if keyboard_enhancement_supported {
        requested.push(TerminalInputCapability::KeyboardProtocol);
    }
    requested
}

fn enable_capability(out: &mut impl Write, capability: TerminalInputCapability) -> io::Result<()> {
    match capability {
        TerminalInputCapability::MouseCapture => execute!(out, EnableMouseCapture),
        TerminalInputCapability::BracketedPaste => execute!(out, EnableBracketedPaste),
        TerminalInputCapability::KeyboardProtocol => execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        ),
    }
}

fn disable_capability(out: &mut impl Write, capability: TerminalInputCapability) -> io::Result<()> {
    match capability {
        TerminalInputCapability::MouseCapture => execute!(out, DisableMouseCapture),
        TerminalInputCapability::BracketedPaste => execute!(out, DisableBracketedPaste),
        TerminalInputCapability::KeyboardProtocol => execute!(out, PopKeyboardEnhancementFlags),
    }
}

fn disable_all(out: &mut impl Write, enabled: &[TerminalInputCapability]) {
    for capability in enabled.iter().rev() {
        let _ = disable_capability(out, *capability);
    }
}

/// Restores exactly the terminal capabilities it enabled.
pub(crate) struct TerminalInputGuard<W: Write> {
    out: W,
    enabled: Vec<TerminalInputCapability>,
}

impl<W: Write> Drop for TerminalInputGuard<W> {
    fn drop(&mut self) {
        disable_all(&mut self.out, &self.enabled);
    }
}

pub(crate) fn keyboard_enhancement_supported() -> bool {
    supports_keyboard_enhancement().unwrap_or(false)
}

fn enable_terminal_input_on<W: Write>(
    out: W,
    keyboard_enhancement_supported: bool,
) -> io::Result<TerminalInputGuard<W>> {
    let mut guard = TerminalInputGuard {
        out,
        enabled: Vec::new(),
    };
    for capability in requested_terminal_input(keyboard_enhancement_supported) {
        enable_capability(&mut guard.out, capability)?;
        guard.enabled.push(capability);
    }
    Ok(guard)
}

pub(crate) fn enable_terminal_input(
    keyboard_enhancement_supported: bool,
) -> io::Result<TerminalInputGuard<std::io::Stdout>> {
    enable_terminal_input_on(stdout(), keyboard_enhancement_supported)
}

/// Left-button press at (column, row).
pub fn left_click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Compute normal capture layout matching `draw_capture`.
pub fn capture_layout(area: Rect) -> CaptureLayout {
    capture_layout_state(area, false, true, false)
}

pub fn capture_recovery_layout(area: Rect) -> CaptureLayout {
    capture_layout_state(area, true, true, false)
}

pub fn capture_layout_for_model(area: Rect, model: &CaptureModel) -> CaptureLayout {
    capture_layout_state(
        area,
        model.is_save_recovery(),
        model.this_project_available(),
        model.shows_project_path(),
    )
}

pub fn capture_layout_state(
    area: Rect,
    save_recovery: bool,
    this_project_available: bool,
    show_path: bool,
) -> CaptureLayout {
    let inner = Block::bordered().inner(area);
    let notes_height = capture_notes_rows(inner.height, show_path);
    let rows = [
        1,
        1,
        notes_height,
        1,
        u16::from(show_path),
        1,
        1,
        inner
            .height
            .saturating_sub(6 + notes_height + u16::from(show_path)),
        1,
    ];
    let mut y = inner.y;
    let mut areas = Vec::with_capacity(rows.len());
    for height in rows {
        let height = height.min(inner.y.saturating_add(inner.height).saturating_sub(y));
        areas.push(Rect::new(inner.x, y, inner.width, height));
        y = y.saturating_add(height);
    }
    let scope_row = areas[3];
    let scope_chips = place_scope_chips(scope_row);
    let scope_path_area = if show_path { areas[4] } else { Rect::default() };
    let buttons = areas[6];
    let save_label = if save_recovery { " Retry " } else { " Save " };
    let cancel_label = " Cancel ";
    let save_width = save_label.chars().count() as u16;
    let cancel_width = cancel_label.chars().count() as u16;
    let save_rect = Rect::new(buttons.x, buttons.y, save_width.min(buttons.width), 1);
    let cancel_x = buttons.x.saturating_add(save_width).saturating_add(2);
    let cancel_rect = if cancel_x < buttons.x.saturating_add(buttons.width) {
        Rect::new(
            cancel_x,
            buttons.y,
            cancel_width.min(
                buttons
                    .x
                    .saturating_add(buttons.width)
                    .saturating_sub(cancel_x),
            ),
            1,
        )
    } else {
        Rect::new(buttons.x, buttons.y, 0, 1)
    };
    CaptureLayout {
        area,
        inner,
        save_recovery,
        subtitle_area: areas[0],
        title_area: areas[1],
        notes_area: areas[2],
        scope_area: scope_row,
        scope_chips,
        this_project_available,
        scope_path_area,
        message_area: areas[5],
        save_chip: Chip {
            rect: save_rect,
            label: save_label,
            value: (),
        },
        cancel_chip: Chip {
            rect: cancel_rect,
            label: cancel_label,
            value: (),
        },
        help_area: areas[8],
    }
}

pub const CAPTURE_NOTES_MAX_ROWS: u16 = 3;
const CAPTURE_FIXED_ROWS: u16 = 6;

fn capture_notes_rows(inner_height: u16, show_path: bool) -> u16 {
    inner_height
        .saturating_sub(CAPTURE_FIXED_ROWS + u16::from(show_path))
        .clamp(1, CAPTURE_NOTES_MAX_ROWS)
}

fn place_scope_chips(row: Rect) -> Vec<Chip<CaptureScopeChoice>> {
    let x_end = row.x.saturating_add(row.width);
    let start = row.x.saturating_add(CAPTURE_FIELD_LABEL_WIDTH);
    let control_width = |label: &str| label.chars().count() as u16 + 4;
    let needed = |compact: bool| {
        CAPTURE_SCOPE_CONTROLS
            .iter()
            .map(|&(full, narrow, _)| control_width(if compact { narrow } else { full }) + 1)
            .sum::<u16>()
            .saturating_sub(1)
    };
    let compact = start.saturating_add(needed(false)) > x_end;
    let mut x = start;
    let mut chips = Vec::with_capacity(CAPTURE_SCOPE_CONTROLS.len());
    for &(full, narrow, choice) in CAPTURE_SCOPE_CONTROLS {
        let label = if compact { narrow } else { full };
        if x >= x_end {
            break;
        }
        let width = control_width(label).min(x_end.saturating_sub(x));
        if width == 0 {
            break;
        }
        chips.push(Chip {
            rect: Rect::new(x, row.y, width, 1),
            label,
            value: choice,
        });
        x = x.saturating_add(width).saturating_add(1);
    }
    chips
}

fn point(column: u16, row: u16) -> Position {
    Position { x: column, y: row }
}

fn verb_intent(model: &BoardModel, index: usize) -> Option<BoardIntent> {
    let entry = *board_verb_items(model).get(index)?;
    match entry.key {
        "space" => Some(BoardIntent::PrimaryVerb),
        "enter" => Some(BoardIntent::OpenTaskPage),
        "d" => Some(BoardIntent::Complete),
        "o" => Some(BoardIntent::Reopen),
        "b" => Some(BoardIntent::ToggleBlock),
        "x" => Some(BoardIntent::SoftDelete),
        "u" => Some(BoardIntent::Undo),
        "e" => Some(BoardIntent::BeginEditTitle),
        "n" => Some(BoardIntent::BeginEditNotes),
        "esc" => Some(BoardIntent::CloseLayer),
        ":" => Some(BoardIntent::OpenCommandPalette),
        "?" => Some(BoardIntent::OpenHelp),
        "+" => Some(BoardIntent::OpenCapture),
        _ => None,
    }
}

fn quick_add_verb_intent(index: usize) -> Option<BoardIntent> {
    match QUICK_ADD_VERBS.get(index)?.key {
        "enter" => Some(BoardIntent::QuickAddSave),
        "ctrl+enter" => Some(BoardIntent::QuickAddSaveNext),
        "tab" => Some(BoardIntent::ExpandQuickAdd),
        "esc" => Some(BoardIntent::CancelQuickAdd),
        _ => None,
    }
}

fn palette_verb_intent(index: usize) -> Option<BoardIntent> {
    match PALETTE_VERBS.get(index)?.key {
        "enter" => Some(BoardIntent::ConfirmCommand),
        "esc" => Some(BoardIntent::CloseCommandSurface),
        _ => None,
    }
}

fn form_verb_intent(model: &BoardModel, index: usize) -> Option<BoardIntent> {
    let dropdown_open = model.input_mode() == BoardInputMode::FormScopeDropdown;
    let focus = model.form_focus()?;
    match form_verb_items(focus, dropdown_open).get(index)?.key {
        "ctrl+enter" | "alt+enter" => Some(BoardIntent::ConfirmEdit),
        "enter" if dropdown_open => Some(BoardIntent::ConfirmFormScopeDropdown),
        "enter" if focus == CaptureField::Scope => Some(BoardIntent::OpenFormScopeDropdown),
        "enter" => Some(BoardIntent::ConfirmEdit),
        "esc" if dropdown_open => Some(BoardIntent::CancelFormScopeDropdown),
        "esc" => Some(BoardIntent::CancelEdit),
        _ => None,
    }
}

fn scope_dropdown_verb_intent(index: usize) -> Option<BoardIntent> {
    match SCOPE_VERBS.get(index)?.key {
        "enter" => Some(BoardIntent::ConfirmProjectChoice),
        "esc" => Some(BoardIntent::CancelProjectPicker),
        _ => None,
    }
}

fn hit_at(hits: &QueueHitMap, pos: Position) -> Option<QueueHitTarget> {
    hits.regions
        .iter()
        .rev()
        .find(|hit| hit.area.contains(pos))
        .map(|hit| hit.target)
}

fn wheel_board_intent(model: &BoardModel, kind: MouseEventKind) -> Option<BoardIntent> {
    match model.input_mode() {
        BoardInputMode::TaskPage => match kind {
            MouseEventKind::ScrollUp => Some(BoardIntent::PageScrollUp),
            MouseEventKind::ScrollDown => Some(BoardIntent::PageScrollDown),
            _ => None,
        },
        BoardInputMode::Normal => {
            let visible = model.visible_ids().len();
            let selected = model.selected_index()?;
            match kind {
                MouseEventKind::ScrollUp if selected > 0 => Some(BoardIntent::SelectPrev),
                MouseEventKind::ScrollDown if selected + 1 < visible => {
                    Some(BoardIntent::SelectNext)
                }
                _ => None,
            }
        }
        BoardInputMode::Palette => {
            let len = model.visible_commands().len();
            let selected = model.command_selected()?;
            match kind {
                MouseEventKind::ScrollUp if selected > 0 => Some(BoardIntent::CommandPrev),
                MouseEventKind::ScrollDown if selected + 1 < len => Some(BoardIntent::CommandNext),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Map a board mouse event through the current frame's renderer-owned hit map.
pub fn map_board_mouse(
    model: &BoardModel,
    hits: &QueueHitMap,
    mouse: MouseEvent,
) -> Option<BoardIntent> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {}
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            return wheel_board_intent(model, mouse.kind)
        }
        _ => return None,
    }
    let pos = point(mouse.column, mouse.row);
    match model.input_mode() {
        BoardInputMode::Palette => match hit_at(hits, pos) {
            Some(QueueHitTarget::Command(index)) => model
                .visible_commands()
                .get(index)
                .map(|_| BoardIntent::SelectCommand(index)),
            Some(QueueHitTarget::CommandChrome) => None,
            Some(QueueHitTarget::Verb(index)) => palette_verb_intent(index),
            _ => Some(BoardIntent::CloseCommandSurface),
        },
        BoardInputMode::ProjectPicker => match hit_at(hits, pos) {
            Some(QueueHitTarget::ProjectOption(index)) => {
                Some(BoardIntent::SelectProjectOption(index))
            }
            Some(QueueHitTarget::Verb(index)) => scope_dropdown_verb_intent(index),
            _ => Some(BoardIntent::CancelProjectPicker),
        },
        BoardInputMode::Help => Some(BoardIntent::CloseLayer),
        BoardInputMode::QuickAdd => match hit_at(hits, pos) {
            // The line already owns keyboard focus, so its click is intentionally inert.
            Some(QueueHitTarget::QuickAddInput) => None,
            Some(QueueHitTarget::Task(id)) => model
                .visible_ids()
                .iter()
                .position(|&visible| visible == id)
                .map(BoardIntent::QuickAddSelectIndex),
            Some(QueueHitTarget::Verb(index)) => quick_add_verb_intent(index),
            // Chosen policy: outside clicks discard the draft and are swallowed, rather than
            // triggering a second board action behind the capture surface.
            _ => Some(BoardIntent::CancelQuickAdd),
        },
        BoardInputMode::EditTitle | BoardInputMode::EditNotes | BoardInputMode::EditScope => {
            match hit_at(hits, pos) {
                Some(QueueHitTarget::FormTitle) => {
                    Some(BoardIntent::FocusFormField(CaptureField::Title))
                }
                Some(QueueHitTarget::FormNotes(_)) => {
                    Some(BoardIntent::FocusFormField(CaptureField::Notes))
                }
                Some(QueueHitTarget::FormScope) => Some(BoardIntent::OpenFormScopeDropdown),
                Some(QueueHitTarget::Verb(index)) => form_verb_intent(model, index),
                _ => None,
            }
        }
        BoardInputMode::TaskPage => match hit_at(hits, pos) {
            Some(QueueHitTarget::FormScope) => None,
            // A click on an item row selects it (AC-21) — the board's click
            // convention: a click selects, never mutates.
            Some(QueueHitTarget::ChecklistItem(index)) => {
                Some(BoardIntent::SelectChecklistItem(index))
            }
            Some(QueueHitTarget::Verb(index)) => verb_intent(model, index),
            _ => None,
        },
        BoardInputMode::FormScopeDropdown => match hit_at(hits, pos) {
            Some(QueueHitTarget::FormScopeOption(index)) => {
                Some(BoardIntent::SelectFormScopeOption(index))
            }
            Some(QueueHitTarget::Verb(index)) => form_verb_intent(model, index),
            _ => None,
        },
        // The item line editor is keyboard-only in this slice: the mouse has no hit
        // region on the section's line yet, so every click is inert rather than
        // reaching the page behind the editor.
        BoardInputMode::EditChecklistItem
        | BoardInputMode::Recovery
        | BoardInputMode::CleanupConfirm
        | BoardInputMode::SaveRecovery => None,
        BoardInputMode::Normal => match hit_at(hits, pos) {
            Some(QueueHitTarget::ProjectChip) => Some(BoardIntent::OpenProjectSelector),
            Some(QueueHitTarget::SectionProject(index)) => {
                Some(BoardIntent::SelectSectionProject(index))
            }
            Some(QueueHitTarget::Drawer) => Some(BoardIntent::ToggleDoneDrawer),
            Some(QueueHitTarget::Task(id)) => model
                .visible_ids()
                .iter()
                .position(|&visible| visible == id)
                .map(BoardIntent::SelectIndex),
            Some(QueueHitTarget::Verb(index)) => verb_intent(model, index),
            Some(QueueHitTarget::DeleteNoticeUndo) => Some(BoardIntent::Undo),
            _ => None,
        },
    }
}

/// Map a mouse event to a capture intent (left click only).
pub fn map_capture_mouse(layout: &CaptureLayout, mouse: MouseEvent) -> Option<CaptureIntent> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return None;
    }
    let pos = point(mouse.column, mouse.row);
    if layout.save_chip.rect.width > 0 && layout.save_chip.rect.contains(pos) {
        return Some(if layout.save_recovery {
            CaptureIntent::RetrySave
        } else {
            CaptureIntent::Save
        });
    }
    if layout.cancel_chip.rect.width > 0 && layout.cancel_chip.rect.contains(pos) {
        return Some(if layout.save_recovery {
            CaptureIntent::CancelSave
        } else {
            CaptureIntent::Cancel
        });
    }
    if layout.save_recovery {
        return None;
    }
    if layout.title_area.contains(pos) {
        return Some(CaptureIntent::FocusField(CaptureField::Title));
    }
    if layout.notes_area.contains(pos) {
        return Some(CaptureIntent::FocusField(CaptureField::Notes));
    }
    for chip in &layout.scope_chips {
        if chip.rect.contains(pos) {
            if chip.value == CaptureScopeChoice::ThisProject && !layout.this_project_available {
                return None;
            }
            return Some(CaptureIntent::SelectScope(chip.value));
        }
    }
    if !layout.scope_path_area.is_empty() && layout.scope_path_area.contains(pos) {
        return Some(CaptureIntent::SelectScope(CaptureScopeChoice::Other));
    }
    if layout.scope_area.contains(pos) {
        return Some(CaptureIntent::CycleScope);
    }
    None
}

pub fn primary_capture_action_sample_mouse(
    action: PrimaryCaptureAction,
    layout: &CaptureLayout,
) -> MouseEvent {
    let (column, row) = match action {
        PrimaryCaptureAction::SaveCapture => (layout.save_chip.rect.x, layout.save_chip.rect.y),
        PrimaryCaptureAction::CancelCapture => {
            (layout.cancel_chip.rect.x, layout.cancel_chip.rect.y)
        }
        PrimaryCaptureAction::EditField => (layout.title_area.x, layout.title_area.y),
        PrimaryCaptureAction::ChangeScope => (layout.scope_area.x, layout.scope_area.y),
    };
    left_click(column, row)
}

pub fn capture_mouse_paths_complete(layout: &CaptureLayout) -> bool {
    PRIMARY_CAPTURE_ACTIONS.iter().all(|&action| {
        let mouse = primary_capture_action_sample_mouse(action, layout);
        map_capture_mouse(layout, mouse)
            .and_then(|intent| super::input::intent_primary_capture_action(&intent))
            == Some(action)
    })
}
