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
  Amended 2026-09-04 (owner smoke): the header row reads `▾ archived · n` (chevron, word, ` · `,
  count) with no rule; when selected it paints the word bold, never a reverse block; unselected
  it is dim.
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
  project, then before the first keypress a two-choice card paints. Amended 2026-09-04 (owner
  smoke): the card has no title row and no option rows; its body is one line
  `project <name> is archived, would you like to unarchive it?` and its footer reads
  `y unarchive · n keep archived`, with both footer entries clickable.
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

### Owner smoke amendments (2026-09-04)

<!-- source: owner live smoke on /tmp/tsk-b-try · ingested 2026-09-04 -->

- **AC-37** The `P` picker paints a dim `─` rule row directly under its tabs row, like the
  board's rule under its tabs. *(Verification type: **test-backed**, render)*
- **AC-38** The archived group is one of the board's collapsible groups: `ctrl+g` keeps its
  existing meaning (toggle all groups) and, while the drawer is open, folds and unfolds the archived
  group together with the project and thread groups; with the drawer closed it leaves the archived
  group alone. `Enter` and click on the header toggle it on its own. The verb bar shows the
  existing `ctrl+g groups` entry as before; no separate archived chord.
  *(Verification type: **test-backed**, integration)*
  Reworded 2026-09-05 (owner): supersedes the earlier "ctrl+g toggles the archived group from any
  selection, opens the drawer when closed, `ctrl+g expand/collapse` in the bar" wording; `ctrl+g`
  is not reassigned.
- **AC-39** No scope dropdown offers an archived project: the task-page scope footer in edit
  mode, the expanded quick-add draft's scope, and the capture surface's scope list. A task that
  already sits in an archived project still shows that scope as its current value.
  *(Verification type: **test-backed**, render)*
- **AC-40** On the picker's archived tab the verb bar reads `ctrl+u unarchive · enter open ·
  esc close`; `ctrl+f` still unarchives. *(Verification type: **test-backed**, render)*
- **AC-41** `Enter` on an archived project in the picker opens that project in read-only focus:
  the chip reads `<name> · archived`, its tasks paint dim, and nothing is persisted by entering.
  *(Verification type: **test-backed**, integration)*
- **AC-42** In read-only focus every mutating verb (`ctrl+s`, `ctrl+d`, `ctrl+o`, `ctrl+b`,
  `ctrl+e`, `ctrl+n`, `ctrl+x`, `ctrl+f`, quick-add `+`, step toggles) refuses with
  `project <name> is archived · ctrl+u unarchive` on the status slot and changes nothing.
  *(Verification type: **test-backed**, integration)*
- **AC-43** In read-only focus `ctrl+u` unarchives the project in place; the focus becomes a
  normal project focus (chip without `· archived`, rows not dim, verbs work).
  *(Verification type: **test-backed**, integration)*
- **AC-44** In read-only focus the task page opens view-only: `ctrl+e`, `ctrl+n` and `Tab` do not
  enter edit mode and paint the same refusal. *(Verification type: **test-backed**, integration)*
- **AC-45** Read-only focus is the only working lens that paints an archived project's tasks;
  leaving it (`Esc`, `P`, `1`/`2`/`3`) hides them again. *(Verification type: **test-backed**, render)*

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
| AC-37, AC-39, AC-40, AC-45 | render |
| AC-38, AC-41, AC-42, AC-43, AC-44 | integration |

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
   Merge amendment (plan stage, 2026-09-04): the project record map is replaced by the disk map on
   every merge, then this process's own not-yet-persisted archive/unarchive intents are re-applied
   (a transient, non-serialised map, cleared with the merge bases). Without it a plain union would
   resurrect a record the process had just removed.
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
| AC-37 | Project picker, Board renderer |
| AC-38 | Key and mouse mapping, Board model and intents |
| AC-39 | Capture scope resolution, Board renderer |
| AC-40 | Project picker, Board renderer |
| AC-41 | Project picker, Board model and intents |
| AC-42 | Board model and intents |
| AC-43 | Board model and intents, Archive domain |
| AC-44 | Board model and intents |
| AC-45 | Lens query, Board model and intents |

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

## Plan

Tasks are dependency-ordered vertical slices. Each leaves `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release` green. Paths are relative to the repo root; "new" marks files that do not exist yet. Every test named under "Failing test first" is written before its code, watched failing, then made green.

**T-1** Archived flag on `Task`, task verbs, history events

Files: `src/domain/task.rs` (change), `src/domain/events.rs` (change), `src/ui/queue.rs` (change, test helper only), `tests/queue_board_render.rs` (change, `task()` helper), `tests/queue_board_model.rs` (change, `task()` helper).

- `Task` gains `pub archived: bool` after `soft_deleted`, `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`. `DomainState::create` sets `archived: false`. Compile fallout: the four `Task { .. }` literals (`src/domain/task.rs` create, `src/ui/queue.rs` test `task_with_thread`, `tests/queue_board_render.rs::task`, `tests/queue_board_model.rs::task`) add `archived: false`.
- `TaskEventKind` gains `Archived` and `Unarchived` (serde `snake_case`, no exhaustive match exists on the kind outside tests).
- `DomainState::archive_task(&mut self, id: Uuid) -> Result<bool, DomainError>` and `unarchive_task(...) -> Result<bool, DomainError>`: `UnknownId` when absent, `SoftDeleted` when soft-deleted; when the flag already has the requested value return `Ok(false)` with no `record_mutation`, no event, no revision change; otherwise set the flag, `record_mutation` with the matching kind, return `Ok(true)`. Status is untouched. Neither verb touches `undo_stack`.
- Failing test first: `archive_task_sets_the_flag_keeps_status_journals_archived_and_pushes_no_undo` in `src/domain/task.rs` tests: archive a `Blocked` task, assert `archived == true`, status still `Blocked`, last event `Archived`, `last_undo()` unchanged from before; then `unarchive_task` clears it with an `Unarchived` event; a second `archive_task` returns `Ok(false)` and adds no event; a soft-deleted task returns `Err(SoftDeleted)`.
- Also: `archive_flag_survives_save_and_load` in `tests/store_persist.rs` (archive, `store.save`, `store.load`, flag and status intact; unarchived task's JSON has no `archived` key).

*Advances:* AC-7, AC-9, AC-34.
*Component:* Archive domain.
*Deps:* none.

**T-2** Store format v2: `projects` map, `STORE_FORMAT_VERSION = 2`, v1 → v2 chain step

Files: `src/domain/task.rs` (change), `src/store.rs` (change), `tests/store_persist.rs` (change), `tests/fixtures/current_store_v2.json` (new), `tests/fixtures/current_store_v1.json` (unchanged, becomes the migration input).

- `src/domain/task.rs`: `pub struct ProjectRecord { pub archived: bool }` (`Serialize, Deserialize, deny_unknown_fields`); `DomainState` gains `#[serde(default)] projects: BTreeMap<String, ProjectRecord>` (always serialized, so an empty map writes `"projects": {}`); `DomainState::new()` initialises it empty; `pub fn projects(&self) -> &BTreeMap<String, ProjectRecord>`. `STORE_FORMAT_VERSION` becomes `2`.
- `src/store.rs`: `fn migrate_v1_to_v2(document: serde_json::Value) -> Result<serde_json::Value, StoreError>` inserts `"projects": {}` when the key is absent; `const MIGRATIONS: &[MigrationStep] = &[migrate_v1_to_v2];` (`migrate_with` stamps the version). Update the module doc line "Empty while v1 is current".
- Test retargeting in `src/store.rs` tests (each currently pins 1): `save_emits_format_version_one` → rename `save_emits_format_version_two`, assert 2; `load_refuses_missing_format_without_rewriting` (`supported: 2`); `load_refuses_noncurrent_format_without_rewriting` loop `[0, 3]`, `supported: 2`; `save_refuses_noncurrent_in_memory_state_without_writing` and `reload_merge_save_refuses_noncurrent_local_state_without_rewriting` use in-memory `format_version: 3` and expect `found: 3, supported: 2`; `save_refuses_newer_format_and_leaves_the_document` expects `supported: 2`; `load_refuses_a_higher_version_and_changes_nothing_in_the_state_dir` uses `[(4u32, 2u32), (4, 3)]` and `supported: 2`; the injected-step seam tests (`migrated_load_backs_up_the_original_before_the_first_higher_version_save`, `save_never_overwrites_an_existing_version_backup`, `failed_migration_step_surfaces_from_load_without_file_changes`) move up one level: seed via `DomainState::new()` (now v2), seam target `3`, injected `identity_v2_to_v3`, backup file `tsk.json.v2`; `v1_document_round_trips_byte_identical_for_an_unchanged_state` → rename `v2_document_round_trips_byte_identical_for_an_unchanged_state`.
- `tests/store_persist.rs`: `literal_current_v1_fixture_pins_the_complete_store_wire_shape` now asserts the load migrated (`format_version() == 2`, `projects().is_empty()`, `task.archived == false`), everything else unchanged; `noncurrent_store_is_refused_without_rewriting_the_file` seeds `format_version: 3` and asserts `"expected 2"`.
- Failing test first: `v1_document_loads_through_the_chain_and_first_save_leaves_tsk_json_v1_beside_the_live_file` in `tests/store_persist.rs`: install `current_store_v1.json` under a temp `TSK_STATE_DIR`, `store.load()` → version 2, no archived task, empty projects; `store.save` → `tsk.json.v1` byte-identical to the fixture, live file `format_version == 2` with `"projects": {}`.
- Also: `literal_current_v2_fixture_round_trips_byte_identical` in `tests/store_persist.rs` against the new `current_store_v2.json` (same task as v1, `"format_version": 2`, `"projects": {}`, no `archived` key): load then save reproduces the fixture bytes.

*Advances:* AC-33, AC-34.
*Component:* Store format v2.
*Deps:* T-1.

**T-3** Project records: archive/unarchive project verbs, hidden predicate, merge rules, name resolution

Files: `src/domain/task.rs` (change), `src/scope.rs` (change), `tests/store_persist.rs` (change).

- `DomainError::UnknownProject(String)` (Display: `unknown project {path}`); `map_domain_error` in `src/cli/steps.rs` and `board_rejection_message` in `src/app.rs` already fall through on `other`, no fallout.
- `DomainState::archive_project(&mut self, path: &str) -> Result<bool, DomainError>`: `UnknownProject` when no task (any status, soft-deleted included) has `TaskScope::Project { path }` and no record exists; `Ok(false)` when already archived; else insert `ProjectRecord { archived: true }`, return `Ok(true)`. `unarchive_project`: remove the record (`Ok(true)`), `Ok(false)` when no record but tasks exist, `UnknownProject` when neither. Neither touches tasks, history, or undo.
- `pub fn is_project_archived(&self, path: &str) -> bool`, `pub fn archived_projects(&self) -> BTreeSet<String>`, `pub fn is_hidden(&self, task: &Task) -> bool` (archived, or scope path archived).
- Merge: transient `#[serde(skip)] project_intents: BTreeMap<String, bool>` recorded by the two verbs (true = archived, false = unarchived), cleared in `clear_merge_bases`. `merge_for_save` and `merge_tasks_from_disk`: replace `self.projects` with the disk map, then re-apply `project_intents` (insert or remove). Last writer wins for the map, this process's own intent survives the locked merge.
- `src/scope.rs::resolve_project_path`: add `domain.projects().keys()` to the basename candidates.
- Failing test first: `archive_project_writes_one_record_and_unarchive_removes_it` in `src/domain/task.rs` tests: create a task in `/repos/a`; `archive_project("/repos/a")` → `projects()` has exactly that key with `archived: true`, `is_hidden(task)` true; `unarchive_project` → map empty; `archive_project("/nowhere")` → `Err(UnknownProject)`.
- Also: `task_and_project_flags_are_independent` (archive task T in P, archive P, unarchive P: T still archived, P's other task not) in `src/domain/task.rs`; `reload_merge_save_keeps_a_sibling_writers_project_record_and_applies_the_local_intent` in `tests/store_persist.rs` (writer B archives Q on disk; writer A with a stale map archives P through `reload_merge_save` → both records on disk; A then unarchives P through `reload_merge_save` → only Q remains).

*Advances:* AC-20, AC-21.
*Component:* Archive domain.
*Deps:* T-2.

**T-4** Lens: hidden tasks and archived projects leave every working lens

Files: `src/ui/queue.rs` (change), `src/ui/board/model.rs` (change), `tests/queue_board_render.rs` (change), `tests/queue_board_loop.rs` (change).

- `src/ui/queue.rs`: `pub fn query_board(tasks: &[Task], archived_projects: &BTreeSet<String>, current_repo: Option<&Path>, lens: BoardLens<'_>, drawer_open: bool) -> QueueView`; `query_lens` keeps its signature and delegates with an empty set (every existing call site stays). Every `live` filter becomes `!soft_deleted && !archived && !archived_projects.contains(scope path)`; `open_project_paths`, thread groups, deck thread blocks, `append_done`, `status_counts` all derive from `live`, so thread headers and counts exclude archived tasks and archived projects vanish from the projects and threads tabs.
- `src/ui/board/model.rs`: `BoardModel` gains `pub(super) archived_projects: BTreeSet<String>` (empty in `from_tasks`; set from `state.archived_projects()` in `from_domain` and `sync_from_domain`); `queue_view()` calls `query_board`; `project_options()` skips archived paths (including `this_repo`); `ensure_home_tab_has_visible_tasks` and `reveal_task_on_home` ignore hidden tasks (helper `fn is_hidden(&self, task: &Task) -> bool`).
- Failing test first: `archived_task_paints_in_no_working_lens_in_any_status_at_any_tier` in `tests/queue_board_render.rs`: for each status and each lens (desk, projects, threads, project focus) render at 80×24, 40×10, and the 130×24 stage G rail, assert the archived title is absent from every row and `assert_buffer_mono` passes.
- Also: `thread_header_and_open_count_exclude_an_archived_task` in `src/ui/queue.rs` tests (thread with one open task archived: no `ThreadBlock`, threads tab no section); `archived_project_hides_its_tasks_from_projects_threads_and_desk_in_motion` in `src/ui/queue.rs` tests; `idle_merge_hides_a_task_archived_by_another_process_without_moving_selection` in `tests/queue_board_loop.rs` (two `TaskStore`s on one temp dir, process B archives task X via `store.locked_transition`, process A with selection on Y runs `revalidate_board_from_store`, X gone from `visible_ids`, selection still Y).

*Advances:* AC-5, AC-6, AC-9, AC-19.
*Component:* Lens query.
*Deps:* T-3.

**T-5** Done drawer ARCHIVED group: collapsible selectable header, dim rows, mouse hits

Files: `src/ui/queue.rs` (change), `src/ui/board/model.rs` (change), `src/ui/board/apply.rs` (change), `src/ui/board/draw.rs` (change), `src/ui/input.rs` (change), `src/ui/mouse.rs` (change), `src/ui/render.rs` (change), `src/app.rs` (change), `tests/queue_board_render.rs` (change), `tests/queue_board_verbs.rs` (change), `tests/queue_board_mouse.rs` (change), `tests/fixtures/queue_board/done_drawer_archived.txt` (new, generated).

- `src/ui/queue.rs`: `SectionKind::Archived`; `pub const ARCHIVED_HEADER_ROW_ID: Uuid` (fixed `Uuid::from_u128` constant); `fn append_archived(sections, in_scope: &[&Task], drawer_open)` after `append_done`, listing `archived && !soft_deleted && project not archived` tasks in the drawer's scope (home lenses: all scopes; project focus: that project), sorted updated desc, pushed only when non-empty, `count = n`. `visible_task_ids(..., archived_collapsed: bool)`: for an `Archived` section push `ARCHIVED_HEADER_ROW_ID`, then its `task_ids` unless collapsed; call sites `src/ui/board/model.rs::visible_ids` and the three `visible_task_ids(` calls in `src/ui/queue.rs` tests.
- `src/ui/board/model.rs`: `archived_collapsed: bool` (init `true`, session-only), `toggle_archived_collapsed()`, `pub fn archived_header_selected(&self) -> bool`, `selected_id()` returns `None` when `selection_id == Some(ARCHIVED_HEADER_ROW_ID)` (so `NO_SELECTION` refusals cover every verb on the header); `seed_selection` unchanged.
- `src/ui/input.rs`: `BoardIntent::ToggleArchivedGroup`; arm in `intent_primary_action` (`None`); `src/app.rs::apply_board_intent_with_save_recovery` navigation allowlist gains it.
- `src/ui/board/apply.rs`: `OpenTaskPage` with the header selected → toggle collapse, `reanchor_selection`, return `None`; `ToggleArchivedGroup` (mouse) → select the header row (`retarget_selection`), toggle, reanchor.
- `src/ui/render.rs`: `ListRow::ArchivedHeader { line, selected }` (sticky); `build_list_rows` paints `paint_collapsible_header` with chevron, title `archived`, count, all spans `style_dim()` (selected row `style_reverse()`), rows skipped when collapsed, task rows painted with `TaskRowPaint { dim: true, .. }` (new field: every span dim, status glyph and `T<n>` kept); rail skips `Archived` like `Done`; `section_title` arm `archived`; `paint_list_row` pushes `QueueHitTarget::ArchivedHeader`; `QueueFrameModel` gains `archived_collapsed: bool` and `archived_header_selected: bool` (construct sites: `src/ui/board/draw.rs` ×5, `tests/queue_board_render.rs` ×3).
- `src/ui/mouse.rs`: `Normal` arm `ArchivedHeader → ToggleArchivedGroup`. `src/ui/board/draw.rs::board_verb_items`: header selected → `enter expand` / `enter collapse`, then `:` `?` `+`.
- Failing test first: `archived_group_paints_below_done_with_its_count_and_no_header_when_empty` in `tests/queue_board_render.rs`: drawer open with two archived tasks → a row reading `archived` with `2` after the DONE rows, no archived titles painted (collapsed); with zero archived → no such row.
- Also (same task): golden scene `done_drawer_archived` added to `golden_scenes()` and regenerated with `cargo test --test queue_board_render regenerate_golden_fixtures -- --ignored` (existing goldens unchanged; verify with git diff); `archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` in `tests/queue_board_verbs.rs` + `tests/queue_board_mouse.rs` (`board_hit_map` → `ArchivedHeader` hit → `map_board_mouse` → toggle; `BoardModel::from_domain` again is collapsed); `expanded_archived_rows_are_dim_keep_glyph_and_identifier_and_are_selectable_and_hit_testable` in `tests/queue_board_render.rs` (check `Modifier::DIM` on every cell of the row, `T<n>` and glyph present, `Task(id)` hit exists, `SelectNext` lands on it, `assert_buffer_mono`); `archived_group_follows_the_drawer_scope` in `src/ui/queue.rs` tests (home lists both scopes; project focus lists only that project's).

*Advances:* AC-10, AC-11, AC-12, AC-13, AC-14, AC-15.
*Component:* Board model and intents.
*Deps:* T-4.

**T-6** `ctrl+f` File verb, `ctrl+u` on an archived selection, verb bar and help card

Files: `src/ui/input.rs` (change), `src/ui/board/apply.rs` (change), `src/ui/board/draw.rs` (change), `src/ui/mouse.rs` (change), `tests/v1_keymap_guard.rs` (change), `tests/queue_board_verbs.rs` (change), `tests/fixtures/queue_board/help.txt` (regenerated).

- `src/ui/input.rs`: `BoardIntent::File`; `NORMAL_KEYMAP` entry `{ code: Char('f'), intent: File, help_chord: "f", help_label: "file", verb: true }` after the `u` entry; `help_chord_shown` `MUTATING` adds `"f"`; `map_task_page`: `Char('f') if verb → File`; `map_project_picker`: check ctrl chords first, `ctrl+f → File`, `ctrl+u → Undo`, other modified keys still `None`; `intent_primary_action` arm (`None`); `primary_action_sample_key` untouched.
- `src/ui/board/apply.rs`: `board_intent_may_persist` adds `File`; `File` arm: picker open → `Ok(None)` (filled by T-8); else `selected_id()` or `NO_SELECTION`; toggle via `archive_task` / `unarchive_task`; no message on success; `sync_from_domain` reanchors (existing rule keeps the pin off hidden rows). `Undo` arm: when the selected task is archived → `unarchive_task`, never `domain.undo()`; picker open → `Ok(None)` (T-8 refines); otherwise unchanged.
- `src/ui/board/draw.rs`: `board_verb_items` and `task_page_verb_items` add `f archive` (unarchived task) or `f unarchive` (archived task). `src/ui/mouse.rs::verb_intent`: `"f" => File`.
- `tests/v1_keymap_guard.rs::normal_mode_keymap_equals_the_readme_and_queue_board_v1_set`: add `(KeyCode::Char('f'), BoardIntent::File)` after `u`, add `Char('f')` to `mutating`, drop `'f'` from the retired list. Help golden regenerated (`help.txt` only; diff it).
- No palette entry (the palette catalog is pinned by `palette_lists_exactly_m1_commands_for_selection_filters_by_subsequence_and_dispatches_same_intents_as_keys` and no criterion asks for one).
- Failing test first: `ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo` in `tests/queue_board_verbs.rs`: `ctrl+f` on a `Review` task → archived, status `Review`, `domain.last_undo()` unchanged; select task B, `ctrl+u` → A still archived (AC-7).
- Also: `ctrl_f_in_the_archived_group_unarchives_and_the_row_returns_to_the_deck` (drawer open, expand, select the row, `ctrl+f`); `ctrl_u_on_an_archived_selection_unarchives_without_popping_the_undo_stack` (undo stack length equal before and after, no `StaleUndo`); existing `u_undoes_with_domain_coverage_and_stale_undo_refused_visibly` stays untouched and green (AC-4); `help_card_lists_ctrl_f_and_the_verb_bar_shows_file_for_a_task_row_and_the_group` (extend `help_card_lists_every_active_tier_binding_and_closes_on_any_key`, plus `board_verb_items` assertions for a deck row and an expanded archived row).

*Advances:* AC-1, AC-2, AC-3, AC-4, AC-7, AC-35.
*Component:* Key and mouse mapping.
*Deps:* T-5.

**T-7** Task page header slot reads `archived`

Files: `src/ui/board/draw.rs` (change), `tests/queue_board_render.rs` (change), `tests/queue_board_edit.rs` (change).

- `task_header_state` (wide column header) and `build_task_page_overlay` (`status_word`) use `archived` in place of the status word when `task.archived`, so the slot reads `archived · <project>` and the single-pane header's right-aligned word is `archived`. Nothing else on the page changes; edit entry and save paths are untouched.
- Failing test first: `task_page_header_slot_reads_archived_for_an_archived_task` in `tests/queue_board_render.rs`: open the page on an archived task at 80×24 and at 130×24 stage F, assert the header row contains `archived` and not the status word; an unarchived task still shows its status.
- Also: `title_edit_on_an_archived_task_persists_and_keeps_the_flag` in `tests/queue_board_edit.rs` (ctrl+e, type, `ConfirmEdit`, `store.reload_merge_save`, reload: title changed, `archived == true`, status unchanged).

*Advances:* AC-8.
*Component:* Board renderer.
*Deps:* T-6.

**T-8** Project picker: main and archived tabs, archive and unarchive in place

Files: `src/ui/board/model.rs` (change), `src/ui/board/apply.rs` (change), `src/ui/board/draw.rs` (change), `src/ui/input.rs` (change), `src/ui/mouse.rs` (change), `src/ui/render.rs` (change), `tests/queue_board_verbs.rs` (change), `tests/queue_board_mouse.rs` (change), `tests/queue_board_render.rs` (change).

- `src/ui/board/model.rs`: `pub enum PickerTab { Main, Archived }`; `ProjectPickerState` gains `tab: PickerTab`, `archived: Vec<PathBuf>`, `archived_selected: usize`; `pub fn archived_project_options(&self) -> Vec<PathBuf>` (sorted from `archived_projects`); `move_project_picker` moves within the active tab; `pub fn picker_tab(&self) -> Option<PickerTab>`; `project_picker_index` returns the active tab's index.
- `src/ui/input.rs`: `BoardIntent::ProjectPickerSwitchTab` (picker `Tab`, `←`, `→`) and `BoardIntent::SelectPickerTab(PickerTab)` (mouse); arms in `intent_primary_action`.
- `src/ui/board/apply.rs`: `File` with picker open: Main + `Project(path)` → `archive_project`, rebuild `options`/`archived`, clamp selection, keep the picker open; if `board_location == Project(path)` set `board_location = Home { Desk }` and `reanchor_selection`; Main + `Home` → `Ok(None)`; Archived + entry → `unarchive_project`, rebuild, keep open. `Undo` with picker open: Archived tab → unarchive (same path); Main tab → `Ok(None)`. Both mutations return `Persist`. `ConfirmProjectChoice` / `SelectProjectOption` on the Archived tab are inert.
- `src/ui/render.rs`: `QueueOverlay::ScopeDropdown` gains `tabs: Option<PickerTabsPaint { archived_active: bool, archived_count: usize }>`; `paint_scope_dropdown` paints a first content row `projects · archived (n)` (active bold, inactive dim) with `QueueHitTarget::PickerTab(PickerTab)` hits, then the active list; an empty archived list paints one dim row `no archived projects`; the `options.is_empty()` early return no longer applies to the picker. `SCOPE_VERBS` adds `f file`. `OverlayPayloads::collect` in `src/ui/board/draw.rs` supplies the active tab's labels. `src/ui/mouse.rs`: `PickerTab(tab) → SelectPickerTab(tab)`, `scope_dropdown_verb_intent` `"f" → File`.
- Failing test first: `ctrl_f_in_the_picker_archives_the_selected_project_and_keeps_the_picker_open` in `tests/queue_board_verbs.rs`: open `P`, move to a project, `ctrl+f` → `domain.is_project_archived` true, `model.input_mode() == ProjectPicker`, main list no longer contains it, `archived_project_options()` does.
- Also: `archived_tab_lists_exactly_the_archived_projects_and_paints_an_empty_state_line` in `tests/queue_board_render.rs` (mono); `ctrl_f_and_ctrl_u_on_the_archived_tab_unarchive_and_every_task_keeps_its_status` in `tests/queue_board_verbs.rs` (record statuses before, compare after; project back on the projects tab); `archived_project_paints_nowhere_on_home_tabs_or_the_picker_main_list` in `tests/queue_board_render.rs` (desk IN MOTION with a started task in the archived project, projects tab, threads tab, picker main list); picker tab click in `tests/queue_board_mouse.rs::the_modal_cards_close_control_and_chrome_behave_the_same_on_palette_help_and_project_picker` extended with a `PickerTab` hit.

*Advances:* AC-16, AC-17, AC-18, AC-19.
*Component:* Project picker.
*Deps:* T-6.

**T-9** Launch card inside an archived project's directory

Files: `src/ui/board/model.rs` (change), `src/ui/mouse.rs` (change), `src/ui/input.rs` (change), `src/ui/board/apply.rs` (change), `src/ui/board/draw.rs` (change), `src/ui/board/chrome.rs` (change), `src/ui/render.rs` (change), `src/app.rs` (change), `tests/archive_launch_card.rs` (new).

- `src/ui/mouse.rs`: `BoardPopup::LaunchCard`. `src/ui/board/model.rs`: `BoardInputMode::LaunchCard`; fields `launch_card: Option<PathBuf>`, `launch_card_shown: bool`, `session_default_scope: Option<TaskScope>`; `input_mode()` maps the popup; `help_line()` returns `LAUNCH_CARD_HELP_LINE` (`"y unarchive · n keep archived"`, new const in `src/ui/input.rs`); `quick_add_scope()` falls back to `session_default_scope` before the snapshot default; `pub fn offer_launch_card(&mut self, state: &DomainState, snapshot: &InvocationSnapshot) -> bool` (raises the card when `snapshot.default_scope` is an archived project and `!launch_card_shown`, sets the flag, returns whether it raised). Compile fallout for the new mode: `map_key` and `map_edit_paste` in `src/ui/input.rs`, `map_board_mouse` in `src/ui/mouse.rs`, `edit_chrome_legends` in `src/ui/board/chrome.rs`.
- `src/app.rs::load_board`: call `model.offer_launch_card(&state, &snapshot)` after building the model.
- `src/ui/input.rs`: `BoardIntent::LaunchUnarchive`, `BoardIntent::LaunchKeepArchived`; `fn map_launch_card`: `y` → unarchive, `n`/`Esc` → keep (no `Enter` default, gate F-1), `ctrl+c` → `Quit`, else `None`; `intent_primary_action` arms.
- `src/ui/board/apply.rs`: `board_intent_may_persist` adds `LaunchUnarchive`; `LaunchUnarchive` → `unarchive_project(path)`, close the card, `Persist` (a failed save lands in Save Recovery via the existing boundary and the default stays the project); `LaunchKeepArchived` → close the card, `session_default_scope = Some(TaskScope::Global)`, `set_message(format!("project {name} is archived · quick-add goes to your desk this session"))`.
- `src/ui/render.rs`: `QueueOverlay::LaunchCard { name: &str }` painted with `paint_modal_card` (title `project <name> is archived`, rows `▸ unarchive  y` and `  keep archived  n`, hits `QueueHitTarget::LaunchOption(usize)`, footer `LAUNCH_FOOTER`); arms in `paint_overlay` and the verb-items match near `QueueOverlay::Help`; `OverlayPayloads::modal` in `src/ui/board/draw.rs` returns it while the popup is up. `src/ui/mouse.rs` `LaunchCard` arm: `LaunchOption(0) → LaunchUnarchive`, `LaunchOption(1) → LaunchKeepArchived`, anything else `None`.
- Failing test first: `launch_in_an_archived_project_paints_the_two_choice_card_before_any_key` in `tests/archive_launch_card.rs`: temp store with project `/tmp/x/proj` archived, snapshot `default_scope = Project(that path)`, `BoardModel::from_domain` + `offer_launch_card` → `input_mode() == LaunchCard`, `draw_board` rows contain `project proj is archived`, `unarchive`, `keep archived`, mono.
- Also: `y_or_a_click_unarchives_durably_and_quick_add_defaults_to_the_project` (drive `apply_board_intent_with_save_recovery` with `store.reload_merge_save` as `persist`, reload store: record gone; `OpenCapture` scope is the project); `n_esc_or_click_keep_archived_defaults_quick_add_to_desk_with_a_status_line` (record still present, `OpenCapture` scope `Global`, `message()` names the project); `card_is_offered_once_per_session_whichever_choice` (second `offer_launch_card` returns false, mode `Normal`); `no_card_when_the_default_is_desk_or_an_unarchived_project`.

*Advances:* AC-22, AC-23, AC-24, AC-25, AC-26.
*Component:* Launch card.
*Deps:* T-8.

**T-10** Quick-add `!p name` to an archived project is refused on the open line

Files: `src/ui/board/apply.rs` (change), `tests/quick_add_capture.rs` (change).

- `lift_quick_add_tokens`: after resolving `!p <arg>` to a path, if `domain.is_project_archived(&path)` return `Err(format!("project {} is archived", short_project(&path)))`. Both `QuickAddSave`/`QuickAddSaveNext` and `ExpandQuickAdd` already route the `Err` to `set_message` and leave the line open; `CancelQuickAdd` clears it (existing status-slot rule).
- Failing test first: `p_token_naming_an_archived_project_refuses_on_the_open_line_and_clears_on_close` in `tests/quick_add_capture.rs`: archive `/repos/other`, type `ship it !p other`, `QuickAddSave` → `IntentOutcome::None`, mode still `QuickAdd`, `message()` contains `other` and `archived`, `domain.tasks()` unchanged; `CancelQuickAdd` → message `None`. `!p /repos/other` behaves the same; `!p` bare and an unarchived name still save.

*Advances:* AC-27.
*Component:* Capture scope resolution.
*Deps:* T-3.

**T-11** CLI `tsk archive` / `tsk unarchive` and `tsk list --archived`

Files: `src/cli/router.rs` (change), `src/main.rs` (change), `src/cli/mod.rs` (change), `src/cli/parser.rs` (change), `src/cli/archive.rs` (new), `src/cli/presenter.rs` (change), `src/cli/list.rs` (change), `tests/cli_archive.rs` (new), `tests/cli_list.rs` (change), `tests/cli_router_process.rs` (change).

- `Surface::Archive`, `Surface::Unarchive` for positionals `archive` / `unarchive`; `src/main.rs` exhaustive match routes them to `headless_main`, global help and `usage_exit` list them; `run_with` dispatches `run_archive(args, archive: bool)`; `presenter::usage` wording becomes `expected add, steps, list, trash, archive, unarchive, or project command` (project lands in T-12, the wording is written once here).
- `src/cli/parser.rs`: `FlagArchive { task: Option<TaskAddress>, state_dir, help }`, `parse_flag_archive(args, verb: &str)` (one positional, `--state-dir`, `--help`, same shape as `parse_flag_trash`).
- `src/cli/archive.rs`: `ArchiveResult { number, title, archived: bool }`, `ArchiveCliError { UnknownTask(String), SoftDeleted(String), Store(String) }`, `run_task(target, archive: bool, state_dir)` inside `locked_transition_if_changed` calling `archive_task` / `unarchive_task` (the returned bool is `changed`). Presenter: `archive_help(verb)`, `archive_usage(verb, reason)` exit 2, `archived(result, verb)` prints `archived T7 title` / `unarchived T7 title` exit 0 (repeat prints the same, exit 0), `archive_rejected` exit 1 for unknown or soft-deleted (`T7 is not on the board`, `T7 is deleted`), exit 3 for store.
- `src/cli/list.rs`: `--archived` flag → `ListView::Archived` (usage error with `--done`, `--deleted`, or a task operand); `Open` and `Done` views add `!domain.is_hidden(task)`; `Archived` rows are tasks with `archived || project archived`, not soft-deleted, one row per id, sorted by `status_group_rank`; `ListRow` gains `#[serde(skip_serializing_if = "Option::is_none")] archived: Option<&'static str>` (`"archived"` wins over `"project archived"`), set only in the archived view so existing JSON shapes are unchanged. Presenter: `ListView::Archived => &[(None, "ARCHIVED")]`, human rows end with ` · archived` / ` · project archived`; `list_help` and `list_usage` mention `--archived`.
- Failing test first: `archive_and_unarchive_by_number_exit_0_and_repeat_is_idempotent` in `tests/cli_archive.rs` (helpers copied from `tests/cli_trash.rs`: `temp_state_dir`, `TempDirGuard`, `cli`): add a task, `tsk archive T1` twice (both exit 0, flag true, one `Archived` event), `tsk unarchive T1` twice (flag false).
- Also: `archive_of_an_unknown_number_or_a_soft_deleted_task_exits_1_with_a_message` in `tests/cli_archive.rs`; `default_list_views_exclude_archived_tasks_and_tasks_of_archived_projects` and `list_archived_marks_task_and_project_rows_once_per_id` (human and `--json`) in `tests/cli_list.rs`; extend `top_level_help_names_subcommands_and_their_help` in `tests/cli_router_process.rs` with `archive`.

*Advances:* AC-29, AC-31, AC-32.
*Component:* CLI archive surfaces.
*Deps:* T-3.

**T-12** CLI `tsk project archive|unarchive <name>` and `tsk add` refusal `project-archived`

Files: `src/cli/router.rs` (change), `src/main.rs` (change), `src/cli/mod.rs` (change), `src/cli/parser.rs` (change), `src/cli/archive.rs` (change), `src/cli/presenter.rs` (change), `src/cli/add.rs` (change), `tests/cli_archive.rs` (change), `tests/cli_add.rs` (change).

- `Surface::Project` for positional `project`; `src/main.rs` match and help; `run_with` dispatches `run_project`. `src/cli/parser.rs`: `FlagProject { action: Option<ProjectAction>, state_dir, help }`, `enum ProjectAction { Archive { name: String }, Unarchive { name: String } }`, `parse_flag_project` (two positionals like `parse_flag_trash`). `src/cli/archive.rs::run_project(name, archive: bool, state_dir)`: inside `locked_transition_if_changed`, resolve with `crate::scope::resolve_project_path(&name, domain, Some(&snapshot_from_env()))`, then `archive_project` / `unarchive_project`; `UnknownProject` → `ArchiveCliError::UnknownProject(format!("no project named {name} has tasks"))` exit 1; success prints `archived project <short name>` / `unarchived project <short name>` exit 0, idempotent.
- `src/cli/add.rs`: `AddError::ProjectArchived(String)` with `code()` `"project-archived"`; in `run`, after `resolve_flag_scope`, refuse when `domain.is_project_archived(path)` (covers cwd default and explicit `-p`); `rejected` prints `tsk add: project-archived: project <name> is archived. Use --desk, -p <other project>, or tsk project unarchive <name>` exit 1. Plan items in `run_plan`: `failed` row `code: "project-archived"`, `error: "project is archived: use --desk, -p, or tsk project unarchive"` (static text, `Failed.error` is `&'static str`). `add_help` names the code.
- Failing test first: `project_archive_and_unarchive_resolve_basename_and_path_exit_0_and_are_idempotent` in `tests/cli_archive.rs`: tasks in `/tmp/x/widget`; `tsk project archive widget` and `WIDGET` (case-insensitive), then `/tmp/x/widget` verbatim, each exit 0 and idempotent; store has exactly one record; `unarchive` removes it; `tsk project archive nothing-here` exits 1.
- Also: `add_into_an_archived_project_exits_1_with_project_archived_and_names_the_ways_out` in `tests/cli_add.rs`: seed a record for a temp git repo path, run `tsk add -t x -p <path>` and (with `HERDR_PLUGIN_CONTEXT_JSON` cwd inside that repo, using the file's existing env guard pattern) `tsk add -t x`; both exit 1, stderr contains `project-archived`, `--desk`, `-p`, `tsk project unarchive`; `list --all` shows nothing new; `--desk` still succeeds.

*Advances:* AC-28, AC-30.
*Component:* Capture scope resolution.
*Deps:* T-11.

**T-13** Docs, site, CLI skill, changelog

Files: `site/src/content/docs/docs/keys.md` (change), `site/src/content/docs/docs/board.md` (change), `site/src/content/docs/docs/capture.md` (change), `site/src/content/docs/docs/cli.md` (change), `site/public/board-demo.js` (change), `site/public/llms.txt` (change), `skills/tsk-cli/SKILL.md` (change), `CHANGELOG.md` (change), `README.md` (change, one line beside the trash note), `CONTEXT.md` (unchanged, terms already present).

- `keys.md`: `ctrl+f` file row (task row toggles archived; picker archives; archived tab unarchives; `ctrl+u` on an archived selection unarchives). `board.md`: done drawer `archived · n` group (closed by default, click or Enter toggles, dim rows), picker tabs, launch card and the keep-archived status line, `working lens` wording. `capture.md`: `!p name` refusal for an archived project. `cli.md` and `skills/tsk-cli/SKILL.md`: `tsk archive`, `tsk unarchive`, `tsk project archive|unarchive`, `tsk list --archived`, `project-archived` error code with its hint, exit contract lines. `board-demo.js`: bare `f` verb on a row and the drawer group (bare keys are deliberate in the demo). `CHANGELOG.md` Unreleased: archive paragraph plus the store format 2 note (`tsk.json.v1` backup on first save). Diff each page against `src/ui/input.rs`, `src/ui/board/commands.rs`, and `src/cli/` before calling it done.
- Verification: reviewer-checked against AC-36 (no automated oracle; `cd site && npm test` must stay green as the parse check). Failing test first: not applicable; run `npm test` in `site/` as the explicit check.

*Advances:* AC-36.
*Component:* Docs and site.
*Deps:* T-12.

<!-- source: mid-build amendment (owner live smoke on the T-13 build, five UX changes plus read-only archived focus) · ingested 2026-09-04 -->

**T-14** Archived header paint, `ctrl+g` toggle, picker rule

Files: `src/ui/render.rs` (change: `paint_archived_header`, `paint_scope_dropdown`), `src/ui/input.rs` (change: `BoardIntent::ToggleArchivedGroup` mapped to `ctrl+g` in Normal mode, help card entry), `src/ui/board/apply.rs` (change: `ToggleArchivedGroup` from keyboard opens the drawer when closed), `src/ui/board/draw.rs` (change: verb bar `ctrl+g expand`/`collapse`), `tests/queue_board_render.rs`, `tests/queue_board_verbs.rs`, `tests/fixtures/queue_board/done_drawer_archived.txt` (regenerated), `tests/fixtures/queue_board/help.txt` (regenerated), `tests/v1_keymap_guard.rs` (change: `g` added).

- Header: `{chevron} archived · {n}`, no rule; selected → word bold, rest dim; unselected → all dim.
- Failing test first: `archived_header_reads_chevron_word_dot_count_and_selection_is_bold_not_reverse` in `tests/queue_board_render.rs` (asserts the row text and that no cell of the row carries `Modifier::REVERSED` when selected; the word carries `BOLD`).
- Also: `ctrl_g_toggles_the_archived_group_from_any_selection_and_opens_the_drawer_when_closed` in `tests/queue_board_verbs.rs`; `picker_paints_a_dim_rule_under_its_tabs` in `tests/queue_board_render.rs`.

*Advances:* AC-13, AC-37, AC-38.
*Component:* Board renderer.
*Deps:* T-13.

**T-15** Launch card body and footer

Files: `src/ui/render.rs` (change: `QueueOverlay::LaunchCard` paint), `src/ui/mouse.rs` (change: footer hits `LaunchOption`), `tests/archive_launch_card.rs` (change).

- No title row, no option rows; body line `project <name> is archived, would you like to unarchive it?`; footer `y unarchive · n keep archived` with both entries hit-testable.
- Failing test first: `launch_card_is_one_message_line_with_choices_in_the_footer` in `tests/archive_launch_card.rs` (asserts absence of the old title and option rows, presence of the message, footer text, and that clicking each footer entry dispatches the right intent).

*Advances:* AC-22.
*Component:* Launch card.
*Deps:* T-13.

**T-16** Scope dropdowns hide archived projects

Files: `src/ui/board/model.rs` or `src/ui/board/apply.rs` (change: wherever `FormScopeDropdown` options are built), `src/ui/capture.rs` (change: capture scope options), `tests/queue_board_edit.rs`, `tests/quick_add_capture.rs`, `tests/capture_*.rs` (whichever covers the capture scope list; verify the file).

- Failing test first: `task_page_scope_dropdown_omits_archived_projects_but_keeps_the_current_scope` in `tests/queue_board_edit.rs`.
- Also: `expanded_quick_add_scope_omits_archived_projects` in `tests/quick_add_capture.rs`; the capture surface equivalent in its test file.

*Advances:* AC-39.
*Component:* Capture scope resolution.
*Deps:* T-13.

**T-17** Read-only archived project focus from the picker

Files: `src/ui/board/model.rs` (change: `BoardLocation::Project` gains a read-only flag or a sibling `ArchivedProject(PathBuf)` variant, chip label, dim rows), `src/ui/board/apply.rs` (change: `ConfirmProjectChoice` on the archived tab opens the focus; every mutating arm checks `model.focus_is_archived()` first and refuses; `Undo` in that focus unarchives the project; task-page edit entry refuses), `src/ui/queue.rs` (change: a lens for the archived project paints its tasks), `src/ui/board/draw.rs` (change: archived-tab verb bar `ctrl+u unarchive · enter open · esc close`, focus verb bar), `src/ui/render.rs` (change: chip `<name> · archived`, dim rows), `src/ui/board/chrome.rs` as needed, `tests/queue_board_verbs.rs`, `tests/queue_board_render.rs`, `tests/queue_board_edit.rs`.

- Failing test first: `enter_on_the_archived_tab_opens_a_read_only_focus_that_persists_nothing` in `tests/queue_board_verbs.rs` (chip text, dim rows, store bytes unchanged).
- Also: `every_mutating_verb_in_read_only_focus_refuses_with_the_archived_message` (table-driven over the chords in AC-42, plus `+`), `ctrl_u_in_read_only_focus_unarchives_in_place`, `task_page_in_read_only_focus_refuses_edit_mode` (`tests/queue_board_edit.rs`), `leaving_read_only_focus_hides_the_archived_projects_tasks_again`, `archived_tab_verb_bar_advertises_ctrl_u_enter_esc` (`tests/queue_board_render.rs`).
- Docs: `site/src/content/docs/docs/{keys,board}.md` and `CHANGELOG.md` gain the `ctrl+g` chord, the read-only focus, and the new card wording (folded here as this is the last task).

*Advances:* AC-40, AC-41, AC-42, AC-43, AC-44, AC-45, AC-36.
*Component:* Project picker.
*Deps:* T-14, T-16.

### Task-to-criterion coverage map

| AC | Advanced by |
| --- | --- |
| AC-1 | T-6 |
| AC-2 | T-6 |
| AC-3 | T-6 |
| AC-4 | T-6 |
| AC-5 | T-4 |
| AC-6 | T-4 |
| AC-7 | T-1, T-6 |
| AC-8 | T-7 |
| AC-9 | T-1, T-4 |
| AC-10 | T-5 |
| AC-11 | T-5 |
| AC-12 | T-5 |
| AC-13 | T-5, T-14 |
| AC-14 | T-5 |
| AC-15 | T-5 |
| AC-16 | T-8 |
| AC-17 | T-8 |
| AC-18 | T-8 |
| AC-19 | T-4, T-8 |
| AC-20 | T-3 |
| AC-21 | T-3 |
| AC-22 | T-9, T-15 |
| AC-23 | T-9 |
| AC-24 | T-9 |
| AC-25 | T-9 |
| AC-26 | T-9 |
| AC-27 | T-10 |
| AC-28 | T-12 |
| AC-29 | T-11 |
| AC-30 | T-12 |
| AC-31 | T-11 |
| AC-32 | T-11 |
| AC-33 | T-2 |
| AC-34 | T-1, T-2 |
| AC-35 | T-6 |
| AC-36 | T-13, T-17 |
| AC-37 | T-14 |
| AC-38 | T-14 |
| AC-39 | T-16 |
| AC-40 | T-17 |
| AC-41 | T-17 |
| AC-42 | T-17 |
| AC-43 | T-17 |
| AC-44 | T-17 |
| AC-45 | T-17 |

### Notes

- Sequencing: T-1 and T-2 must land in the same PR, T-2 immediately after T-1. T-1 changes the on-disk task shape (`archived`) without bumping the version; a v1 binary refuses such a task under `deny_unknown_fields`, so the bump in T-2 is what makes the shape change safe. They are two tasks only because the checker needs one component per task (Archive domain vs Store format v2); they are one review unit.
- `STORE_FORMAT_VERSION` goes to 2 in T-2 (the store task) with a real `v1 → v2` chain step in `MIGRATIONS` (`src/store.rs`); `migrate_with` and the chain scaffolding exist from spec A and are not rewritten. The injected-step seam tests move up one level (v2 seed, seam 3, `tsk.json.v2`) so they keep testing the seam rather than the shipped step.
- The owner's `~/.tsk` is never touched. Every test and the live smoke use `TSK_STATE_DIR` under `/tmp` (per-binary atomic counter plus nanos for temp dir names, as the existing helpers do). Project paths in tests are temp dirs or literal `/repos/...` strings, never the owner's repos.
- Regression tests are watched failing first: write the test, run it red, add the code, run it green; for the two "unchanged behaviour" criteria (AC-4 existing undo tests, AC-26 no card) the check is that the pre-existing tests stay green and the new negative test passes on the first run without product changes beyond the task.
- Pairs merged from the suggested slicing: CLI task verbs and "default list excludes hidden" became one task (T-11) because both are verified by the same `run_with` e2e file and `list --archived` is meaningless without the exclusion; ctrl+f intent, ctrl+u re-route, verb bar and help card stay one task (T-6) because a chord with no reducer arm is not a useful state; the drawer group and the ARCHIVED `SectionKind` were kept together in T-5, so T-4 (lens) introduces no new section kind and needs no `section_title` arm. Split against the suggestion: the domain slice into T-1 and T-2, see the first note.
- Selectable header: `visible_ids` carries `queue::ARCHIVED_HEADER_ROW_ID` as a row; `selected_id()` hides it, so every task verb on the header refuses with `select a task first`, and `seed_selection` (IN MOTION, then ON DECK) never lands there. `reanchor_selection` treats it like any id.
- Project record merge: the transient `project_intents` map is the only way a single process's unarchive survives `merge_for_save` (a plain union would resurrect the record it just removed). It is `#[serde(skip)]` and cleared in `clear_merge_bases`.
- Picker `ctrl+u`: Archived tab unarchives; Main tab is inert (it was unmapped before this feature, so "undo exactly as before" holds).
- No palette command for archive: `palette_lists_exactly_m1_commands_for_selection_filters_by_subsequence_and_dispatches_same_intents_as_keys` pins the catalog and no criterion asks for one.
- Goldens: T-5 adds `done_drawer_archived.txt`; T-6 changes `help.txt`. Both regenerate with `cargo test --test queue_board_render regenerate_golden_fixtures -- --ignored`; diff the output, never hand-edit. All other goldens must be byte-identical after every task.
- `HANDOFF.md` is out of scope for this plan.
- Live Herdr smoke is required at the end (after T-13), on an isolated store: `cargo build --release`, then `TSK_STATE_DIR=/tmp/tsk-archive-smoke` with a temp git repo as cwd. Flow: seed tasks with `tsk add` (one in the temp repo project, one desk); open the board; `ctrl+f` on a task row (row disappears, no undo entry); `z` opens the drawer, `archived · 1` header below DONE, collapsed; `Enter` on the header expands, rows dim with glyph and `T<n>`; select the row, `ctrl+u` unarchives (row returns to the deck); `P`, select the temp repo project, `ctrl+f` archives it (picker stays open, project gone from main); `Tab` to the archived tab, see it listed, `Esc`; projects tab shows no group; quit; relaunch `tsk` from inside that repo directory, see the card `project <name> is archived`, press `n`, see the dim desk status line, `+` shows a desk-scoped draft; type `x !p <name>` and `Enter`, see the refusal naming the project, `Esc`; quit; CLI: `tsk list` (excludes), `tsk list --archived` (marks `project archived`), `tsk add -t y` from inside the repo (exit 1, `project-archived` hint), `tsk project unarchive <name>`, `tsk archive T1`, `tsk list --archived` (marks `archived`), `tsk unarchive T1`. Read the pane after each step; fix anything that only fails live. If `HERDR_ENV` is unset, say so in the build report instead.
