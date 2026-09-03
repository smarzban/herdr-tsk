# Verification report: wide task split

Date: 2026-09-03
Main-integrated product head before final report refresh: `6ed7d8f`

## Result

The stage-slider rework passed the full Rust green bar and the site checks; the owner then
smoked it live. `git diff --check main...HEAD` was clean before this report was written.

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test aggregate at commit 02dcc8f: 721 passed, 0 failed, 5 ignored across 26 result blocks
tests/wide_task_split.rs at commit 02dcc8f: 47 passed
site npm test: 6 passed, 0 failed
```

Owner live smoke on the stage slider passed at ~178 cols: 0→A→G→F→G→A→0, Enter/Esc origin
memory, editing state slot, shrink below 110 and grow back.

Live herdr smoke of the rework was not run by the implementer; the boxed-design smoke and
evidence below are kept under one superseded heading.

## Proof map

Refreshed for the stage-slider rework (`stage-slider-rework.md`). Unless noted, tests live in
`tests/wide_task_split.rs`.

| Criterion | Type | Proof |
| --- | --- | --- |
| AC-1 | test-backed | wide_stage_geometry_has_exact_allocations_and_preserves_narrow_mapping (src/ui/tier.rs), stage_a_board_keeps_its_meta_column |
| AC-2 | test-backed | shrinking_and_growing_keeps_every_stage_and_its_session, narrow_board_is_unchanged_by_the_stage_model |
| AC-3 | test-backed | wide_stage_geometry_has_exact_allocations_and_preserves_narrow_mapping, wide_stage_geometry_is_bounded_and_exhausts_every_column (src/ui/tier.rs), stage_a_board_keeps_its_meta_column |
| AC-4 | test-backed | wide_hits_stay_inside_their_column_or_the_footer, wide_frames_are_mono_at_every_stage_width_and_height |
| AC-5 | test-backed | stage_a_keys_slide_both_ways_open_and_retarget_the_pane, board_row_click_selects_without_changing_stage_or_peeking |
| AC-6 | test-backed | board_row_click_selects_without_changing_stage_or_peeking, stage_zero_keys_slide_open_select_and_stay_put |
| AC-7 | test-backed | stage_zero_keys_slide_open_select_and_stay_put, stage_a_keys_slide_both_ways_open_and_retarget_the_pane, app_keyboard_route_owns_stage_slider_keys (src/app.rs) |
| AC-8 | test-backed | stage_g_keys_slide_open_close_and_navigate_the_page, stage_f_keys_return_to_the_rail_or_the_origin, stage_keys_fall_through_to_editor_semantics_while_editing |
| AC-9 | test-backed | task_surface_controls_in_g_and_f_match_the_single_pane_page, enter_in_stage_f_closes_the_page_like_the_single_pane_page |
| AC-10 | test-backed | board_row_click_selects_without_changing_stage_or_peeking, left_side_click_moves_focus_left_from_the_rail, row_double_click_opens_the_full_page_and_records_the_origin |
| AC-11 | test-backed | stage_a_task_column_click_moves_to_g_then_dispatches_against_the_painted_frame, app_mouse_click_moves_stage_a_to_g_before_dispatching_same_control, app_task_scrollbar_moves_stage_refreshes_bound_and_routes_page_scroll (src/app.rs) |
| AC-12 | test-backed | shrinking_and_growing_keeps_every_stage_and_its_session |
| AC-13 | test-backed | shrinking_and_growing_keeps_every_stage_and_its_session |
| AC-14 | test-backed | shrinking_during_an_edit_keeps_mode_draft_cursor_and_binding |
| AC-15 | test-backed | shrinking_and_growing_keeps_every_stage_and_its_session |
| AC-16 | test-backed | dirty_draft_refuses_keyboard_retarget_in_stage_a, dirty_draft_refuses_rail_row_click_and_keeps_the_editor, changed_thread_and_scope_drafts_refuse_retarget_and_paint_unsaved |
| AC-17 | test-backed | dirty_refusal_clears_after_save_and_the_pane_retargets_again, dirty_refusal_clears_after_cancel, dirty_draft_slides_left_to_a_and_right_back_to_g_untouched |
| AC-18 | test-backed | empty_pane_paints_no_task_header_and_is_inert, stage_a_task_column_click_without_selection_is_inert, stage_keys_need_a_selection |
| AC-19 | test-backed | repeated_threshold_crossings_keep_every_stage_live, repeated_threshold_resizes_keep_board_loop_live (tests/queue_board_loop.rs) |
| AC-20 | test-backed | stage_changes_never_mutate_the_domain, threshold_crossings_without_task_verbs_leave_domain_unchanged (tests/queue_board_loop.rs) |
| AC-21 | test-backed | stage_zero_keys_slide_open_select_and_stay_put, stage_a_keys_slide_both_ways_open_and_retarget_the_pane, stage_g_keys_slide_open_close_and_navigate_the_page, stage_f_keys_return_to_the_rail_or_the_origin, stage_keys_need_a_selection, narrow_routes_are_unchanged_by_the_slider |
| AC-22 | test-backed | wide_paints_exactly_one_footer_rule_row_at_every_stage, task_column_header_replaces_the_in_pane_header_with_stage_weight, status_row_crumb_and_keys_follow_the_stage, status_row_refusal_wins_over_the_crumb_then_the_keys, status_row_shows_editor_keys_while_an_editor_is_active, stage_zero_and_full_task_render_the_standard_tier_at_130x24 |
| AC-23 | test-backed | wide_frames_are_mono_at_every_stage_width_and_height (and every `render` call in the suite asserts mono) |
| AC-24 | test-backed | rail_wraps_titles_with_indent_four_and_dims_every_cell |

## Stage-slider rework

The boxed 50/50 design was replaced by the four-stage slider (commits `feat: paint the wide
stage slider chrome` through `docs: describe the wide stage slider`, then the owner-smoke
tweaks `fix: move focus left when the rail is clicked`, `fix: give the task header its glyph
and an underline rule`, and `fix: let every left-side press reach dispatch`).
`tests/wide_task_split.rs` was rewritten (47 tests as of `02dcc8f`), the three `src/app.rs` focus tests were
rewritten for stages, and `tests/queue_board_loop.rs` / `tests/queue_board_mouse.rs` wide
fixtures were adapted. Narrow goldens were not regenerated.

## Boxed-design evidence (superseded)

Everything below this heading verifies the earlier boxed 50/50 design. It is kept for
history only; the test names it cites no longer exist.

### Fresh PR-boundary verification

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release && cd site && npm ci && npm test && npm run build
exit 0
cargo test after task-box Review panel remediation: 718 passed, 0 failed, 5 ignored across 26 result blocks
wide_renderer_consumes_shared_compact_density_at_width_and_height_boundaries: ok
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

### Live terminal smoke

Using isolated state and config directories, the release binary was exercised at 184, 110, and 109 columns. Verified side-by-side rendering, selected-task retargeting, keyboard focus transfer, task-side mouse focus-before-dispatch, editor-preserving shrink/grow, editor-safe board clicks, intended step selection, cancellation, and repeated threshold crossings. The process remained live and the isolated data was removed afterward.

### Review panel and repair verification

Review panel discovery ran 9 seats, with 8 votes and one lost qwen holistic seat. Seven findings were kept for repair: F-14, F-7, F-8, F-1, F-10, F-13, and F-9. Two repair commits added live-editor focus, app-level keyboard/mouse/scrollbar wiring tests, refreshed retained scroll bounds, dirty thread/scope coverage, clean-editor retargeting, and same-bound editor safety.

The final two-seat verification found no regressions. Terra marked all kept findings resolved. Claude marked six resolved and returned F-8 as still present without evidence; the harness judged F-8 resolved from the concrete app-level focus-before-dispatch helpers and tests cited by Terra and present in `src/app.rs`.

### Main integration

Merged `origin/main` at release `v0.4.0` without rewriting feature commits. The only textual conflict was `AGENTS.md`; the resolution keeps main's current 0.4.0 store and Ctrl-key guidance plus the new responsive board contract. Four feature fixtures were adapted to main's stabilized `DomainState::create` signature. The full Rust and site bars passed, then the rebuilt release binary received a fresh isolated terminal smoke at 110 and 109 columns. The split, active editor preservation, and same-bound editor click safety all passed.

### Owner-approved task-box-only follow-up

The wide-only compositor now leaves the board unboxed across its full left allocation and paints one bordered task panel in the right allocation. The task's left border is the sole visual center separator. Its border and title are cyan plus bold with task focus and dark gray plus dim with board focus; all content remains monochrome. The bounded Review panel remediation proves both renderers consume the shared responsive density at width and height boundaries, checks every task-ring cell across representative sizes and focus states, and proves numbered titles follow the rendered selection. Below 110 columns remains unboxed.

```text
wide_geometry_balances_touching_allocations_without_a_gap: ok
exact_110_wide_frame_paints_unboxed_board_beside_titled_task_box: ok
wide_task_border_style_tracks_focus_without_coloring_board: ok
wide_no_selection_task_box_uses_plain_title_and_inert_interior: ok
wide_hits_and_copy_regions_stay_inside_board_allocation_or_task_interior: ok
wide_renderer_consumes_shared_compact_density_at_width_and_height_boundaries: ok
wide_task_border_title_follows_rendered_selection_across_numbered_tasks: ok
wide_task_drag_uses_bordered_content_edges_for_autoscroll: ok
focused_wide_task_surface_matches_single_pane_keyboard_and_mouse_outcomes: ok
cargo test aggregate: 718 passed, 0 failed, 5 ignored across 26 result blocks
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

### Mechanical corroboration

`sdlc-check 0.20.1 --require ledger --require verification-report`: passed with 0 findings and 0 notes before the initial push, after Review panel repairs, after final main integration, and after the task-box-only follow-up.
