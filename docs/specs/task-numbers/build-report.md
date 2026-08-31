# Build report — task-numbers

Branch: `feat/task-numbers` in `.worktrees/task-numbers` (from `origin/main` @ `c2bca55`).
Baseline green bar: passed 2026-08-31 in this worktree.

## Task ledger

| Task | Status | Commit | AC advanced | Notes |
| --- | --- | --- | --- | --- |
| T-1 | done | 77a352f0ce54bb42c5e45ee4eac77b97b27cc83b | AC-1 AC-2 AC-3 AC-4 AC-6 AC-7 AC-9 AC-10 AC-11 AC-12 AC-13 | one remediations pass on tests; no second review |
| T-2 | done | 0614f1f5983b1e7f4f7cf6b4c83a2b039b2e9794 | AC-5 AC-20 AC-21 AC-22 | initial review clean |
| T-3 | done | 75b522da022876f68c7266c17be8fdb63233ac43 | AC-8 AC-14 AC-15 AC-16 AC-17 AC-18 AC-19 AC-28 | remediations on test oracles |
| T-4 | done | 9c1ccad4c4e4186a8eb0d98b16378ebbebffba9b | AC-23 AC-24 AC-25 AC-26 AC-27 | remediations: scope hits + real-path footer test |
| T-5 | done | 1b5f96086bfedf4ddbc56e6632b07d9cd5dd7590 | AC-29 | remediations: compact board.md wording |

### T-1 (@ `77a352f0ce54bb42c5e45ee4eac77b97b27cc83b`)

Green bar: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release` exit 0.

```
test domain::task::tests::locked_create_assigns_number_one_then_two ... ok
test domain::task::tests::edit_status_scope_thread_complete_soft_delete_leave_number_unchanged ... ok
test domain::task::tests::undo_of_complete_and_of_soft_delete_keeps_the_same_number ... ok
test pre_number_store_upgrades_assigning_distinct_numbers_including_done_and_deleted ... ok
test upgraded_store_preserves_numbers_on_second_load ... ok
test overlapping_creates_under_lock_receive_distinct_numbers ... ok
test v2_store_is_refused_when_writer_format_is_older_and_the_file_is_unwritten ... ok
test reload_merge_save_keeps_numbers_and_counter_across_another_writer ... ok
```

### T-2 (@ `0614f1f5983b1e7f4f7cf6b4c83a2b039b2e9794`)

Green bar exit 0. Initial review: no blocking findings.

```
test json_list_rows_include_number ... ok
test human_list_rows_show_bare_digits ... ok
test json_created_and_existing_include_number ... ok
test plan_created_and_existing_include_number ... ok
test existing_add_does_not_advance_the_counter ... ok
```

### T-3 (@ `75b522da022876f68c7266c17be8fdb63233ac43`)

Green bar exit 0.

```
test list_bare_digits_finds_the_task_from_another_cwd ... ok
test list_bare_digits_finds_done_and_deleted_tasks ... ok
test list_uuid_still_finds_the_task ... ok
test list_bare_digits_with_scope_or_status_flags_is_usage ... ok
test list_unknown_number_matches_unknown_uuid_refusal ... ok
test steps_bare_digits_act_on_the_task ... ok
test steps_uuid_still_acts_on_the_task ... ok
test steps_unknown_number_is_unknown_task ... ok
```

### T-4 (@ `9c1ccad4c4e4186a8eb0d98b16378ebbebffba9b`)

Green bar exit 0 after remediations.

```
test board_row_meta_shows_bare_digits_not_hash_or_t_prefix ... ok
test peek_shows_bare_digits ... ok
test task_page_footer_shows_bare_digits ... ok
test done_drawer_rows_show_bare_digits ... ok
test numbered_board_at_40x10_paints_in_bounds_wraps_titles_and_keeps_tasks_reachable ... ok
test ac_25_task_page_footer_number_precedes_the_form_scope_hit_target ... ok
```

### T-5 (@ `1b5f96086bfedf4ddbc56e6632b07d9cd5dd7590`)

`cd site && npm test` exit 0, 4 passed.

```
demo rows paint bare task numbers ... ok
```

## Deviations

Isolation created at `.worktrees/task-numbers`; parent clone stays on `fix/steps-notes-spacing`.
T-1 High findings remediated (tests). Owner skipped a second review.
T-3 Medium test-oracle findings remediated.
T-4 blocked on red suite; remediations updated hit tests and added a real-path footer test.
T-5 Medium compact-docs finding remediated.
Live herdr smoke of the board was not run.
