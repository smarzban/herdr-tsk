# Verification report — task-numbers

Pre-PR AC → proof map. Proof identifiers are taken from `build-report.md`
evidence blocks. Checker corroboration runs after this file is written.

| Criterion | Type | Proof |
| --- | --- | --- |
| AC-1 | test-backed | locked_create_assigns_number_one_then_two |
| AC-2 | test-backed | overlapping_creates_under_lock_receive_distinct_numbers |
| AC-3 | test-backed | locked_create_assigns_number_one_then_two |
| AC-4 | test-backed | overlapping_creates_under_lock_receive_distinct_numbers |
| AC-5 | test-backed | existing_add_does_not_advance_the_counter |
| AC-6 | test-backed | edit_status_scope_thread_complete_soft_delete_leave_number_unchanged |
| AC-7 | test-backed | undo_of_complete_and_of_soft_delete_keeps_the_same_number |
| AC-8 | test-backed | list_bare_digits_finds_done_and_deleted_tasks |
| AC-9 | test-backed | reload_merge_save_keeps_numbers_and_counter_across_another_writer |
| AC-10 | test-backed | pre_number_store_upgrades_assigning_distinct_numbers_including_done_and_deleted |
| AC-11 | test-backed | pre_number_store_upgrades_assigning_distinct_numbers_including_done_and_deleted |
| AC-12 | test-backed | upgraded_store_preserves_numbers_on_second_load |
| AC-13 | test-backed | v2_store_is_refused_when_writer_format_is_older_and_the_file_is_unwritten |
| AC-14 | test-backed | list_bare_digits_finds_the_task_from_another_cwd |
| AC-15 | test-backed | list_bare_digits_with_scope_or_status_flags_is_usage |
| AC-16 | test-backed | list_uuid_still_finds_the_task |
| AC-17 | test-backed | list_unknown_number_matches_unknown_uuid_refusal |
| AC-18 | test-backed | steps_bare_digits_act_on_the_task |
| AC-19 | test-backed | steps_uuid_still_acts_on_the_task |
| AC-20 | test-backed | json_list_rows_include_number |
| AC-21 | test-backed | json_created_and_existing_include_number |
| AC-22 | test-backed | human_list_rows_show_bare_digits |
| AC-23 | test-backed | board_row_meta_shows_bare_digits_not_hash_or_t_prefix |
| AC-24 | test-backed | peek_shows_bare_digits |
| AC-25 | test-backed | ac_25_task_page_footer_number_precedes_the_form_scope_hit_target |
| AC-26 | test-backed | done_drawer_rows_show_bare_digits |
| AC-27 | test-backed | numbered_board_at_40x10_paints_in_bounds_wraps_titles_and_keeps_tasks_reachable |
| AC-28 | reviewer-checked | Pass: `skills/tsk-cli/SKILL.md` documents `tsk list 12` and `tsk steps 12`, JSON `number`, UUID still valid, cwd ignored on direct lookup, and the existing retry/exit contract. |
| AC-29 | reviewer-checked | Pass: `board.md` and `cli.md` show store-global bare digits; `board-demo.js` paints them; `keys.md` is unchanged. |
