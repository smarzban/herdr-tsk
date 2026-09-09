---
title: Board
description: Find tasks, switch projects, and keep work moving.
---

Click to navigate, or use the keyboard. The footer shows actions for the current selection; those actions are clickable.

## Navigate

| View | Key | Contents |
| --- | --- | --- |
| **desk** | `1` | Blocked/review and started tasks across all live projects; ready tasks from your desk |
| **selected project** | `2` | Tasks in the selected project |
| **projects** | `3` | Project overview |

Launching inside a Git repository opens that project. Outside a repository, tsk uses the current directory as the project. Reopening from another directory updates the existing board's project context.

The middle tab remembers your selected project. If none is selected, `2` opens the project picker.

## Projects

Press `p` to choose a project, or open **projects** for an overview.

- Click a project to select it; double-click or press `Enter` to open it.
- Press `/` to search. Type or paste, then press `Enter` to open the result.
- Press `Esc` to clear and close search.
- Check the footer for the selected project's full path.

Counts show work needing attention, in progress, and ready. A dim `·` means zero. `here` marks the launch project. At 100 columns or wider, the overview also lists threads.

## Threads

A thread groups related tasks within a project, such as `release` or `login-fix`.

| Where | Action |
| --- | --- |
| Project board | Press `t` or click the filter to choose a thread |
| Projects overview | Press `v` or click **Overview** to view a thread across projects |

Type to filter the choices. Use arrows or `Tab` to select, `Enter` to choose, and `Esc` to close. `j` and `k` are search text in these selectors.

Assign threads when [capturing](/docs/capture/#title-tokens) or [editing a task](/docs/task-page/#scope-and-thread).

## Status

| Section | Status |
| --- | --- |
| **NEEDS YOU** | `blocked`, `review` |
| **IN MOTION** | `started` |
| **ON DECK** | `ready` |
| Done drawer | `done` |

On your desk, **ON DECK** contains only desk tasks. On a project board, it contains that project's ready tasks. Use the thread filter to narrow them.

Select a task, then click a footer action or use:

| Key | Action |
| --- | --- |
| `ctrl+s` | Start a ready task; return a done task to ready |
| `ctrl+d` | Mark done |
| `ctrl+o` | Set ready |
| `ctrl+b` | Set blocked; press again to return to ready |
| `ctrl+r` | Set review; press again to return to ready |

`ctrl+s` leaves started, blocked, and review tasks unchanged. Done tasks must be reopened before blocking or sending to review.

Agents can set any status with [the CLI](/docs/cli/#status). Task status does not change automatically when steps are checked or an agent stops.

## Mouse

| Action | Result |
| --- | --- |
| Click a tab or selector | Change view or open its choices |
| Click a task | Peek in narrow panes; select in wide panes |
| Click the same task again in a narrow pane | Close its peek |
| Double-click a task | Open it full screen |
| Click its `T` number | Copy the task number |
| Click a footer action | Run that action |
| Wheel or drag a scrollbar | Scroll |
| Drag across text | Select and copy on release |

Open peeks show notes and the task's project or thread. Below 110 columns, `→` opens a peek and `←` closes it. Peeks show up to five wrapped note lines; the [task page](/docs/task-page/) shows the rest.

## Wide stage slider

At **110 usable columns** or wider, use `→` and `←` to move through a four-stage slider:

| View | What you see |
| --- | --- |
| Board | Full-width board |
| Split | Board beside task details; selecting another task updates the details |
| Task | Task details beside a narrow board rail |
| Full screen | Task details only |

Press `Enter` from the board to open a task full screen. `Esc` returns to the view you left. From task focus, `Esc` returns focus to the board beside it.

Click inside the task column to focus it. Click a rail row to bring back the split board. While editing, arrows move the text cursor instead.

Narrowing the pane shows one surface; widening it restores the selected view. Each new session starts with the board alone.

| Pane size | Layout |
| --- | --- |
| 110 columns or wider | Board and task views |
| At least 78×24, below 110 columns | Single board or task page |
| Smaller | Compact layout; usable down to 40×10 |

## Completed tasks

Press `d` to open or close the done drawer. Select a done task and press `ctrl+o` to return it to ready.

The drawer's **archived** group starts closed. Click its heading or press `Enter` on it to expand. `g` folds or unfolds the group while the drawer is open.

## Archive

Archiving hides work without changing its status.

| Archive | Restore |
| --- | --- |
| Task: select it and press `ctrl+f` | Open the done drawer's archived group, select it, then press `ctrl+u` or `ctrl+f` |
| Project: press `p`, select it, then `ctrl+f` | Switch to the picker's **archived** tab, select it, then `ctrl+u` or `ctrl+f` |

Use `Tab`, `←`, or `→` to switch project-picker tabs. Archiving has no undo entry.

### View an archived project

Press `Enter` on a project in the picker's **archived** tab. Its tasks open read-only. Press `ctrl+u` to restore the project, or `Esc`, `p`, or `1`–`3` to leave.

Archived projects are excluded from task-destination pickers. Restoring a project leaves individually archived tasks archived.

### Launch inside an archived project

tsk asks whether to restore it. Choose `y` to restore, or `n`/`Esc` to keep it archived. Keeping it archived sends new captures to your desk for that session.

## Delete and undo

Press `ctrl+x` twice to delete the selected task. `ctrl+Delete` is an alternative.

Press `ctrl+u` to undo a deletion or completion. Undo refuses if another writer has changed the task since that action. On an archived selection, `ctrl+u` restores it instead.

Deleted tasks remain available through [trash commands](/docs/cli/#trash) for a limited time. They do not appear on the board.

## Palette

Press `:` and type to find an action. Use arrows or `Tab` to select, `Enter` to run, and `Esc` to close.

| Available actions | When |
| --- | --- |
| New task, undo, done drawer, help, quit | Always |
| Set ready/started/blocked/review, edit notes, change scope, delete | A task is selected |
| Reopen | A done task is selected |
| Retry save, cancel save | A save has failed |

Search matches letters in order: `ssr` finds `set status: review`.

## Help

Press `?` for every shortcut. Scroll with arrows, `j`/`k`, page keys, or the wheel. Close with `Esc`, `?`, or `q`.

## Save failures

If saving fails, the draft stays open.

- `r` or `Enter`: retry.
- `c` or `Esc`: cancel the pending change.

Resolve the failure before making another change. See [storage](/docs/storage/) for directory and backup information.
