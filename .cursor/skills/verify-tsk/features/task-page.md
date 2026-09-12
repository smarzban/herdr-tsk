# Task page

The task page lets a user open one task full-frame (or beside the board when wide), read notes, and toggle steps without leaving the product surface.

## Sub-features

- `page-open` opens the selected task with Enter.
- `page-step-toggle` toggles a stored step with Enter when the step is selected.
- `page-close` returns with Esc.

## How to get to it (user POV)

- Select a task on the board and press Enter (or double-click). A single click peeks in a narrow pane and opens details beside the board at ≥110 columns; this harness has no mouse.
- At ≥110 columns, `→` slides toward the page (first stop is split, not full-frame). This recipe uses 80×24 so Enter opens the page full-frame. Below 110, `→` peeks.
- Press Esc to close.

## Driving it with control-tsk

Preconditions:

- `control-tsk doctor` reports `"ok": true`.
- A desk task titled `Verify task page` exists with notes `plain note` and one open step `first step` (seed via CLI).
- Board is running at 80×24.

- **Seed.** Run `control-tsk cli -- add -t "Verify task page" -n "plain note" --desk --json`, then `control-tsk cli -- steps <number> add "first step"`.
- **Select.** Dump and `board keys j` until `▸` is on `Verify task page`. First open selects Welcome; bare Enter would open that notice.
- **Open page.** Run `control-tsk board keys enter`.
- **Confirm notes.** Run `control-tsk board wait "plain note"`.
- **Toggle step.** Run `control-tsk board keys tab` then `enter`. Tab-first is required: Enter with no stored step selected closes the full page. Confirm with `control-tsk cli -- list <number> --json` that `steps[0]` has `"text":"first step"` and `"done": true`.
- **Close.** Run `control-tsk board keys esc`, then `control-tsk board wait "ON DECK"`. Do not wait for `desk`: the page Scope line already paints it.

## Gotchas

- The page is view-first. `Tab` in view mode selects steps and **+ step**; it does not start title/notes edit. Use `ctrl+e` / `ctrl+n` to edit.
- `Shift+Enter` is the whole-session save chord while editing. Do not confuse it with step Enter.
- `list --desk --json` does not include `steps`. Direct `list <number> --json` does.
- Below 110 columns there is peek on `→`; this recipe uses Enter for the page.
