---
title: Board
description: Tabs, sections, status, peek, and the done drawer.
---

Home is a tabbed board: **desk** · **projects** · **threads**. Tabs show only at
home (`1` / `2` / `3`). `P` opens the project picker from the keyboard.

## Tabs

- **desk**: IN MOTION is every started task, from any project. ON DECK is desk
  work that is ready, blocked, or review. Thread headers sit above those open
  desk tasks.
- **projects**: one collapsible group per project. Single-click a header to
  collapse it. Double-click a header to focus that project. Inside a group:
  started, then review, blocked, ready.
- **threads**: thread names across projects, with collapsible project sub-groups
  under each name. Same status order.

Project focus (`P` → a project) hides the tabs and paints the project name chip
on the right (`P ▾`). At home the chip is hidden. Collapse state is session-only.
It does not persist.

Each persisted row begins with a dim store-global identifier such as
`T30`. Click the identifier to copy it. Age and project remain trailing meta when shown.

## Sections

One urgency-ordered list. Sections are computed, not navigated.

- **IN MOTION**: work you have started
- **ON DECK** when you are on a project board: that project's ready, blocked, and
  review tasks, with thread headers above their open rows
- **z** opens the done drawer

Thread headers on a scoped deck show `#name` and an open count. They take a list
row of space. They are not selectable and clicks do not land on them.

Unthreaded tasks list after thread groups in the same deck.

## Human status

`ready` · `started` · `blocked` · `review` · `done`

Human status is the source of truth. Completing every step does not complete the
task. Agents do not auto-complete work.

`ctrl+space` starts a ready task, or reopens a done one. `ctrl+d` marks done.
`ctrl+o` reopens. `ctrl+b` toggles blocked. **review** is set from the command
palette (`:` → `set status: review`), not from a dedicated letter.

## Peek and mouse

`→` peeks notes under the selected row (up to five wrapped lines). `←` closes
the peek.

Click a dim task identifier to copy it. Click elsewhere on a row to peek.
Click the same row again to close. A fast double-click opens the
[task page](/docs/task-page/). The wheel scrolls the list.

## Pane size

| Pane | What you get |
| --- | --- |
| at least 78×24 | standard board: section headers, row meta, full verb legend |
| smaller | compact: glyph, `T` identifier, and title; help, palette, and the task page take the full pane |
| down to 40×10 | still operable |

A typical herdr split is 78 columns, which is the standard board.

## Palette and help

`:` opens a searchable command palette. Type to filter. `q` is a query character
here, not quit. Leave with `Esc`.

Mutating keys always use Ctrl. Legacy `verb_modifier: "alt"` settings are read as
Ctrl.

`?` opens the help card. Any key closes it.
