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

## Task ledger

| Task | Commit | ACs | Notes |
| --- | --- | --- | --- |
| T1 migration hook | (pending) | AC-1..AC-4 | chain empty at v1; seams `load_unlocked_supported` / `save_unlocked_supported` |
| T2 store signature | | AC-5, AC-6 | |
| T3 undo cap + prune | | AC-7..AC-9 | |
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
