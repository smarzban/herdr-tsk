# Capture UI

**Responsibility.** Standalone capture form (`AppMode::Capture`): title, notes,
thread, scope; save or cancel; process exits. Reached from herdr **Quick capture**
(`TSK_MODE=capture`), not from board `+` (that is quick-add on the board).

**Public surface.** `CaptureModel`, `CaptureField`, `CaptureOutcome`,
`CaptureScopeChoice`, `apply_capture_intent`, `draw_capture`, `format_scope`,
`CAPTURE_TITLE` (`"Capture"`), `CAPTURE_SCOPE_CONTROLS`, layout helpers in
`ui::mouse`.

## How it works

`CaptureModel::from_snapshot` seeds title from `title_prefill` when present, scope
from `default_scope`, empty notes/thread. Fields: Title, Notes, Thread, Scope
(`CaptureField`). Tab / Shift-Tab cycle. Enter in Title saves; Enter in Notes is
a newline; Ctrl+Enter saves from any field. Esc cancels the process.

Scope controls include desk and this-project (disabled with
`CAPTURE_THIS_PROJECT_UNAVAILABLE` when `this_repo` is none). Thread uses
`normalize_thread`; refusals paint on the field, not as a process error.

Save calls `capture_save` with `Some(store)`. Empty title stays on the form with
`TITLE_REQUIRED_MESSAGE` (`"Title required"`). Store failure uses the same
`SaveRecovery` pattern as the board (`CAPTURE_SAVE_RECOVERY_HELP_LINE`:
`r retry  ·  c cancel`).

Shared editors: `EditBuffer`, `map_form_edit_key` / `map_capture_key_state`,
`wrap_text` for Notes. Notes max painted rows in the overlay:
`CAPTURE_NOTES_MAX_ROWS` = 3. Field label width: `CAPTURE_FIELD_LABEL_WIDTH` = 10.

Mouse: `map_capture_mouse` against `capture_layout_for_model`. Tests assert
`capture_mouse_paths_complete` so a new layout region cannot silently ignore
clicks.

## Invariants

- This is a separate surface from board quick-add; do not open `AppMode::Capture`
  from `+`.
- Create still goes through domain; empty title does not persist.
- Form + mode outlive a failed save (same as the board).
- Capsule/provenance come from the snapshot even when the user picks another
  scope.

## Error paths

Title required stays in-form. Domain/store errors: in-form message or save
recovery. Cancel: no write, process exits 0 from `run_capture`'s cancel path
(the binary treats `Ok(())` as success).

## Extension points

New fields belong on `CaptureField` + layout + mapper together. Prefer sharing
`EditBuffer` rather than a third draft type.
