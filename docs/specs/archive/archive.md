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
