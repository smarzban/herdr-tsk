# Build report: archive

Date started: 2026-09-04
Branch: `feat/archive`
Base: `main` at `ed68cda`
Worktree: `/Users/saeed/.herdr/worktrees/herdr-tasks/feat-archive` (herdr worktree, workspace `archive`, inherited by build)
Tier: full spec, but per owner instruction after T-2 the per-task reviewer was dropped in favour
of a continuous run (T-3..T-13, then live smoke) by one implementer (pi, `zai/glm-5.3-flash`,
Herdr pane `w1F:p1`), with the whole-change review by Review panel at PR time (as spec A).
Conductor: claude; verifies each task's green bar from its own run and records it here.
Gate: `gate-report.md` ready to build at `ea63a4b`; F-1 resolved (no `Enter` on the launch card).

## Baseline

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 764 passed, 0 failed (sum over test binaries)
cargo build --release: finished release profile
```

## Task ledger

| Task | Status | Commit | AC advanced | Review | Notes |
| --- | --- | --- | --- | --- | --- |
| T-1 | done | 100b321 | AC-7, AC-9, AC-34 | conductor spot-check | one review unit with T-2, commits adjacent as the plan requires; revert proofs in `T-1+T-2-implementer-report.md` |
| T-2 | done | d076d93 | AC-33, AC-34 | conductor spot-check | `STORE_FORMAT_VERSION = 2`, chain `[migrate_v1_to_v2]`, `current_store_v2.json` fixture added, `current_store_v1.json` byte-unchanged |
| T-3 | done | e912bb0 | AC-20, AC-21 | conductor spot-check | project verbs, `is_hidden`, transient project intents merge |
| T-4 | done | ffa848c | AC-5, AC-6, AC-9, AC-19 | conductor spot-check | `query_board` with archived project set |
| T-5 | done | 809cf5b | AC-10..AC-15 | conductor spot-check | ARCHIVED drawer group, sentinel header row, `done_drawer_archived` golden; golden-count guard 6→7 |
| T-6 | done | fce4211 + 4fd23f8 + conductor fix | AC-1..AC-4, AC-7, AC-35 | conductor spot-check | remediation: `f` verb moved last on the board bar (implementer) and the task-page bar (conductor) so `+ capture` and `ctrl+a step` stay on the 80-col bar; guards restored to `main` assertions; board goldens differ from main only by the clipped ` · f…` tail |
| T-7 | done | 3792793 | AC-8 | conductor spot-check | header slot `archived` |
| T-8 | done | 2863b96 | AC-16..AC-19 | conductor spot-check | picker tabs, `ProjectPickerSwitchTab` / `SelectPickerTab` |
| T-9 | done | d8d3dbb | AC-22..AC-26 | conductor spot-check | launch card, no `Enter` default (gate F-1) |
| T-10 | done | 1456b86 | AC-27 | conductor spot-check | quick-add `!p` refusal |
| T-11 | done | 263ce1c | AC-29, AC-31, AC-32 | conductor spot-check | `tsk archive|unarchive`, `list --archived` |
| T-12 | done | 521e39e | AC-28, AC-30 | conductor spot-check | `tsk project archive|unarchive`, `project-archived` error code |
| T-13 | done | a3886a3 | AC-36 | conductor spot-check | docs, site, skill, changelog, verification-report |
| T-14 | done | 9ec1bd9 | AC-13, AC-37, AC-38 | conductor spot-check | owner-smoke amendment; `ctrl+g` reassigned from `ToggleAllGroups` (palette keeps `toggle groups`), owner to confirm |
| T-15 | done | 6793472 | AC-22 | conductor spot-check | titleless card; border gap fixed by conductor in 61a6e45 |
| T-16 | done | cf70dbf | AC-39 | conductor spot-check | |
| T-17 | done | ded748a | AC-36, AC-40..AC-45 | conductor spot-check | read-only archived focus; docs folded in |

## Evidence

Per-task green-bar captures are appended below by the conductor from its own runs.

### T-1 + T-2 (conductor run, head `d076d93`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
exit 0
cargo test: 768 passed, 0 failed (baseline 764, +4)
```

### T-3 .. T-13 plus remediation (conductor run, head after `fix(T-6)` task-page commit)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 803 passed, 0 failed (baseline 764, +39)
cargo build --release: finished release profile
$ cd site && npm test
pass 10, fail 0
```

Live smoke: `.agent-sdlc/briefs/archive/SMOKE-report.md` (implementer, `HERDR_ENV=1`,
`TSK_STATE_DIR=/tmp/tsk-archive-smoke`, 23 steps, nothing failed live). Conductor did not re-drive
the smoke; the review panel runs next on the whole change.

## Review fixes (Review panel on main..HEAD, eight kept findings, all fixed)

| K | Commit | Regression test (watched failing first) |
| --- | --- | --- |
| K1 | `5dd3850` fix(K1): plan-form add refuses archived-project items | `tests/cli_add.rs::plan_items_resolving_to_an_archived_project_refuse_with_project_archived` (red: all three items saved, exit 0) |
| K2 | `137b8d8` fix(K2): list `--archived` conflicts are usage errors | `tests/cli_list.rs::archived_conflicts_are_usage_errors` (red: no conflict validation, exit 0) |
| K3 | `3bf08ef` fix(K3): keep archived is a non-persisting outcome | `tests/archive_launch_card.rs::keep_archived_persists_nothing` (red: outcome was `Persisted`, store rewritten) |
| K4 | `7b5c4f3` fix(K4): archived header pin drives viewport follow | `tests/queue_board_render.rs::archived_header_selection_follows_the_viewport` (red: header stayed below the fold) |
| K5 | `f5dde44` fix(K5): idle merge resets project focus archived by another process | `tests/queue_board_loop.rs::idle_merge_leaves_a_project_focus_archived_by_another_process` (red via hand-revert: `selected_project` stayed `Some("/repos/focus")`) |
| K6 | `1319319` fix(K6): v1 migration strips defensive archived keys | `tests/store_persist.rs::v1_migration_strips_defensive_archived_keys` (red: `archived == true` survived) |
| K7 | `d5df66f` fix(K7): archive refusals name the invoked verb | `tests/cli_archive.rs::unarchive_refusals_name_the_unarchive_verb` (red: `tsk archive: T99 ...` hardcoded) |
| K8 | `dc3d802` fix(K8): picker file refusal for a taskless project paints on the status slot | `tests/queue_board_verbs.rs::file_on_a_taskless_invocation_repo_refuses_on_the_status_line` (red via hand-revert: no message, picker open) |

Advisory (not fixed, per review): the picker archived tab paints live state while acting on
its snapshot after an idle merge.

Final: `cargo test` 811 passed, 0 failed; clippy `-D warnings` clean; release build ok.

### Verify regressions (conductor fixes)

| Id | Commit | Fix | Test (watched red on hand revert) |
| --- | --- | --- | --- |
| V1 | (this commit) | plan-form `failed` rows sorted by item index after the in-transaction archived refusals join the pre-transaction parse failures | `cli_add::plan_failed_rows_keep_item_order_when_an_archived_refusal_precedes_a_parse_failure` (red: `[1, 0]`) |
| V2 | (this commit) | the "never an archived quick-add default" invariant moved to `OpenCapture` (falls back to desk only when the resolved default is archived); `sync_from_domain` no longer sets a session-wide desk default; redundant `was_focused` block removed from the picker `File` arm | `queue_board_verbs::picker_archive_of_the_focused_project_keeps_an_unarchived_cwd_default_for_quick_add` (red: `Global`) |

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 813 passed, 0 failed
```

## Owner-smoke amendment round (T-14..T-17, spec at `44a2c25`)

| Task | Commit | Failing test(s) watched red first |
| --- | --- | --- |
| T-14 archived header paint, `ctrl+g`, picker rule | `9ec1bd9` | `queue_board_render::archived_header_reads_chevron_word_dot_count_and_selection_is_bold_not_reverse`, `queue_board_render::picker_paints_a_dim_rule_under_its_tabs`, `queue_board_verbs::ctrl_g_toggles_the_archived_group_from_any_selection_and_opens_the_drawer_when_closed` |
| T-15 launch card body and footer | `6793472` | `archive_launch_card::launch_card_is_one_message_line_with_choices_in_the_footer` |
| T-16 scope dropdowns hide archived projects | `cf70dbf` | `queue_board_edit::task_page_scope_dropdown_omits_archived_projects_but_keeps_the_current_scope`, `quick_add_capture::expanded_quick_add_scope_omits_archived_projects`, `src/ui/capture.rs::tests::capture_scope_never_offers_an_archived_this_project` |
| T-17 read-only archived project focus | `ded748a` | `queue_board_verbs::enter_on_the_archived_tab_opens_a_read_only_focus_that_persists_nothing`, `::every_mutating_verb_in_read_only_focus_refuses_with_the_archived_message`, `::ctrl_u_in_read_only_focus_unarchives_in_place`, `::leaving_read_only_focus_hides_the_archived_projects_tasks_again`, `queue_board_render::archived_tab_verb_bar_advertises_ctrl_u_enter_esc`, `queue_board_edit::task_page_in_read_only_focus_refuses_edit_mode` |

Keymap change of record: `ctrl+g` moved from `ToggleAllGroups` to `ToggleArchivedGroup`
(AC-38). Group toggling keeps its palette command; the superseded chord test was renamed
to `ctrl_g_maps_to_the_archived_group_and_the_palette_keeps_group_toggle`.

Final: `cargo test` **826 passed, 0 failed**; `cd site && npm test` 10/10; clippy
`-D warnings` clean; release build ok. Live smoke 2:
`.agent-sdlc/briefs/archive/SMOKE2-report.md` (11 steps on `/tmp/tsk-b-try`, nothing
failed live, store left as found). Verification report rewritten as one
`| Criterion | Type | Proof |` table over AC-1..AC-45.

## Per-task green-bar evidence (checker form)

The conductor ran the full bar at each unit boundary (T-1+T-2, T-3..T-13, the K1..K8 fixes, T-14..T-17); the per-task counts below are the implementer's reported totals at each commit, cross-checked against the conductor's boundary runs (768, 803, 811, 826).

### T-1 (@ 100b321)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 768 passed, 0 failed
```

### T-2 (@ d076d93)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 768 passed, 0 failed
```

### T-3 (@ e912bb0)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 771 passed, 0 failed
```

### T-4 (@ ffa848c)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 775 passed, 0 failed
```

### T-5 (@ 809cf5b)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 780 passed, 0 failed
```

### T-6 (@ fce4211)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 786 passed, 0 failed
```

### T-7 (@ 3792793)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 788 passed, 0 failed
```

### T-8 (@ 2863b96)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 791 passed, 0 failed
```

### T-9 (@ d8d3dbb)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 796 passed, 0 failed
```

### T-10 (@ 1456b86)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 797 passed, 0 failed
```

### T-11 (@ 263ce1c)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 801 passed, 0 failed
```

### T-12 (@ 521e39e)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 803 passed, 0 failed
```

### T-13 (@ a3886a3)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 803 passed, 0 failed
```

### T-14 (@ 9ec1bd9)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 815 passed, 0 failed
```

### T-15 (@ 6793472)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 817 passed, 0 failed
```

### T-16 (@ cf70dbf)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 821 passed, 0 failed
```

### T-17 (@ ded748a)

Conductor capture at the branch head `61a6e45` (T-17 plus the border fix), the full bar;
re-captured after the delta-review fixes D1..D8 and the owner's AC-38 remediation, whose
regression tests are appended to the same run below (the superseded dedicated-chord tests
were deleted with their code):

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build --release
exit 0
$ cargo test > /tmp/archive-final-test.txt 2>&1; rc=$?
rc=0
cargo test: 832 passed, 0 failed (sum over 30 test binaries, re-captured after the AC-38 remediation)
test domain::task::tests::archive_project_writes_one_record_and_unarchive_removes_it ... ok
test domain::task::tests::archive_task_sets_the_flag_keeps_status_journals_archived_and_pushes_no_undo ... ok
test domain::task::tests::task_and_project_flags_are_independent ... ok
test store::tests::save_emits_format_version_two ... ok
test ui::capture::tests::capture_scope_never_offers_an_archived_this_project ... ok
test ui::queue::tests::archived_group_follows_the_drawer_scope ... ok
test ui::queue::tests::thread_header_and_open_count_exclude_an_archived_task ... ok
test card_is_offered_once_per_session_whichever_choice ... ok
test keep_archived_persists_nothing ... ok
test launch_in_an_archived_project_paints_the_two_choice_card_before_any_key ... ok
test launch_card_is_one_message_line_with_choices_in_the_footer ... ok
test y_or_a_click_unarchives_durably_and_quick_add_defaults_to_the_project ... ok
test no_card_when_the_default_is_desk_or_an_unarchived_project ... ok
test n_esc_or_click_keep_archived_defaults_quick_add_to_desk_with_a_status_line ... ok
test add_into_an_archived_project_exits_1_with_project_archived_and_names_the_ways_out ... ok
test plan_failed_rows_keep_item_order_when_an_archived_refusal_precedes_a_parse_failure ... ok
test plan_items_resolving_to_an_archived_project_refuse_with_project_archived ... ok
test unarchive_refusals_name_the_unarchive_verb ... ok
test archive_and_unarchive_by_number_exit_0_and_repeat_is_idempotent ... ok
test project_archive_and_unarchive_resolve_basename_and_path_exit_0_and_are_idempotent ... ok
test archive_of_an_unknown_number_or_a_soft_deleted_task_exits_1_with_a_message ... ok
test archived_conflicts_are_usage_errors ... ok
test default_list_views_exclude_archived_tasks_and_tasks_of_archived_projects ... ok
test list_archived_marks_task_and_project_rows_once_per_id ... ok
test task_page_scope_dropdown_omits_archived_projects_but_keeps_the_current_scope ... ok
test task_page_in_read_only_focus_refuses_edit_mode ... ok
test title_edit_on_an_archived_task_persists_and_keeps_the_flag ... ok
test idle_merge_hides_a_task_archived_by_another_process_without_moving_selection ... ok
test idle_merge_leaves_a_project_focus_archived_by_another_process ... ok
test archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it ... ok
test all_golden_frames_pass_no_color_sgr_scan ... ok
test archived_tab_verb_bar_advertises_ctrl_u_enter_esc ... ok
test archived_header_reads_chevron_word_dot_count_and_selection_is_bold_not_reverse ... ok
test archived_group_paints_below_done_with_its_count_and_no_header_when_empty ... ok
test archived_header_selection_follows_the_viewport ... ok
test archived_tab_lists_exactly_the_archived_projects_and_paints_an_empty_state_line ... ok
test archived_project_paints_nowhere_on_home_tabs_or_the_picker_main_list ... ok
test expanded_archived_rows_are_dim_keep_glyph_and_identifier_and_are_selectable_and_hit_testable ... ok
test picker_paints_a_dim_rule_under_its_tabs ... ok
test task_page_header_slot_reads_archived_for_an_archived_task ... ok
test surface_goldens_board_accordion_palette_help_drawer_exist_for_reviewer_side_by_side ... ok
test archived_task_paints_in_no_working_lens_in_any_status_at_any_tier ... ok
test archived_group_is_collapsed_on_a_fresh_model_and_enter_or_click_on_the_header_toggles_it ... ok
test ctrl_f_and_ctrl_u_on_the_archived_tab_unarchive_and_every_task_keeps_its_status ... ok
test ctrl_f_in_the_archived_group_unarchives_and_the_row_returns_to_the_deck ... ok
test ctrl_f_in_the_picker_archives_the_selected_project_and_keeps_the_picker_open ... ok
test ctrl_f_archives_the_selected_task_keeping_status_and_pushing_no_undo ... ok
test ctrl_u_on_an_archived_selection_unarchives_without_popping_the_undo_stack ... ok
test ctrl_u_in_read_only_focus_unarchives_in_place ... ok
test enter_on_the_archived_tab_opens_a_read_only_focus_that_persists_nothing ... ok
test every_mutating_verb_in_read_only_focus_refuses_with_the_archived_message ... ok
test file_on_a_taskless_invocation_repo_refuses_on_the_status_line ... ok
test leaving_read_only_focus_hides_the_archived_projects_tasks_again ... ok
test help_card_lists_ctrl_f_and_the_verb_bar_shows_file_for_a_task_row_and_the_group ... ok
test u_undoes_with_domain_coverage_and_stale_undo_refused_visibly ... ok
test expanded_quick_add_scope_omits_archived_projects ... ok
test p_token_naming_an_archived_project_refuses_on_the_open_line_and_clears_on_close ... ok
test archive_flag_survives_save_and_load ... ok
test literal_current_v2_fixture_round_trips_byte_identical ... ok
test v1_migration_strips_defensive_archived_keys ... ok
test v1_document_loads_through_the_chain_and_first_save_leaves_tsk_json_v1_beside_the_live_file ... ok
test reload_merge_save_keeps_a_sibling_writers_project_record_and_applies_the_local_intent ... ok
test normal_mode_keymap_equals_the_readme_and_queue_board_v1_set ... ok
test esc_leaves_read_only_focus_and_never_quits ... ok
test opening_the_picker_from_read_only_focus_lands_home_on_cancel ... ok
test unarchiving_from_the_picker_converts_a_read_only_focus_in_place ... ok
test ctrl_f_on_the_archived_tab_still_unarchives_from_read_only_focus ... ok
test tab_and_field_focus_on_a_read_only_task_page_stay_in_view_mode ... ok
test picker_list_capacity_counts_the_rule_row_on_a_short_frame ... ok
test idle_merge_converts_a_read_only_focus_whose_project_was_unarchived ... ok
test toggle_all_groups_folds_and_unfolds_the_archived_group_with_the_others_when_the_drawer_is_open ... ok
test ui::board::model::tests::toggle_all_groups_toggles_only_the_active_home_tabs_top_level_groups ... ok
```

## Delta review fixes (two panel runs on 55addf3..3b46eb0, eight kept findings)

| D | Commit | Regression test (watched failing first) |
| --- | --- | --- |
| D1 | `0d48521` fix(D1): esc leaves the read-only archived focus instead of quitting | `queue_board_verbs::esc_leaves_read_only_focus_and_never_quits` (red: `left: Quit`) |
| D2 | `52447fe` fix(D2): read-only task page refuses tab and field-focus edit entries | `queue_board_edit::tab_and_field_focus_on_a_read_only_task_page_stay_in_view_mode` (red: no refusal, Tab entered edit) |
| D3 | `b94dd35` fix(D3): P leaves the read-only archived lens before opening the picker | `queue_board_verbs::opening_the_picker_from_read_only_focus_lands_home_on_cancel` (red: still in the lens) |
| D4 | `f29ecd2` fix(D4): picker list capacity counts its rule row | `queue_board_render::picker_list_capacity_counts_the_rule_row_on_a_short_frame` (red at 78x12: selected last option clipped) |
| D5 | `966458a` fix(D5): a read-only focus converts when its project is unarchived elsewhere | `queue_board_loop::idle_merge_converts_a_read_only_focus_whose_project_was_unarchived` + `queue_board_verbs::unarchiving_from_the_picker_converts_a_read_only_focus_in_place` (both red: focus stayed archived) |
| D6 | `09ea780` fix(D6): ctrl+g with no archived rows in scope refuses instead of moving the selection | `queue_board_verbs::ctrl_g_with_no_archived_rows_in_scope_says_so_and_moves_nothing` (red: drawer opened, selection moved) |
| D7 | `7bc084e` fix(D7): the read-only gate stands down while a popup owns its intents | `queue_board_verbs::ctrl_f_on_the_archived_tab_still_unarchives_from_read_only_focus` (red with D3's exit hand-reverted: `left: None, right: Persist`, so the gate alone carries the flow) |
| D8 | `dbfac56` fix(D8): ctrl+g verb entry for any selection and a clickable chip | `queue_board_render::verb_bar_shows_ctrl_g_for_any_selection_while_the_drawer_has_archived_rows` + `queue_board_mouse::clicking_the_ctrl_g_verb_chip_toggles_the_archived_group` (red: no entry for a task row; no `g` arm in `verb_intent`) |

Final: `cargo test` **836 passed, 0 failed**; `cd site && npm test` 10/10; clippy
`-D warnings` clean; fmt clean; release build ok.

## Owner decision (AC-38 reworded at `3a8bcb7`)

| Decision | Commit | Regression test (watched failing first) |
| --- | --- | --- |
| `ctrl+g` keeps `ToggleAllGroups`; the archived group joins toggle-all while the drawer is open, and is left alone while it is shut. The dedicated archived chord, its verb-bar entry, its `no archived tasks here` refusal, and the `"g"` mouse arm are gone; `Enter`/click on the header still toggles the group alone. | `f4e596b` (this commit, pre-amend `1dec377`) fix(AC-38): archived group joins toggle-all-groups, ctrl+g keeps its meaning | `queue_board_verbs::toggle_all_groups_folds_and_unfolds_the_archived_group_with_the_others_when_the_drawer_is_open` (red: `ctrl+g` mapped to `ToggleArchivedGroup`; then with `archived_joins` hand-forced to `false`, red again on "the archived group folded with it") |

Deleted with the superseded design: `ctrl_g_toggles_the_archived_group_from_any_selection_and_opens_the_drawer_when_closed`, `ctrl_g_maps_to_the_archived_group_and_the_palette_keeps_group_toggle`, `ctrl_g_with_no_archived_rows_in_scope_says_so_and_moves_nothing` (D6), `verb_bar_shows_ctrl_g_for_any_selection_while_the_drawer_has_archived_rows` and `clicking_the_ctrl_g_verb_chip_toggles_the_archived_group` (D8). `tests/v1_keymap_guard.rs` is back to main's expectation for `g`; `tests/fixtures/queue_board/help.txt` regenerated (`ctrl+g groups`).

Final: `cargo test` **832 passed, 0 failed**; `cd site && npm test` 10/10; clippy
`-D warnings` clean; fmt clean; release build ok.
