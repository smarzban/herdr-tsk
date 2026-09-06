---
title: Steps
description: Flat checklists on a task, from the page and from the CLI.
---

A task can carry a flat, ordered list of steps. One level. No nesting. Each step
has a stable id, one line of text, and a done flag.

Checking every step does **not** mark the task done. Steps are progress on the
page, not a second status.

Markdown `- [ ]` in notes is ordinary text. Use steps for checklists.

## On the task page

Open the task with `Enter`. The step cursor begins inactive. In view mode, `Tab`
and `Shift+Tab` cycle stored steps and the trailing dim `+ step` target without
leaving task view. The target sits immediately below the final stored step as `   + step`.
`Enter`, `ctrl+a`, or a click on it opens an independent new-step editor, it does not
start task editing. `ctrl+a` also opens that editor from every task-edit field or
inline step state.

- `ctrl+s` toggles the selected step, otherwise it starts or reopens the task
- `ctrl+d` completes a selected open step, `ctrl+o` reopens a selected done step
- `ctrl+e` on a selected stored step starts task editing and opens it inline
- In task editing, Tab runs Title, Notes, stored steps, `+ step`, Scope, Thread
- Reaching a stored step through Tab or arrows opens its inline editor
- `ctrl+x` in view first marks the selected step and the second press removes it,
  while task editing immediately hides and stages its removal until save

The section is labelled `steps <done>/<total>`. Rows paint `▪` open, `✓` done, and
`✗` while a step is marked for removal.

A new-step draft lives in the steps section. From view or task edit, it persists
independently: it never joins the enclosing task edit session. Plain `Enter` saves one new
step and opens the next empty editor. `Shift+Enter` saves that step and the enclosing
task-edit session. An empty new-step row discards if you click elsewhere. Existing-step
renames and removals are staged with Title, Notes, Thread, and Scope. Plain `Enter` parks
an existing-step rename in that session, then `Shift+Enter` saves all staged changes
(`Alt+Enter` is the legacy-terminal fallback). `Esc` cancels a field; Esc from task
editing restores staged removals. A failed save keeps its drafts until you retry or cancel.

Wheel still scrolls the page. Bare `↓` activates the first stored step, then arrows
move the selected stored step. In an existing-step editor, arrows move among stored
steps and stage the prior draft; in an add editor, arrows scroll the shared page
without closing or trapping that row.

## From the CLI

```bash
tsk steps <task> add "Write the failing test"
tsk steps <task> toggle <step-short-id>
tsk steps <task> rename <step-short-id> "Pin the saved id"
tsk steps <task> remove <step-short-id>
tsk list <task>
```

The task is a store-global number (`T12`, `t12`, or bare `12`) or a UUID from
`tsk add --json` or `tsk list --json`. Direct lookup ignores cwd.

A step short id is the shortest unambiguous prefix of that step's id, as printed
by `tsk list <task>`.

`toggle` flips the flag. A blind retry after an unseen success flips it back.
`rename` is idempotent on the trimmed text. `remove` is not: a retry after an
unseen success is `unknown-step`. Verify with `tsk list <task>` before retrying
toggle or remove.

Soft-deleted tasks refuse step changes.

Full flags and exit codes: [CLI](/docs/cli/).
