# Store hardening build report

Task ledger for spec A (store hardening), branch `feat/store-hardening`.
Every regression test was watched failing with its fix hunk reverted by hand
(never `git checkout`), then restored. Baseline before T1: 727 tests passing.

## Spec interpretation notes

- **T4 trash eligibility (contradiction in the spec, resolved in favour of the
  ACs).** The model paragraph says a soft-deleted task moves to trash when "no
  undo entry on the stack targets it", but AC-10 requires the task in
  `trash.jsonl` after completing another task, while its SoftDelete entry is
  still on the stack. AC-10's own parenthetical names the intended trigger:
  "so the delete is no longer the top undo entry". The ACs are the contract,
  so the implemented rule is: a soft-deleted task stays live only while the
  **top** undo entry is its SoftDelete entry (or until `deleted_at` is more
  than 7 days old, which trashes it regardless). With the literal "no entry
  anywhere targets it" rule, AC-10 is unpassable.
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

## Task ledger

| Task | Commit | ACs | Notes |
| --- | --- | --- | --- |
| T1 migration hook | f8340d8 | AC-1..AC-4 | chain empty at v1; seams `load_unlocked_supported` / `save_unlocked_supported` |
| T2 store signature | 71348e4 | AC-5, AC-6 | |
| T3 undo cap + prune | (pending) | AC-7..AC-9 | prune lives in task.rs (needs the private fields); UNDO_CAP in undo.rs |
| T4 trash file | | AC-10..AC-16 | |
| T5 docs | | docs items | |

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
