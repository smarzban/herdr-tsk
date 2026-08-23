# Checklists

Feature: per-task checklists — a flat, ordered list of items with stable identity,
rendered and operable on the task page and from the CLI.

## Brief

<!-- source: two-agent design review settled with the user, 2026-08-22 · ingested 2026-08-22 -->

**Problem.** Tasks need capture-granular sub-steps that show progress without turning
the queue board into a project tree. Notes hold prose; status is one verb-sized
signal; neither carries "three of five steps done" or gives a script something stable
to tick off.

**Scope.** One flat checklist per task: items with stable identity, one line of text,
and a done flag. The task page grows a checklist section (between notes and the meta
footer) with an item cursor; mutating verbs are modifier-protected (`Alt+a` add,
`Alt+space` toggle, contextual `Alt+e` rename, `Alt+x` twice delete); a one-line
editor inline in the section owns add/rename input. The CLI grows `check` subcommands
to add items and to toggle them by short id; rename and remove stay page-side
verbs. Every mutation is journaled and revision-guarded through the existing save
path.

**Non-goals.** No nesting or subtask trees; no per-item status beyond done; no
markdown/checkbox parsing of notes; no reorder verb; no undo verb; no CLI rename or
remove (named fast-follow; the domain commands exist but only the page exposes
them); no board-surface editing or board progress badge (badge is a named
fast-follow); quick-add bar unchanged.

**Chosen approach.** A structured `checklist` field on the task, over a
markdown-in-notes convention (rejected: identity by line number breaks stale-render
toggles and CLI addressing; capture would require typing marker syntax in the full
notes editor) and over nested subtasks (rejected: violates the flat board model).
See ADR-0001. Review corrected two supporting claims: store concurrency is task-level
under either design (not a differentiator), and typed history events were possible
under the rejected design too. The verdict rested on stable identity and CLI parity.

**Resolved decisions.** Storage: structured field, serde-defaulted, no migration.
Keys: `Alt+a` add (free of the `e`/`n` page verbs), `Alt+space` toggle,
`Alt+x` twice delete with inline confirm, contextual `Alt+e` disambiguated by item
cursor. Editor: inline in the checklist section, not the board status-row slot (the
page is already a takeover surface and owns its own refusals). Addressing: stable
item ids, prefix-matched. Merge behavior: unchanged task-level rejection; concurrent
same-task mutations conflict exactly as title-vs-notes does today. Item cursor
lifecycle: inactive when the page opens; a bare ↓ activates it on the first item;
↑ from the first item deactivates it, and while inactive ↑ scrolls notes while ↓
re-activates; a task with no items never activates a cursor and its arrows scroll
exactly as before this feature. *(Amended 2026-08-22 after live testing:
activation is re-activatable; the item input renders in the page footer slot on
the quick-add line pattern; with items present the content region splits half
notes / half checklist with items stacking from the top of their half; clicking an
item row selects it.)*

**Glossary terms touched.** checklist, checklist item, item done, toggle, item
cursor, item short id (mirrored into `CONTEXT.md`).

**ADR.** `docs/specs/adr/0001-checklist-structured-field.md`.

## Acceptance Criteria

**AC-1** A store file written before this feature — task objects carrying no
checklist field — loads without error, and every task so loaded presents an empty
checklist. *(Verification type: **test-backed** — integration)*

**AC-2** Saving and reloading a task preserves each checklist item's identity,
text, done flag, and list position. *(Verification type: **test-backed** —
integration)*

**AC-3** Each checklist mutation kind (add, toggle, rename, remove) appends its own
typed history event and changes the task revision. *(Verification type:
**test-backed** — unit)*

**AC-4** Toggling a checklist item leaves the task's human status unchanged,
including the toggle that completes the checklist. *(Verification type:
**test-backed** — unit)*

**AC-5** The task page renders a checklist section between the notes block and the
meta footer when the task has at least one item. *(Verification type:
**test-backed** — unit)*

**AC-6** The task page of a task with no checklist items renders no checklist
section. *(Verification type: **test-backed** — unit)*

**AC-7** The checklist section label shows done-count and total-count matching the
item states. *(Verification type: **test-backed** — unit)*

**AC-8** Bare ↑/↓ with the item cursor active move it through checklist items,
driving page scroll when the cursor reaches a hidden row. *(Verification type:
**test-backed** — unit)*

**AC-9** Checklist-mutating verbs dispatched from the task page require the verb
modifier; the same bare keys produce no checklist mutation. *(Verification type:
**test-backed** — unit)*

**AC-10** With the item cursor active, the rename verb edits the highlighted item;
with the cursor inactive, it edits the task title. *(Verification type:
**test-backed** — unit)*

**AC-11** The delete verb marks the highlighted item for deletion visibly; a second
press removes the item; any intervening key clears the mark without removing.
*(Verification type: **test-backed** — unit)*

**AC-12** In the item line editor: Enter saves and closes; Ctrl+Enter saves and
reopens the line empty in add mode, while in rename mode it saves and closes like
Enter; Esc closes without saving. *(Verification type: **test-backed** — unit)*

**AC-13** Empty or whitespace-only item text is refused by a message painted on the
editor line itself, and the refusal clears when the line closes. *(Verification
type: **test-backed** — unit)*

**AC-14** A failed save while the item editor is open keeps the editor and its input
mode allocated until Retry/Cancel (r/c/Esc) resolve it; a cancelled failed save
returns to page view leaving no edit mode without its surface. *(Verification type:
**test-backed** — integration)*

**AC-15** `check <task> add` creates one item carrying a stable identity;
`check <task> toggle` resolves an unambiguous item short id; an unknown or ambiguous
short id exits with an error and mutates nothing. *(Verification type:
**test-backed** — integration)*

**AC-16** Listing a task's checklist prints one line per item with its `[x]`/`[ ]`
state and its item short id. *(Verification type: **test-backed** — integration)*

<!-- amended 2026-08-22: live-testing feedback — activation is re-activatable, not one-shot -->

**AC-17** On a task with steps and the step cursor inactive, a bare ↓ activates
(or re-activates) the cursor on the first step instead of scrolling. Keyboard
↓ then advances the active cursor through steps before it can scroll the page.
*(Verification type: **test-backed** — unit)*

**AC-18** With the step cursor active, bare ↑ moves the cursor toward the first
step before it scrolls page content; ↑ from the first step deactivates the cursor.
While inactive, ↑ scrolls notes and ↓ re-activates the cursor. *(Verification
type: **test-backed** — unit)*

**AC-19** On a task with no checklist items, bare ↑/↓ scroll the task page exactly
as before this feature. *(Verification type: **test-backed** — unit)*

**AC-20** `check <task> add` refuses item text that is empty after trimming or
contains a C0 control character: non-zero exit, nothing persisted. *(Verification
type: **test-backed** — integration)*

<!-- added 2026-08-22: live-testing feedback (user-directed amendments) -->

**AC-21** A single mouse click on a checklist item row moves the item cursor to
that item; a click selects, it never toggles. *(Verification type: **test-backed**
— unit)*

**AC-22** While the task page shows a task with checklist items in view mode, the
footer verb bar lists the item-add verb alongside the existing verbs. *(Verification
type: **test-backed** — unit)*

**AC-23** A first delete-verb press marks the highlighted item and shows a footer
message prompting a second press to remove; the message clears when the mark
clears. *(Verification type: **test-backed** — unit)*

**AC-24** When a task has at least one step, notes and steps form one scrollable
content stream between the fixed page header and footer. Short notes reserve at
least the stream's top half before the steps label; longer notes push the label
and top-anchored steps down, and scrolling reveals them without clipping.
*(Verification type: **test-backed** — unit)*

**AC-25** The step add/rename input renders as a one-line input in the page footer
slot, reusing the quick-add line pattern; Enter/Ctrl+Enter/Esc semantics are
unchanged and the line owns its refusals and clears them on close. *(Verification
type: **test-backed** — unit)*

**AC-26** On a task page with steps, keyboard arrows own the step-cursor lifecycle
and mouse-wheel scrolling owns the shared content stream: wheel events never
activate, move, or deactivate the cursor. *(Verification type: **test-backed** —
unit)*

**AC-27** While Notes is being edited after the shared content stream has scrolled,
the terminal caret remains on its matching visible draft row, accounting for the
stream offset. *(Verification type: **test-backed** — unit)*

### Negative criteria

- NC-1: No code path parses checklist markers out of notes text.
- NC-2: No nesting — an item carries no parent or children.
- NC-3: No rule anywhere observes checklist completion to change task status.
- NC-4: No board-surface checklist editing and no board progress badge in this
  feature.
- NC-5: No reorder verb.
- NC-6: The quick-add bar accepts title and scope tokens only; no checklist syntax.
- NC-7: No new color, theme, or styling machinery; mono modifiers only.
- NC-8: No CLI verb renames or removes checklist items in this feature.

### Verification map

| Criterion | Oracle |
| --- | --- |
| AC-1 | integration test |
| AC-2 | integration test |
| AC-3 | unit test |
| AC-4 | unit test |
| AC-5 | unit test |
| AC-6 | unit test |
| AC-7 | unit test |
| AC-8 | unit test |
| AC-9 | unit test |
| AC-10 | unit test |
| AC-11 | unit test |
| AC-12 | unit test |
| AC-13 | unit test |
| AC-14 | integration test |
| AC-15 | integration test |
| AC-16 | integration test |
| AC-17 | unit test |
| AC-18 | unit test |
| AC-19 | unit test |
| AC-20 | integration test |
| AC-21 | unit test |
| AC-22 | unit test |
| AC-23 | unit test |
| AC-24 | unit test |
| AC-25 | unit test |
| AC-26 | unit test |
| AC-27 | unit test |

## Design

<!-- source: two-agent design review settled with the user, 2026-08-22 · ingested 2026-08-22 -->

Feature-level design fitted into the existing architecture. The store, locking, and
merge machinery are untouched — the checklist rides the existing task save path.

### Components

1. **Checklist domain model** — owns the item shape (stable id, one line of text,
   done flag), the ordered list on the task, and the four mutation commands (add,
   toggle, rename, remove); each command journals a typed event and bumps the task
   revision. Contract: input is a loaded task plus item id / text; output is the
   mutated task; errors are refusal values for unknown item id and empty text — it
   never touches status, scope, or notes.
2. **Task page view model** — builds the page payload: the checklist section
   (extracted item views with done flag, never the raw storage), the done/total
   counts, the item cursor index, the delete-mark state, and the editor line state.
   Contract: input is the task plus page state; output is the payload the renderer
   paints; no mutation of the task.
3. **Task page renderer** — paints the section, glyphs, cursor row, delete mark,
   editor line, and the editor's own refusals; records scroll bounds. Contract:
   input is the view payload; output is painted cells; anything painted on the
   editor line owns hiding/clearing per the surface rules.
4. **Page key map** — maps task-page keys to intents: bare arrows own the item
   cursor lifecycle — inactive on page open, a first bare ↓ activates it on the
   first item, ↑ from the first item deactivates it and returns arrows to note
   scrolling, and a task with no items never activates one; with the cursor
   active, arrows move it. Mutating verbs require the modifier; the rename verb is
   disambiguated by the active/inactive cursor state; the delete verb follows
   mark-then-confirm. Contract: input is key code + modifier + page state; output
   is an intent or none; bare mutating letters map to none.
5. **Item editor surface** — the one-line add/rename input on the page: buffer
   lifecycle, Enter / Ctrl+Enter / Esc semantics, refusal ownership, and the
   save-recovery interplay (the editor and its mode outlive the save call until
   Retry/Cancel resolve). Contract: input is key events and domain save results;
   output is applied mutations or a held-failed-save state; a cancelled failed save
   restores page view with no orphan mode.
6. **Checklist CLI** — the `check` subcommand family and checklist output in task
   listing: item creation, short-id resolution, state printing. Contract: input is
   argv (task id, subcommand, text or short id); output is an exit report; unknown or
   ambiguous short ids exit non-zero with nothing persisted.

### Data flow and key state

Add/rename flows: page key → item editor buffer → domain command → store save
(revision + event, existing atomic path) → page payload rebuild → paint. Toggle and
delete flows skip the editor: key → intent → domain command → save → repaint. CLI
flows: argv → validation at the boundary → same domain commands → same save path.
Key state: the ordered item list on the task (persisted); the item cursor index,
delete-mark, and editor buffer (page-session only, never persisted). Checklist
mutations push no undo entry: the board undo verb's semantics are unchanged, and
item removal is permanent — mark-then-confirm is its only guard.

### Trust and failure boundaries

Untrusted text enters at two boundaries: the editor line (single line; empty after
trim refused on the line) and the CLI (positional text; same refusal discipline as
task titles, including control-character rejection). Item short ids from the CLI are
resolved at the boundary — unknown or ambiguous refuses before any mutation. A failed
save crosses into save recovery: the editor stays allocated, `r`/`c`/Esc resolve it,
and no reload happens underneath an unresolved save.

### Criterion-to-component map

| Criterion | Component |
| --- | --- |
| AC-1 | Checklist domain model |
| AC-2 | Checklist domain model |
| AC-3 | Checklist domain model |
| AC-4 | Checklist domain model |
| AC-5 | Task page view model, Task page renderer |
| AC-6 | Task page view model, Task page renderer |
| AC-7 | Task page view model, Task page renderer |
| AC-8 | Task page view model, Page key map |
| AC-9 | Page key map |
| AC-10 | Page key map |
| AC-11 | Page key map, Task page view model |
| AC-12 | Item editor surface |
| AC-13 | Item editor surface |
| AC-14 | Item editor surface |
| AC-15 | Checklist CLI |
| AC-16 | Checklist CLI |
| AC-17 | Page key map, Task page view model |
| AC-18 | Page key map, Task page view model |
| AC-19 | Page key map |
| AC-20 | Checklist CLI |
| AC-21 | Page key map, Task page view model |
| AC-22 | Task page renderer |
| AC-23 | Item editor surface |
| AC-24 | Task page renderer |
| AC-25 | Item editor surface |
| AC-26 | Page key map, Task page view model |
| AC-27 | Task page view model, Task page renderer |

### Outside the checker

- Store persistence (existing, unchanged): the checklist rides the existing task
  serialization, atomic write, and lock; no store file changes.

### ADRs created

- `docs/specs/adr/0001-checklist-structured-field.md` (structured field over a notes
  convention; records the withdrawn concurrency argument).

## Tech Stack

No new products — reuses the declared stack (green bar: `cargo fmt --check && cargo
clippy --all-targets -- -D warnings && cargo test && cargo build --release`).

Existing dependencies cover the feature: `uuid` for item identity, `serde` for the
defaulted field. CI pin is Rust 1.96.0 with rustfmt and clippy. No load-bearing
library claims — nothing new is leaned on.

## Plan

**T-1 — Checklist domain field and commands.** Add the item shape (stable id, one
line of text, done flag) and the ordered defaulted field to the task; implement the
add/toggle/rename/remove commands, each journaling its typed event and bumping the
revision; refusals for unknown item id and empty-after-trim text.

- Files: `src/domain/task.rs`, `src/domain/events.rs`, `tests/store_persist.rs`.
- Test first: in the `src/domain/task.rs` test module,
  `checklist_mutations_journal_events_and_bump_revision` (drives all four commands;
  asserts one typed event per kind and a changed revision per mutation) and
  `toggle_item_keeps_human_status_including_completing_last_item`. In
  `tests/store_persist.rs`, `pre_checklist_store_decodes_with_empty_checklists`
  (fixture JSON whose tasks carry no checklist field) and
  `checklist_round_trip_preserves_identity_flags_and_order`.
- *Advances:* AC-1, AC-2, AC-3, AC-4. *Component:* Checklist domain model. *Deps:*
  none.

**T-2 — Task page checklist section rendering.** Extract the item views the page
paints (done flag + text, never raw storage), build the section payload with
done/total counts, and paint it between the notes block and the meta footer; no
section when the checklist is empty.

- Files: `src/ui/board/model.rs`, `src/ui/board/draw.rs`, `src/ui/render.rs`,
  `tests/queue_board_render.rs`.
- Test first: `task_page_paints_checklist_section_between_notes_and_footer` (≥1 item;
  asserts position and done/total label) and
  `task_page_without_items_paints_no_checklist_section` (empty checklist; asserts no
  section in the payload).
- *Advances:* AC-5, AC-6, AC-7. *Component:* Task page view model. *Deps:* T-1.

**T-3 — Page item cursor and modifier-protected verbs.** Wire bare ↑/↓ to move the
item cursor (driving scroll at hidden rows); dispatch `Alt+a` add, `Alt+space`
toggle, contextual `Alt+e` rename (item when the cursor is active, title otherwise),
and mark-then-confirm `Alt+x` delete; bare mutating letters stay inert.

- Files: `src/ui/input.rs`, `src/ui/board/model.rs`, `src/app.rs`,
  `tests/v1_keymap_guard.rs`, `tests/queue_board_verbs.rs`.
- Test first: in `tests/v1_keymap_guard.rs`,
  `bare_page_keys_never_mutate_checklist`. In `tests/queue_board_verbs.rs`,
  `arrow_keys_move_item_cursor_and_drive_scroll`,
  `modifier_toggle_flips_item_under_cursor`,
  `rename_verb_targets_item_or_title_by_cursor`,
  `delete_verb_marks_then_removes_on_second_press`,
  `first_bare_down_activates_item_cursor_without_scrolling`,
  `up_from_first_item_deactivates_cursor_and_restores_scroll`,
  `no_item_task_arrows_scroll_notes_unchanged`.
- *Advances:* AC-8, AC-9, AC-10, AC-11, AC-17, AC-18, AC-19.
  *Component:* Page key map. *Deps:* T-1, T-2.

**T-4 — Item line editor and save recovery.** The one-line add/rename input on the
section: Enter saves and closes, Ctrl+Enter saves and reopens empty (add), Esc
cancels; empty-after-trim text refused on the line and cleared on close; a failed
save holds the editor and its mode until Retry/Cancel resolve it.

- Files: `src/ui/board/model.rs`, `src/ui/board/apply.rs`, `src/app.rs`,
  `src/save_recovery.rs`, `tests/f6_save_recovery.rs`.
- Test first: `item_editor_enter_saves_ctrl_enter_reopens_esc_cancels`,
  `rename_mode_ctrl_enter_saves_and_closes`,
  `empty_item_text_refusal_paints_on_line_and_clears_on_close`, and the decisive
  regression `cancelled_failed_item_editor_save_leaves_no_orphan_edit_mode` (must be
  observed red against the pre-fix behavior before the fix lands).
- *Advances:* AC-12, AC-13, AC-14. *Component:* Item editor surface. *Deps:* T-1,
  T-3.

**T-5 — Checklist CLI.** The `check` subcommand family: `add` (one item per
invocation, stable identity), `toggle` by unambiguous short id (unknown/ambiguous
refuses with nothing persisted), and checklist lines in single-task listing.

- Files: `src/cli/parser.rs`, `src/cli/router.rs`, `src/cli/check.rs` (new),
  `src/cli/list.rs`, `src/cli/presenter.rs`, `CHANGELOG.md`, `tests/cli_check.rs`
  (new), `tests/cli_list.rs`.
- Test first: `check_add_then_toggle_round_trips_item_state`,
  `check_toggle_unknown_or_ambiguous_prefix_refuses_without_mutation`,
  `check_add_refuses_empty_and_control_char_text_without_mutation`,
  `list_task_prints_checklist_lines_with_state_and_short_id`.
- *Advances:* AC-15, AC-16, AC-20. *Component:* Checklist CLI. *Deps:* T-1.

<!-- source: mid-build amendment (terminology rename, user-directed 2026-08-22) · ingested 2026-08-22 -->

**Terminology amendment:** the product vocabulary renames **checklist → steps**
and **checklist item / item → step**, everywhere user-visible or script-visible:
the section heading (`steps done/total`), the CLI family (`herdr-tasks steps
<task> add|toggle`), refusal tokens (`empty-step-text`, `invalid-step-text`,
`unknown-step`, `ambiguous-step`), the `--json` key (`"steps"`), the glossary,
SKILL.md, and the CHANGELOG. Grammar: plural names surfaces and collections;
singular names the thing a verb acts on and its attributes (step cursor, step
short id). Acceptance-criteria wording above retains its original
"checklist"/"item" terms as process history — the meaning is unchanged, only the
product vocabulary moves. Design component names likewise stay as written. Old
stores written during live testing must keep decoding via serde aliases on the
field and the renamed event variants. No alias subcommand: nothing is released.

**T-9 — Full terminology rename to steps.** Rename the product vocabulary
checklist/item → steps/step across domain, UI, CLI, docs, and tests, with serde
aliases preserving decode of stores written under the old names.

- Files: the full checklist-bearing surface — `src/domain/task.rs`,
  `src/domain/events.rs`, `src/ui/` (board model/apply/draw/chrome, input,
  render, mouse), `src/cli/check.rs` (becomes `src/cli/steps.rs`),
  `src/cli/{parser,router,list,presenter,mod,add}.rs`, `src/main.rs`,
  `skills/herdr-tasks-cli/SKILL.md`, `CHANGELOG.md`, `CONTEXT.md` (committed
  hunks only), and the test files carrying the vocabulary.
- Test first: `steps_alias_decodes_pre_rename_store_events_and_field`,
  `steps_cli_add_then_toggle_round_trips_step_state`, plus the renamed
  equivalents of the existing proof tests (see brief).
- *Advances:* AC-1 through AC-25 (re-delivers every criterion's surface under
  the final vocabulary).
- *Component:* Checklist domain model. *Deps:* T-6, T-7, T-8.

**T-6 — Halved content layout with top-anchored items.** With at least one item,
split the region between the page header and footer into a notes half (top) and a
checklist half (bottom) separated by the section divider; items paint from the top
of their half, each new item directly below the previous (replacing the
footer-anchored block). The notes-edit floor reservation still holds at the
compact floor.

- Files: `src/ui/render.rs`, `src/ui/board/draw.rs`, `src/ui/board/model.rs`,
  `tests/queue_board_render.rs`.
- Test first: `content_splits_in_half_once_the_first_item_lands`,
  `items_stack_from_the_top_below_the_divider`.
- *Advances:* AC-24. *Component:* Task page renderer. *Deps:* T-2.

**T-7 — Footer item input, verb indicator, delete hint.** Move the item add/rename
line to the page footer slot as a one-line input reusing the quick-add line
pattern (semantics unchanged: Enter save-close, Ctrl+Enter add-reopen /
rename-close, Esc cancel; the line owns its refusals and clears them on close).
List the item-add verb in the footer verb bar when a checklist is present. Show a
footer "press Alt+x again to remove" message while a delete mark is set; clear it
when the mark clears.

- Files: `src/ui/render.rs`, `src/ui/board/draw.rs`, `src/ui/board/model.rs`,
  `src/ui/board/apply.rs`, `src/ui/board/chrome.rs`, `src/app.rs`,
  `tests/queue_board_verbs.rs`, `tests/queue_board_render.rs`.
- Test first: `item_input_uses_the_footer_quick_add_line`,
  `footer_lists_the_item_add_verb`,
  `delete_mark_shows_press_again_footer_message`.
- *Advances:* AC-22, AC-23, AC-25. *Component:* Item editor surface. *Deps:* T-4,
  T-6.

**T-8 — Cursor re-activation and click-to-select.** Make ↓ re-activate the item
cursor after a deactivation (↑ from the first item still deactivates; while
inactive ↑ scrolls notes, ↓ re-activates). A single mouse click on an item row
moves the cursor to that item (select, never toggle).

- Files: `src/ui/board/apply.rs`, `src/ui/mouse.rs`, `src/ui/board/model.rs`,
  `tests/queue_board_verbs.rs`, `tests/queue_board_mouse.rs`.
- Test first: `down_reactivates_the_cursor_after_deactivation`,
  `clicking_an_item_row_selects_it`.
- *Advances:* AC-17, AC-18, AC-21. *Component:* Page key map. *Deps:* T-3, T-6.

**T-10 — Keyboard cursor priority, wheel-only stream scroll, and Notes caret offset.**
Keyboard arrows retain their cursor-first step navigation: active ↑ moves or
deactivates the cursor before the shared stream can move, and ↓ activates or
advances it. Wheel events remain stream-only. Offset the Notes edit caret by the
shared stream scroll so it lands on the draft row actually painted.

- Files: `src/ui/board/apply.rs`, `src/ui/render.rs`, `tests/queue_board_verbs.rs`,
  `tests/queue_board_render.rs`.
- Test first: `active_cursor_up_precedes_shared_scroll`,
  `wheel_scroll_never_moves_step_cursor`, and
  `notes_edit_caret_accounts_for_shared_stream_scroll`.
- *Advances:* AC-17, AC-18, AC-24, AC-26, AC-27. *Components:* Page key map,
  Task page view model, Task page renderer. *Deps:* T-9.

### Task-to-criterion coverage map

| AC | Advanced by |
| --- | --- |
| AC-1 | T-1, T-9 |
| AC-2 | T-1, T-9 |
| AC-3 | T-1, T-9 |
| AC-4 | T-1, T-9 |
| AC-5 | T-2, T-9 |
| AC-6 | T-2, T-9 |
| AC-7 | T-2, T-9 |
| AC-8 | T-3, T-9 |
| AC-9 | T-3, T-9 |
| AC-10 | T-3, T-9 |
| AC-11 | T-3, T-9 |
| AC-12 | T-4, T-9 |
| AC-13 | T-4, T-9 |
| AC-14 | T-4, T-9 |
| AC-15 | T-5, T-9 |
| AC-16 | T-5, T-9 |
| AC-17 | T-3, T-8, T-9 |
| AC-18 | T-3, T-8, T-9 |
| AC-19 | T-3, T-9 |
| AC-20 | T-5, T-9 |
| AC-21 | T-8, T-9 |
| AC-22 | T-7, T-9 |
| AC-23 | T-7, T-9 |
| AC-24 | T-6, T-9, T-10 |
| AC-25 | T-7, T-9 |
| AC-26 | T-10 |
| AC-27 | T-10 |

### Notes

- Green bar between tasks: `cargo fmt --check && cargo clippy --all-targets --
  -D warnings && cargo test && cargo build --release`.
- Every task's first test run must be observed red against pre-change behavior
  (repo rule: a regression test must fail without its fix).
- This feature touches the task page and host surfaces: live herdr smoke applies
  when `HERDR_ENV=1`; otherwise state that it was not run.
- Build briefs land in `.agent-sdlc/briefs/checklists/`; the directory is created
  for this run (`.agent-sdlc/` is untracked local process state).
