# AGENTS.md

## Routing

Would this instruction make sense to a stranger who cloned this repo? If no, it
belongs in `AGENTS.local.md`.

A gitignored `AGENTS.local.md` may exist beside this file; if present, read it
before starting. Edits go to `AGENTS.md` or `AGENTS.local.md`, never `CLAUDE.md`.

If private-routed content appears and no `AGENTS.local.md` exists yet, create one.
The committed `.gitignore` already covers it.

@AGENTS.local.md

## Product

**herdr-tasks** is a herdr plugin: a queue board for capture and human-status
verbs. Park, resume, linking, and dispatch-start are not board or palette
actions. Dispatch recovery can still open for a persisted attempt. Crate and
plugin are `0.1.0`.

A gitignored `HANDOFF.md` may hold this clone’s live working state.

### Board

- Sections are computed, never navigated: IN MOTION · project groups when showing
  all projects, ON DECK when scoped · global last · done drawer (`z`)
- Standard ≥78×24, compact below, operable to 40×10
- Human status: `ready` · `started` · `blocked` · `review` · `done`. The store
  still reads old `todo`/`doing` values.
- Mutating verbs (`space` `d` `o` `b` `e` `n` `x` `u` `q`) need Alt, or Ctrl
  if the user flipped it in the palette. Bare letters do nothing. Nav, peek,
  `Enter`, `P`, `z`, `:`, `?`, `+`, and `Esc` stay bare.
- Task creation is the quick-add bar, never a form takeover. `+` opens a one-line
  title input on the status-row slot with a blank row above and below, list still
  visible. `Enter` saves and closes, `Ctrl+Enter` saves and stays open, `Tab`
  expands the draft onto the task page with a title·notes·scope stash, so Esc
  returns to the line and a second `Tab` restores what was typed. Scope tokens in
  the title: bare `!p` global, `!p name` by project basename (case-insensitive),
  `!p /path` verbatim; tokens are stripped from the saved title. A saved task
  becomes the selection. Success has no status message: the row flash is the
  feedback. Refusals paint while the line is open and clear when it closes.
- There is no inline board capture form. Creation detail lives on the task page;
  the standalone Capture UI (`AppMode::Capture`, `src/ui/capture.rs`) is a
  separate surface reached through the host launcher.
- Row click peeks. Click the same row again closes peek. Fast double-click opens
  the page.
- Task page is view-first. Only `e`/`n`/Tab (with the verb modifier) enter edit
  mode, the one exception being a quick-add draft expanded with `Tab`, which opens
  straight into Notes edit mode because a draft has nothing to view. The scope
  footer is inert until an edit has started.
- Mono modifiers only. No color theme module.
- No live attention poll on the board.

## Build / test / verify

- Build: `cargo build --release`
- `herdr-plugin.toml` launches `./target/release/herdr-tasks`. Rebuild in-repo
  before live smoke. A running board keeps the old binary until you quit it.
- Test: `cargo test` (plain, parallel)
- A regression test must fail without its fix. Write it, revert the fix, watch it
  fail, restore the fix. Use content that actually crosses the boundary under test.
- Temp state dirs need a per-binary atomic counter, not just `SystemTime::now()`.
- Green bar: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`
- CI installs Rust **1.96.0** with `rustfmt` + `clippy` on ubuntu and macos. The
  frame-time bench is Linux-only.

### Live herdr smoke

When the change touches the board, host integration, or panes, do not call it
done on unit tests alone.

If `HERDR_ENV=1`: rebuild in-repo, drive the real path with herdr, read the pane,
and fix anything that only fails live.

If `HERDR_ENV` is unset, say that live smoke was not run.

## Conventions

- Human status is source of truth. Never auto-complete tasks from agent status.
- State under `HERDR_PLUGIN_STATE_DIR`. Config under `HERDR_PLUGIN_CONFIG_DIR`.
  Verb modifier is `settings.json` in that config dir; the palette flips it.
- UI chrome lives in `src/ui/` (`board/` model·apply·commands·chrome·draw,
  `capture`, `mouse`, `input`, `render`).
- `map_edit` and `map_board_form_key` share `map_form_edit_key`. Save-recovery
  `r`/`c`/Esc must reach Retry/Cancel even if a form is allocated
  (`src/app.rs` allowlist).
- A form and its input mode must outlive the save call. Never clear the form or
  switch mode before the persistence boundary confirms: a cancelled failed save
  otherwise leaves an edit mode with no form allocated, which no key can escape.
- Anything painted on the status-row slot hides `status_message` while it is up.
  A surface that lives there owns showing its own refusals and clearing them on
  close, or the message is invisible and then leaks onto the board afterwards.
