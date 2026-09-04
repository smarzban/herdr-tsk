# Store hardening build report

Task ledger for spec A (store hardening), branch `feat/store-hardening`.
Every regression test was watched failing with its fix hunk reverted by hand
(never `git checkout`), then restored. Baseline before T1: 727 tests passing.

## Spec interpretation notes

- **T4 trash eligibility (contradiction in the spec, resolved in favour of the
  ACs).** The model paragraph says a soft-deleted task moves to trash when "no
  undo entry on the stack targets it", but AC-10 requires the task in
  `trash.jsonl` after completing another task, while its SoftDelete entry is
  still on the stack. No stack-shape rule satisfies both, and the naive
  "top entry" reading also breaks an existing green concurrent-undo test; the
  final rule (below, T4 notes) uses entry timestamps and satisfies every AC
  and the existing guarantee.
- **T1 `migrate` stamps `format_version` centrally.** Each applied step's
  output version is stamped by `migrate_with` (index `i` produces `i + 2`),
  so injected test steps only transform shape.
- **T1 save accepts a lower live version.** `save` refuses live versions 0
  and above-supported exactly as today; a live version in `(0, supported)`
  is backed up to `tsk.json.v<N>` then replaced (the write-after-migrated-load
  path). At v1 that range is empty, so no current behaviour changes.
- **T4 restore uses a typed lock scope, not `locked_transition`.** The spec
  says restore runs "under `locked_transition`", but `locked_transition`
  flattens errors to `String` and the CLI exit-code contract (1 refusal vs
  3 store I/O) needs a typed error. `restore_from_trash` holds the same
  exclusive lock across load, trash rewrite, state mutation, and replace.
- **T3 prune placement.** `prune_undo_for_persistence` lives in
  `src/domain/task.rs` (it needs the private `tasks`/`undo_stack` fields,
  which undo.rs, a sibling module, cannot touch); `UNDO_CAP` lives in
  `src/domain/undo.rs`, as the spec's file list says. The prune runs inside
  `save_unlocked_supported`, the single funnel every save path and
  `locked_transition` passes through, after number assignment at the callers
  and after `merge_for_save`'s `merge_undo_entries` union, so the cap always
  runs after the union.
- **T4 trash eligibility (final rule).** The model paragraph ("no undo entry
  on the stack targets it") and AC-10 ("complete another task" puts the
  delete in trash while its SoftDelete entry is still on the stack) cannot
  both hold under any stack-shape rule, and the naive "top entry" reading
  also breaks the existing green test
  `reload_merge_save_keeps_distinct_concurrent_undo_entries`, whose stack
  shape is identical to AC-10's but whose above entry is an EARLIER
  concurrent completion that must stay undoable. The implemented rule keeps
  every AC and that guarantee green: a soft-deleted task moves to trash when
  (a) its `deleted_at` is more than 7 days old, or (b) no live undo entry
  targets it, or (c) a LATER undoable action exists: some live entry's
  recorded action (its target task's matching history event; live entries
  match exactly after the boundary prune) is timestamped after this task's
  delete. An earlier or concurrent action does not finalize the delete.
- **T4 list --deleted order.** The spec orders the deleted view by
  `deleted_at` descending; two existing cli_list tests that pinned the old
  status-rank order were updated to the new order (their setup deletes in a
  known sequence). Row format is unchanged; trash entries render identically
  and keep their numbers.
- **T4 test seam.** `RecordingFilesystem` labels files by path (live vs
  trash), adding `TrashWrite`/`TrashFileSync` stages so AC-16 can fail the
  trash sync without failing the live sync. A plain `FileSync` injection
  cannot distinguish the two, and a revert proof showed it would pass for
  the wrong reason. Review K1 added `RealFilesystemWithFailure` (real file
  operations with one injectable stage) because the recording seam performs
  no disk writes, and K4 removed `open_append` from the seam entirely.
- **T4 restore ordering (revised by review K1).** The first implementation
  followed the spec order (rewrite trash, then insert, then replace live),
  which loses the task from both files when the live save fails. Review K1
  reversed it: insert, replace live, then rewrite trash without the line. A
  crash between the live replace and the trash rewrite now leaves the task in
  both places (readers dedupe by id, live wins) instead of in neither; the
  rewrite failure is also cleaned by the next trash write, which drops lines
  whose id is live and not soft-deleted.
- **T5 site build.** `cd site && npm test` passes (10/10). `npm run build`
  was not run locally: `site/node_modules` is not installed in this worktree
  and the spec's Done-means names `npm test`; CI runs the full
  `npm ci && npm test && npm run build`.

## Task ledger

| Task | Commit | ACs | Notes |
| --- | --- | --- | --- |
| T1 migration hook | f8340d8 | AC-1..AC-4 | chain empty at v1; seams `load_unlocked_supported` / `save_unlocked_supported` |
| T2 store signature | 71348e4 | AC-5, AC-6 | |
| T3 undo cap + prune | e230894 | AC-7..AC-9 | prune lives in task.rs (needs the private fields); UNDO_CAP in undo.rs |
| T4 trash file | 6303bd1 | AC-10..AC-16 | see the eligibility note; two existing cli_list --deleted expectations updated to the spec's new order |
| T5 docs | c2d5de8 | docs items | README, install.md, cli.md, SKILL.md, CHANGELOG; site npm test green |
| live smoke | (this commit) | Done-means | herdr pane beside the agent; see below |

## Review fixes

All six kept findings fixed in this worktree, one commit each, each with a
regression test watched failing against the hand-reverted fix. Durability rule
held throughout: the trash is durable before the live document loses a task.

| K | Commit | Fix | Revert proof |
| --- | --- | --- | --- |
| K1 restore ordering | 3ee0098 | insert, save live, then rewrite trash; `restore_from_trash_with` seam | revert put the trash rewrite before the live save; with `RealFilesystemWithFailure` failing only `FileSync` the rewrite genuinely landed and the line was lost: `failed_restore_live_save_keeps_the_trash_line_for_retry` failed; restored |
| K2 torn non-UTF-8 tail | 1731052 | `load_trash_unlocked` reads bytes, splits on `b'\n'`, per-line UTF-8 then JSON, skips failures (`parse_trash_lines`) | reverted to `fs::read_to_string`: `load_trash_skips_a_torn_non_utf8_tail` failed (load errors instead of skipping); restored |
| K3 dedupe by id | 362d247 | `parse_trash_lines` dedupes by task id, last line wins; rewrite re-serializes the deduped entries via `write_trash_atomic` | removed the dedupe: `trash_lines_dedupe_by_id_with_the_last_line_winning` and `cli_trash::list_deleted_dedupes_duplicate_trash_lines` failed; restored |
| K4 torn-tail glue | b4bf6cb | dropped O_APPEND; trash is read tolerantly + deduped, purge-expired and live-restored lines dropped, eligible added (skipping present ids), whole file written via temp+sync+rename+dir-sync, then tasks removed, then live replaced; docs/comments updated away from "append-only" | restored the O_APPEND append-then-rewrite flow: `trash_rewrite_drops_torn_tails_instead_of_gluing_new_lines` failed (the torn tail glued the new line, purge deleted it); restored. AC-16 (`TrashFileSync` injection) still leaves tsk.json unchanged |
| K5 future deleted_at | 464e9d6 | purge keeps a line when `duration_since` errs (clock step-back) | reverted to `is_ok_and(age <= cap)`: `trash_rewrite_keeps_lines_with_a_future_deleted_at` failed (future line purged); restored |
| V1 verify regression | (this commit) | `restore_from_trash_with` treats a failed trailing trash rewrite as deferred cleanup once the live document is durable; readers hide the stale line, the next trash rewrite drops it | fix written after the test: `failed_trash_rewrite_after_a_durable_restore_still_reports_success` failed on the `?` version, passed once the error is dropped |
| K6 local state after trash | 855e6dd | `reload_merge_save_with` removes from `local` every task `durable` no longer holds, plus their undo entries | removed the propagation: `reload_merge_save_drops_trashed_tasks_from_the_callers_state` failed (local kept the task and its entry); restored |

Note: `trash_append_purges_expired_lines_and_drops_malformed_ones` was renamed
to `trash_rewrite_purges_expired_lines_and_drops_malformed_ones` in K4.

## Regression proofs (fix hunk reverted by hand, test watched failing, fix restored)

### T1

- Revert A: replaced the `found < supported` migration branch in
  `load_unlocked_supported` with a strict `found != supported` refusal.
  Failed without the fix: `migrated_load_backs_up_the_original_before_the_first_higher_version_save`,
  `save_never_overwrites_an_existing_version_backup`,
  `failed_migration_step_surfaces_from_load_without_file_changes`. Restored.
- Revert B: removed the `retain_version_backup` call from
  `save_unlocked_supported`. Failed without the fix:
  `migrated_load_backs_up_the_original_before_the_first_higher_version_save`
  (panics reading `tsk.json.v1`). Restored.

### T2

- Revert: `state_signature` zeroed `dev`/`ino` (old `(mtime, len)` identity).
  Failed without the fix:
  `store::tests::signature_distinguishes_same_mtime_same_length_replaces`
  and the product-level
  `app::idle_store_revalidation_tests::idle_tick_sees_a_replace_that_keeps_mtime_and_length`
  (idle tick misses the replace). Restored.

### T3

- Revert A: removed the `state.prune_undo_for_persistence()` call from
  `save_unlocked_supported` (the single funnel every save path and
  `locked_transition` passes through). Failed without the fix:
  `save_keeps_exactly_fifty_undo_entries_and_evicts_the_oldest`,
  `save_drops_stale_undo_entries_and_keeps_live_ones_beneath_them`,
  `prune_runs_before_the_cap_so_a_dead_entry_never_evicts_a_live_one`,
  `cap_runs_after_merge_undo_entries_union`. Restored.
- Revert B: swapped the order inside `prune_undo_for_persistence` so the cap
  evicted before the stale prune. Failed without the fix:
  `prune_runs_before_the_cap_so_a_dead_entry_never_evicts_a_live_one`
  (a cap-first prune evicts the live oldest entry to keep a dead one).
  Restored.

### T4

- Revert R1: removed `self.move_eligible_to_trash(state, filesystem)?` from
  `save_unlocked_supported`. Failed without the fix:
  `soft_delete_moves_to_trash_once_a_later_undoable_action_is_on_top`,
  `eight_day_old_soft_delete_moves_to_trash_even_as_top_undo_entry`,
  `trash_rewrite_purges_expired_lines_and_drops_malformed_ones`, and the
  cli_trash round-trip/list tests. Restored.
- Revert R2: disabled `drop_tasks_trashed_elsewhere` in both
  `merge_tasks_from_disk` and `merge_for_save`. Failed without the fix:
  `reload_merge_save_does_not_resurrect_a_task_another_process_trashed` and
  `merge_tasks_from_disk_drops_a_locally_held_task_that_was_trashed_elsewhere`.
  Restored.
- Revert R3: swallowed the trash-move error (`let _ =`) so a failed trash
  sync no longer aborts the save before the live replace. Failed without the
  fix: `trash_sync_failure_leaves_the_live_document_untouched` (the save
  succeeds and the task leaves the live document). Restored. This proof
  required the path-aware `RecordingFilesystem` stages: with a shared
  `FileSync` stage the injected failure hits the live sync too and the test
  passes for the wrong reason (verified).
- Revert R4: removed the live-duplicate refusal in `restore_from_trash`.
  Failed without the fix: `restore_refuses_when_the_task_is_already_live`
  (a crash-duplicated task restores a second copy instead of refusing).
  Restored.

## Live smoke (HERDR_ENV=1)

Pane `w1E:p2`, split right of the agent pane via `herdr pane split`, running
`env TSK_STATE_DIR=/tmp/tsk-store-hardening ./target/release/tsk` from the
worktree. State dir was emptied before the run; `~/.tsk` untouched.

1. Quick-add (`+`, title, `Enter`) three tasks: T1 "smoke task one", T2
   "smoke task two", T3 "smoke task three"; each painted under the
   `feat-store-hardening` project group and became the selection.
2. `ctrl+x` soft-deleted T1: the row left the board and the status row read
   `Deleted "smoke task one" · u Undo`. (The selection had landed on T1 after
   the add sequence, so the first `up` + `ctrl+x` hit T1 rather than T2; the
   flow under test is unchanged.)
3. `ctrl+d` completed T2: the open count dropped to 1 and the footer read
   `1 done`.
4. `ctrl+q` quit. On disk: `trash.jsonl` had exactly **one line** — T1, with
   `deleted_at` `[1788523448, 940568000]` matching its `soft_deleted` history
   event — beside `tsk.json`, `tsk.json.1`, and `tsk.json.lock`.
5. `tsk list --deleted` printed `DELETED / - 1 smoke task one` (exit 0);
   `tsk list` showed only T3.
6. `tsk trash restore T1` printed `restored T1 smoke task one`, exit 0;
   `trash.jsonl` went to 0 lines; `tsk list` showed T1 and T3 ready.
7. Reopened the board in the same pane: T1 painted beside T3 under the
   project group, `1 done` in the footer (`tsk list --done` shows T2).
   `ctrl+q` quit cleanly.
