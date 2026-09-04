---
title: Capture
description: Quick-add on the board, title tokens, and the capture overlay.
---

Press `+` for a one-line title on the status-row slot. The list stays visible.

- `Enter` saves and closes
- `Shift+Enter` saves and stays open
- `Tab` expands onto the [task page](/docs/task-page/) (title · notes · scope)

A project board defaults the draft to that project. A project-less board defaults
to your desk. Home keeps the cwd-derived default (the repo you opened from, or
desk if there is no repo).

A saved task becomes the selection. Success has no status message. The row flash
is the feedback. Refusals (empty title, bad thread) paint while the line is open
and clear when it closes.

Title is required.

## Title tokens

`!p` and `!t` each consume one whitespace-delimited argument. They can sit anywhere
in the line, in either order. A token followed by another token or by a `#word` is
bare. Tokens are stripped from the saved title, and the remaining words are
rejoined with single spaces.

| Token | Effect |
| --- | --- |
| `!p` | desk |
| `!p name` | project by basename (case-insensitive) |
| `!p /path` | that path verbatim |
| `!t` | unthread |
| `!t name` | normalized thread (lowercase ASCII alphanumerics and hyphens, first character alphanumeric, at most 32 characters) |

An ambiguous project basename is stored as typed. `tsk list --all --json` is how
you find a typo scope later.

An archived project refuses capture: a title token naming one leaves the line
open with a refusal that names the project (`project <name> is archived`), and
the refusal clears when the line closes. `tsk add` refuses the same way with
error code `project-archived`.

## Quick capture (herdr)

**Quick capture** opens the capture form as a short-lived overlay, without first
opening a split board:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

Tab through Title, Notes, Thread, Scope. `Enter` in Title or Thread saves. `Enter`
in Notes inserts a line. `Ctrl+Enter` saves from Title, Notes, or Thread, with
`Alt+Enter` as the legacy-terminal fallback; on Scope, plain `Enter` saves. `Esc`
cancels and leaves.

If text is selected in the pane you invoked it from, it arrives as the title.

Scope is three chips: **This project** (the repo the focused pane is in; marked
unavailable outside one), **Desk**, and **Other…**, which discloses a path field.
`1` `2` `3` pick a chip, `s` or `Space` cycles, `p` or `e` edits the path and `Enter`
confirms it. Refusals paint in the card: `Title required`, `invalid thread name`.

An idle board watching the same `~/.tsk` picks up the new task on the next tick.

## Expanded form

From the `+` line, `Tab` opens the task page in notes edit because a draft has
nothing to view yet. `Esc` returns to the line. A second `Tab` restores what you
typed.
