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

> The boxed-design ledger below is **superseded** by the stage-slider rework ledger that
> follows it. Its test names were deleted or renamed when `tests/wide_task_split.rs` was
> rewritten; the current criterion-to-test map lives in `verification-report.md`.

| Task | Status | Commit | AC advanced | Notes |
| --- | --- | --- | --- | --- |
| T-1 | done | 8e2df2e | AC-3 | Initial review passed: 0 Critical, 0 Important, 5 Minor |
| T-2 | done | 1620e3c | AC-1, AC-2, AC-4, AC-5, AC-6, AC-18 | Initial review found 1 Important; remediation re-review passed |
| T-3 | done | 7db49c8 | AC-7, AC-8, AC-12, AC-13, AC-15, AC-19, AC-20 | Four blockers remediated; re-review approved |
| T-4 | done | d90f631 | AC-9, AC-10, AC-11 | Owner-approved round 2 passed; Rust and site green |
| T-5 | done | a9c7523 | AC-14, AC-16, AC-17 | Initial review passed: 0 Critical, 0 Important, 5 Minor |

## Stage-slider rework ledger (current)

The boxed design was replaced by the four-stage slider specified in
`stage-slider-rework.md`. One commit per task, test-first; the boxed ledger's tests were
rewritten in the same commits (44 tests in `tests/wide_task_split.rs` at the end of T5,
plus adapted fixtures in `tests/queue_board_loop.rs` and `tests/queue_board_mouse.rs`).

| Task | Commit | Content |
| --- | --- | --- |
| T1 geometry | `ac66ce1` | `WideStage` and `resolve_responsive`: board/rule/task rects per stage, frame-derived density |
| T2 chrome | `f74851f` | unboxed columns, dim `│` rule column, task header rule, rail renderer, shared footer with stage crumb, inert empty pane |
| T3 routing | `2a5e82b` | `route_responsive_key` key table, Enter/Esc origin memory, no-selection guards, resize continuity |
| T4 mouse | `72bb0eb` | stage A focus-before-dispatch, rail retargets, row double-click opens F, G/F parity with the single-pane page |
| T5 dirty drafts | `2b27324` | slides keep parked drafts, retarget refusals in A and on the rail, `unsaved` header slot |
| T6 docs | `d28e9f2` | AGENTS.md, technical docs, site pages, demo, spec acceptance criteria |

Tweaks from owner smoke-testing, one commit each:

| Commit | Content |
| --- | --- |
| `296e807` | a left-side click takes focus left: rail row selects and lands in A; blank rail space slides too |
| `ed0b1a7` | the header regains its status glyph; the dash rule moves to the row under the title |
| `3049a1a` | the press gate keeps stage slides, so a blank rail press reaches dispatch |

## Boxed-design history (superseded)

Everything below this line records the boxed 50/50 era: T-1 to T-5, its smokes, repairs,
the task-box follow-up, and the main integration and mechanical corroboration of that time.
It is kept for history only; its test names no longer exist.

### T-1 (@ `8e2df2e`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test aggregate: 686 passed, 0 failed, 5 ignored across 26 result blocks
wide_geometry_activates_at_110_and_109_stays_single: ok
wide_geometry_balances_touching_allocations_without_a_gap: ok
wide_geometry_uses_narrower_content_for_shared_density: ok
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

## Live UX and UI smoke (boxed design, superseded)

Ran the release binary with isolated `TSK_STATE_DIR` and `TSK_CONFIG_DIR` in a controlled 184x45 pseudo-terminal launched from Herdr, then drove the real Crossterm event path.

- At 184 columns, the board, one-column divider, and selected task page painted together with distinct alpha/beta notes and steps.
- At exactly 110 columns the split remained active; at 109 the focused task editor filled the frame.
- An unsaved title draft survived 110 to 109 to 110 with edit mode intact, then `Esc` restored the persisted title.
- Returning to board focus and changing selection immediately retargeted the task side without inline peek.
- A task-side mouse click entered title edit through focus-before-dispatch; a board-row mouse click was inert during that editor, then selected and retargeted normally after cancellation and board focus.
- A task-side step mouse click focused the task and selected the intended step, reflected by the `toggle step` verb.
- Five repeated 109/110 threshold crossings left the process alive. The smoke process was then exited and its isolated state/config directories were deleted.

## Post-build PR review repairs (boxed design, superseded)

Review panel discovery kept F-14, F-7, F-8, F-1, F-10, F-13, and F-9. Commit `4ee409d` added visible editor focus, app-level keyboard/mouse/scrollbar handoff coverage, retained scroll-bound refresh, thread/scope dirty tests, and clean-editor retargeting. Verification then found same-bound editor clicks could hide the editor; commit `4586803` made those clicks inert and added clean/dirty regressions.

Final repair green bar:

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release && cd site && npm ci && npm test && npm run build
exit 0
cargo test aggregate: 728 passed, 0 failed, 5 ignored across 26 result blocks
app_keyboard_route_owns_responsive_focus_handoffs: ok
app_mouse_click_focuses_task_before_dispatching_same_control: ok
app_task_scrollbar_focuses_refreshes_bound_and_routes_page_scroll: ok
board_focused_task_field_edit_focuses_and_paints_live_editor: ok
clean_task_editor_board_click_retargets_and_focuses_board: ok
clean_task_editor_same_bound_row_click_keeps_task_focus: ok
dirty_task_scope_dropdown_same_bound_row_click_keeps_visible_editor: ok
site npm test: 6 passed, 0 failed
site build: 10 pages built
```

Final two-seat Review panel verification reported no regressions. Terra marked every kept finding resolved. Claude marked six resolved and returned F-8 as still present without evidence; the harness judged F-8 resolved from the concrete app-level helper and test evidence.

## Main integration

Merged `origin/main` at `v0.4.0` as `6ed7d8f`, resolved the `AGENTS.md` guidance conflict, and adapted four feature test fixtures to the stabilized domain creation signature. The complete Rust bar passed with 711 passed, 0 failed, and 5 ignored across 26 result blocks. Site tests passed 6 of 6 and built 10 pages. A fresh isolated smoke of the rebuilt release binary passed at 110 and 109 columns, including active editor preservation and same-bound row click safety.

## Owner-approved task-box-only follow-up (superseded)

The wide compositor now lets the board use its complete left allocation and places only the task page inside a bordered right allocation. The task's left border is the sole visual center separator. This changes presentation geometry and explicit task-border focus styling only; input tables, domain behavior, persistence, and below-threshold rendering are unchanged. The bounded Review panel remediation made the responsive resolver's density authoritative for both rendered surfaces, expanded full-ring styling coverage, and pinned numbered-title retargeting. Fresh red and green evidence is recorded in `.agent-sdlc/briefs/wide-task-split/PR-30-boxed-chrome-report.md`.

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
sdlc-check 0.20.1 --require ledger --require verification-report: 0 findings, 0 notes
```

## Final build corroboration

`sdlc-check 0.20.1 --require ledger --require verification-report`: 0 findings, 0 notes after final report refresh. `git diff --check origin/main...HEAD`: clean at integrated product head `6ed7d8f`.

## Deviations

- T-4 received one owner-approved extra bounded remediation on 2026-09-02. It was limited to the cross-surface verb-hit regression introduced by round 1 and one finding-scoped re-review.
