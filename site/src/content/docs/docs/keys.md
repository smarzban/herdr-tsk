---
title: Keys
description: Board, task page, and capture key chords.
---

Mutating keys need **Alt** (or **Ctrl**, if you flip it in the palette). Bare
letters do nothing, so typing in a focused board cannot complete or delete work.

Nav, peek, `Enter`, `P`, `1` / `2` / `3`, `z`, `:`, `?`, `+`, and `Esc` stay
bare.

The web demo on the landing page uses bare verb letters on purpose. Browsers
steal Alt-chords.

## Board

| Key | Does |
| --- | --- |
| `j` `k` or `↑` `↓` | move selection |
| wheel | scroll the list |
| `alt+space` | start the selected task, or reopen it if it is done |
| `alt+d` | done |
| `alt+o` | reopen |
| `alt+b` | toggle blocked |
| `Enter` | open the [task page](/docs/task-page/) |
| `→` `←` | peek notes under the row (up to five lines) |
| `+` | [capture](/docs/capture/) |
| `alt+e` | edit title |
| `alt+x` or `alt+Delete` | delete (`alt+u` undoes) |
| `z` | done drawer |
| `:` | command palette |
| `?` | help |
| `P` | project picker |
| `1` `2` `3` | home tabs: desk · projects · threads |
| `Esc` | close one layer |
| `alt+q` | quit |

Click a row to peek. Click it again to close. A fast double-click opens the page.

`Esc` closes one layer at a time.

## Task page

The page is view-first. Verbs still need the modifier, except Tab and arrows.

| Key | Does |
| --- | --- |
| `alt+e` | edit title (or rename the highlighted step) |
| `alt+n` | edit notes |
| `Tab` / `Shift+Tab` | move between title, notes, thread, scope |
| `alt+a` | add a [step](/docs/steps/) |
| `↓` | activate the step cursor, then move it |
| `↑` | move the step cursor; from the first step, deactivates it |
| `alt+space` | toggle the highlighted step, or start/reopen the task |
| `alt+x` | mark, then remove, the highlighted step; otherwise delete the task |
| `Ctrl+Enter` or `Alt+Enter` | save from any field |
| `Esc` | cancel the field, or close the page |

The scope footer does nothing until an edit has started. Field clicks are inert
until then too.

## Capture line and form

| Key | Does |
| --- | --- |
| `Tab` / `Shift+Tab` | Title, Notes, Thread, Scope |
| `Enter` in Title | save |
| `Enter` in Notes | new line |
| `Ctrl+Enter` or `Alt+Enter` | save from any field |
| `Esc` | cancel |

On the board `+` line: `Enter` saves and closes, `Ctrl+Enter` saves and stays
open, `Tab` expands onto the task page.
