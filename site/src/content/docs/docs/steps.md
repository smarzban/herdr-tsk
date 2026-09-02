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
and `Shift+Tab` select and cycle steps without leaving task view. A step click also
selects it, and `Enter` keeps that selection read-only.

- `ctrl+space` toggles a selected step in either view or an edit session
- `ctrl+e` on a selected step starts the task edit session and opens its inline editor
- From an existing-step editor, `Tab` cycles Title, Notes, Thread, Scope, then returns to steps
- Clicking a task field or another existing step moves the one active text cursor there and stages the prior step draft
- Scope is a choice, not text: it has focus but never a blinking cursor
- `↑` / `↓` move a selected step; `↑` from the first step returns to page reading

With the cursor selected, the usual verbs act on that step:

| Key | Does |
| --- | --- |
| `ctrl+a` | add an inline step |
| `ctrl+space` | toggle done |
| `ctrl+e` | edit the highlighted step inline |
| `ctrl+x` | mark, then a second press removes |

A new-step draft lives in the steps section. `Shift+Enter` is its only save chord
and opens an empty next row. Existing-step changes join the task edit session with
Title, Notes, Thread, and Scope: move among them freely, then `Shift+Enter` saves
all staged changes and exits editing. Plain `Enter` does not save. `Esc` cancels
the active field. A failed save keeps its drafts until you retry or cancel.

Wheel still scrolls the page. Before a step is selected, arrows read the page;
a selected step owns them during the edit session.

## From the CLI

```bash
tsk steps <task> add "Write the failing test"
tsk steps <task> toggle <step-short-id>
tsk list <task>
```

The task is a store-global number (`T12`, `t12`, or bare `12`) or a UUID from
`tsk add --json` or `tsk list --json`. Direct lookup ignores cwd.

A step short id is the shortest unambiguous prefix of that step's id, as printed
by `tsk list <task>`.

`toggle` flips the flag. A blind retry after an unseen success flips it back.
Verify with `tsk list <task>` before retrying.

Soft-deleted tasks refuse step changes.

Full flags and exit codes: [CLI](/docs/cli/).
