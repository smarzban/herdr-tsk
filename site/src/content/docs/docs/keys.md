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

## Board

| Key | Does |
| --- | --- |
| `j` `k` or `↑` `↓` | move selection |
| wheel | scroll the list |
| `ctrl+space` | start the selected task, or reopen it if it is done |
| `ctrl+d` | done |
| `ctrl+o` | reopen |
| `ctrl+b` | toggle blocked |
| `Enter` | open the [task page](/docs/task-page/) |
| `→` `←` | peek notes under the row (up to five lines) |
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
| `Tab` / `Shift+Tab` | select and cycle steps, wrapping at either end; without steps, move through task fields |
| `ctrl+a` | add an inline [step](/docs/steps/) after editing has started |
| `Enter` or click | edit the selected step inline, after editing has started |
| `↓` `↑` | move a step selected by Tab |
| `ctrl+space` | toggle the highlighted step, or start/reopen the task |
| `ctrl+x` | mark, then remove, the highlighted step; otherwise delete the task |
| `Shift+Enter` | save task edits; step add saves and opens the next row |
| `Esc` | cancel the field, or close the page |

The scope footer does nothing until an edit has started. Field clicks are inert
until then too.

## Capture line and form

| Key | Does |
| --- | --- |
| `Tab` / `Shift+Tab` | Title, Notes, Thread, Scope |
| `Enter` in Title | save |
| `Enter` in Notes | new line |
| `Ctrl+Enter` | save from any field |
| `Esc` | cancel |

On the board `+` line: `Enter` saves and closes, `Shift+Enter` saves and stays
open, `Tab` expands onto the task page.
