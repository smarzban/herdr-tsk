# Archive

Spec B. Depends on spec A (`docs/specs/store-hardening/`) for the migration chain; this is its
first real step (format v1 → v2).

## Brief

### Problem / intent

Done and finished-with work has no way off the board short of deleting it. The done drawer grows
without bound, and a project that is over still occupies the projects tab and the `P` picker.
The owner wants to put tasks and whole projects out of sight while keeping them, with an
explicit way back, and without the trash semantics of deletion (nothing here expires).

### Scope

- **Archived task**: a flag on a task. Any human status can be archived; the status is kept and
  restored on unarchive. Archived tasks leave every working lens (desk, projects, threads, IN
  MOTION, ON DECK, thread headers and counts) and appear only in the done drawer's archived group.
- **Archived project**: a flag on a project record. An archived project leaves the projects tab,
  the threads tab, desk IN MOTION and the picker's main list; it appears only on the picker's
  archived tab, which is also where it is unarchived. Archiving a project does not touch its
  tasks' own flags; unarchive returns the project exactly as left.
- **Drawer archived group**: inside the existing `z` done drawer, below done, a collapsible
  `archived · n` header, closed by default, single-click or Enter toggles, session-only collapse
  state, rows dimmed. Follows the drawer's scope. Lists individually archived tasks only; archived
  projects live in the picker tab.
- **Picker archived tab**: a second tab in the `P` picker listing archived projects; `ctrl+f` on
  one unarchives it.
- **One verb, `ctrl+f` ("file")**: on a task row (board or drawer) toggles the task's archived
  flag; in the picker on a project archives it, on the archived tab unarchives it. No undo entry;
  the second press is the way back.
- **`ctrl+u` on an archived selection unarchives it**: an archived task row in the drawer group,
  or a project on the picker's archived tab. On any other selection `ctrl+u` stays undo. The
  selection decides, not the undo stack; this is the one chord with two meanings, accepted
  because archived rows exist only in those two places.
- **Launch inside an archived project's directory**: once per session a two-choice modal card
  (`project <name> is archived` · `unarchive` / `keep archived`, `y` / `n` / Esc, clickable like
  Save Recovery). Unarchive flips the record under the store lock and the board proceeds with
  that project as the quick-add default. Keep archived falls back to desk as the quick-add default
  for the session with a dim one-line status message; no second ask that session.
- **Quick-add `!p name` to an archived project**: refused while the line is open, message names
  the project.
- **Task page**: an archived task opens normally; the header slot reads `archived`; edits allowed.
- **CLI, full parity**: `tsk archive T<n>`, `tsk unarchive T<n>`, `tsk project archive <name>`,
  `tsk project unarchive <name>` (name resolves as `!p name` does: basename case-insensitive, or a
  `/path` verbatim), `tsk list --archived` (individually archived tasks plus tasks whose project is
  archived, marked). Default `tsk list` views exclude both. `tsk add` inside an archived project's
  directory with no explicit scope refuses with error code `project-archived` and a hint naming
  `--desk`, `-p`, and `tsk project unarchive`.
- **Store**: `archived: bool` on `Task` (default false, skipped when false); a lazy `projects` map
  on `DomainState` keyed by scope path whose only field for now is `archived: true`; a record
  exists only while its project is archived. `STORE_FORMAT_VERSION` becomes 2 with a v1 → v2 step
  that adds the empty map. Existing v1 files migrate on load per spec A; the owner's files are
  regenerated anyway.
- Keymap, help card, `site/` docs (`keys.md`, `board.md`, `cli.md`, `public/board-demo.js`),
  `skills/tsk-cli/SKILL.md`, `CHANGELOG.md`.

### Non-goals

- Archiving done tasks automatically, by age or count. Owner decision: done stays live and
  visible.
- A separate archive file. The board paints archived rows, so a file buys nothing (see spec A's
  reasoning for trash).
- An `archived` human status.
- Project rename or move (spec A's #3 was dropped from A; the project record added here is its
  future home).
- A per-project "don't ask again" for the launch card. Revisit if the herdr plugin pane makes it
  annoying.
- Thread archival. Threads are not entities.

### Chosen approach

Archive is a **flag, not a place**. Alternatives considered:

1. Third file (`archive.jsonl`) beside trash. Rejected: the board reads archived rows for the
   drawer and picker, so every start and idle poll would load and merge it anyway, doubling the
   lock, merge and fsync surface for no size win.
2. `HumanStatus::Archived`. Rejected: conflates the state of the work with whether it is on the
   radar, and unarchive would lose the real status.
3. Two dedicated chords (archive, unarchive). Rejected: one context-sensitive chord is fewer
   keys to learn and matches the toggle nature of the flag. `ctrl+u` is additionally allowed as
   unarchive when the selection is archived, because "undo the archive" is what a user reaching
   for undo on that row means.

### Resolved key decisions

| Decision | Answer |
| --- | --- |
| Verb | `ctrl+f`, context decides archive vs unarchive; free and terminal-safe (`ctrl+a` is add step) |
| Which tasks can be archived | any status; status kept and restored |
| Where archived tasks show | done drawer, collapsible `archived · n` group, closed by default, dimmed |
| Where archived projects show | `P` picker archived tab only |
| Project archive and its tasks | independent flags; project unarchive restores as left |
| Launch in archived project dir | once-per-session two-choice card; keep → desk default + status line |
| `!p` to archived project | refused with message |
| Undo | no undo entry; `ctrl+f` again or `ctrl+u` on the archived selection unarchives |
| Task page on archived task | normal, header slot `archived`, editable |
| CLI | full parity incl. `tsk project archive/unarchive`, `tsk list --archived`, `project-archived` error code |
| Store shape | `archived` flag on task; lazy `projects` map keyed by path; format v2 via spec A's chain |
| Glossary | `open task` now excludes archived |

### Glossary terms touched

`archived`, `archived project`, `project record`, `archived group`, `file`, `open task` (amended).
See `CONTEXT.md`.

### ADRs

`docs/specs/adr/0004-lazy-project-record.md`.

## Acceptance Criteria

Verification type for every criterion is declared inline. Test-backed criteria name only the
kind of oracle; concrete test names are the plan's job. "Working lens" means: desk tab, projects
tab, threads tab, project focus, IN MOTION, ON DECK, thread headers and their counts, the rail,
and default `tsk list` views. "Session" means one board process from launch to quit.

### Task archive

- **AC-1** Given an unarchived task selected on the board or in the done drawer, when `ctrl+f` is
  pressed, then the task's archived flag is set and its human status is unchanged.
  *(Verification type: **test-backed**, unit)*
- **AC-2** Given an archived task selected in the drawer's archived group, when `ctrl+f` is
  pressed, then its archived flag is cleared and its human status is unchanged.
  *(Verification type: **test-backed**, unit)*
- **AC-3** Given an archived task selected, when `ctrl+u` is pressed, then its archived flag is
  cleared and the undo stack is not popped. *(Verification type: **test-backed**, unit)*
- **AC-4** Given a non-archived selection, when `ctrl+u` is pressed, then behaviour is undo exactly
  as before this feature (existing undo tests unchanged and green).
  *(Verification type: **test-backed**, unit)*
- **AC-5** Given an archived task, then no working lens paints its row, in every status and at
  every width tier. *(Verification type: **test-backed**, render)*
- **AC-6** Given a thread whose only ready/blocked/review task is archived, then that thread
  paints no header and its open count excludes the archived task.
  *(Verification type: **test-backed**, unit)*
- **AC-7** Archiving pushes no undo entry: after `ctrl+f` on task A, `ctrl+u` with task B selected
  does not change A's archived flag. *(Verification type: **test-backed**, unit)*
- **AC-8** Given an archived task, when its task page opens, then the header status slot reads
  `archived`, and a saved title or notes edit persists exactly as for an unarchived task.
  *(Verification type: **test-backed**, integration)*
- **AC-9** The archived flag survives save and load, and a second process sharing the state dir
  sees the flag on its next idle merge without moving its selection.
  *(Verification type: **test-backed**, integration)*

### Drawer archived group

- **AC-10** Given the done drawer open with n ≥ 1 archived tasks in the drawer's scope, then a
  header row reading `archived · n` paints below the done rows; with n = 0 no header paints.
  *(Verification type: **test-backed**, render)*
- **AC-11** The archived group is collapsed on every board launch: only the header paints until it
  is expanded. *(Verification type: **test-backed**, render)*
- **AC-12** A single click on the header, or `Enter` with the header selected, toggles the group
  between collapsed and expanded; the state is not persisted (a relaunch is collapsed again).
  *(Verification type: **test-backed**, integration)*
- **AC-13** Expanded rows paint dim, keep the task's status glyph and its `T<n>` prefix, and are
  selectable and hit-testable like done rows. *(Verification type: **test-backed**, render)*
- **AC-14** The archived group follows the drawer's scope: at home it lists archived tasks the
  home drawer would list if they were done; in project focus only that project's.
  *(Verification type: **test-backed**, unit)*
- **AC-15** Every frame containing the archived group passes the existing mono assertion (no
  colour anywhere). *(Verification type: **test-backed**, render)*

### Project archive

- **AC-16** Given the `P` picker open on its main list with a project selected, when `ctrl+f` is
  pressed, then that project is archived and disappears from the main list without closing the
  picker. *(Verification type: **test-backed**, integration)*
- **AC-17** The picker has an archived tab listing exactly the archived projects; when it is empty
  the tab paints an empty-state line rather than nothing. *(Verification type: **test-backed**, render)*
- **AC-18** Given the archived tab with a project selected, when `ctrl+f` or `ctrl+u` is pressed,
  then the project is unarchived and returns to the main list and the projects tab with every task
  in the status it had. *(Verification type: **test-backed**, integration)*
- **AC-19** Given an archived project, then none of: projects tab, threads tab, desk IN MOTION,
  picker main list, paints that project or any of its tasks.
  *(Verification type: **test-backed**, render)*
- **AC-20** Task and project flags are independent: archive task T in project P, archive P,
  unarchive P; T is still archived and P's other tasks are not.
  *(Verification type: **test-backed**, unit)*
- **AC-21** A project record exists in the store exactly while the project is archived: archive
  writes one, unarchive removes it, and a store with no archived projects has an empty project
  map. *(Verification type: **test-backed**, unit)*

### Launch inside an archived project

- **AC-22** Given the board launched with a cwd-derived quick-add default resolving to an archived
  project, then before the first keypress a two-choice card paints reading
  `project <name> is archived` with options `unarchive` and `keep archived`.
  *(Verification type: **test-backed**, render)*
- **AC-23** Given the card, when `y` is pressed or `unarchive` is clicked, then the project is
  unarchived durably and the quick-add default for the session is that project.
  *(Verification type: **test-backed**, integration)*
- **AC-24** Given the card, when `n` or `Esc` is pressed or `keep archived` is clicked, then the
  project stays archived, the quick-add default for the session is desk, and a dim one-line status
  message says so. *(Verification type: **test-backed**, integration)*
- **AC-25** The card is shown at most once per session, whichever choice was made.
  *(Verification type: **test-backed**, unit)*
- **AC-26** Given the board launched with a default that is not an archived project, then no
  card paints. *(Verification type: **test-backed**, render)*

### Capture refusals

- **AC-27** Given the quick-add line open, when a title carrying `!p <name>` naming an archived
  project is submitted, then nothing is saved, the line stays open, and a refusal naming the
  project paints on the status slot until the line closes.
  *(Verification type: **test-backed**, integration)*
- **AC-28** `tsk add` whose resolved scope is an archived project, whether from cwd or an explicit
  `-p`/`--project`, exits 1 with error code `project-archived`, persists nothing, and its human
  text names `--desk`, `-p`, and `tsk project unarchive`.
  *(Verification type: **test-backed**, e2e)*

### CLI

- **AC-29** `tsk archive T<n>` sets the flag and `tsk unarchive T<n>` clears it, each exiting 0
  and idempotent on repeat; an unknown number or a soft-deleted task exits 1 with a message.
  *(Verification type: **test-backed**, e2e)*
- **AC-30** `tsk project archive <name>` and `tsk project unarchive <name>` resolve `<name>` by
  the `!p` rules (basename case-insensitive, or a `/path` verbatim), exit 0 and are idempotent; a
  name matching no project that has tasks exits 1. *(Verification type: **test-backed**, e2e)*
- **AC-31** Default `tsk list` views (open, done) exclude individually archived tasks and tasks of
  archived projects. *(Verification type: **test-backed**, e2e)*
- **AC-32** `tsk list --archived` lists individually archived tasks and tasks of archived projects,
  each row marked `archived` or `project archived` respectively, one row per task id.
  *(Verification type: **test-backed**, e2e)*

### Store format

- **AC-33** The store format version is 2; a v1 document loads through the migration chain with
  no archived tasks and an empty project map, and its first save leaves `tsk.json.v1` beside the
  live file. *(Verification type: **test-backed**, unit)*
- **AC-34** A v2 document with no archived tasks and no archived projects round-trips
  byte-identical, and an unarchived task serialises without an archived key.
  *(Verification type: **test-backed**, unit)*

### Chrome and docs

- **AC-35** The `?` help card lists `ctrl+f` with a label, and the verb bar shows it where the
  selection can be archived or unarchived. *(Verification type: **test-backed**, render)*
- **AC-36** `site/src/content/docs/docs/{keys,board,capture,cli}.md`, `public/board-demo.js`,
  `skills/tsk-cli/SKILL.md` and `CHANGELOG.md` describe archive as built: the chord, the drawer
  group, the picker tab, the launch card, the refusals, and the four CLI verbs.
  *(Verification type: **reviewer-checked**, axis Spec Conformance. Pass/fail: does each named
  page state the behaviour the criteria above define, with no stale keymap? Justification: prose
  accuracy is not cheaply automatable; `npm test` only checks the pages parse.)*

### Negative criteria (out of bounds)

- **NC-1** No task is archived automatically, by age, count, or status change.
- **NC-2** No new file in the state directory; archived tasks and project records live in the
  live document.
- **NC-3** `HumanStatus` gains no variant.
- **NC-4** No project rename or move verb, CLI or board.
- **NC-5** No persisted "don't ask again" for the launch card.
- **NC-6** Threads cannot be archived.
- **NC-7** No colour is introduced anywhere.

### Verification map

| AC | Oracle |
| --- | --- |
| AC-1, AC-2, AC-3, AC-4, AC-6, AC-7, AC-14, AC-20, AC-21, AC-25, AC-33, AC-34 | unit |
| AC-5, AC-10, AC-11, AC-13, AC-15, AC-17, AC-19, AC-22, AC-26, AC-35 | render |
| AC-8, AC-9, AC-12, AC-16, AC-18, AC-23, AC-24, AC-27 | integration |
| AC-28, AC-29, AC-30, AC-31, AC-32 | e2e (CLI process) |
| AC-36 | reviewer-checked, Spec Conformance |

### Deferred

- Per-project suppression of the launch card (NC-5): revisit if the herdr plugin pane makes it
  a nag.
- `tsk project list` (which projects exist, which are archived): not asked for; `tsk list
  --archived` covers the task side.

### Glossary terms touched

`working lens` and `session` as defined at the top of this section; mirrored into `CONTEXT.md`.

## Design

Feature level. Fits the existing shape: domain verbs mutate `DomainState`, the store persists it,
the lens query derives sections from tasks, the board model turns intents into domain calls and
re-derives its rows, the renderer paints rows and chrome, the CLI shares domain and store. Nothing
here introduces a new layer; every component below is an existing module gaining one
responsibility, except the two picker/card surfaces which extend the existing popup shape.

### Components

1. **Archive domain** — owns the archived flag on a task, the lazy project record map, and the
   four verbs (`archive task`, `unarchive task`, `archive project`, `unarchive project`) plus the
   derived predicate *hidden* (task archived, or its project archived). Verbs record a history
   event and a new revision, push no undo entry, and are idempotent. Errors: unknown task,
   soft-deleted task, project with no tasks and no record. Inputs are ids or scope paths; output
   is the mutated state. The one place any other component learns whether something is archived.
2. **Store format v2** — bumps the document version to 2 and adds the v1 → v2 chain step (empty
   project map, no archived tasks). Contract: a v1 document loads migrated in memory; the first
   save writes `tsk.json.v1` (spec A's rule); an unarchived task serialises with no archived key;
   an empty project map serialises as an empty map. Errors unchanged from spec A.
3. **Lens query** — derives board sections from tasks and the project record map. Contract: no
   working-lens section contains a hidden task; the project set excludes archived projects;
   thread headers and counts use *open task* (which excludes archived); the done drawer gains an
   ARCHIVED section (header + rows, collapsible) listing archived tasks in the drawer's scope,
   after the DONE rows. Input: tasks, project record map, lens, drawer state, archived-group
   collapse state. Output: `QueueView`. No errors (pure derivation).
4. **Board model and intents** — turns key/mouse intents into domain calls and session state. New
   intent *File* (from `ctrl+f`); *Undo* is re-routed to *unarchive* when the selection is an
   archived task or an archived project in the picker. Owns session-only state: archived-group
   collapse (default collapsed), launch-card shown flag, session quick-add default override
   (desk after "keep archived"). Contract: every mutation goes through the domain then the store's
   merge-save; refusals paint on the status slot and clear per the status-slot rule; selection
   after archive reanchors to a visible row (never rests on a hidden task).
5. **Key and mouse mapping** — maps `ctrl+f` to *File* in Normal, drawer, picker and task-page
   modes; keeps `ctrl+u` mapped to *Undo* (the model decides). Adds hit regions for the archived
   header, archived rows, picker tabs and the launch card's two options. Contract: bare `f` does
   nothing; the help card lists `ctrl+f`.
6. **Board renderer** — paints the ARCHIVED header (`archived · n`, dim) and its rows (dim,
   status glyph and `T<n>` kept), the verb bar entry for *File* when the selection is archivable,
   and the task-page header slot `archived`. Contract: mono only; header rows consume row budget
   like thread headers but, unlike them, the archived header is selectable so `Enter` can toggle
   it.
7. **Project picker** — the existing `P` picker gains two tabs: main (unarchived projects plus
   Home) and archived (archived projects, empty-state line when none). `ctrl+f` on main archives
   the selected project in place; `ctrl+f` or `ctrl+u` on archived unarchives it. Contract: the
   picker never closes on archive/unarchive; Home cannot be archived.
8. **Launch card** — a two-choice modal on the existing popup shape (as Save Recovery): shown at
   most once per session when the invocation default resolves to an archived project. Options
   `unarchive` (`y`, click) and `keep archived` (`n`, `Esc`, click). Contract: while up, it owns
   the status slot and blocks board verbs; `unarchive` is a domain verb plus merge-save; `keep`
   sets the session default to desk and paints the status line.
9. **Capture scope resolution** — resolves `!p` tokens (quick-add) and `-p` / cwd defaults (CLI
   add) to a scope, and now refuses an archived project. Contract: quick-add refusal message
   names the project and follows the open-line refusal rule; CLI refusal is error code
   `project-archived`, exit 1, nothing persisted, hint text naming `--desk`, `-p`, and
   `tsk project unarchive`.
10. **CLI archive surfaces** — `tsk archive T<n>`, `tsk unarchive T<n>`, `tsk project archive
    <name>`, `tsk project unarchive <name>`, `tsk list --archived`. Contract: verbs exit 0 and are
    idempotent, refusals exit 1 with a message, store I/O exits 3 (existing exit contract); default
    list views exclude hidden tasks; `--archived` rows carry `archived` or `project archived`.
    Name resolution reuses component 9's `!p` rules.
11. **Docs and site** — keymap, board, capture and CLI pages, the web demo, the CLI skill and the
    changelog describe the feature as built. Contract: reviewer-checked against AC-36.

### Data flow and key state

- Board verb: key → mapping (5) → intent → model (4) → domain verb (1) → store merge-save (2) →
  `sync_from_domain` → lens (3) → renderer (6). Same path the existing status verbs take.
- Launch: invocation snapshot (cwd) → scope resolution (9) → domain `is project archived` (1) →
  model (4) raises the card (8) → choice → domain verb or session default.
- CLI verb: parser → router → surface (10) → `locked_transition` → domain verb (1) → store (2).
- Key state: the archived flag and project records live in the live document (kind: single
  versioned JSON document, as today). Collapse, card-shown and session default are model state,
  never persisted.

### Trust and failure boundaries

- Untrusted input enters at the CLI parser (numbers, names, paths) and at the quick-add token
  parser; both validate before touching the domain. Paths from `-p /path` are used verbatim as
  today.
- A failed merge-save after an archive verb lands in the existing Save Recovery popup; the
  in-memory flag stays until Retry or Cancel resolves it, like any other verb.
- The launch card's `unarchive` failing to save shows Save Recovery in place of the card; the
  session default is not changed until the save succeeds.
- Two processes: archive verbs are ordinary revision-guarded mutations, so a concurrent edit of
  the same task is rejected by the existing merge rule rather than overwritten. Project records
  are merged by key; last writer wins is acceptable because the record has one boolean.

### Criterion → component map

| AC | Component(s) |
| --- | --- |
| AC-1 | Archive domain, Board model and intents, Key and mouse mapping |
| AC-2 | Archive domain, Board model and intents |
| AC-3 | Board model and intents, Archive domain |
| AC-4 | Board model and intents |
| AC-5 | Lens query |
| AC-6 | Lens query, Archive domain |
| AC-7 | Archive domain |
| AC-8 | Board renderer, Board model and intents |
| AC-9 | Store format v2, Archive domain |
| AC-10 | Lens query, Board renderer |
| AC-11 | Board model and intents, Lens query |
| AC-12 | Board model and intents, Key and mouse mapping |
| AC-13 | Board renderer, Lens query |
| AC-14 | Lens query |
| AC-15 | Board renderer |
| AC-16 | Project picker, Archive domain |
| AC-17 | Project picker, Board renderer |
| AC-18 | Project picker, Archive domain |
| AC-19 | Lens query, Project picker |
| AC-20 | Archive domain |
| AC-21 | Archive domain, Store format v2 |
| AC-22 | Launch card, Capture scope resolution |
| AC-23 | Launch card, Archive domain |
| AC-24 | Launch card, Board model and intents |
| AC-25 | Board model and intents |
| AC-26 | Launch card |
| AC-27 | Capture scope resolution, Board model and intents |
| AC-28 | Capture scope resolution, CLI archive surfaces |
| AC-29 | CLI archive surfaces, Archive domain |
| AC-30 | CLI archive surfaces, Capture scope resolution |
| AC-31 | CLI archive surfaces, Archive domain |
| AC-32 | CLI archive surfaces |
| AC-33 | Store format v2 |
| AC-34 | Store format v2 |
| AC-35 | Board renderer, Key and mouse mapping |
| AC-36 | Docs and site |

### ADRs created

`docs/specs/adr/0004-lazy-project-record.md` (written at the idea stage; covers flag-not-place
and the lazy record). No new ADR: the `ctrl+u` dual meaning is a keymap choice, reversible, and
recorded in the Brief's decisions table.

### Glossary terms touched

`hidden` (a task that is archived or whose project is archived; the predicate every working lens
filters on). Mirrored into `CONTEXT.md`.

## Tech Stack

Feature level. No new dependency. Every component from the design lands on a crate already pinned
in `Cargo.toml` / `Cargo.lock` (versions read from the lockfile on 2026-09-04) or on the standard
library. Adding anything would duplicate an existing capability, so none is justified.

| Component | Product | Version | Claim the component leans on | Status |
| --- | --- | --- | --- | --- |
| Archive domain | Rust std (`BTreeMap`, `Uuid` from `uuid`) | uuid 1.24 | none new | n/a |
| Store format v2 | `serde` + `serde_json` | 1.0 / 1.0.151 | `#[serde(default)]` on a new `BTreeMap` field plus `skip_serializing_if` on a `bool` coexist with `deny_unknown_fields` | `verified-by-probe`: the repo already does exactly this for `Task.number`, `Task.thread`, `Task.steps` (see `src/domain/task.rs`), and spec A's migration tests exercise the chain |
| Lens query, Board model, Key/mouse mapping, Project picker, Launch card, Capture scope resolution | Rust std, existing `ui::*` modules | toolchain 1.96.0 | `ctrl+f` arrives as `KeyCode::Char('f')` with `KeyModifiers::CONTROL` | `verified-by-probe`: identical delivery path to the existing `ctrl+e` / `ctrl+n` / `ctrl+x` chords in `src/ui/input.rs`, covered by their tests |
| Board renderer | `ratatui` + `crossterm` | 0.30.2 / 0.29.0 | dim modifier and selectable header rows | `verified-by-probe`: thread headers and dim peek rows already paint with `Modifier::DIM`; golden fixtures in `tests/queue_board_render.rs` |
| CLI archive surfaces | Rust std (hand-rolled parser in `src/cli/parser.rs`) | toolchain 1.96.0 | none new | n/a |
| Docs and site | Astro / Starlight in `site/` | as pinned in `site/package-lock.json` | none new | n/a |

Green bar (unchanged): `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`; site: `cd site && npm test`.

Unverified: nothing. Rejected: a `clap`-style argument parser for the new CLI verbs would duplicate the existing hand-rolled parser and its exit contract tests.
