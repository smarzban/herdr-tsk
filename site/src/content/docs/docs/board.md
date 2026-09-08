---
title: Board
description: Tabs, sections, status, peek, and the done drawer.
---

The board keeps three navigation destinations visible: **desk** · **selected project** ·
**projects**. Use `1` / `2` / `3` from normal board mode. `p` opens the project picker
from the keyboard.

## Tabs

- **desk**: your overview. NEEDS YOU is blocked or review work across all live
  projects and your desk. IN MOTION is every started task, from any project. ON DECK is desk work
  that is ready: general to-dos, future projects, anything that belongs to no
  repository, with project attribution on other work.
- **selected project**: the project board for slot 2. Its local thread filter (shown as
  `all` or `#name`) can narrow every status section.
- **projects**: one selectable overview row per live project, with NEEDS YOU,
  IN MOTION, and READY counts anchored to the right edge so the name column takes the
  remaining width; one blank row separates the legend from the first project. A zero
  paints as a dim `·`; a live NEEDS YOU count paints bold. Rows show the project's
  basename only, with a dim `here` after the project you launched from. The status row
  names the selected row's full path, which is how same-named projects are told apart;
  while the `/` search is open that path moves to the row above the query.
  At 100 columns or wider a dim THREADS column lists that project's open threads, most
  recently touched first, ending in `+n` when they do not all fit. Enter opens the
  selected project in slot 2; a click selects a row and a fast second click on it opens it.
  Press `/` or click the footer's `/ search projects` affordance, type a project name
  (paste works too), then press Enter to open the match. Esc clears the query and
  returns to navigation. The `Overview` selector (`v`) can replace the index with a
  flat cross-project thread board, with project attribution (including `desk`).

At startup, a pane inside a repository opens that project, including when it is
empty. Outside a repository it opens Desk. Reopening tsk from another project
updates the existing board to that invocation project, or Desk outside a repository.

The three navigation slots stay visible everywhere as **desk** · **selected project** ·
**projects**. Their keyboard shortcuts remain `1`, `2`, and `3`. The selected-project
slot keeps its identity while Desk or Projects is active; if it is empty, `2` opens the
project picker. The active destination's chip shows only its selected value, such as
`all ▾`, `#release ▾`, or `Overview ▾`, and opens its picker. A selector that wraps below
the tabs has one blank line above it. Both project and cross-project thread views go
straight to the status sections, without a separate task-count summary.
Collapse state is session-only.

Thread selectors (`t` within a project, `v` on Projects) accept typing and paste.
All letters, including `j` and `k`, are search text; use arrows or Tab to move between
options. An unmatched query stays visible with `no matching options` so you can edit
it or press Esc to close.

The projects search opens in the shared footer slot: closed it reads `/ search projects`,
while focused it shows the query and caret there. Letters, digits, Backspace, and
bracketed paste edit the query, while Enter opens the selected match and Esc clears and
closes it. No search row appears above the project table.

Each persisted row begins with a dim store-global identifier such as
`T30`. Click the identifier to copy it (`copy sent: T30`). Collapsed task rows
show no project or thread label. Opening peek shows the project on global rows,
or `#thread` on project rows, on a dim `└─ label` line below the peek notes.
That line is read-only peek content, and long labels wrap without truncation.
Titles use the full width and wrap two cells before the right edge.
Task-page informational dates remain in the footer. A scoped
group with nothing open reads `no open tasks here — p rescope or + add`.

## Sections

One urgency-ordered list. Sections are computed, not navigated. Desk NEEDS YOU contains
blocked and review tasks from all live projects and the desk; project boards contain
only that project's tasks.

- **NEEDS YOU**: blocked and review, on desk (desk/global tasks) and on a project
  board (that project). Omitted when empty. Sits above IN MOTION.
- **IN MOTION**: work you have started
- **ON DECK** when you are on a project board: that project's ready tasks, with
  thread labels in their peek
- **z** opens the done drawer

An archived task keeps its human status and leaves every working lens (desk,
projects, project focus, and default `tsk list` views). The done
drawer lists archived tasks in its scope under an `▾ archived · n` group below
DONE: closed on every launch, one click or `Enter` on the header toggles it on its
own, and expanded rows paint dim with their status glyph and `T<n>`. The header
paints the word bold when it holds the selection, never a reverse block. It is the
done drawer's only collapsible group: while the drawer is open, `g` folds and
unfolds it; with the drawer closed `g` leaves it alone. `d` opens and closes the
drawer itself. `ctrl+f` on a row
files it, and `ctrl+f` or `ctrl+u` on an archived
selection brings it back; neither writes an undo entry.

Thread headers on a scoped deck show `#name` and an open count. They take a list
row of space. They are not selectable and clicks do not land on them.

Unthreaded tasks list after thread groups in the same deck.

## Human status

`ready` · `started` · `blocked` · `review` · `done`

Human status is the source of truth. Completing every step does not complete the
task. Agents do not auto-complete work.

`ctrl+s` starts a ready task, or reopens a done one. On a started, blocked, or
review task it does nothing, and the verb bar drops the entry. `ctrl+d` marks done.
`ctrl+o` sets ready again from any status; the palette's `reopen` entry appears only
for a done task. `ctrl+b` toggles blocked ↔ ready; `ctrl+r` toggles review ↔ ready. A
done task answers `completed tasks cannot be blocked` (or `… go to review`). With
nothing selected, every verb answers `select a task first`.

The verb row shows only the seats that matter for the selected row: `enter open`, the
status verbs its status makes meaningful (`ctrl+s start` on ready, `ctrl+d done` and
`ctrl+b block` or `unblock` while open, `ctrl+o reopen` when done), `+ add`, `? help`.
Archive, delete, undo, the drawer and the palette are reached by their keys, `?`, or `:`.

## Delete and undo

`ctrl+x` (or `ctrl+Delete`) asks once (`press ctrl+x again to delete`), then soft-deletes the selected task. It leaves every lens
and the status row reads `Deleted "title" · ctrl+u undo` until your next action. While
that notice is visible, the board prompt begins `ctrl+u undo`. Nothing on the board
hard-deletes; `tsk list --deleted` still shows it.

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

Everything painted as a control is clickable: the tabs, destination chips, picker
options, verb-bar entries, `ctrl+u undo` on a delete notice, and the DONE header (which
closes the drawer). When the list overflows, a scrollbar appears on the right: click
the track to jump, drag the thumb to scroll. Dragging across text selects it and copies
on release (`copied`), using the terminal's clipboard protocol. With the quick-add line
open, clicking a task row discards the draft and selects that row.

## Project picker

`p` opens the project picker with two tabs: main (unarchived projects plus
Home) and archived. `Tab` or the arrows flip tabs; the archived tab lists every
archived project and reads `no archived projects` when empty. `ctrl+f` on a
main-tab project archives it in place — the picker stays open and the project
leaves the main list, the projects index, desk IN MOTION, and the rail.
`ctrl+f` or `ctrl+u` on an archived-tab entry unarchives it, and every task
returns in the status it had. A dim rule sits under the tabs row. The main tab's
footer reads `↑↓ move · enter choose · ctrl+f archive · esc close`; the archived tab's
footer reads `ctrl+u unarchive · enter open · esc close`.

## Read-only archived focus

`Enter` on an archived-tab entry opens that project in read-only focus: the chip
reads `<name> · archived`, its tasks paint dim, and nothing is written by
entering. It is the only lens that paints an archived project's tasks; `Esc`,
`p`, or `1`/`2`/`3` leave it for your desk and hide them again (`Esc` there never quits,
and `p` leaves the lens before the picker paints). If the project is unarchived by any
other route while you sit in it, the focus becomes an ordinary project focus. Every mutating verb (`ctrl+s`,
`ctrl+d`, `ctrl+o`, `ctrl+b`, `ctrl+r`, `ctrl+e`, `ctrl+n`, `ctrl+x`, `ctrl+f`, `+`, and
step toggles) refuses with `project <name> is archived · ctrl+u unarchive` and
changes nothing; the task page opens view-only for the same reason. `ctrl+u`
unarchives the project in place and the focus becomes an ordinary project
focus.

No scope dropdown offers an archived project: not the task page's scope footer,
not an expanded quick-add draft, not the capture surface. A task already inside
an archived project still shows that scope as its own value.

## Launch inside an archived project

When the board starts with its quick-add default inside an archived project, a
card asks `project <name> is archived, would you like to unarchive it?` before
the first keypress, with `y unarchive · n keep archived` in its footer (both
clickable): `y` unarchives it durably, `n` or `Esc` keeps it archived and sends
quick-add to your desk for this session (a dim status line says so). The card
shows at most once per session, and launching anywhere else paints nothing.

## Pane size

| Pane | What you get |
| --- | --- |
| at least 110 columns | wide stage slider: board, board beside page, rail beside page, or page. The rail paints no meta, done drawer, or peek |
| at least 78×24 and below 110 columns | standard single-pane board: section headers, peek-only attribution, full verb legend |
| smaller | compact: glyph, `T<number>` identifier, and wrapped title; project/thread attribution appears only inside peek; help, palette, and the task page take the full pane |
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
| `help` · `quit` | always |
| `Retry save` · `Cancel save` | a save is waiting on you (the only two entries then) |

Mutating keys always use Ctrl.

`?` opens the help card, which lists the board chords and, below them, the task-page
chords. Any key closes it. Help, the palette, and the project
picker are boxed cards: the `[x]` in the corner or a click outside also closes them.

## When a save fails

If the store cannot be written, the status row reads `save failed: <reason> ·
Retry or Cancel` and the board keeps your draft. `r` or `Enter` retries and
answers `saved`; `c` or `Esc` cancels and drops the change (`save cancelled`).
Until you choose, mutating verbs are held; moving around, peeking, and quitting
still work. `Esc` does not skip the choice.

## Website demo

The browser demo supports a repeatable desk → select → start → open → Escape flow.
Titles retain all text and align continuation lines beneath the title; collapsed rows
have no attribution, and narrow peeks show project or thread labels beneath notes.
Browser status shortcuts are bare letters. On an open task, Tab selects steps, Enter toggles, `a` adds, `e` renames the selected
step, and `x` twice removes it. Shift+Enter saves a rename; Escape cancels.
The demo is a subset: full task editing, project archive and save recovery are not yet reproduced. It is not a complete app emulator.

Selected task rows use a plain selection arrow before the status symbol and task number, without a filled highlight. Active board tabs use bright underlined text; inactive tabs remain dim.
