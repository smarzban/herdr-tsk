# Verification report: archive (spec B)

Every criterion of `docs/specs/archive/archive.md` (AC-1..AC-45, including the
2026-09-04 owner-smoke amendments) mapped to its proof. Suite at T-17: `cargo test`
**826 passed / 0 failed**; `cd site && npm test` **10/10** (the parse check behind AC-36).

Type is the criterion's own verification type. Proof names the test(s) as
`file::test_name`, or the pass/fail answer for the one reviewer-checked criterion.

| Criterion | Type | Proof |
| --- | --- | --- |
| AC-1 | test-backed | `tests/queue_board_verbs.rs::ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo` |
| AC-2 | test-backed | `tests/queue_board_verbs.rs::ctrl_f_in_the_archived_group_unarchives_and_the_row_returns_to_the_deck` |
| AC-3 | test-backed | `tests/queue_board_verbs.rs::ctrl_u_on_an_archived_selection_unarchives_without_popping_the_undo_stack` |
| AC-4 | test-backed | `tests/queue_board_verbs.rs::u_undoes_with_domain_coverage_and_stale_undo_refused_visibly` (pre-existing, untouched, green) + the empty-stack no-op assert in `tests/queue_board_verbs.rs::ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo` |
| AC-5 | test-backed | `tests/queue_board_render.rs::archived_task_paints_in_no_working_lens_in_any_status_at_any_tier` |
| AC-6 | test-backed | `src/ui/queue.rs::tests::thread_header_and_open_count_exclude_an_archived_task` |
| AC-7 | test-backed | `tests/queue_board_verbs.rs::ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo` + `src/domain/task.rs::tests::archive_task_sets_the_flag_keeps_status_journals_archived_and_pushes_no_undo` |
| AC-8 | test-backed | `tests/queue_board_render.rs::task_page_header_slot_reads_archived_for_an_archived_task` + `tests/queue_board_edit.rs::title_edit_on_an_archived_task_persists_and_keeps_the_flag` |
| AC-9 | test-backed | `tests/store_persist.rs::archive_flag_survives_save_and_load` + `tests/queue_board_loop.rs::idle_merge_hides_a_task_archived_by_another_process_without_moving_selection` + `tests/queue_board_loop.rs::idle_merge_leaves_a_project_focus_archived_by_another_process` |
| AC-10 | test-backed | `tests/queue_board_render.rs::archived_group_paints_below_done_with_its_count_and_no_header_when_empty` |
| AC-11 | test-backed | `tests/queue_board_verbs.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` |
| AC-12 | test-backed | `tests/queue_board_verbs.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` (Enter) + `tests/queue_board_mouse.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` (click) + `tests/queue_board_render.rs::archived_header_selection_follows_the_viewport` |
| AC-13 | test-backed | `tests/queue_board_render.rs::expanded_archived_rows_are_dim_keep_glyph_and_identifier_and_are_selectable_and_hit_testable` + (amended header) `tests/queue_board_render.rs::archived_header_reads_chevron_word_dot_count_and_selection_is_bold_not_reverse` |
| AC-14 | test-backed | `src/ui/queue.rs::tests::archived_group_follows_the_drawer_scope` |
| AC-15 | test-backed | `assert_buffer_mono` in every archived render test + golden `tests/fixtures/queue_board/done_drawer_archived.txt` via `tests/queue_board_render.rs::surface_goldens_board_accordion_palette_help_drawer_exist_for_reviewer_side_by_side` and `tests/queue_board_render.rs::all_golden_frames_pass_no_color_sgr_scan` |
| AC-16 | test-backed | `tests/queue_board_verbs.rs::ctrl_f_in_the_picker_archives_the_selected_project_and_keeps_the_picker_open` + `tests/queue_board_verbs.rs::file_on_a_taskless_invocation_repo_refuses_on_the_status_line` |
| AC-17 | test-backed | `tests/queue_board_render.rs::archived_tab_lists_exactly_the_archived_projects_and_paints_an_empty_state_line` |
| AC-18 | test-backed | `tests/queue_board_verbs.rs::ctrl_f_and_ctrl_u_on_the_archived_tab_unarchive_and_every_task_keeps_its_status` |
| AC-19 | test-backed | `tests/queue_board_render.rs::archived_project_paints_nowhere_on_home_tabs_or_the_picker_main_list` |
| AC-20 | test-backed | `src/domain/task.rs::tests::task_and_project_flags_are_independent` |
| AC-21 | test-backed | `src/domain/task.rs::tests::archive_project_writes_one_record_and_unarchive_removes_it` + `tests/store_persist.rs::reload_merge_save_keeps_a_sibling_writers_project_record_and_applies_the_local_intent` |
| AC-22 | test-backed | `tests/archive_launch_card.rs::launch_in_an_archived_project_paints_the_two_choice_card_before_any_key` + (amended body/footer) `tests/archive_launch_card.rs::launch_card_is_one_message_line_with_choices_in_the_footer` |
| AC-23 | test-backed | `tests/archive_launch_card.rs::y_or_a_click_unarchives_durably_and_quick_add_defaults_to_the_project` |
| AC-24 | test-backed | `tests/archive_launch_card.rs::n_esc_or_click_keep_archived_defaults_quick_add_to_desk_with_a_status_line` + `tests/archive_launch_card.rs::keep_archived_persists_nothing` |
| AC-25 | test-backed | `tests/archive_launch_card.rs::card_is_offered_once_per_session_whichever_choice` |
| AC-26 | test-backed | `tests/archive_launch_card.rs::no_card_when_the_default_is_desk_or_an_unarchived_project` |
| AC-27 | test-backed | `tests/quick_add_capture.rs::p_token_naming_an_archived_project_refuses_on_the_open_line_and_clears_on_close` |
| AC-28 | test-backed | `tests/cli_add.rs::add_into_an_archived_project_exits_1_with_project_archived_and_names_the_ways_out` + (plan/JSON form) `tests/cli_add.rs::plan_items_resolving_to_an_archived_project_refuse_with_project_archived` + `tests/cli_add.rs::plan_failed_rows_keep_item_order_when_an_archived_refusal_precedes_a_parse_failure` |
| AC-29 | test-backed | `tests/cli_archive.rs::archive_and_unarchive_by_number_exit_0_and_repeat_is_idempotent` + `tests/cli_archive.rs::archive_of_an_unknown_number_or_a_soft_deleted_task_exits_1_with_a_message` + `tests/cli_archive.rs::unarchive_refusals_name_the_unarchive_verb` |
| AC-30 | test-backed | `tests/cli_archive.rs::project_archive_and_unarchive_resolve_basename_and_path_exit_0_and_are_idempotent` |
| AC-31 | test-backed | `tests/cli_list.rs::default_list_views_exclude_archived_tasks_and_tasks_of_archived_projects` |
| AC-32 | test-backed | `tests/cli_list.rs::list_archived_marks_task_and_project_rows_once_per_id` + `tests/cli_list.rs::archived_conflicts_are_usage_errors` |
| AC-33 | test-backed | `tests/store_persist.rs::v1_document_loads_through_the_chain_and_first_save_leaves_tsk_json_v1_beside_the_live_file` + `src/store.rs::tests::save_emits_format_version_two` + `tests/store_persist.rs::v1_migration_strips_defensive_archived_keys` |
| AC-34 | test-backed | `tests/store_persist.rs::literal_current_v2_fixture_round_trips_byte_identical` + `tests/store_persist.rs::archive_flag_survives_save_and_load` (no `archived` key on unarchived tasks) |
| AC-35 | test-backed | `tests/queue_board_verbs.rs::help_card_lists_ctrl_f_and_the_verb_bar_shows_file_for_a_task_row_and_the_group` + `tests/v1_keymap_guard.rs::normal_mode_keymap_equals_the_readme_and_queue_board_v1_set` |
| AC-36 | reviewer-checked | **Pass.** `site/src/content/docs/docs/{keys,board,capture,cli}.md`, `site/public/board-demo.js`, `site/public/llms.txt`, `skills/tsk-cli/SKILL.md`, `CHANGELOG.md`, `README.md` diffed against `src/ui/input.rs`, `src/ui/board/commands.rs` (no palette entry, per plan) and `src/cli/`; the T-14..T-17 amendments (`ctrl+g` reassignment, header paint, picker rule, card wording, read-only focus, scope-dropdown rule) landed in `keys.md`, `board.md` and `CHANGELOG.md`; `cd site && npm test` 10/10 |
| AC-37 | test-backed | `tests/queue_board_render.rs::picker_paints_a_dim_rule_under_its_tabs` |
| AC-38 | test-backed | `tests/queue_board_verbs.rs::ctrl_g_toggles_the_archived_group_from_any_selection_and_opens_the_drawer_when_closed` + `tests/queue_board_verbs.rs::ctrl_g_maps_to_the_archived_group_and_the_palette_keeps_group_toggle` + `tests/queue_board_verbs.rs::archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it` (Enter/click unchanged); the verb-bar half is asserted inside `ctrl_g_toggles_the_archived_group_from_any_selection_and_opens_the_drawer_when_closed` (`board_verb_items` carries `ctrl+g expand`/`collapse` while the drawer is open and the group has rows) |
| AC-39 | test-backed | `tests/queue_board_edit.rs::task_page_scope_dropdown_omits_archived_projects_but_keeps_the_current_scope` + `tests/quick_add_capture.rs::expanded_quick_add_scope_omits_archived_projects` + `src/ui/capture.rs::tests::capture_scope_never_offers_an_archived_this_project` |
| AC-40 | test-backed | `tests/queue_board_render.rs::archived_tab_verb_bar_advertises_ctrl_u_enter_esc` + `tests/queue_board_verbs.rs::ctrl_f_and_ctrl_u_on_the_archived_tab_unarchive_and_every_task_keeps_its_status` (`ctrl+f` still unarchives) |
| AC-41 | test-backed | `tests/queue_board_verbs.rs::enter_on_the_archived_tab_opens_a_read_only_focus_that_persists_nothing` |
| AC-42 | test-backed | `tests/queue_board_verbs.rs::every_mutating_verb_in_read_only_focus_refuses_with_the_archived_message` |
| AC-43 | test-backed | `tests/queue_board_verbs.rs::ctrl_u_in_read_only_focus_unarchives_in_place` |
| AC-44 | test-backed | `tests/queue_board_edit.rs::task_page_in_read_only_focus_refuses_edit_mode` |
| AC-45 | test-backed | `tests/queue_board_verbs.rs::leaving_read_only_focus_hides_the_archived_projects_tasks_again` + `tests/queue_board_render.rs::archived_project_paints_nowhere_on_home_tabs_or_the_picker_main_list` |

## Notes

- Review-panel fixes K1..K8 and verify regressions V1..V2 are folded into the rows above
  (AC-28, AC-29, AC-32, AC-33, AC-9, AC-12, AC-16, AC-24); the commit-by-commit ledger is
  in `docs/specs/archive/build-report.md`.
- Advisory finding left unfixed by owner direction: the picker's archived tab paints live
  state while acting on its snapshot after an idle merge.
