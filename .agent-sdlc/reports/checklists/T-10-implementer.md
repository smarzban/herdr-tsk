# T-10 implementer report

## Delivered

- Keyboard arrows now move or deactivate an active step cursor before shared-content scrolling.
- Wheel scroll remains content-only and does not alter the step cursor.
- The Notes edit caret subtracts the shared stream offset before placement.
- Updated the local checklist spec with revised AC-17, AC-18, and AC-24; added AC-26/AC-27, component and task coverage mappings, and the T-10 plan.

## Test-first evidence

Against `ef572e3`, before the fixes:

- `active_cursor_up_precedes_shared_scroll` failed: Up left `second step` selected while it scrolled the stream.
- `notes_edit_caret_accounts_for_shared_stream_scroll` failed: caret y was 5, expected 4.

Both pass after the fixes. The active-cursor regression also verifies the wheel leaves that cursor unchanged.

## Verification

Passed: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`.

Live smoke passed with `HERDR_ENV=1`: rebuilt release binary in a temporary state directory, opened it in a new Herdr pane, created and opened `T-10 live smoke`, added `live step one`, exercised cursor keys, read the pane, then closed the pane. No plugin link, push, or PR posting occurred.
