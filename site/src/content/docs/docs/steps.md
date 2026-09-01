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

Open the task with `Enter`. The step cursor starts inactive.

- Bare `↓` activates it on the first step (again after you deactivate it)
- `↑` / `↓` move it while it is active
- `↑` from the first step deactivates it
- A click on a step row moves the cursor there

With the cursor active, the usual verbs act on that step:

| Key | Does |
| --- | --- |
| `alt+a` | add a step (footer line) |
| `alt+space` | toggle done |
| `alt+e` | rename the highlighted step |
| `alt+x` | mark, then a second press removes |

The add/rename line is a one-line draft. `Enter` applies it. `Ctrl+Enter` adds
and leaves the line open empty. `Esc` cancels. A failed save keeps the line
until you retry or cancel.

Wheel still scrolls the page. Arrows belong to the step cursor while it is
active.

## From the CLI

```bash
tsk steps <task> add "Write the failing test"
tsk steps <task> toggle <step-short-id>
tsk list <task>
```

The task is a store-global number (bare digits) or a UUID from `tsk add --json`
or `tsk list --json`. Direct lookup ignores cwd.

A step short id is the shortest unambiguous prefix of that step's id, as printed
by `tsk list <task>`.

`toggle` flips the flag. A blind retry after an unseen success flips it back.
Verify with `tsk list <task>` before retrying.

Soft-deleted tasks refuse step changes.

Full flags and exit codes: [CLI](/docs/cli/).
