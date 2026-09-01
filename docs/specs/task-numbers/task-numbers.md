# Task numbers

Feature: a store-global sequential task number so a person can say "implement
task 12" and an agent can resolve that to one task.

## Brief

<!-- source: spawn investigation (fable, sol, kimi) settled with the user on store-global 12, 2026-08-31 · ingested 2026-08-31 -->

**Problem.** Tasks are identified by UUID. A person cannot tell an agent
"implement task 12" and have it resolve. `tsk list` and `tsk steps` already take
that UUID; step short ids already exist inside one task. Neither is speakable as
a handle for the task itself.

**Scope.** Every durably created task receives one store-global sequential
integer, the task number, spoken and typed as `12`. It is unique across desk,
every project, and every thread, including done and soft-deleted tasks. Cwd does
not participate once the number is present: `tsk list 12` finds that task
whether it lives on the desk or in another project. The number is shown on the
board row, peek, task page, and human `tsk list`; JSON `list` / `add` emit it
beside the UUID; `tsk list` and `tsk steps` accept it wherever they accept a
task UUID today. UUID still works. The skills doc tells agents to resolve
"task 12" through `tsk list 12`. Existing tasks, including done and
soft-deleted, receive numbers on upgrade. An older binary must not rewrite the
store and erase numbers.

**Non-goals.** No per-project, per-desk, or per-thread numbering; no
cwd-relative "12 means this repo"; no `proj-12` composite; no UUID short prefix
as the human form; no title slug; no reuse after delete; no `T12` or `#12` as
the board form (`#` is the thread glyph); no create-undo; no SQLite or sidecar
index; no dark-engine revival; UUID remains internal identity; human status
remains the only status source of truth.

**Chosen approach.** One store-wide counter, over (a) per-project integers and
(b) UUID prefixes. The store is one JSON document for one person, so the
numbering unit matches the ambiguity unit. Per-project numbers make "task 12"
mean two tasks the moment desk and a project both hold 12, and then every spoken
reference needs a prefix. UUID prefixes are unspeakable, and the shortest
unambiguous prefix lengthens as the store grows, so a handle recorded last week
can go stale. Recorded as ADR-0004.

**Resolved key decisions.**

- Canonical form is bare digits: spoken "task 12", typed `12`, painted `12`.
  Not `#12` on the board (`#` is the thread glyph, and `12` is a valid thread
  name). Not `T12` on the board (owner settled).
- CLI addressing accepts the bare number anywhere a task UUID is accepted
  today. UUID remains valid. Direct lookup ignores cwd and scope flags, the
  same rule UUID lookup already has.
- Assigned once, when creation is durably accepted. Never reused, never
  changed by edit, status, scope move, thread, complete, or soft-delete.
  Done drawer and deleted view still show it. Restore brings the same number
  back.
- Title-idempotent add that finds an existing task returns that task's number
  and does not consume a new one.
- The number is an address, not an ordering promise. Gaps are valid.
- Internals stay UUID-only: selection, revisions, merge matching, undo,
  history, dispatch-attempt links. The number is a boundary alias.
- Step identity is unchanged: step short ids remain prefixes of step UUIDs
  inside one task.
- Board chrome: the number is extra cells, titles still wrap, nothing
  truncates. Compact 40×10 stays operable.
- Site docs and the web demo follow the board/CLI change (keymap unchanged, so
  `keys.md` is untouched unless a later stage finds a key).

**Glossary terms touched.** task number (added); step short id (wording
corrected so it no longer claims to be how tasks are addressed).

**ADR.** `docs/specs/adr/0004-store-global-task-number.md`.

## Acceptance Criteria

Term pins for this section: a task **carries** number N when its stored task
number equals N. **Direct lookup** is `list` or `steps` invoked with a
single-task operand. **Bare digits** are a decimal integer with no prefix
character (`12`, not `#12`, not `T12`). **Same class of error as unknown UUID**
means that command's existing unknown-UUID refusal (list: usage refusal; steps:
`unknown-task`), never an empty success.

### Assignment

**AC-1** A newly persisted task carries a task number. *(Verification type:
**test-backed** — integration)*

**AC-2** No two tasks in the store carry the same task number. *(Verification
type: **test-backed** — integration)*

**AC-3** Of two tasks persisted in sequence, the later carries a strictly greater
number. *(Verification type: **test-backed** — integration)*

**AC-4** Two overlapping durable creates assign two different numbers.
*(Verification type: **test-backed** — integration)*

**AC-5** An add that reports the task as already existing returns that task's
number, and no task's number changes. *(Verification type: **test-backed** —
integration)*

### Stability

**AC-6** After a task carries a number, every later mutation of that task leaves
the number unchanged. *(Verification type: **test-backed** — integration)*

**AC-7** Undo of complete or of soft-delete leaves the task carrying the same
number. *(Verification type: **test-backed** — integration)*

**AC-8** Direct lookup by number finds the task when it is done and when it is
soft-deleted. *(Verification type: **test-backed** — integration)*

**AC-9** A later create never carries a number that any task has already carried,
including soft-deleted tasks. *(Verification type: **test-backed** —
integration)*

### Upgrade

**AC-10** A store written before this feature loads without error. *(Verification
type: **test-backed** — integration)*

**AC-11** After opening that store, every task in it, including done and
soft-deleted, carries a distinct task number. *(Verification type:
**test-backed** — integration)*

**AC-12** A second load of that store preserves each task number exactly.
*(Verification type: **test-backed** — integration)*

**AC-13** A binary that cannot preserve task numbers refuses a store that has
them, and does not write that store. *(Verification type: **test-backed** —
integration)*

### CLI

**AC-14** Direct lookup with bare digits N lists the unique task that carries N.
The result does not depend on cwd or invocation default. *(Verification type:
**test-backed** — integration)*

**AC-15** Direct lookup with bare digits combined with a scope, thread, or status
filter is refused with the same class of error as combining a UUID with those
filters. *(Verification type: **test-backed** — integration)*

**AC-16** Direct lookup with the task's UUID still lists that task.
*(Verification type: **test-backed** — integration)*

**AC-17** Direct lookup with bare digits no task carries fails with the same
class of error as unknown UUID on that command. *(Verification type:
**test-backed** — integration)*

**AC-18** `steps` with bare digits N acts on the unique task that carries N.
*(Verification type: **test-backed** — integration)*

**AC-19** `steps` with the task's UUID still acts on that task. *(Verification
type: **test-backed** — integration)*

**AC-20** JSON list output includes each row's task number. *(Verification type:
**test-backed** — integration)*

**AC-21** JSON add output includes the task number on created and on existing
outcomes, including plan rows. *(Verification type: **test-backed** —
integration)*

**AC-22** Human list output shows each row's task number as bare digits.
*(Verification type: **test-backed** — integration)*

### Board

**AC-23** A board task row shows the task number as bare digits. *(Verification
type: **test-backed** — unit)*

**AC-24** Peek shows the task number as bare digits. *(Verification type:
**test-backed** — unit)*

**AC-25** The task page shows the task number as bare digits. *(Verification
type: **test-backed** — unit)*

**AC-26** The done drawer shows the task number as bare digits. *(Verification
type: **test-backed** — unit)*

**AC-27** At 40×10, numbered rows paint in bounds, titles wrap rather than
truncate, and every listed task remains reachable by arrow navigation.
*(Verification type: **test-backed** — unit)*

### Docs

**AC-28** The CLI skills doc documents task number as the human handle, UUID
still valid, cwd ignored on direct lookup, and the verify-before-retry rules
unchanged. *(Verification type: **reviewer-checked** — axis: Spec Conformance.
Pass/fail: does `skills/tsk-cli/SKILL.md` document bare-digit lookup for `list`
and `steps`, JSON emission of the task number, UUID still valid, cwd ignored,
and the existing retry/exit contract? Justification: prose-contract completeness
is not cheaply automatable; the carrying task is the CLI task that produces the
doc.)*

**AC-29** Site board and CLI docs, and the board demo, show the task number as
bare digits. *(Verification type: **reviewer-checked** — axis: Spec Conformance.
Pass/fail: do `site/src/content/docs/docs/board.md`, `cli.md`, and
`site/public/board-demo.js` show `12` not `#12` or `T12`, with no keymap change
in `keys.md`? Justification: site copy and the demo are not in the Rust test
bar; the carrying task is the site-docs task that produces those files.)*

### Negative criteria (out of bounds)

**NC-1** No two scopes independently number from 1: a desk task and a project
task never both carry 12.

**NC-2** Direct lookup never uses cwd to choose among tasks.

**NC-3** No surface accepts or paints `proj-N` as the task number.

**NC-4** No surface paints a UUID prefix as the task number.

**NC-5** No title slug is used as the task number.

**NC-6** The board never paints the task number as `#N` or `TN`.

**NC-7** No create-undo is added.

**NC-8** No second store, sidecar index, or database is introduced.

**NC-9** Addressing a task by number does not change its human status.

**NC-10** No attention, park/resume, linking, or dispatch surface returns.

**NC-11** Step short ids remain prefixes of step identity, not task numbers.

**NC-12** `keys.md` is unchanged.

### Verification map

| Criterion | Oracle kind / review axis |
| --- | --- |
| AC-1 | integration |
| AC-2 | integration |
| AC-3 | integration |
| AC-4 | integration |
| AC-5 | integration |
| AC-6 | integration |
| AC-7 | integration |
| AC-8 | integration |
| AC-9 | integration |
| AC-10 | integration |
| AC-11 | integration |
| AC-12 | integration |
| AC-13 | integration |
| AC-14 | integration |
| AC-15 | integration |
| AC-16 | integration |
| AC-17 | integration |
| AC-18 | integration |
| AC-19 | integration |
| AC-20 | integration |
| AC-21 | integration |
| AC-22 | integration |
| AC-23 | unit |
| AC-24 | unit |
| AC-25 | unit |
| AC-26 | unit |
| AC-27 | unit |
| AC-28 | Spec Conformance (reviewer-checked) |
| AC-29 | Spec Conformance (reviewer-checked) |

### Deferred

None. CLI input sugar (`#12`, `T12`) is not in scope; the canonical form is bare
digits.

### Glossary terms touched

None beyond **task number** (already in `CONTEXT.md`). **Bare digits** and
**direct lookup** are pinned above, local to this section.

## Design

Feature-level: every component is a change to an existing surface except the
allocator, which is new. The load-bearing shape: the task number is an immutable
alias on the task record, minted only at the locked document-write boundary;
selection, revisions, merge, undo, and history stay on task identity (ADR-0004).
Board paint puts bare digits in the existing dim meta run, not in the title.

### Components

1. **Task number field** — the integer address stored on the task. Input: a
   number produced by the allocator. Output: that integer for presentation and
   lookup. Never present on a draft. Errors: none at read. Edits, status, scope,
   thread, complete, and soft-delete do not take this field as input, so they
   cannot change it. Uniqueness is an invariant of the document: two tasks never
   carry the same number.
2. **Number allocator** — the only writer of new numbers. Kind: a monotonic
   counter on the versioned document, advanced under the existing exclusive lock.
   Input: a locked document plus either a newly persisted task or a pre-feature
   document whose tasks have no numbers. Output: those tasks carrying numbers,
   counter at least one greater than every assigned number, document version that
   old writers cannot round-trip. Errors: overlapping creates are serialized by
   the lock (two creates, two numbers); an upgrade that cannot be written refuses
   the open and does not expose temporary numbers; a binary that cannot preserve
   numbers refuses a numbered store and does not write it. Idempotent add that
   finds an existing task returns that number and does not advance the counter.
   Legacy assignment covers every task including done and soft-deleted, once, in
   created-at order (identity as the deterministic tie-break), then persists
   before any lookup is served.
3. **Operand resolver** — CLI boundary that turns a single-task operand into task
   identity. Input: argv string. Output: the existing UUID lookup path. Accepts a
   UUID or bare digits; bare digits resolve to the unique task that carries that
   number, including done and soft-deleted, ignoring cwd and invocation default.
   Errors: malformed operand is usage; unknown number uses the same class of
   error as unknown UUID on that command; combining a number with scope, thread,
   or status filters uses the same refusal as combining a UUID with those
   filters. Domain commands never see a number.
4. **CLI number presentation** — list and add output. Input: tasks that carry
   numbers. Output: JSON rows and add results include a numeric `number` beside
   `id`; human list rows show bare digits. Created and existing add outcomes both
   emit it, including plan rows. Errors: none beyond existing add/list failures.
5. **Board number chrome** — paints the number as dim bare digits in the existing
   row-meta run (with age and project), on peek meta, on the task-page footer, and
   on done-drawer rows. Input: the task's number. Output: painted cells; titles
   still wrap; copied title text stays title-only. Registers no new hit target
   and no selectable identity. Errors: none (read-only). At 40×10 the extra cells
   ride the existing wrap engine so every listed task stays reachable by arrows.
6. **CLI skills doc** — the skills file that agents read. Input: the CLI
   contract above. Output: prose that names task number as the human handle, UUID
   still valid, cwd ignored on direct lookup, JSON `number`, and the existing
   retry/exit rules. Errors: none.
7. **Site number docs** — board and CLI pages plus the board demo. Input: the
   board/CLI contract. Output: those pages and the demo show `12`, not `#12` or
   `T12`. `keys.md` is not edited. Errors: none.

Rejected shapes: a per-scope counter (ADR-0004); minting in memory before the
lock (two creators can mint the same number); deriving the number from created-at
rank at read time (a later compact or reorder would renumber spoken handles);
prefixing the title (spends wrap budget on every row; meta is already the small-fact
column).

### Data flow and key state

Durable create → exclusive lock → allocator mints → document write → board chrome
and CLI presentation. Direct lookup: argv → operand resolver → domain by identity
→ list/steps as today. Pre-feature open: exclusive lock → allocator backfills →
versioned write → then serve. Key state lives in one place: the task record plus
one document-level next-number counter. No session state, no secondary index.
Selection stays identity-pinned; the number is not a selection key.

### Trust and failure boundaries

Untrusted input is the CLI operand (and, for agents, whatever they copy from
list). The resolver validates at that boundary; the domain and store only ever
see task identity. Failure modes: malformed or unknown operand → command-local
refusal, nothing persisted; store I/O on create → existing exit-3 indeterminate
contract, verify with list; overlapping creates → lock, distinct numbers; upgrade
write failure → refuse open; old writer against a numbered store → refuse, live
file untouched. Renderer failure cannot mint or change a number.

### Criterion → component map

| Criterion | Component(s) |
| --- | --- |
| AC-1 | Number allocator, Task number field |
| AC-2 | Number allocator, Task number field |
| AC-3 | Number allocator |
| AC-4 | Number allocator |
| AC-5 | CLI number presentation, Number allocator |
| AC-6 | Task number field |
| AC-7 | Task number field |
| AC-8 | Operand resolver |
| AC-9 | Number allocator |
| AC-10 | Number allocator |
| AC-11 | Number allocator, Task number field |
| AC-12 | Number allocator, Task number field |
| AC-13 | Number allocator |
| AC-14 | Operand resolver |
| AC-15 | Operand resolver |
| AC-16 | Operand resolver |
| AC-17 | Operand resolver |
| AC-18 | Operand resolver |
| AC-19 | Operand resolver |
| AC-20 | CLI number presentation |
| AC-21 | CLI number presentation |
| AC-22 | CLI number presentation |
| AC-23 | Board number chrome |
| AC-24 | Board number chrome |
| AC-25 | Board number chrome |
| AC-26 | Board number chrome |
| AC-27 | Board number chrome |
| AC-28 | CLI skills doc |
| AC-29 | Site number docs |

### ADRs created

- `docs/specs/adr/0004-store-global-task-number.md` (idea stage): numbers are
  store-global, not per-project.

No new ADR for the document-version bump: it is the existing "older writer cannot
round-trip" rule applied to this field, already named in ADR-0004 consequences.

### Glossary terms touched

None. **Task number** already in `CONTEXT.md`.

Constitution check: no `constitution.md` exists in this repo; the shape was
checked against `AGENTS.md` standing product rules (human status sovereignty, UUID
selection pinning, wrap-never-truncate, one JSON document) with no conflict.

## Tech Stack

**No new products — reuses the declared stack.** Every component is work in this
crate: the allocator is a counter on the existing JSON document, the field is an
integer on the existing task record, CLI and board ride the existing binary, docs
are Markdown and the existing board demo. No dependency is added to `Cargo.toml`;
no version changes. (Green bar: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`, per `AGENTS.md`; CI Rust
1.96.0.)

No load-bearing external-library claims exist to probe: integer fields and
`format_version` refusal are already exercised by this store (`format_version` is
a document integer; newer documents are refused, not rewritten). In-repo
precedent, not an external claim.

Unverified: none. Glossary terms touched: none.

## Plan

**T-1 — Domain field, allocator, upgrade, format bump.** Add `number: Option<u64>`
on the task (absent means unassigned: drafts and pre-upgrade rows) and
`next_task_number: u64` on the document. Mint only inside the exclusive locked
save: a newly persisted UUID receives the counter, the counter advances, merge
takes `max` of the two counters and never assigns a number an existing UUID
already has. Bump `STORE_FORMAT_VERSION` to 2; keep `LEGACY_STORE_FORMAT_VERSION`
at 1. A v1 document, on first locked open, assigns distinct numbers to every
task including done and soft-deleted, in `created_at` order with id as
tie-break, writes the v2 document, then serves lookups. A binary that cannot
preserve numbers refuses a v2 store and does not write it. Edit, status, scope,
thread, complete, and soft-delete do not take `number` as input. Include compile
fallout of the new fields on every `Task` / `DomainState` literal.

- Files: `src/domain/task.rs`, `src/store.rs`, `src/ui/queue.rs`,
  `src/ui/render.rs`, `src/capture.rs`, `src/domain/undo.rs`,
  `tests/store_persist.rs`, `tests/queue_board_model.rs`,
  `tests/queue_board_render.rs`, `tests/e2e_persist.rs`.
- Test first: in `src/domain/task.rs`,
  `locked_create_assigns_number_one_then_two`,
  `edit_status_scope_thread_complete_soft_delete_leave_number_unchanged`,
  `undo_of_complete_and_of_soft_delete_keeps_the_same_number`. In
  `tests/store_persist.rs`,
  `pre_number_store_upgrades_assigning_distinct_numbers_including_done_and_deleted`,
  `upgraded_store_preserves_numbers_on_second_load`,
  `overlapping_creates_under_lock_receive_distinct_numbers`,
  `v2_store_is_refused_when_writer_format_is_older_and_the_file_is_unwritten`.
- *Advances:* AC-1, AC-2, AC-3, AC-4, AC-6, AC-7, AC-9, AC-10, AC-11, AC-12, AC-13.
  *Component:* Number allocator. *Deps:* none.

**T-2 — CLI presentation.** List JSON rows and add JSON (flag created/existing and
plan created/existing rows) include numeric `number` beside `id`. Human list rows
show bare digits. Idempotent add that reports existing emits that task's number
and does not advance the counter.

- Files: `src/cli/list.rs`, `src/cli/add.rs`, `src/cli/presenter.rs`,
  `tests/cli_list.rs`, `tests/cli_add.rs`.
- Test first: in `tests/cli_list.rs`, `json_list_rows_include_number`,
  `human_list_rows_show_bare_digits`. In `tests/cli_add.rs`,
  `json_created_and_existing_include_number`,
  `plan_created_and_existing_include_number`,
  `existing_add_does_not_advance_the_counter`.
- *Advances:* AC-5, AC-20, AC-21, AC-22.
  *Component:* CLI number presentation. *Deps:* T-1.

**T-3 — CLI addressing and skills.** Direct lookup accepts bare digits N or a UUID.
`list N` and `steps N` act on the unique task that carries N, including done and
soft-deleted, ignoring cwd and invocation default. Combining a number with scope,
thread, or status filters is refused the same way as combining a UUID with those
filters. Unknown number uses the same class of error as unknown UUID on that
command. Update `skills/tsk-cli/SKILL.md` so agents resolve "task 12" via
`tsk list 12`, UUID still valid, cwd ignored, JSON `number`, retry/exit contract
unchanged.

- Files: `src/cli/parser.rs`, `src/cli/list.rs`, `src/cli/steps.rs`,
  `src/cli/presenter.rs`, `tests/cli_list.rs`, `tests/cli_steps.rs`,
  `skills/tsk-cli/SKILL.md`.
- Test first: in `tests/cli_list.rs`,
  `list_bare_digits_finds_the_task_from_another_cwd`,
  `list_bare_digits_finds_done_and_deleted_tasks`,
  `list_uuid_still_finds_the_task`,
  `list_bare_digits_with_scope_or_status_flags_is_usage`,
  `list_unknown_number_matches_unknown_uuid_refusal`. In `tests/cli_steps.rs`,
  `steps_bare_digits_act_on_the_task`,
  `steps_uuid_still_acts_on_the_task`,
  `steps_unknown_number_is_unknown_task`.
- *Advances:* AC-8, AC-14, AC-15, AC-16, AC-17, AC-18, AC-19, AC-28.
  *Component:* Operand resolver. *Deps:* T-1, T-2.

**T-4 — Board number chrome.** Paint dim bare digits in the existing row-meta run
(with age and project), on peek meta, on the task-page footer as the first
component, and on done-drawer rows. Titles still wrap. No `#` or `T` prefix. No
new hit target. Document the painted number in `AGENTS.md`'s board section.
Regenerate golden fixtures via the ignored test, never by hand.

- Files: `src/ui/render.rs`, `src/ui/board/draw.rs`, `AGENTS.md`,
  `tests/queue_board_render.rs`, `tests/fixtures/` (golden board renders).
- Test first: in `tests/queue_board_render.rs`,
  `board_row_meta_shows_bare_digits_not_hash_or_t_prefix`,
  `peek_shows_bare_digits`,
  `task_page_footer_shows_bare_digits`,
  `done_drawer_rows_show_bare_digits`,
  `numbered_board_at_40x10_paints_in_bounds_wraps_titles_and_keeps_tasks_reachable`.
- *Advances:* AC-23, AC-24, AC-25, AC-26, AC-27.
  *Component:* Board number chrome. *Deps:* T-1.

**T-5 — Site number docs.** Board and CLI pages and the board demo show `12`, not
`#12` or `T12`. Do not edit `keys.md`.

- Files: `site/src/content/docs/docs/board.md`, `site/src/content/docs/docs/cli.md`,
  `site/public/board-demo.js`.
- Test first: in `site/` the existing `npm test` assertions (extend the demo/docs
  tests if present; otherwise the verification is the reviewer pass/fail on AC-29
  plus `npm test` still green). Add a demo fixture assertion that a painted row
  contains `12` and does not contain `#12` or `T12`.
- *Advances:* AC-29.
  *Component:* Site number docs. *Deps:* T-4.

### Task-to-criterion coverage map

| Criterion | Advanced by |
| --- | --- |
| AC-1 | T-1 |
| AC-2 | T-1 |
| AC-3 | T-1 |
| AC-4 | T-1 |
| AC-5 | T-2 |
| AC-6 | T-1 |
| AC-7 | T-1 |
| AC-8 | T-3 |
| AC-9 | T-1 |
| AC-10 | T-1 |
| AC-11 | T-1 |
| AC-12 | T-1 |
| AC-13 | T-1 |
| AC-14 | T-3 |
| AC-15 | T-3 |
| AC-16 | T-3 |
| AC-17 | T-3 |
| AC-18 | T-3 |
| AC-19 | T-3 |
| AC-20 | T-2 |
| AC-21 | T-2 |
| AC-22 | T-2 |
| AC-23 | T-4 |
| AC-24 | T-4 |
| AC-25 | T-4 |
| AC-26 | T-4 |
| AC-27 | T-4 |
| AC-28 | T-3 |
| AC-29 | T-5 |

### Notes

Mint inside the locked save, never in the in-memory `create` helper: two CLI
processes must not both mint 13. Golden fixtures regenerate with
`cargo test --test queue_board_render regenerate_golden_fixtures -- --ignored`.
Do not rewrite `HANDOFF.md`. Live board smoke uses an isolated `TSK_STATE_DIR`,
never `~/.tsk`. Site-only files are outside the Rust green bar; T-5 still runs
`npm test` in `site/`.
