---
title: Capture
description: Quick-add on the board, title tokens, and the herdr quick-capture popup.
---

Press `+` for a one-line title on the status-row slot. The list stays visible. Its
prompt reads `enter save · tab details · esc close`; `Shift+Enter` still saves and
stays open, and is listed in help rather than the short prompt.

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
| `!t name` | normalized thread (lowercase ASCII alphanumerics, hyphens, and dots, first character alphanumeric, at most 32 characters) |

An ambiguous project basename is stored as typed. `tsk list --all --json` is how
you find a typo scope later.

An archived project refuses capture: a title token naming one leaves the line
open with a refusal that names the project (`project <name> is archived`), and
the refusal clears when the line closes. `tsk add` refuses the same way with
error code `project-archived`.

## Quick capture (herdr)

**Quick capture** opens the expanded quick-add page in a short-lived popup, without
first opening a split board:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

It is the same expanded page you get with `+` then `Tab` on the board, but it opens
with the cursor in Title. Type the title on the page header, notes under it; thread
and `+ step` are on the page. `Shift+Enter` saves and closes the popup. `Esc` cancels:
one press discards the draft and closes the popup without creating a task. If the
save fails, the popup stays open with the draft editable and the usual retry/cancel
recovery.

If text is selected in the pane you invoked it from, it arrives as the title. Scope
defaults to the repo of the focused pane, or the desk outside one; change it on the
page's scope field, or with `!p` tokens in the title. An archived project is never
a default: the draft falls back to the desk.

An idle board watching the same `~/.tsk` picks up the new task on the next tick.

## Expanded form

From the `+` line, `Tab` opens the task page in notes edit because a draft has
nothing to view yet. Thread and `+ step` are on that page. `Esc` returns to the
line. A second `Tab` restores what you typed.

The mouse wheel and page scrollbar can reach `+ step` below long notes without
leaving the draft. Typing or moving the Notes cursor brings it back into view.

In the capture popup, clicking Title or Notes places the cursor at the clicked
character, including wrapped and scrolled note lines.

The page scrollbar also works while a new step is being typed. A field click
that refuses to leave an unfinished step keeps that step’s cursor unchanged.
