---
title: Capture
description: Quick-add bar and title tokens for project and thread.
---

Press `+` for a one-line title on the status-row slot. The list stays visible.

- `Enter` saves and closes
- `Ctrl+Enter` saves and stays open
- `Tab` expands onto the task page (title · notes · scope)

A project board defaults the draft to that project. A project-less board defaults
to your desk. Home keeps the cwd-derived default.

## Title tokens

`!p` and `!t` each consume one whitespace-delimited argument. Tokens are stripped
from the saved title.

| Token | Effect |
| --- | --- |
| `!p` | desk |
| `!p name` | project by basename (case-insensitive) |
| `!p /path` | that path verbatim |
| `!t` | unthread |
| `!t name` | normalized thread (lowercase ASCII alphanumerics and hyphens, ≤32 chars) |

A saved task becomes the selection. Success has no status message: the row flash
is the feedback.
