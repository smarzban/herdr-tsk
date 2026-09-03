# App loop

**Responsibility.** Choose Board vs Capture, load store + snapshot + settings,
run the crossterm/ratatui loop, persist through save recovery, idle-merge
external writes. Does not derive queue sections (that is `ui::queue`) and does
not own domain rules.

**Public surface.** `MODE_ENV`, `AppMode`, `resolve_mode` / `resolve_mode_from`,
`load_board` / `load_board_model`, `run`, `board_frame` / `board_idle_tick`,
`StoreWatch`, `BoardSaveContext`, `apply_board_intent_with_save_recovery`,
walkthrough helpers, drag autoscroll helpers. `save_recovery::{SaveRecovery, SaveFailure}`.
Scheduler constants live in `ui::scheduler`.

## How it works

`app::run` is what `tsk_tui::run` calls after `main` has classified the surface as
Board or Capture.

### Mode

`resolve_mode_from(mode_env, args)`: `TSK_MODE=capture` (any case) or positional
`capture` → `AppMode::Capture`, else Board. Process-level validation of unknown
commands is `cli::router`, not this helper.

### Board loop (shape)

1. Load `TaskStore`, `DomainState`, snapshot, and `BoardModel`.
2. Seed `StoreWatch` from the file we just loaded so the first idle tick does not
   immediately re-merge. The launch path does **not** call
   `open_walkthrough_for_launch` (that helper is for tests; the palette can still
   replay the card).
3. Enter raw mode / alternate screen; query keyboard enhancement *before* the
   alternate screen (it can block on a round-trip).
4. Each iteration: `board_idle_tick` → settle (`record_walkthrough_dismissal` if a
   card just closed, expire ephemeral messages) → paint → wait
   `board_poll_duration`.
5. On `FramePoll::Idle`, `revalidate_board_from_store` if the watch says `tsk.json`
   mtime/size changed **and** save recovery is not pending.
6. On event: coalesce resize bursts (`RESIZE_DEBOUNCE` 50 ms, cap 250 ms), paint
   at the settled size, then dispatch. Keys go through `route_responsive_key` (the
   stage slider: intent, inert, or hand the key to the surface) and the existing
   `board_keyboard_intent`. Mouse uses translated frame hits through
   `map_responsive_board_mouse`; a stage A press on the task column slides to G first
   and then dispatches the same click against the painted hits. Paste goes through
   `map_edit_paste` for the focused mode.
7. Mutating intents take a domain clone as `BoardSaveContext.baseline` *before*
   `apply_intent` (`board_intent_needs_fresh_state` / `board_intent_may_persist`).
8. `IntentOutcome::Persist` → `reload_merge_save`. Failure → `SaveRecovery::fail`
   and `BoardInputMode::SaveRecovery`. Quit is allowed during recovery so a failed
   save cannot trap the session.

Wait duration: `scheduler::next_wait`. Idle base 250 ms. While text-drag
autoscroll is armed: 33 ms floored at 25 ms.

### Save recovery

`SaveRecovery<DomainState>` holds baseline (last persisted) and working (intended)
snapshots plus the error string. Only one unresolved failure (`debug_assert`).

- **Retry:** persist working; on success replace the loop's `domain` and
  `end_save_recovery(Retried)`; on failure keep both snapshots, update the error.
- **Cancel:** restore baseline into `domain`, `sync_from_domain`,
  `end_save_recovery(Cancelled)`. Cancel of a failed quick-add has distinct chrome
  so “save cancelled” is not claimed for a capture that never landed.

The form and its mode stay allocated through this whole window (invariant 19).
Responsive presentation is derived for each event from the model's `WideStage`. At 110
usable columns or wider, board or rail hits use the left column, task hits are
translated from the task column past its pad cell, and the shared footer's hits span
the frame. Coordinates outside the focused column are inert except for the explicit
row selects, the stage A task-column slide, and the footer. Scrollbar and drag
ownership use the same column (`drag_content_area`).
`board_keyboard_intent` only hands keys to `map_board_form_key` when the resolved
mode is a **form field** (`EditTitle` / `EditNotes` / `EditThread` / `EditScope` /
`FormScopeDropdown`). `SaveRecovery` is not on that allowlist, so `r` / `c` / Esc
reach Retry/Cancel.

Confirm-edit is judged against the durable baseline *before* the reducer
(`confirm_edit_refusal_against_the_record`) so a stale target refuses without
mutating, and the session (mode, draft, cursor, binding) stays intact.

### Idle merge

`StoreWatch` stores `(modified, len)` of `tsk.json`. `poll` returns the new
signature without recording it. `record` happens only after `store.load` and
`merge_tasks_from_disk` succeed. A transient read error leaves `last_seen` alone
so the next tick retries. Skipped entirely while recovery is pending.

`sync_from_domain` rules: [invariants](invariants.md) §16.

### Capture loop

`run_capture`: load store, `CaptureModel::from_snapshot`, form loop, exit process
on save or cancel (overlay-style). Save uses `capture_save` with `Some(&store)`
and the same recovery type parameterized on capture's working state.

## Invariants

[Invariants](invariants.md) §§16, 19, 20, 29, 30.

- `board_frame` owns settle-before-wait; do not move settle into the loop's
  `continue` paths.
- Do not clear a form before `persist` returns.
- Walkthrough write errors stay on the message row.

## Error paths

Store load at launch fails `run` (binary prints `tsk: {err}`, exit 1). Mid-session
save failures become recovery. Idle load failures are silent retries. Walkthrough
write failures are chrome messages.

## Extension points

New mutating intents must be added to `board_intent_may_persist` (single list the
loop and the chrome-row lifetime both read). New modes that can outrank a form
must stay off the form-key allowlist if they need their own keymap.
