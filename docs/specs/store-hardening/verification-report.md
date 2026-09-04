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
| AC-10 | delete + later complete + save → trash with right `deleted_at` | (T4 pending) |
| AC-11 | delete + immediate save stays live, undo restores | (T4 pending) |
| AC-12 | 8-day-old delete trashed even as top entry | (T4 pending) |
| AC-13 | 31-day line purged on next append, 29-day stays, malformed dropped/skipped | (T4 pending) |
| AC-14 | process A does not resurrect a task B trashed | (T4 pending) |
| AC-15 | `list --deleted` shows trash; `trash restore` round-trip; refusals | (T4 pending) |
| AC-16 | trash sync failure leaves `tsk.json` unchanged, task live | (T4 pending) |
