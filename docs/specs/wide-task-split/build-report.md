# Build report: wide task split

Date started: 2026-09-02
Branch: `feat/wide-task-split`
Base: `main` at `3f6f722`
Worktree: `/Users/saeed/.herdr/worktrees/herdr-tasks/feat-wide-task-split`
Tier: full, one implementer and one initial reviewer per task

## Baseline

Green before product changes:

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: all test binaries passed, no failed tests
cargo build --release: finished release profile
```

## Task ledger

| Task | Status | Commit | AC advanced | Notes |
| --- | --- | --- | --- | --- |
| T-1 | done | 8e2df2e | AC-3 | Initial review passed: 0 Critical, 0 Important, 5 Minor |
| T-2 | done | 1620e3c | AC-1, AC-2, AC-4, AC-5, AC-6, AC-18 | Initial review found 1 Important; remediation re-review passed |
| T-3 | done | 7db49c8 | AC-7, AC-8, AC-12, AC-13, AC-15, AC-19, AC-20 | Four blockers remediated; re-review approved |
| T-4 | done | d90f631 | AC-9, AC-10, AC-11 | Owner-approved round 2 passed; Rust and site green |
| T-5 | done | a9c7523 | AC-14, AC-16, AC-17 | Initial review passed: 0 Critical, 0 Important, 5 Minor |

### T-1 (@ `8e2df2e`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test aggregate: 686 passed, 0 failed, 5 ignored across 26 result blocks
wide_geometry_activates_at_110_and_109_stays_single: ok
wide_geometry_reserves_one_divider_and_balances_remaining_columns: ok
wide_geometry_uses_the_narrower_half_for_shared_density: ok
wide_geometry_is_bounded_across_supported_sizes: ok
cargo build --release: finished release profile
```

Initial review: pass, 0 Critical, 0 Important, 5 Minor. No remediation pass was required.

### T-2 (@ `1620e3c`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test aggregate: 691 passed, 0 failed, 5 ignored across 26 result blocks
wide_layout_activates_at_110_and_109_stays_single_pane: ok
wide_split_never_paints_or_hits_outside_supported_frames: ok
wide_board_selection_repaints_task_side_without_inline_peek: ok
wide_split_without_selection_paints_inert_task_empty_state: ok
wide_preview_task_side_controls_are_inert_until_focus_routing_exists: ok
cargo build --release: finished release profile
```

Initial review: revise, 0 Critical, 1 Important, 6 Minor. Remediation round 1 removed preview task-side action hits while preserving task-number copy hits. Finding-scoped re-review: pass, prior Important marked ADDRESSED, 0 new Critical or Important findings.

### T-3 (@ `7db49c8`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test aggregate: 702 passed, 0 failed, 5 ignored across 26 result blocks
wide_board_enter_and_right_focus_the_same_selected_task: ok
task_view_escape_and_left_return_focus_without_resetting_page_session: ok
shrinking_with_board_focus_preserves_selection_and_list_scroll: ok
shrinking_with_task_focus_preserves_page_scroll_and_session: ok
growing_back_to_wide_restores_focus_and_page_session: ok
repeated_threshold_resizes_keep_board_loop_live: ok
threshold_crossings_without_task_verbs_leave_domain_unchanged: ok
editing_task_session_escape_and_left_keep_editor_semantics: ok
narrow_board_after_focus_return_uses_board_verbs_and_row_clicks: ok
cargo build --release: finished release profile
```

Initial review: revise, 1 Critical, 3 Important, 5 Minor. Remediation round 1 addressed all four blocking findings. Re-review approved with each prior blocker marked ADDRESSED. One input desynchronization found outside the remediation diff was recorded as non-blocking there and carried into T-4.

### T-4 (@ `d90f631`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release && cd site && npm ci && npm test && npm run build
exit 0
cargo test aggregate: 713 passed, 0 failed, 5 ignored across 26 result blocks
focused_wide_task_surface_matches_single_pane_keyboard_and_mouse_outcomes: ok
wide_board_task_click_selects_and_returns_board_focus: ok
wide_task_control_click_focuses_task_before_dispatch: ok
wide_mouse_coordinates_outside_live_surface_hits_are_inert: ok
parked_help_round_trip_reopens_task_page_mode: ok
task_editor_ignores_unfocused_board_verb_hits: ok
quick_add_ignores_unfocused_task_preview_verb_hits: ok
wide_help_and_palette_close_on_foreign_surface_verbs_without_dispatching_them: ok
cargo build --release: finished release profile
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

Initial review: revise, 1 Critical, 1 Important, 6 Minor. Round 1 addressed both prior blockers but introduced 1 Important cross-surface verb regression. Owner-approved round 2 addressed it; final finding-scoped re-review approved with 0 new Critical or Important findings.

### T-5 (@ `a9c7523`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release && cd site && npm ci && npm test && npm run build
exit 0
cargo test aggregate: 719 passed, 0 failed, 5 ignored across 26 result blocks
shrinking_during_task_edit_preserves_mode_draft_cursor_and_binding: ok
dirty_wide_task_session_refuses_keyboard_task_switch: ok
dirty_wide_task_session_refuses_mouse_task_switch_and_keeps_binding: ok
dirty_switch_refusal_preserves_draft_and_clears_after_save: ok
dirty_switch_refusal_clears_after_cancel: ok
task_session_dirty_uses_only_approved_step_changes: ok
cargo build --release: finished release profile
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

Initial review: pass, 0 Critical, 0 Important, 5 Minor. No remediation pass was required.

## Live UX and UI smoke

Ran the release binary with isolated `TSK_STATE_DIR` and `TSK_CONFIG_DIR` in a controlled 184x45 pseudo-terminal launched from Herdr, then drove the real Crossterm event path.

- At 184 columns, the board, one-column divider, and selected task page painted together with distinct alpha/beta notes and steps.
- At exactly 110 columns the split remained active; at 109 the focused task editor filled the frame.
- An unsaved title draft survived 110 to 109 to 110 with edit mode intact, then `Esc` restored the persisted title.
- Returning to board focus and changing selection immediately retargeted the task side without inline peek.
- A task-side mouse click entered title edit through focus-before-dispatch; a board-row mouse click was inert during that editor, then selected and retargeted normally after cancellation and board focus.
- A task-side step mouse click focused the task and selected the intended step, reflected by the `toggle step` verb.
- Five repeated 109/110 threshold crossings left the process alive. The smoke process was then exited and its isolated state/config directories were deleted.

## Final build corroboration

`sdlc-check 0.20.1 --require ledger`: 0 findings, 0 notes. `git diff --check main...HEAD`: clean. The worktree is clean at `a9c7523`.

## Deviations

- T-4 received one owner-approved extra bounded remediation on 2026-09-02. It was limited to the cross-surface verb-hit regression introduced by round 1 and one finding-scoped re-review.
