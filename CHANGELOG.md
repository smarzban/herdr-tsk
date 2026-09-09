# Changelog

## Unreleased

Fixed: a reopen request replaced by another of the same length within one mtime tick
(inode reuse on APFS) was delivered as the first request. The request watch now also
hashes the file bytes.

Fixed: at 110 columns or wider, `Tab` on the quick-add line expanded into a draft page
that was never painted, so the board stayed on screen while keys went to the hidden
draft. The draft now fills the frame at every width.

The board checks GitHub for a newer release at most once a day and paints a dim
`vX.Y.Z available` on the status row. Set `TSK_NO_UPDATE_CHECK` to disable it.

`tsk setup grok` installs the CLI skill into `~/.grok/skills/`. Empty `--skill-dir` is usage. A symlink at the skill folder or `SKILL.md` is refused.

The installer configures PATH for Bash and Zsh, then tells you to reopen the terminal or run an export command.

## 0.6.0

- Native macOS/Linux binaries, a checksum-verifying installer, and Homebrew packaging. `tsk setup herdr` connects the same binary to Herdr with board and capture shortcuts.
- Agent CLI commands to change status, edit tasks, and rename or remove steps.
- **NEEDS YOU** brings blocked and review tasks to the top of the board.
- Redesigned projects index with search, status counts, and persistent desk/project navigation.
- Quick capture opens the task editor in a Herdr popup. Expanded quick-add supports threads and steps.
- Clearer keyboard controls and help: `Ctrl+R` toggles review, `Enter` toggles stored steps, and `Shift+Enter` saves edits. Task deletion asks for confirmation.
- Cleaner task rows: project and thread labels appear only in an open peek, leaving more room for titles.

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
