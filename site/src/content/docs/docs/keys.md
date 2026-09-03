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

## Wide split focus

At 110 usable columns or wider, the unboxed board and bordered task page share the
frame. The task border is cyan plus bold with task focus and dark gray plus dim with
board focus. With board focus, `Enter` or `→` moves to task focus. From task view, `Esc` or `←`
returns to board focus. Active task editors keep the field, step, and save keys
listed below. Mouse clicks follow the same focus owner.

## Board

| Key | Does |
| --- | --- |
| `j` `k` or `↑` `↓` | move selection |
| wheel | scroll the list |
| `ctrl+s` | start the selected task, or reopen it if it is done |
| `ctrl+d` | done |
| `ctrl+o` | reopen |
| `ctrl+b` | toggle blocked |
| `Enter` | open the [task page](/docs/task-page/), or focus it in wide split |
| `→` `←` | peek notes below 110 columns; in wide split, `→` focuses the task |
| `+` | [capture](/docs/capture/) |
| `ctrl+e` | edit title |
| `ctrl+x` or `ctrl+Delete` | delete (`ctrl+u` undoes) |
| `z` | done drawer |
| `:` | command palette |
| `?` | help |
| `P` | project picker |
| `1` `2` `3` | home tabs: desk · projects · threads |
| `ctrl+g` | expand or collapse all visible groups |
| `Esc` | close one layer |
| `ctrl+q` | quit |

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

## Capture line and form

| Key | Does |
| --- | --- |
| `Tab` / `Shift+Tab` | Title, Notes, Thread, Scope |
| `Enter` in Title | save |
| `Enter` in Notes | new line |
| `Ctrl+Enter` | save from any field (`Alt+Enter` is the legacy-terminal fallback) |
| `Esc` | cancel |

On the board `+` line: `Enter` saves and closes, `Shift+Enter` saves and stays
open, `Tab` expands onto the task page.
