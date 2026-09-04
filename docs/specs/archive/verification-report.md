# Verification report: archive (spec B)

Every AC mapped to its proving test(s). Suite counts: `cargo test` 803 passed /
0 failed at T-12+T-13; `cd site && npm test` 10/10 for AC-36.

| AC | Proof |
| --- | --- |
| AC-1 | `tests/queue_board_verbs.rs::ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo` |
| AC-2 | `tests/queue_board_verbs.rs::ctrl_f_in_the_archived_group_unarchives_and_the_row_returns_to_the_deck` |
| AC-3 | `tests/queue_board_verbs.rs::ctrl_u_on_an_archived_selection_unarchives_without_popping_the_undo_stack` |
| AC-4 | `tests/queue_board_verbs.rs::u_undoes_with_domain_coverage_and_stale_undo_refused_visibly` (pre-existing, untouched, green) + the empty-stack no-op assert in `ctrl_f_archives_...` |
| AC-5 | `tests/queue_board_render.rs::archived_task_paints_in_no_working_lens_in_any_status_at_any_tier` |
| AC-6 | `src/ui/queue.rs::tests::thread_header_and_open_count_exclude_an_archived_task` |
| AC-7 | `ctrl_f_archives_...` (ctrl+u on task B leaves A archived) + `src/domain/task.rs::tests::archive_task_sets_the_flag_keeps_status_journals_archived_and_pushes_no_undo` |
| AC-8 | `tests/queue_board_render.rs::task_page_header_slot_reads_archived_for_an_archived_task` + `tests/queue_board_edit.rs::title_edit_on_an_archived_task_persists_and_keeps_the_flag` |
| AC-9 | `tests/store_persist.rs::archive_flag_survives_save_and_load` + `tests/queue_board_loop.rs::idle_merge_hides_a_task_archived_by_another_process_without_moving_selection` |
| AC-10 | `tests/queue_board_render.rs::archived_group_paints_below_done_with_its_count_and_no_header_when_empty` |
| AC-11 | `tests/queue_board_verbs.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` |
| AC-12 | same verbs test (Enter) + `tests/queue_board_mouse.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` (click); fresh model re-collapsed in both |
| AC-13 | `tests/queue_board_render.rs::expanded_archived_rows_are_dim_keep_glyph_and_identifier_and_are_selectable_and_hit_testable` |
| AC-14 | `src/ui/queue.rs::tests::archived_group_follows_the_drawer_scope` |
| AC-15 | `assert_buffer_mono` in every archived render test + golden `tests/fixtures/queue_board/done_drawer_archived.txt` via `surface_goldens_...` and `all_golden_frames_pass_no_color_sgr_scan` |
| AC-16 | `tests/queue_board_verbs.rs::ctrl_f_in_the_picker_archives_the_selected_project_and_keeps_the_picker_open` |
| AC-17 | `tests/queue_board_render.rs::archived_tab_lists_exactly_the_archived_projects_and_paints_an_empty_state_line` |
| AC-18 | `tests/queue_board_verbs.rs::ctrl_f_and_ctrl_u_on_the_archived_tab_unarchive_and_every_task_keeps_its_status` |
| AC-19 | `tests/queue_board_render.rs::archived_project_paints_nowhere_on_home_tabs_or_the_picker_main_list` |
| AC-20 | `src/domain/task.rs::tests::task_and_project_flags_are_independent` |
| AC-21 | `src/domain/task.rs::tests::archive_project_writes_one_record_and_unarchive_removes_it` + `tests/store_persist.rs::reload_merge_save_keeps_a_sibling_writers_project_record_and_applies_the_local_intent` |
| AC-22 | `tests/archive_launch_card.rs::launch_in_an_archived_project_paints_the_two_choice_card_before_any_key` |
| AC-23 | `tests/archive_launch_card.rs::y_or_a_click_unarchives_durably_and_quick_add_defaults_to_the_project` |
| AC-24 | `tests/archive_launch_card.rs::n_esc_or_click_keep_archived_defaults_quick_add_to_desk_with_a_status_line` |
| AC-25 | `tests/archive_launch_card.rs::card_is_offered_once_per_session_whichever_choice` |
| AC-26 | `tests/archive_launch_card.rs::no_card_when_the_default_is_desk_or_an_unarchived_project` |
| AC-27 | `tests/quick_add_capture.rs::p_token_naming_an_archived_project_refuses_on_the_open_line_and_clears_on_close` |
| AC-28 | `tests/cli_add.rs::add_into_an_archived_project_exits_1_with_project_archived_and_names_the_ways_out` |
| AC-29 | `tests/cli_archive.rs::archive_and_unarchive_by_number_exit_0_and_repeat_is_idempotent` + `archive_of_an_unknown_number_or_a_soft_deleted_task_exits_1_with_a_message` |
| AC-30 | `tests/cli_archive.rs::project_archive_and_unarchive_resolve_basename_and_path_exit_0_and_are_idempotent` |
| AC-31 | `tests/cli_list.rs::default_list_views_exclude_archived_tasks_and_tasks_of_archived_projects` |
| AC-32 | `tests/cli_list.rs::list_archived_marks_task_and_project_rows_once_per_id` |
| AC-33 | `tests/store_persist.rs::v1_document_loads_through_the_chain_and_first_save_leaves_tsk_json_v1_beside_the_live_file` + `src/store.rs::tests::save_emits_format_version_two` |
| AC-34 | `tests/store_persist.rs::literal_current_v2_fixture_round_trips_byte_identical` + `archive_flag_survives_save_and_load` (no `archived` key on unarchived tasks) |
| AC-35 | `tests/queue_board_verbs.rs::help_card_lists_ctrl_f_and_the_verb_bar_shows_file_for_a_task_row_and_the_group` + `tests/v1_keymap_guard.rs::normal_mode_keymap_equals_the_readme_and_queue_board_v1_set` |
| AC-36 | Reviewer-checked: `site/src/content/docs/docs/{keys,board,capture,cli}.md`, `site/public/board-demo.js`, `skills/tsk-cli/SKILL.md`, `CHANGELOG.md`, `README.md` diffed against `src/ui/input.rs`, `src/ui/board/commands.rs` (no palette entry, per plan), and `src/cli/`; `cd site && npm test` 10/10 |
