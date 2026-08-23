# T-10 review fixes

## Red proof

Before the Notes fix, `cargo test --test queue_board_render notes_edit -- --nocapture` failed: after deep wheel scrolling, no Notes draft row painted and the caret landed on step content. The one-scroll regression also failed after its expectation was tightened to the cursor-window origin.

The verification-regression tests were observed red first: restoring the cursor-presence-only footer guard made `stale_step_cursor_falls_back_to_the_live_task_status_verb` fail with `space toggle step`; removing the shared focus reset from form navigation made `shift_tab_from_scope_resets_notes_stream_origin_and_aligns_caret` paint step rows instead of the Notes draft.

## Fixes

- Notes edit resets the shared reading offset when it enters the cursor-windowed draft, keeping draft rows and caret aligned.
- The footer step input forwards recovery/refusal feedback, so it remains visible while that surface owns the status-row slot.
- Added direct cursor lifecycle and wheel assertions, an active-cursor footer `toggle step` assertion, and the mouse route for `a step`.
- Added the missing `ConfirmEditNext` persistence-classification assertion and a failed-save step-rename retry test.
- Clarified AC-19's intentional blank shared-flow row and AC-27's edit-entry reset.

## Review assessment

F12 now has classifier coverage. F13 now exercises the rename save-recovery handoff. F18/F19 remain low, out of requested scope. F22 is accepted product behavior and the local spec now says so.

## Verification

Passed: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release` (312 unit tests plus integration suite, no flakes). The two new regressions pass: stale step cursors advertise `start`, and Scope Shift+Tab returns Notes to the shared-stream origin with its caret on the painted draft row.

Live smoke passed with `HERDR_ENV=1`: rebuilt release, opened a temporary-state board in a new Herdr pane, created and opened `review smoke`, added `live step`, activated it with Down, and read the pane. The active footer truthfully changed to `alt+space toggle step`. The pane was closed; no plugin link, push, or PR comment was made.
