---
title: Task page
description: "View and edit one task: title, notes, thread, and scope."
---

Below 110 usable columns, `Enter` opens the selected task full height. At 110
usable columns or wider the page is the right column of the
[stage slider](/docs/board/#wide-stage-slider): a preview beside the board in stage
A, the focused page beside a dim rail in G, or the whole frame in F. Its header sits
on the selector row, `▸ T12 title … started · tsk` with the status glyph restored,
dim in A and bold in G and F, and a dash rule on the row under it.
With no selection the column reads `no task` and is inert. `←` from G parks the page
beside the board without resetting page scroll or the step cursor; `→` brings it back.

The dim `T30` prefix in the header copies that task identifier when clicked. A click
inside the stage A task column moves to G before running the same action the control
has in the single-pane page. The footer reads `scope · #thread · created … · updated …`. In a wide column
the project lives in the header slot and the scope slot shows only while an edit
session can change it.

It is view-first. Nothing is in edit mode until you ask. `ctrl+e` edits the title,
`ctrl+n` edits notes. In view, Tab and Shift+Tab loop only through stored steps and
`+ step`, never task fields. The trailing dim target paints as `   + step`. `Enter`,
`ctrl+a`, or a click on it opens its independent editor, then Enter saves one step
and selects that new stored step.

Bare `↓` activates the first stored step, then arrows move the selection. `ctrl+s`
toggles that selected step without changing the task's human status.

`ctrl+e` on a stored-step selection starts task editing with the step inline. In task
editing, Tab runs Title, Notes, stored steps, `+ step`, Scope, Thread, then Title.
Shift+Tab reverses that same loop. Clicking a field or another existing step moves
the one active text cursor there and stages the prior step change. Scope and Thread
first show as selected controls, with no blinking cursor. `Enter` on Scope opens its
picker and `Enter` again chooses the highlighted scope. `Enter` on Thread, or a
second click, opens or closes its text editor without leaving the task edit session.
The scope footer does nothing until task editing starts.

Plain `Enter` parks an existing-step rename without saving the task session.
`Shift+Enter` is the visible task-session save chord: it saves Title, Notes, Thread,
Scope, staged existing-step edits, and staged removals, then exits editing. `Alt+Enter`
is the legacy-terminal fallback. A staged
`ctrl+x` removal disappears immediately and returns if task editing is cancelled. New
steps save independently from view or task edit: Enter saves one and selects it,
Shift+Enter saves one and opens the next empty editor. Field `Esc` cancels that
field, while Esc from task editing restores staged removals.

Notes are multiline, so `Enter` inserts a line. `Shift+Enter` saves and does not
insert a line.

Thread uses the same name rules as capture `!t`. The field is optional.

While an edit is unsaved, moving the selection to another task refuses with
`save or cancel edits before switching tasks`. Saving a task that another writer
deleted meanwhile answers `that task is deleted`. An empty step paints
`text required` on its own row.

## Notes markdown

View and peek style a small markdown subset. Edit is always the raw source. No
color.

| Marker | Paint |
| --- | --- |
| `**bold**` | bold |
| `*em*` or `_em_` | underline |
| `` `code` `` | dim, ticks kept |
| `#` … `######` | bold + underline, dim hashes |
| `-` or `*` lists | dim bullet |
| fenced ` ``` ` | dim fence and body, no inline inside |

Unmatched `*` and word-internal `_` stay literal. `- [ ]` stays text.
[Steps](/docs/steps/) own checklists.

Peek paints the same markers, then dims every span, with a `│` gutter closed by
an L-shaped `└` connector.

## Peek vs page

Below 110 columns, `→` or a row click peeks up to five wrapped note lines, then
`… N more lines` if there are more, or `no notes yet`. At wide widths, selection
updates the task column and no inline peek opens.

Text on the page wraps. It does not truncate.

See [keys](/docs/keys/) for the full chord table.
