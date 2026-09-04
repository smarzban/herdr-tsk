# Store hardening verification report

Maps every AC in `store-hardening.md` to named tests. Unit tests live in
`src/store.rs`, `src/domain/task.rs`, `src/domain/undo.rs`, `src/app.rs`;
integration tests in `tests/`.

| AC | Requirement | Test |
| --- | --- | --- |
| AC-1 | v1 loads/saves exactly as before, byte-identical | `store::tests::v1_document_round_trips_byte_identical_for_an_unchanged_state` (plus the pre-existing format/save tests that stay green) |
| AC-2 | injected v1→v2 step loads migrated; first save leaves `tsk.json.v1` byte-identical, live at v2; second save leaves it alone | `store::tests::migrate_with_walks_the_chain_from_the_given_version`, `store::tests::migrated_load_backs_up_the_original_before_the_first_higher_version_save`, `store::tests::save_never_overwrites_an_existing_version_backup` |
| AC-3 | higher version refused, no state-dir change (listing + bytes) | `store::tests::load_refuses_a_higher_version_and_changes_nothing_in_the_state_dir` |
| AC-4 | failing migration step surfaces from load, no file changes | `store::tests::failed_migration_step_surfaces_from_load_without_file_changes` |
| AC-5 | equal mtime + equal length saves produce different signatures on Unix | `store::tests::signature_distinguishes_same_mtime_same_length_replaces`, `app::idle_store_revalidation_tests::idle_tick_sees_a_replace_that_keeps_mtime_and_length` |
| AC-6 | missing file → None; unchanged file → equal signature twice | `store::tests::signature_missing_file_is_none_and_unchanged_file_is_stable` |
| AC-7 | 51 undoable actions + save leaves exactly 50 | `domain::undo::tests::save_keeps_exactly_fifty_undo_entries_and_evicts_the_oldest`, `domain::undo::tests::cap_runs_after_merge_undo_entries_union` |
| AC-8 | stale entry pruned, live entry beneath kept | `domain::undo::tests::save_drops_stale_undo_entries_and_keeps_live_ones_beneath_them`, `domain::undo::tests::prune_runs_before_the_cap_so_a_dead_entry_never_evicts_a_live_one`, `domain::undo::tests::stale_top_entry_still_refuses_in_memory_because_pruning_runs_only_at_save` |
| AC-9 | existing undo tests stay green | `domain::undo::tests::soft_delete_then_undo_clears_soft_deleted`, `domain::undo::tests::complete_then_undo_returns_status_ready`, `domain::undo::tests::undo_with_empty_stack_is_noop` (unchanged, green in full `cargo test`) |
| AC-10 | delete + later complete + save → trash with right `deleted_at` | `store::tests::soft_delete_moves_to_trash_once_a_later_undoable_action_is_on_top`, `cli_trash::trash_restore_round_trip_refusals_and_events` (trashing step) |
| AC-11 | delete + immediate save stays live, undo restores | `store::tests::fresh_soft_delete_stays_live_while_the_top_undo_entry_restores_it` |
| AC-12 | 8-day-old delete trashed even as top entry | `store::tests::eight_day_old_soft_delete_moves_to_trash_even_as_top_undo_entry` |
| AC-13 | 31-day line purged on next trash write, 29-day stays, malformed dropped/skipped | `store::tests::trash_rewrite_purges_expired_lines_and_drops_malformed_ones`, `cli_trash::list_deleted_skips_malformed_trash_lines` |
| AC-14 | process A does not resurrect a task B trashed | `trash_store::reload_merge_save_does_not_resurrect_a_task_another_process_trashed`, `trash_store::merge_tasks_from_disk_drops_a_locally_held_task_that_was_trashed_elsewhere` |
| AC-15 | `list --deleted` shows trash; `trash restore` round-trip; refusals | `cli_trash::list_deleted_shows_a_trashed_task_with_its_number`, `cli_trash::trash_restore_round_trip_refusals_and_events`, `cli_trash::restore_accepts_the_bare_number_and_the_uuid`, `cli_trash::restore_refuses_when_the_task_is_already_live`, `cli::router::tests::trash_positional_selects_trash_surface`, `cli::parser::tests::trash_parse_accepts_restore_with_number_and_flags` |
| AC-16 | trash sync failure leaves `tsk.json` unchanged, task live | `store::tests::trash_sync_failure_leaves_the_live_document_untouched` |

## T5 docs items

| Item | Where |
| --- | --- |
| `~/.tsk` must be on a local disk; flock-style lock + rename replace; NFS/Dropbox/iCloud break both; `TSK_STATE_DIR` escape hatch | `README.md` (store paragraph), `site/src/content/docs/docs/install.md` (Standalone) |
| `trash.jsonl` beside `tsk.json`, `tsk.json.1`, `tsk.json.v<N>` | `README.md`, `site/src/content/docs/docs/install.md` |
| Windows not tested in CI | `README.md`, `site/src/content/docs/docs/install.md` (Requirements) |
| `tsk trash restore T<n>` | `site/src/content/docs/docs/cli.md` (trash section, commands table, exit contract), `skills/tsk-cli/SKILL.md` (Trash) |
| `tsk list --deleted` includes trash for 30 days | `site/src/content/docs/docs/cli.md` (list), `skills/tsk-cli/SKILL.md` (listing filters) |
| One CHANGELOG entry per user-visible item (trash, restore, undo cap, synced-folder note) | `CHANGELOG.md` Unreleased |

## Review fixes (K1..K6)

| Finding | Fix test |
| --- | --- |
| K1 restore ordering | `store::tests::failed_restore_live_save_keeps_the_trash_line_for_retry` |
| K2 torn non-UTF-8 tail | `store::tests::load_trash_skips_a_torn_non_utf8_tail` |
| K3 dedupe by id | `store::tests::trash_lines_dedupe_by_id_with_the_last_line_winning`, `cli_trash::list_deleted_dedupes_duplicate_trash_lines` |
| K4 torn-tail glue (no O_APPEND) | `store::tests::trash_rewrite_drops_torn_tails_instead_of_gluing_new_lines` (AC-16 `store::tests::trash_sync_failure_leaves_the_live_document_untouched` re-verified meaningful) |
| K5 future deleted_at | `store::tests::trash_rewrite_keeps_lines_with_a_future_deleted_at` |
| K6 local state after trash | `store::tests::reload_merge_save_drops_trashed_tasks_from_the_callers_state` |
| V1 (verify regression) trash-rewrite failure after a durable restore reported as failure, retry said "not in trash" | `store::tests::failed_trash_rewrite_after_a_durable_restore_still_reports_success` |

## Done-means

| Item | Status |
| --- | --- |
| Green bar in this worktree | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release` green; 763 tests after review fixes (baseline 727) |
| `cd site && npm test` | 10/10 pass |
| Every AC maps to a named test | table above |
| Every regression test watched failing without its fix | `build-report.md`, per-task revert notes |
| Live smoke under HERDR_ENV=1 | `build-report.md` Live smoke section |
