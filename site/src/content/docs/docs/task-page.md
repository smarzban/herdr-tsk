---
title: Task page
description: "View and edit one task: title, notes, thread, and scope."
---

`Enter` opens the selected task full height.

It is view-first. Nothing is in edit mode until you ask. `alt+e` edits the title,
`alt+n` edits notes, `Tab` moves between title, notes, thread, and scope. The
scope footer does nothing until an edit has started.

`Ctrl+Enter` or `Alt+Enter` saves from any field. Field `Esc` cancels that field.
Page `Esc` closes the page.

Notes are multiline, so `Enter` inserts a line. `Ctrl+Enter` saves when the
terminal reports it. `Alt+Enter` saves everywhere else. Both save. Neither
inserts a line.

Thread uses the same name rules as capture `!t`. The field is optional.

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

Peek paints the same markers, then dims every span, with a `│` gutter.

## Peek vs page

On the list, `→` or a row click peeks up to five wrapped note lines. Open the
page when you want the full notes, thread, scope, and steps.

Text on the page wraps. It does not truncate.

See [keys](/docs/keys/) for the full chord table.
