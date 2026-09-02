# Verification report: wide task split

Date: 2026-09-03
Final reviewed product head before report refresh: `4586803`

## Result

The full Rust green bar, site checks, criterion-linked tests, and isolated live terminal smoke passed. `git diff --check main...HEAD` was clean before this report was written.

## Proof map

| Criterion | Type | Proof |
| --- | --- | --- |
| AC-1 | test-backed | wide_layout_activates_at_110_and_109_stays_single_pane |
| AC-2 | test-backed | wide_layout_activates_at_110_and_109_stays_single_pane |
| AC-3 | test-backed | wide_geometry_reserves_one_divider_and_balances_remaining_columns |
| AC-4 | test-backed | wide_split_never_paints_or_hits_outside_supported_frames |
| AC-5 | test-backed | wide_board_selection_repaints_task_side_without_inline_peek |
| AC-6 | test-backed | wide_board_selection_repaints_task_side_without_inline_peek |
| AC-7 | test-backed | wide_board_enter_and_right_focus_the_same_selected_task |
| AC-8 | test-backed | task_view_escape_and_left_return_focus_without_resetting_page_session |
| AC-9 | test-backed | focused_wide_task_surface_matches_single_pane_keyboard_and_mouse_outcomes |
| AC-10 | test-backed | wide_board_task_click_selects_and_returns_board_focus |
| AC-11 | test-backed | wide_task_control_click_focuses_task_before_dispatch |
| AC-12 | test-backed | shrinking_with_board_focus_preserves_selection_and_list_scroll |
| AC-13 | test-backed | shrinking_with_task_focus_preserves_page_scroll_and_session |
| AC-14 | test-backed | shrinking_during_task_edit_preserves_mode_draft_cursor_and_binding |
| AC-15 | test-backed | growing_back_to_wide_restores_focus_and_page_session |
| AC-16 | test-backed | dirty_wide_task_session_refuses_keyboard_task_switch, dirty_wide_task_session_refuses_mouse_task_switch_and_keeps_binding |
| AC-17 | test-backed | dirty_switch_refusal_preserves_draft_and_clears_after_save, dirty_switch_refusal_clears_after_cancel |
| AC-18 | test-backed | wide_split_without_selection_paints_inert_task_empty_state |
| AC-19 | test-backed | repeated_threshold_resizes_keep_board_loop_live |
| AC-20 | test-backed | threshold_crossings_without_task_verbs_leave_domain_unchanged |

## Fresh PR-boundary verification

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release && cd site && npm ci && npm test && npm run build
exit 0
cargo test after Review panel repairs: 728 passed, 0 failed, 5 ignored across 26 result blocks
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

## Live terminal smoke

Using isolated state and config directories, the release binary was exercised at 184, 110, and 109 columns. Verified side-by-side rendering, selected-task retargeting, keyboard focus transfer, task-side mouse focus-before-dispatch, editor-preserving shrink/grow, editor-safe board clicks, intended step selection, cancellation, and repeated threshold crossings. The process remained live and the isolated data was removed afterward.

## Review panel and repair verification

Review panel discovery ran 9 seats, with 8 votes and one lost qwen holistic seat. Seven findings were kept for repair: F-14, F-7, F-8, F-1, F-10, F-13, and F-9. Two repair commits added live-editor focus, app-level keyboard/mouse/scrollbar wiring tests, refreshed retained scroll bounds, dirty thread/scope coverage, clean-editor retargeting, and same-bound editor safety.

The final two-seat verification found no regressions. Terra marked all kept findings resolved. Claude marked six resolved and returned F-8 as still present without evidence; the harness judged F-8 resolved from the concrete app-level focus-before-dispatch helpers and tests cited by Terra and present in `src/app.rs`.

## Mechanical corroboration

`sdlc-check 0.20.1 --require ledger --require verification-report`: passed with 0 findings and 0 notes before the initial push and again after Review panel repairs and report refresh.
