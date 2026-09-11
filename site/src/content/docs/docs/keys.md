---
title: Keys
description: Keyboard shortcuts by action and surface.
---

Use the mouse, the keyboard, or both. Tabs, selectors, tasks, and footer actions are clickable; double-click a task to open it.

Press `?` on the board for all shortcuts. Status and delete shortcuts use **Ctrl**; navigation uses bare keys. The website demo uses bare status keys because browsers reserve control chords.

## Board

| Action | Key |
| --- | --- |
| Select previous / next task | `↑` / `↓` or `k` / `j` |
| Open task full screen | `Enter` |
| Peek / close peek below 110 columns | `→` / `←` |
| Move through wide views | `→` / `←` |
| Add task | `+` |
| Edit title | `ctrl+e` |
| Start ready task; return done task to ready | `ctrl+s` |
| Mark done | `ctrl+d` |
| Set ready | `ctrl+o` |
| Toggle blocked / ready | `ctrl+b` |
| Toggle review / ready | `ctrl+r` |
| Delete, with a second press to confirm | `ctrl+x` or `ctrl+Delete` |
| Undo completion/deletion; restore an archived selection | `ctrl+u` |
| Archive / restore selected task | `ctrl+f` |
| Open / close done drawer | `d` |
| Expand / collapse its archived group | `g` |
| Desk / selected project / projects | `1` / `2` / `3` |
| Project picker | `p` |
| Project thread filter | `t` |
| Cross-project view selector | `v` on Projects |
| Search projects | `/` on Projects |
| Command palette | `:` |
| All shortcuts | `?` |
| Close a layer | `Esc` |
| Quit from board view | `ctrl+q` or `ctrl+c` |

Done tasks cannot be blocked or sent to review until reopened. `ctrl+s` leaves started, blocked, and review tasks unchanged.

## Task page

These keys apply in **view mode**:

| Action | Key |
| --- | --- |
| Select steps and **+ step** | `Tab` / `Shift+Tab` |
| Activate first step, then move among steps | `↓`, then `↑` / `↓` |
| Toggle selected step | `Enter` |
| Add step | `ctrl+a` |
| Edit title or selected step | `ctrl+e` |
| Edit notes | `ctrl+n` |
| Change task status | Board status shortcuts above |
| Mark/remove selected step; otherwise confirm/delete task | `ctrl+x` |
| Close task | `Esc` |

Click a step to select it. Click **+ step** to add. Field clicks become editable only after task editing starts.

## Editing

| Action | Key |
| --- | --- |
| Next / previous field | `Tab` / `Shift+Tab` |
| Save task edit | `Shift+Enter` |
| Cancel field | `Esc` or `ctrl+c` |
| Move from Title to Notes | `Enter` in Title |
| New line | `Enter` in Notes |
| Add another step | `ctrl+a` on the task page |
| Save new step and open next empty row | `Enter` in new step |
| Retain existing-step rename in session | `Enter` in existing step |
| Stage selected-step removal | `ctrl+x` in task edit |
| Open Scope picker / confirm selection | `Enter` |
| Cycle Scope | `Space` or `←` / `→` |
| Open / close selected Thread editor | `Enter` |
| Line start / end | `Home` / `End` |
| Word left / right | `ctrl+←` / `ctrl+→` |
| Delete backward / forward | `Backspace` / `Delete` |
| Move through wrapped notes | `↑` / `↓` |

`ctrl+e` moves to line end in text editors. `ctrl+a` moves to line start in quick-add; on the task page it adds a step instead. Paste preserves line breaks in Notes and converts them to spaces in single-line fields.

Editing keys take precedence over view-mode status shortcuts. In the step editor, `ctrl+d` and `ctrl+o` still address the task. `Alt+Enter` does not save the task edit.

## Quick-add

| Action | Key |
| --- | --- |
| Save and close | `Enter` |
| Save and keep adding | `Shift+Enter` |
| Expand details | `Tab` |
| Cancel line / return from details | `Esc` |

In Herdr quick capture, `Shift+Enter` saves and closes the popup. `Esc` closes an active step editor or scope picker first; otherwise it discards the popup. During save recovery, it cancels the pending save. [Capture guide](/docs/capture/).

## Pickers

| Surface | Controls |
| --- | --- |
| Project picker | Arrows or `j`/`k` select; `Tab` or `←`/`→` switch tabs; `Enter` opens; `Esc` or `q` closes |
| Project archive | `ctrl+f` archives; on archived tab, `ctrl+f` or `ctrl+u` restores |
| Archived project view | `ctrl+u` restores; `Esc`, `p`, or `1`–`3` leaves |
| Thread/view selector | Type or paste to filter; arrows or `Tab` select; `Enter` chooses; `Esc` closes |
| Projects search | Type or paste; `Backspace` edits; `Enter` opens result; `Esc` clears and closes |
| Palette | Type to filter; arrows or `Tab` select; `Enter` runs; `Esc` closes |
| Help | Arrows, `j`/`k`, page keys, or wheel scroll; `Esc`, `?`, or `q` closes |
| Archived-project launch prompt | `y` restores; `n` or `Esc` keeps archived |
| Failed save | `r` or `Enter` retries; `c` or `Esc` cancels |

`j` and `k` are text in thread/view filters, not navigation.

## Wide stage slider

At 110 usable columns or wider, arrows move through these views when you are not editing:

| View | `→` | `←` | `Esc` |
| --- | --- | --- | --- |
| Board | Split | No change | No change |
| Split, board focused | Task with rail | Board | No change |
| Task with rail | Full screen | Split | Split |
| Full screen | No change | Task with rail | Return to the view remembered when opening |

`Enter` from the board opens full screen and remembers the previous view. `Tab` navigates task fields or steps; it does not switch wide views.

[Board views and mouse behavior](/docs/board/#wide-stage-slider).
