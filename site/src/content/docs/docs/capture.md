---
title: Capture
description: Quick-add on the board, title tokens, and the capture overlay.
---

Press `+` for a one-line title on the status-row slot. The list stays visible.

- `Enter` saves and closes
- `Ctrl+Enter` saves and stays open
- `Tab` expands onto the [task page](/docs/task-page/) (title · notes · scope)

A project board defaults the draft to that project. A project-less board defaults
to your desk. Home keeps the cwd-derived default (the repo you opened from, or
desk if there is no repo).

A saved task becomes the selection. Success has no status message. The row flash
is the feedback. Refusals (empty title, bad thread) paint while the line is open
and clear when it closes.

Title is required.

## Title tokens

`!p` and `!t` each consume one whitespace-delimited argument. Tokens are stripped
from the saved title.

| Token | Effect |
| --- | --- |
| `!p` | desk |
| `!p name` | project by basename (case-insensitive) |
| `!p /path` | that path verbatim |
| `!t` | unthread |
| `!t name` | normalized thread (lowercase ASCII alphanumerics and hyphens, first character alphanumeric, at most 32 characters) |

An ambiguous project basename is stored as typed. `tsk list --all --json` is how
you find a typo scope later.

## Quick capture (herdr)

**Quick capture** opens the capture form as a short-lived overlay, without first
opening a split board:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

Tab through Title, Notes, Thread, Scope. `Enter` in Title saves. `Enter` in Notes
inserts a line. `Ctrl+Enter` saves from any field. `Esc` cancels
and leaves.

An idle board watching the same `~/.tsk` picks up the new task on the next tick.

## Expanded form

From the `+` line, `Tab` opens the task page in notes edit because a draft has
nothing to view yet. `Esc` returns to the line. A second `Tab` restores what you
typed.
