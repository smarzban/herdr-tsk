---
title: Task page
description: "View and edit one task: title, notes, thread, and scope."
---

`Enter` opens the selected task full height. The dim `T30` prefix in the
header copies that task identifier when clicked.

It is view-first. Nothing is in edit mode until you ask. `ctrl+e` edits the title,
`ctrl+n` edits notes, and `Tab` selects and cycles steps when they exist. A step
click also selects it, while `Enter` remains read-only. `ctrl+e` on that selection
starts task editing with the step inline. From an existing-step row, Tab cycles
Title, Notes, Thread, Scope, then the steps. Clicking a field or another existing
step moves the one active text cursor there and stages the prior step change. Scope
has focus but no blinking cursor. The scope footer does nothing until task editing
starts.

`Shift+Enter` is the only task-session save chord: it saves Title, Notes, Thread,
Scope, and every staged existing-step edit, then exits editing. A new step keeps
its own Shift+Enter save-and-next loop. Field `Esc` cancels that field. Page `Esc`
closes the page.

Notes are multiline, so `Enter` inserts a line. `Shift+Enter` saves and does not
insert a line.

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

Peek paints the same markers, then dims every span, with a `│` gutter closed by
an L-shaped `└` connector.

## Peek vs page

On the list, `→` or a row click peeks up to five wrapped note lines. Open the
page when you want the full notes, thread, scope, and steps.

Text on the page wraps. It does not truncate.

See [keys](/docs/keys/) for the full chord table.
