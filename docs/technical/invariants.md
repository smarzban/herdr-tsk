# Invariants

Rules a maintainer must not break. Each rule names why it holds, what enforces it, and
what fails if it is violated. Subsystem pages restate the subset they own.

## Domain

1. **Human status is the only progress authority.** `HumanStatus` on `Task` is what the
   board and CLI show. `last_observed`, capsule, and agent meta never drive `set_status`,
   `complete`, or `reopen`. Completing every step never completes the task
   (`DomainState::toggle_step` journals `StepChecked` / `StepUnchecked` only).
   *Breaks:* an agent observation or a full checklist silently marks work done.

2. **Create and edit refuse empty titles.** Title is trimmed; whitespace-only is
   `DomainError::EmptyTitle` and adds no task. CLI additionally refuses any C0 control
   (`invalid-title`) *before* trim, including a control-only title.
   *Enforced by:* `DomainState::create` / `edit`; `cli::add::has_c0_control`.

3. **Soft-delete keeps the row in the store.** `get` still finds it. Views and `list`
   (without `--deleted`) hide it. Undo of soft-delete is `restore`.
   *Breaks:* a hard delete that drops history, or a view that paints deleted rows as live.

4. **Every semantic mutation refreshes `revision` and appends history.** `record_mutation`
   stamps a new v4 revision, `updated_at`, and a `TaskEvent`. Create is the one path that
   writes `Created` without going through `record_mutation` (it still assigns a revision).
   *Breaks:* two writers can no longer revision-guard the same task.

5. **`merge_base_revision` is save intent, never durable.** It is set on mutation, used
   under the store lock, and cleared after a successful replace (`clear_merge_bases`).
   Serde keeps the field so an in-memory clone can round-trip during a failed save; a
   successful write must not retain it.
   *Enforced by:* `TaskStore::save` / `reload_merge_save` / `locked_transition*`.

6. **Legacy missing revision is a nil UUID base, not a real revision.**
   `LEGACY_MERGE_BASE_REVISION = Uuid::nil()`. UUID v4 revisions never equal nil. A
   first mutation of a pre-revision task is accepted against a disk copy that still has
   `revision: null`.
   *Breaks:* first edit of an old task spuriously conflicts, or a nil revision is
   persisted as if it were assigned.

7. **Undo is LIFO, revision-guarded, and a no-op on an empty stack.** Only `complete` and
   `soft_delete` push. A stale or legacy (no `expected_revision`) entry is *retained* and
   returns `StaleUndo` — a refused undo must not expose an older entry.
   *Enforced by:* `DomainState::undo` in `src/domain/undo.rs`.

8. **A task owns at most one active dispatch attempt.** `start_dispatch_attempt` refuses
   `ActiveDispatchAttempt`. Completed / fully-cleaned attempts are removed from
   `DomainState`, not kept as history. Disk owns attempt lifecycle under the lock: a
   stale in-memory copy must not recreate an attempt disk already dropped
   (`merge_attempts_for_save` retains only ids still on disk).

9. **Steps are a flat ordered list.** One level, stable ids, no reordering by a verb.
   Old stores name the field `checklist`; new writes use `steps` only. Notes markdown
   `- [ ]` stays literal text (ADR-0001).

10. **A thread is a normalized name, not an entity.** No thread id, status, or lifecycle.
    Identity is the name. The field persists on the task regardless of status (ADR-0002).
    **Threads tab** groups any non-done, non-deleted task that carries the name
    (including `started`). **Scoped ON DECK headers** group *open* tasks only
    (ready / blocked / review): a thread whose remaining work is all started has
    no ON DECK header (those rows live in IN MOTION).
    `normalize_thread`: ASCII alphanumerics and hyphens, first char alphanumeric, ≤32
    chars, stored lowercase.

11. **`TaskScope::Global` serializes as `"global"` and displays as desk.** Do not rename
    either without a store migration.

12. **`STORE_FORMAT_VERSION` is 1.** Missing `format_version` loads as
    `LEGACY_STORE_FORMAT_VERSION` (also 1). That legacy constant **must stay 1** when
    the current constant is later bumped, so old files still decode. Newer documents
    are refused, not rewritten (`StoreError::UnsupportedFormat`).

## Persistence

13. **One board directory.** `TSK_STATE_DIR` else `$HOME/.tsk` else relative `.tsk-state`.
    Empty env values fall through. `HERDR_PLUGIN_STATE_DIR` is ignored. Config mirrors
    this with `TSK_CONFIG_DIR` / `.tsk-config`. Never fall back to `std::env::temp_dir()`.

14. **Exclusive lock on `tsk.json.lock` covers load-modify-save.** `File::lock` / unlock
    on drop. Orphan `.tsk.json.tmp.*` sweep runs only under that lock.

15. **Atomic replace is temp + fsync + rename + directory fsync.** Unique temp
    (`.tsk.json.tmp.{pid}.{nanos}`). Success retains the previous live file as
    `tsk.json.1` via hard-link *before* replace. A corrupt live JSON must not overwrite
    that backup.

16. **Idle merge is disk-wins on revision mismatch, and must not move the user's place.**
    `merge_tasks_from_disk` replaces a task when revisions differ (no wall-clock).
    `BoardModel::sync_from_domain` never changes the home tab or selection for tasks
    merged in from disk, except an otherwise-empty view surfacing the first arriving
    task. A save this surface just made pins selection only when the current lens
    paints that task; otherwise it reanchors near the saved task's old position.

17. **Config writes are not the store.** Walkthrough and settings copy the temp+rename
    shape but **do not** lock or fsync the directory. A crash can lose a just-recorded
    dismissal (walkthrough reappears once). The walkthrough payload is a constant, so
    two writers producing the same bytes are harmless. Do not reuse this code for a
    varying payload.

## Board session

18. **Selection rests only on a row the current lens paints.** Collapse counts as
    invisibility. `seed_selection` and reanchor fallbacks must never pin a collapsed
    or out-of-lens task. Thread headers consume row budget but are not selectable or
    hit-testable (ADR-0003). `task_ids` in a deck section is the concatenation of
    thread blocks then loose unthreaded ids, in paint order.

19. **A form and its input mode outlive the save call.** Never clear the form or switch
    mode before the persistence boundary confirms. A cancelled failed save otherwise
    leaves an edit mode with no form, which no key can escape.

20. **Save-recovery keys reach Retry/Cancel even if a form is allocated.**
    `board_keyboard_intent` is an *allowlist* of form-field modes. `SaveRecovery`,
    palette, help, and other surfaces that outrank the form must not be swallowed by
    `map_board_form_key`. `r` / `c` / Esc are the recovery chords.

21. **Anything painted on the status-row slot hides `status_message` while it is up.**
    Quick-add owns its refusals and clears them on close. Otherwise the message is
    invisible during the overlay and leaks onto the board afterwards.

22. **Mutating verbs need Ctrl** (`VerbModifier::Ctrl`). Bare `space` `d` `o` `b`
    `e` `n` `x` `u` `q` do nothing. Nav, peek, `Enter`, `P`, `1`/`2`/`3`, `z`, `:`
    `?`, `+`, `Esc` stay bare. Legacy `alt` settings deserialize as Ctrl.

23. **Quick-add is the only board create path.** There is no inline board capture form.
    Success has no status message; the row flash is the feedback. `!p` / `!t` tokens
    are stripped from the saved title.

24. **Task page is view-first.** Only modifier `e` / `n` / Tab enter edit, except a
    quick-add draft expanded with `Tab`, which opens straight into Notes edit. The
    scope footer is inert until an edit has started. Field regions are inert to click
    in `TaskPage` until an edit state is already open.

25. **Text wraps, never truncates.** One engine (`ui::edit::wrap_text`, word-boundary,
    display-cell measured) feeds notes, task-page titles, list rows, quick-add, capture
    Notes, and peek. Task-page notes wrap at `row_width - 6`. The capped task-page
    header and the verb bar's tier-budget ellipsis are chrome limits, not task text.
    `escaped_draft_rows` remains only for single-line Title/Thread editors.

26. **One definition of a line break:** `\r\n`, lone `\n`, or lone `\r`
    (`ui::split_line_breaks`). Presenters, paste flattening, and line movement must
    agree. A CRLF paste is stored verbatim; a presenter that only knew `\n` would
    leave `\u{000d}` on every line.

27. **Control characters never paint as controls.** `ui::terminal_text` turns C0/C1
    into `\u{00xx}` escapes. Notes markdown is view/peek only; edit is raw source.
    Peek paints the same markers then dims every span.

28. **Mono modifiers only.** `ui::render::MONO_MODIFIERS`. No color theme module.
    Tests assert buffers have no SGR color.

29. **Frame loop is settle → paint → wait.** `board_frame` owns that order so an idle
    poll cannot skip settle. Idle merge is skipped while save recovery is pending
    (reloading disk would replace the working snapshot being retried). A failed
    `store.load` during watch must *not* advance `StoreWatch::last_seen`.

30. **Walkthrough dismissal is presented, never propagated.** A write failure stays on
    the chrome row; the board stays open. Unreadable `walkthrough.json` reads as not
    dismissed (fail toward showing the card, never toward keeping the board shut).

## CLI

31. **Closed global-flag set.** Only `--find-board-pane` and `--help`, and only before
    the first positional. Anything else before a subcommand is usage (exit 2).

32. **Exit contract.** `0` success (add: every item created or already existed). `1`
    item refusal (retry only the failed subset). `2` usage/parse (nothing persisted).
    `3` store I/O (commit indeterminate; verify with `list` before retry). Flag-add
    idempotency key is trimmed title + resolved scope + normalized thread among
    non-soft-deleted tasks.

33. **`--state-dir` is the test/ops override** for a single invocation; it does not
    change the process-wide default used by the TUI unless `TSK_STATE_DIR` is set.

## Plugin

34. **Pane identity is the label `tsk`, exact.** `board_pane::BOARD_PANE_LABEL` must
    equal `herdr-plugin.toml` `[[panes]] title`. Terminal title is not identity (a
    standalone `tsk` in some other pane can show the same title). Pane ids passed to
    `plugin pane focus` must be flag-safe: non-empty, not start with `-`,
    `[A-Za-z0-9_.:-]+`.
