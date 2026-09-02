# Changelog

## Unreleased

The landing page and docs live in `site/` again (Astro + Starlight, moved back
from `smarzban/tsk-site`). They are not part of the `tsk` binary. Site CI is
`npm ci && npm run build`; Vercel Root Directory is `site`.

Mono markdown notes: view-mode task page notes and peek paint a small markdown
subset with bold / dim / underline / reverse only: `**strong**`, `*em*` /
`_em_`, `` `code` `` (dim, ticks kept), fenced code blocks (dim fence ticks,
plain body), `#` headings (bold+underline, distinct from `**strong**`), and
`-` / `*` list markers. Peek uses the same markers, all dim. Edit mode stays raw
source. Checkbox-looking lines stay literal text (structured steps own
checklists).

Resize debounce: a burst of terminal `Resize` events (pane-edge drag) waits a
short quiet window and paints layout once at the settled size. A key or mouse
event that arrives during that window is deferred until after that paint.

Edge auto-scroll during text drag-select: holding a selection near the top or
bottom of the board list or task-page notes viewport scrolls that surface while
the drag stays armed, with a faster idle tick so the motion keeps up with the
pointer. Leaving the edge zone or releasing the button clears it.

Board list scrollbar: when the deck is taller than the viewport, a thinner thumb (`▌`)
paints on the right edge with a one-column gap so titles stay clear of it. The gutter
is blank and clickable (no `│` track). Titles and age wrap/fit instead of clipping to
`…`. Clicking the gutter or dragging the thumb moves the viewport without changing
selection or peeking. The current section header (desk, project, thread, DONE) pins
under the tabs with a one-row gap once it would scroll off, and the next header
replaces it. The task page body scrollbar uses the same paint helper, including
gutter click and thumb drag.

Pre-v0.4 store cleanup: `tsk.json` is now strict `format_version: 1` schema.
Missing or non-1 formats refuse without rewrite; legacy status, checklist, revision,
undo, dark-engine, and host-metadata compatibility shapes are no longer loaded.

Modal overlays: `?` help, `:` command palette, and `P` project scope now open
as a shared, centered mono card (dim box-drawing border on all four sides, bold
title with an `[x]` close control, and a footer key legend) instead of their own
bespoke layouts. The board stays visible around the card, and the verb bar goes
blank while one is open since the card's footer already names its own keys.

Mouse text selection: a left-button drag highlights the cells it covers (any
surface: board rows, task page, overlays) and release copies the painted text
to the system clipboard via OSC 52: the terminal or host multiplexer performs
the copy, so terminals without OSC 52 support ignore it. Clicks are deferred
until mouse-up so a drag on a board row does not also peek; a bare click still
behaves as before. A one-cell pointer wobble stays a click. Painters declare
content-only copyable rects (task titles past the glyph, peek notes past the
`│` gutter, page notes past their indent, task-page title past glyph and
status word), so chrome never reaches the clipboard. The reverse highlight uses
those same rects, so a multi-line drag does not paint through the peek `│`
gutter. A Down→Up move without intermediate Drag events still counts as a
selection. A brief `copied` status clears itself after two seconds and restores
any sticky status it covered (e.g. save-recovery).

The live document is **`tsk.json`** (lock `tsk.json.lock`, previous `tsk.json.1`).
A first run creates an empty `~/.tsk`; leftover `tasks.json` is not read.

The projectless scope is now **desk**: board header, `tsk list` labels, and the new
`--desk` flag. Stored scope values are unchanged,
so no data migration is needed.

Store hardening: `tsk.json` carries `format_version` (currently 1), and every
other format is refused rather than rewritten; each replace keeps the previous file
as `tsk.json.1`; leftover `.tsk.json.tmp.*` files are swept under the lock; the
exclusive lock uses `std::fs::File::lock` instead of `fs2`.

Rebrand to **tsk** ("a task board for your terminal"). The crate is now
`tsk-tui` building the `tsk` binary. The store unifies at `~/.tsk` (`tsk.json`
and `settings.json` side by side), overridable with `TSK_STATE_DIR` /
`TSK_CONFIG_DIR`; injected host variables like `HERDR_PLUGIN_STATE_DIR` are
ignored so the herdr pane and a bare terminal edit one board. There is no
automatic move from the old locations; a first run creates an empty `~/.tsk`.
The herdr plugin id stays `herdr-tasks`.

## 0.3.0

Tasks can carry an optional normalized thread. Headless add accepts `--thread`
and plan item `thread`; `herdr-tasks list --thread <name>` filters within the
selected scope, JSON rows always include `thread`, and human rows append
` #name` only for threaded tasks.

## 0.2.0

Per-task steps: a flat, ordered list on the task page with a shared notes and
steps scroll region, step cursor, and modifier-protected add, toggle, rename,
and delete verbs. Headless, `herdr-tasks steps <task-id> add <text>` creates
one step and `herdr-tasks steps <task-id> toggle <step-short-id>` flips one
step by unambiguous id prefix; single-task `herdr-tasks list <task-id>` prints
one line per step with its `[x]`/`[ ]` state and step short id. Step progress
never changes task status.

## 0.1.0

Queue board for capture, organization, and human-status verbs inside herdr.

Standard layout at 78×24, compact below. Human status is `ready`, `started`,
`blocked`, `review`, `done`. Mutating keys use Alt (Ctrl from the palette).
Peek, task page, project scope, done drawer, and undo are on the board.

Park, resume, linking, and dispatch-start are not board actions. Dispatch
recovery still opens if a persisted attempt is already in the store.
