# Changelog

## Unreleased

`tsk setup herdr` registers embedded plugin assets with Herdr 0.9+, sharing the
installed standalone binary. It adds prefix+t board and prefix+a quick capture,
asks before replacing conflicting shortcuts, and refuses noninteractive conflicts
without changes. No source checkout, separate build, or automatic install-time link.

Native distribution: an owner-run draft-release workflow builds four
macOS/Linux architectures, generates checksums and a version-pinned Homebrew formula.
The installer downloads stable release assets, verifies SHA-256, and installs to
~/.local/bin. Homebrew installs from the checksum-pinned smarzban/tap/tsk formula.

Board attribution appears only inside an open peek, on a dim `└─ label` line
below its notes. Collapsed rows show no labels. Titles wrap two cells before the right edge without a label column.

Quick capture opens the expanded quick-add page in a herdr popup instead of the old
capture form. It is the same page as `+` then `Tab` on the board, opening with the
cursor in Title: scope and selected-text prefill from the focused pane, `Shift+Enter`
saves and closes the popup, `Esc` cancels and closes without creating a task, and a
failed save keeps the draft editable with retry/cancel. `tsk capture` (or
`TSK_MODE=capture`) opens the same page.

Thread names accept dots (`v0.0.6`). A refusal names the rule instead of `invalid thread name`.

Expanded quick-add can set thread and add steps. Enter on a new step saves it and opens the next empty row; Shift+Enter saves the step and the task-edit session. An empty new-step row discards if you click elsewhere. Deleting a task asks `press ctrl+x again to delete` first.

NEEDS YOU section: blocked and review tasks sit above IN MOTION on desk and on a
project board. Desk ON DECK / project ON DECK keep ready work. An empty deck
header is omitted while NEEDS YOU has rows.

`tsk edit --notes` keeps newlines and tabs, same as `tsk add -n`.

Agent CLI mutations: `tsk status T<n> <status>` sets ready, started, blocked, or
review, or done (`start` is an alias for `started`; idempotent on repeat), `tsk edit T<n>`
updates title and/or notes, and `tsk steps` gains `rename` and `remove`.

Human CLI output escapes C0/C1 controls in titles, step text, project names, and usage reasons that echo argv.
`tsk list T<n>` documents live-store lookup vs `trash.jsonl`. Install docs keep
`./target/release/tsk` after build, with an optional PATH export.

## 0.5.0

Wide task view: at 110 usable columns or wider, the board and task page form a
four-stage slider. Arrow keys move between the full board, board plus task,
rail plus task, and full task page while preserving drafts and focus across
resizes. Narrow layouts retain the existing board, peek, and page behavior.

Private state permissions: on Unix the state/config directory (`~/.tsk` by
default) is `0700` and every file in it — `tsk.json`, `tsk.json.1`,
`tsk.json.v<N>`, `trash.jsonl`, `tsk.json.lock`, `walkthrough.json`, and any
temp file — is `0600`, for fresh writes and tightened after the fact for paths
an older version left readable. Tightening only strips bits; stricter modes an
owner chose are kept. State/config directory roots that are symlinks are
refused rather than chmodding their targets. Other platforms claim no mode.

Archive for tasks and projects: `ctrl+f` on a board row toggles a task's
archived flag (no undo entry), and archived tasks keep their human status while
leaving every working lens — desk, selected project, projects, and project focus,
default `tsk list` views. The done drawer gains a collapsible `archived · n`
group (closed on launch, dim rows) that a click or `Enter` on its header toggles
and that `ctrl+g` folds with every other group while the drawer is open, the project picker gains main/archived tabs with in-place archive and
unarchive, and launching inside an archived project raises a one-time card
asking `project <name> is archived, would you like to unarchive it?` (`y`
unarchives, `n` keeps archived and sends quick-add to the desk for the
session). `Enter` on the picker's archived tab opens that project in read-only
focus: its tasks paint dim under a `<name> · archived` chip, every mutating verb
refuses with `project <name> is archived · ctrl+u unarchive`, and `ctrl+u`
unarchives in place. No scope dropdown offers an archived project.
Quick-add `!p <name>`
and `tsk add` into an archived project refuse with code `project-archived`. New
CLI: `tsk archive T<n>`, `tsk unarchive T<n>`,
`tsk project archive|unarchive <name>` (all idempotent), and
`tsk list --archived` with `archived` / `project archived` row marks.

Store format 2: documents gain an always-present `projects` map (empty when no
project is archived). A v1 store loads through a migration chain and its first
save leaves a byte-identical `tsk.json.v1` backup beside the live file.

Trash for deleted tasks: a soft-deleted task leaves the board store once it is
no longer undoable (an undo entry no longer restores it) or once it has been
deleted for 7 days, and moves to `trash.jsonl` beside `tsk.json`. The file
holds one line per task with its delete time, is rewritten atomically on each
change, and lines purge after 30 days. `tsk list --deleted` now lists live
soft-deleted tasks and trash entries together, newest deletion first, and
`tsk trash restore <task>` puts a trashed task back on the board with its
number.

`tsk trash restore T<n>` restores a trashed task by number (or UUID):
not soft-deleted, with a `restored` history event, a new revision, and its old
number. Refusals (no such line, or the task already live) exit 1 with
`T<n> is not in trash`; usage exits 2, store I/O exits 3.

Bounded undo: the saved undo stack is capped at 50 entries, and entries whose
task is gone or whose revision moved on are dropped at the save boundary.
In-memory undo behaviour is unchanged.

Synced-folder note: keep `~/.tsk` on a local disk. The writer lock is
`flock`-style and every save is a rename-based atomic replace; NFS, Dropbox,
iCloud Drive, and similar synced folders can break both. `TSK_STATE_DIR` is
the escape hatch. Windows is not tested in CI.

The landing page and complete user guide now ship from this repository at
https://gettsk.sh. Current code and release metadata use the MIT license.

## 0.4.0

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
