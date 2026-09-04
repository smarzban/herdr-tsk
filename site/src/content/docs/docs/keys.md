---
title: Keys
description: Board, task page, and capture key chords.
---

Mutating keys need **Ctrl**. Bare letters do nothing, so typing in a focused
board cannot complete or delete work.

Nav, peek, `Enter`, `P`, `1` / `2` / `3`, `z`, `:`, `?`, `+`, and `Esc` stay
bare.

The web demo on the landing page uses bare verb letters on purpose. Browsers
steal control chords. On its projects and threads tabs, bare `g` toggles groups
and the palette offers the same command.

## Wide stage slider

At 110 usable columns or wider the board is a four-stage slider, and focus is the
stage: **0** the board alone · **A** board beside the task page (board focus) ·
**G** a dim rail beside the page (task focus) · **F** the page alone.

| Stage | `→` | `←` | `Enter` | `Esc` | `j` `k` |
| --- | --- | --- | --- | --- | --- |
| 0 | → A | nothing | → F | nothing | select |
| A | → G | → 0 | → F | nothing | select, retarget pane |
| G | → F | → A | task-page verb | → A | task-page nav |
| F | nothing | → G | task-page verb | → back where `Enter` left | task-page nav |

`Enter` remembers the stage it left; `Esc` from F returns there. `Tab` is never a
stage key. `→` never peeks at wide widths. Active task editors keep the field, step,
and save keys listed below; the status row's right side names the keys that apply
right now. Mouse clicks follow the same stage: a click in the stage A task column
moves to G, then runs the control you clicked.

## Board

| Key | Does |
| --- | --- |
| `j` `k` or `↑` `↓` | move selection |
| wheel | scroll the list |
| `ctrl+s` | start the selected task, or reopen it if it is done |
| `ctrl+d` | done |
| `ctrl+o` | reopen |
| `ctrl+b` | toggle blocked |
| `Enter` | open the [task page](/docs/task-page/) full width (stage F at wide widths) |
| `→` `←` | peek notes below 110 columns; at wide widths, slide the stage |
| `+` | [capture](/docs/capture/) |
| `ctrl+e` | edit title |
| `ctrl+x` or `ctrl+Delete` | delete (`ctrl+u` undoes) |
| `ctrl+f` | file: toggle the task's archived flag. No undo entry. In the picker, archives the selected project; on the picker's archived tab (or `ctrl+u` on an archived selection) it unarchives |
| `z` | done drawer |
| `:` | command palette |
| `?` | help |
| `P` | project picker |
| `1` `2` `3` | home tabs: desk · projects · threads |
| `ctrl+g` | expand or collapse all visible groups |
| `Esc` | close one layer |
| `ctrl+q` or `ctrl+c` | quit |

Click a dim task identifier (`T30`) to copy it. Click elsewhere on a row to peek, click it again to close, and fast double-click to open the page.

`Esc` closes one layer at a time.

## Task page

The page is view-first. Verbs still need the modifier, except Tab and arrows.

| Key | Does |
| --- | --- |
| `ctrl+e` | edit title, or start task editing with the highlighted step inline |
| `ctrl+n` | edit notes |
| `Tab` / `Shift+Tab` | in view, loop forward or backward through stored steps and `+ step`; in task editing, loop Title, Notes, stored steps, `+ step`, Scope, Thread |
| `ctrl+a` | open an independent inline [step](/docs/steps/) editor from view or any task-edit focus |
| `Enter` | opens or saves selected `+ step`, parks an existing-step rename, opens Scope's picker, or toggles Thread's selected text editor |
| click a step | selects it in view, opens it inline only during task editing; click `   + step` to add from any page edit state |
| `↓` `↑` | Down activates the first stored step, then arrows move selected steps; an open add row scrolls the page |
| `ctrl+s` | toggle the selected step, otherwise start or reopen the task |
| `ctrl+d` / `ctrl+o` | complete / reopen the selected step, otherwise act on the task |
| `ctrl+x` | in view, first mark then remove a selected step on a second press; in task edit immediately hide and stage its removal, otherwise delete the task |
| `Shift+Enter` | task-edit save, or save a step and open the next empty step editor (`Alt+Enter` is the legacy-terminal save fallback) |
| `Esc` | cancel the field, restore staged task-edit changes, or close the page |

Title, Notes, Thread, and Scope clicks are inert until task editing starts. Once it
has, Title and Notes open their fields, Scope opens its picker, and Thread first
selects, then opens or closes its text editor on a second click.

## Any text field

Title, notes, thread, step, and quick-add share one editor. The palette query takes
only typing, arrows, Tab, Enter, Esc, and Backspace.

| Key | Does |
| --- | --- |
| `ctrl+a` / `ctrl+e` | line start / end (on the task page `ctrl+a` adds a step instead) |
| `ctrl+←` / `ctrl+→` | word left / right |
| `Home` / `End` | line start / end |
| `Backspace` / `Delete` | delete back / forward |
| `↑` `↓` in notes | move between wrapped rows |
| paste | inserts; line breaks stay in notes, elsewhere they become spaces |

## Palette, picker, and recovery

| Surface | Keys |
| --- | --- |
| `:` palette | type to filter · `↑` `↓` or `Tab` / `Shift+Tab` move · `Enter` run · `Esc` close |
| `P` project picker | `j` `k` or arrows · `Tab` main/archived tabs · `Enter` choose · `ctrl+f` archive (archived tab: unarchive) · `Esc` or `q` cancel |
| launch card | `y` unarchive · `n` or `Esc` keep archived |
| `?` help | any key closes |
| save failed | `r` or `Enter` retry · `c` or `Esc` cancel |

## Capture line and form

| Key | Does |
| --- | --- |
| `Tab` / `Shift+Tab` | Title, Notes, Thread, Scope |
| `Enter` in Title or Thread | save |
| `Enter` in Notes | new line |
| `Ctrl+Enter` in Title, Notes, or Thread | save (`Alt+Enter` is the legacy-terminal fallback) |
| `1` `2` `3` on Scope | this project · desk · other path |
| `s`, `Space`, `←` `→` on Scope | cycle the scope |
| `p` or `e` on Scope | type another path; `Enter` confirms it |
| `Enter` on Scope | save |
| `Esc` | cancel |

On the board `+` line: `Enter` saves and closes, `Shift+Enter` saves and stays
open, `Tab` expands onto the task page.
