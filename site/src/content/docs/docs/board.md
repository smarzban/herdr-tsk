---
title: Board
description: Tabs, sections, status, peek, and the done drawer.
---

Home is a tabbed board: **desk** · **projects** · **threads**. Tabs show only at
home (`1` / `2` / `3`). `P` opens the project picker from the keyboard.

## Tabs

- **desk**: your own planning space. IN MOTION is every started task, from any
  project. ON DECK is desk work that is ready, blocked, or review: general to-dos,
  future projects, anything that belongs to no repository. Thread headers sit
  above those open desk tasks.
- **projects**: one collapsible group per project. Single-click a header to
  collapse it. Double-click a header to focus that project. Inside a group:
  started, then review, blocked, ready.
- **threads**: thread names across projects, with collapsible project sub-groups
  under each name. Same status order.

The board opens on desk. If desk would be empty while open tasks exist elsewhere,
it opens on projects instead, then threads.

Project focus (`P` → a project) hides the tabs and paints the project name chip
on the right (`P ▾`). At home the chip is hidden. Collapse state is session-only.
It does not persist.

Each persisted row begins with a dim store-global identifier such as
`T30`. Click the identifier to copy it (`copy sent: T30`). At standard size a row
ends with `project · age`, where age reads `40s`, `12m`, `3h`, or `5d`. A scoped
group with nothing open reads `no open tasks here — P rescope or + capture`.

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

`ctrl+s` starts a ready task, or reopens a done one. On a started, blocked, or
review task it does nothing, and the verb bar drops the entry. `ctrl+d` marks done.
`ctrl+o` sets ready again from any status. `ctrl+b` toggles blocked; a done task
answers `completed tasks cannot be blocked`. **review** is set from the command
palette (`:` → `set status: review`), not from a dedicated letter. With nothing
selected, every verb answers `select a task first`.

## Delete and undo

`ctrl+x` (or `ctrl+Delete`) soft-deletes the selected task. It leaves every lens
and the status row reads `Deleted "title" · u Undo` until your next action. Nothing
on the board hard-deletes; `tsk list --deleted` still shows it.

`ctrl+u` undoes the most recent delete or done, then the one before it. If the task
changed on disk since then, undo refuses with `changed since the undoable action`
and the delete notice comes back so you can try again.

## Wide stage slider

At 110 usable columns or wider the board is a four-stage slider. Focus and geometry
are the same thing:

| Stage | Left | Right | Focus |
| --- | --- | --- | --- |
| 0 | board, full width | none | board |
| A | board, 40% | task page | board |
| G | dim rail, 32 columns | task page | task |
| F | none | task page, full width | task |

One dim `│` rule separates the columns. There is no box and no colour: the task
header rule (`T12 title ──── started · tsk`) is dim in A and bold in G and F, and its
right slot reads `editing <field>` or `unsaved` when that applies. One footer spans
the frame: the rule, the status row with a dim stage crumb on the right, and the verb
bar for whichever side owns focus. The session opens in stage 0; the stage is never
saved.

`→` and `←` slide the stage. `Enter` opens F and remembers where it came from; `Esc`
returns there. Changing the selection in A retargets the pane, with no peek. Click a
board row to select it in place; click a rail row to select it and move focus back to
the board; double-click to open F. Click inside the stage A task column to move to G
and run the control you clicked. Below 110 columns
the stage is kept: 0 and A show the board, G and F show the page, and growing back
restores the same view.

## Peek and mouse

Below 110 columns, `→` peeks notes under the selected row (up to five wrapped
lines). `←` closes
the peek.

Below 110 columns, click a dim task identifier to copy it, click elsewhere on a row
to peek, and click the same row again to close. At wide widths a board or rail row
click selects it in place and a fast double-click opens the
[task page](/docs/task-page/). The wheel scrolls the list.

Everything painted as a control is clickable: the tabs, the project chip, the
verb-bar entries, `u Undo` on a delete notice, and the DONE header (which closes
the drawer). Group headers collapse on click and focus their project on a fast
double-click; a project sub-header under a thread does the same. When the list
overflows, a scrollbar appears on the right: click the track to jump, drag the thumb
to scroll. Dragging across text selects it and copies on release (`copied`), using
the terminal's clipboard protocol. With the quick-add line open, clicking a task row
discards the draft and selects that row.

## Pane size

| Pane | What you get |
| --- | --- |
| at least 110 columns | wide stage slider: board, board beside page, rail beside page, or page. The rail paints no meta, done drawer, or peek |
| at least 78×24 and below 110 columns | standard single-pane board: section headers, row meta, full verb legend |
| smaller | compact: glyph, `T` identifier, and title; help, palette, and the task page take the full pane |
| down to 40×10 | still operable |

A typical herdr split is 78 columns, which is the standard board.

## Palette and help

`:` opens a searchable command palette. Type to filter: the query matches when its
letters appear in order anywhere in a label, so `ssr` finds `set status: review`.
`q` is a query character here, not quit. `↑` `↓` or `Tab` / `Shift+Tab` move,
`Enter` runs, `Esc` closes.

| Command | Shows when |
| --- | --- |
| `set status: ready` · `started` · `blocked` · `review` | a task is selected |
| `edit notes` · `change scope` | a task is selected |
| `new task` | always |
| `delete` | a task is selected |
| `reopen` | the selected task is done |
| `undo` · `done drawer` | always |
| `toggle groups` | on the projects or threads tab |
| `help` · `quit` | always |
| `Retry save` · `Cancel save` | a save is waiting on you (the only two entries then) |

Mutating keys always use Ctrl.

`?` opens the help card. Any key closes it. Help, the palette, and the project
picker are boxed cards: the `[x]` in the corner or a click outside also closes them.

## When a save fails

If the store cannot be written, the status row reads `save failed: <reason> ·
Retry or Cancel` and the board keeps your draft. `r` or `Enter` retries and
answers `saved`; `c` or `Esc` cancels and drops the change (`save cancelled`).
Until you choose, mutating verbs are held; moving around, peeking, and quitting
still work. `Esc` does not skip the choice.
